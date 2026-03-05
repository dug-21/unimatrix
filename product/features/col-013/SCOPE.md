# col-013: Extraction Rule Engine

## Problem Statement

Unimatrix accumulates knowledge only through explicit `context_store` calls by agents. This is a fundamental bottleneck: agents must know what to store, when to store it, and how to categorize it. In practice, most valuable knowledge — implicit conventions, knowledge gaps, dead entries, recurring friction patterns, file dependencies — emerges from behavioral signals that no agent explicitly captures. The system observes everything (via hooks persisted to the `observations` table by col-012) but extracts nothing.

Additionally, maintenance operations (confidence refresh, co-access cleanup, HNSW compaction, session GC) require manual `maintain=true` calls on `context_status`, which agents rarely invoke. This means the knowledge base degrades silently unless a human or agent remembers to trigger maintenance.

col-013 solves both problems: it extracts knowledge automatically from observation data via deterministic rules, and it runs maintenance automatically via a background timer — transforming Unimatrix from a passive store into an active knowledge acquisition engine.

## Goals

1. Implement an `ExtractionRule` trait mirroring the existing `DetectionRule` pattern in unimatrix-observe, producing `ProposedEntry` values from observation data
2. Ship 5 initial extraction rules: knowledge gap, implicit convention, dead knowledge, recurring friction, file dependency
3. Build a quality gate pipeline ensuring auto-extracted entries meet trust thresholds (near-duplicate check, contradiction check, rate limiting, cross-feature validation, confidence floor)
4. Store auto-extracted entries with `trust_source: "auto"` and appropriate provenance metadata
5. Replace the manual `maintain=true` path on `context_status` with automatic background maintenance via `tokio::spawn` + interval timer
6. Make `context_status` read-only (reports maintenance status, no longer performs writes)
7. Refactor CRT subsystems as needed: crt-002 (new trust_source value), crt-003 (extract single-entry contradiction check), crt-005 (per-trust_source lambda breakdown, maintenance relocation)
8. Absorb col-005 (Auto-Knowledge Extraction) — its three tiers map to rules 2, 4, and 5

## Non-Goals

- **Neural models or ML-based extraction** — deferred to crt-007 (Neural Extraction Pipeline)
- **LLM API integration** — deferred to crt-009
- **Lesson extraction from failure traces** — remains agent-driven
- **New MCP tools** (e.g., `context_review`) — deferred to crt-009
- **Multi-repository extraction** — per-repo scope only (dsn-phase concern)
- **`Proposed` entry status** — the vision mentions a Proposed status for low-confidence extractions but this requires new status handling across all MCP tools; defer to a follow-up. All stored entries use existing `Active` status with confidence scores reflecting extraction certainty
- **Daemon mode** — extraction runs during session lifetime via `tokio::spawn`, not as a persistent daemon

## Background Research

### Existing Codebase Patterns

**DetectionRule trait (unimatrix-observe):** 21 detection rules across 4 categories implement `DetectionRule::detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>`. The `ExtractionRule` trait mirrors this pattern but produces `ProposedEntry` values instead of findings. The `default_rules()` pattern provides a registry of all rules.

**Observations table (col-012, schema v7):** SQLite table with columns `id, session_id, ts_millis, hook, tool, input, response_size, response_snippet`. Indexed by session_id and ts_millis. Provides the queryable data source for extraction rules.

**Contradiction scanning (crt-003):** `scan_contradictions()` in `infra/contradiction.rs` loops over all active entries, re-embeds content, searches HNSW neighbors, and applies the multi-signal conflict heuristic. The inner logic (single-entry check) needs extraction for point-of-insertion use by the quality gate.

**Maintenance path (crt-005):** `StatusService::run_maintenance()` performs co-access cleanup, confidence refresh (batch 100), and graph compaction. Currently triggered only by `maintain=true` on `context_status`. This code moves to a background tick function.

**Trust source scoring (crt-002):** Confidence computation uses `trust_source` as a scoring factor. Current values: "agent" (0.5), "human" (0.7), "system" (0.6), other (0.3). The new "auto" value needs a weight of 0.35.

**Session registry (col-010):** `SessionRegistry` handles session lifecycle with stale session sweep in the maintain path.

### Technical Landscape

- The extraction pipeline runs in the same `tokio` runtime as the MCP server
- `spawn_blocking` is the established pattern for CPU-bound store operations
- The existing dedup infrastructure (cosine similarity >= 0.92) in `context_store` can be reused
- `ObservationRow` (from col-012's SQLite source) differs from `ObservationRecord` (unimatrix-observe's in-memory type) — a conversion or unified type is needed

### Constraints Discovered

- The `observations` table uses `ObservationRow` (SQLite rows) while detection rules use `ObservationRecord` (in-memory structs) — the extraction rules need access to both observation data and the knowledge store
- Rate limiting (max 10/hour) requires tracking extraction timestamps, likely a simple in-memory counter or a small SQLite table
- Cross-feature validation requires querying observations across multiple session/feature boundaries
- The background tick must not block the MCP request path — all maintenance and extraction runs via `tokio::spawn` + `spawn_blocking`

## Proposed Approach

### ExtractionRule Trait + 5 Rules

New `extraction` module in `unimatrix-observe` (or a new `unimatrix-extract` crate if the dependency graph requires it — the extraction rules need access to both observations and the Store). The trait signature:

```rust
pub trait ExtractionRule: Send {
    fn name(&self) -> &str;
    fn evaluate(&self, observations: &[ObservationRow], store: &Store) -> Vec<ProposedEntry>;
}
```

Five rules:
1. **KnowledgeGapRule** — `context_search` calls with zero results, same query pattern across 2+ features
2. **ImplicitConventionRule** — Same file/path pattern in 100% of observed features
3. **DeadKnowledgeRule** — Entry accessed in features 1-N but not in N+1 through N+5 (access cliff)
4. **RecurringFrictionRule** — Same hotspot (from detection rules) in 3+ features
5. **FileDependencyRule** — Consistent read-before-edit chains across 3+ features

### Quality Gate Pipeline

Sequential checks before storing any proposed entry:
1. Near-duplicate check (cosine >= 0.92 against existing entries)
2. Point-of-insertion contradiction check (refactored from crt-003)
3. Content validation (min length, category allowlist)
4. Rate limit (max 10 auto-extractions per hour, in-memory counter)
5. Cross-feature validation (minimum 2-5 features depending on rule type)
6. Confidence floor (< 0.2 discard)

### Background Maintenance Tick

Single `tokio::spawn` with `tokio::time::interval` (~1 hour default):
- Confidence refresh (batch 100 stale entries)
- Co-access cleanup (>30 day pairs)
- HNSW graph compaction (if stale ratio > 10%)
- Session GC (timed-out cleanup)
- Extraction pipeline trigger (run rules on accumulated observations since last run)

### CRT Refactors

- **crt-002**: Add `"auto" -> 0.35` to trust_source scoring (~5 lines)
- **crt-003**: Extract `check_single_entry_contradiction()` from `scan_contradictions()` (~30 lines refactored)
- **crt-005**: Add `coherence_by_source: HashMap<String, f64>` to StatusReport (~40 lines)
- **crt-005**: Relocate maintenance operations from `StatusService::run_maintenance()` to `maintenance_tick()` (~100 lines moved)

### context_status Changes

- Remove (or deprecate) the `maintain` parameter
- Add fields to StatusReport: `last_maintenance_run`, `next_maintenance_scheduled`, `extraction_stats` (counts by rule, last run time)
- `context_status` becomes purely diagnostic

## Acceptance Criteria

- AC-01: `ExtractionRule` trait defined with `name()` and `evaluate()` methods matching the proposed signature
- AC-02: KnowledgeGapRule produces gap entries from zero-result `context_search` observations across 2+ features
- AC-03: ImplicitConventionRule produces convention entries from 100%-consistent file patterns across all observed features
- AC-04: DeadKnowledgeRule produces deprecation signals for entries with access cliffs (accessed in N features, absent in N+5)
- AC-05: RecurringFrictionRule produces lesson-learned entries from hotspots recurring in 3+ features
- AC-06: FileDependencyRule produces dependency entries from consistent read-before-edit chains across 3+ features
- AC-07: Quality gate rejects near-duplicates (cosine >= 0.92 against existing entries)
- AC-08: Quality gate rejects entries that contradict existing knowledge (point-of-insertion contradiction check)
- AC-09: Quality gate enforces rate limit of max 10 auto-extractions per hour
- AC-10: Quality gate enforces cross-feature validation (no entry from single feature's observations)
- AC-11: Quality gate discards entries with confidence < 0.2
- AC-12: Auto-extracted entries stored with `trust_source: "auto"` and provenance metadata linking to source observations
- AC-13: Background maintenance tick runs automatically via `tokio::spawn` + `tokio::time::interval`
- AC-14: Maintenance tick performs confidence refresh, co-access cleanup, HNSW compaction, and session GC
- AC-15: Extraction pipeline triggers piggyback on maintenance tick infrastructure
- AC-16: `context_status` becomes read-only — reports maintenance status (last run, next scheduled) but performs no writes
- AC-17: crt-002 confidence scoring includes `"auto" -> 0.35` trust_source weight
- AC-18: crt-003 `check_single_entry_contradiction()` function extracted from `scan_contradictions()` and usable by quality gate
- AC-19: crt-005 StatusReport includes per-trust_source lambda breakdown (`coherence_by_source`)
- AC-20: All existing tests continue to pass (no regressions from CRT refactors or maintenance relocation)
- AC-21: Extraction rules have unit tests with synthetic observation data
- AC-22: Quality gate pipeline has integration tests covering each rejection path

## Constraints

- **col-012 dependency**: The `observations` table (schema v7) must exist. col-012 is merged on main (commit 1ea06a2).
- **Schema**: No new schema migration required — auto-extracted entries use existing `entries` table with `trust_source: "auto"`. Rate limiting state is in-memory (resets on server restart, which is acceptable).
- **Performance**: Background tick must not block MCP request handling. All maintenance and extraction work runs via `spawn_blocking` in a dedicated `tokio::spawn` task.
- **Crate boundaries**: Extraction rules need access to both `Store` (unimatrix-store) and observation data. The `unimatrix-observe` crate currently does not depend on `unimatrix-store`. Either: (a) add unimatrix-store dependency to unimatrix-observe, (b) create a new crate, or (c) put extraction logic in unimatrix-server where both are available. Architecture decision needed.
- **Backward compatibility**: `maintain` parameter on `context_status` should be deprecated gracefully (accepted but ignored, or retained as emergency override) rather than removed, to avoid breaking existing agent workflows.
- **~675 lines total**: ~600 lines new code + ~75 lines CRT refactors (per ASS-015 scoping)

## Open Questions

1. **Crate placement**: Should `ExtractionRule` implementations live in `unimatrix-observe` (requires new store dependency), a new `unimatrix-extract` crate, or in `unimatrix-server`? The existing `DetectionRule` is in `unimatrix-observe` but doesn't need store access.
2. **ObservationRow vs ObservationRecord**: Should extraction rules consume the SQLite `ObservationRow` type directly, or should there be a conversion to the existing `ObservationRecord` type? Or a unified type?
3. **Maintain parameter deprecation strategy**: Should `maintain=true` be silently ignored, produce a deprecation warning, or be retained as an "immediate maintenance" override? The product vision says "deprecated or retained as run NOW emergency override."
4. **Extraction trigger frequency**: The background tick runs maintenance hourly. Should extraction run on the same cadence, or more frequently (e.g., after each session close)?

## Tracking

GitHub Issue to be created during Session 1 synthesis phase.
