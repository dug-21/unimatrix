# col-010b — Retrospective Evidence Synthesis & Lesson-Learned Persistence

**Phase**: Collective
**Status**: Scoped
**Depends on**: col-010 (P0 merged — SESSIONS, INJECTION_LOG, from_structured_events())
**Resolves**: GitHub Issue #65

---

## Background

col-010 delivered P0 (session lifecycle persistence, schema v5, structured retrospective). The P1
components were explicitly split out via ADR-006 because they are independent of the col-011 critical
path. col-010b picks up those two components and delivers them as a self-contained feature.

The design session for col-010 fully specified P1 behaviour. The following documents from col-010 are
authoritative source material and must be read before designing col-010b:

| Document | Relevant Sections |
|----------|------------------|
| `product/features/col-010/specification/SPECIFICATION.md` | FR-10, FR-11, FR-12 |
| `product/features/col-010/architecture/ARCHITECTURE.md` | Components 6–7 |
| `product/features/col-010/architecture/ADR-004-lesson-learned-fire-and-forget-embedding.md` | Lesson-learned embedding |
| `product/features/col-010/architecture/ADR-005-provenance-boost-query-time-constant.md` | Provenance boost |
| `product/features/col-010/RISK-TEST-STRATEGY.md` | R-09 (blocking gate for Component 6) |
| `product/features/col-010/IMPLEMENTATION-BRIEF.md` | P1 component summary and constraints |
| `product/features/col-010/ACCEPTANCE-MAP.md` | AC-15 through AC-24 |

col-010b requires its own full Session 1 design cycle (architect, spec, risk strategist, vision guardian,
synthesizer) to produce the implementation artifacts (pseudocode, test plan, etc.).

---

## Problem Statement

The col-002 retrospective engine produces verbose output (~87KB per feature cycle) that is difficult for
agents to act on — all evidence is included with no synthesis or prioritisation. Additionally, hotspot
findings are ephemeral: they are produced and discarded each call, with no persistence in the Unimatrix
knowledge base. This means future agents cannot benefit from prior retrospective analysis, and
lesson-learned findings never influence search ranking.

---

## Goals

### Goal 1 — Evidence-Limited Retrospective Output (`evidence_limit: usize`)

Add an `evidence_limit` parameter to `context_retrospective` (default `3`). Each
`HotspotFinding.evidence` array is truncated server-side to at most `evidence_limit` items before
serialisation. The `hotspots: Vec<HotspotFinding>` type is unchanged — truncation is applied without
modifying the in-memory report.

`evidence_limit = 0` disables truncation and reproduces the pre-col-010b full-evidence behaviour
(backward-compatible for any callers that need complete arrays).

### Goal 2 — Evidence Synthesis (HotspotNarrative)

Add an additive `narratives: Option<Vec<HotspotNarrative>>` field to `RetrospectiveReport`, populated
only when the structured-events path is used (`from_structured_events()`). Each `HotspotNarrative`
provides a synthesised, human-readable summary of a hotspot alongside timestamp-clustered events,
top-5 affected files, and (for sleep_workarounds) a monotone sequence pattern.

Add `recommendations: Vec<Recommendation>` covering four hotspot types with templated actionable text.

### Goal 3 — Lesson-Learned Auto-Persistence

After `context_retrospective` returns a report with ≥1 hotspot or recommendation, automatically write a
`category:lesson-learned` entry to Unimatrix. The entry carries full ONNX embedding (fire-and-forget
via `tokio::spawn` — does not block the response). De-duplication via supersede: a second retrospective
call for the same feature_cycle deprecates the prior lesson-learned entry and writes a new one.
`trust_source = "system"` for correct 0.7 trust score.

### Goal 4 — Provenance Boost for `lesson-learned` Entries

Add `PROVENANCE_BOOST = 0.02` (named constant in `confidence.rs`) applied at search re-ranking time
to any entry with `category == "lesson-learned"`. Applied additively alongside co-access affinity in
both the UDS ContextSearch path and the MCP `context_search` tool. Does not modify stored confidence
values — the stored weight invariant (`W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92`)
is preserved.

---

## Build Components

### Component 1 — Evidence-Limited Output + Wire Type (P1-A)
- Add `evidence_limit: Option<usize>` to the `context_retrospective` wire request type (`wire.rs`)
- Server-side truncation in `tools.rs` context_retrospective handler: truncate each
  `hotspot.evidence` to `evidence_limit` items before serialisation; do not modify in-memory report
- Default: `3`; `0` = unlimited
- **Blocking gate**: before implementing, audit all existing `context_retrospective` integration tests
  for assertions on exact `hotspots[].evidence` array lengths. Update those tests to pass
  `evidence_limit = 0` (restore full arrays) or update expected count to ≤ 3. This is R-09 from
  col-010's risk strategy — do not skip.

### Component 2 — Evidence Synthesis (`from_structured_events()` extension)
- New types in `unimatrix-observe/src/types.rs` (additive, `#[serde(default)]`):
  - `HotspotNarrative { hotspot_type, summary, clusters: Vec<EvidenceCluster>, top_files: Vec<(String, u32)>, sequence_pattern: Option<String> }`
  - `EvidenceCluster { window_start, event_count, description }`
  - `Recommendation { hotspot_type, action, rationale }`
- Add `narratives: Option<Vec<HotspotNarrative>>` and `recommendations: Vec<Recommendation>` to
  `RetrospectiveReport` with `#[serde(default, skip_serializing_if = "...")]`
- Extend `from_structured_events()` in `structured.rs` to populate narratives:
  - Timestamp clustering: `CLUSTER_WINDOW_SECS: u64 = 30` sliding window
  - Sequence extraction: monotone-increasing numeric values in sleep_workarounds → `"30s→60s→90s→120s"`
  - Top-N file lists: top-5 by mutation count with `"... and N more"` suffix
  - Entry performance correlation: `injection_success_rate` from SESSIONS data
- Recommendation templates in `report.rs` covering 4 hotspot types:
  - `permission_retries` → "Add common build/test commands to settings.json allowlist"
  - `coordinator_respawns` → "Review coordinator agent lifespan and handoff patterns"
  - `sleep_workarounds` → "Use run_in_background + TaskOutput instead of sleep polling"
  - `compile_cycles` (only when `measured > 10.0`) → "Consider incremental compilation or targeted cargo test invocations"
- `narratives` is `None` when the JSONL fallback path is used

### Component 3 — Lesson-Learned Auto-Persistence
- After `context_retrospective` builds report with `hotspots.len() > 0 OR recommendations.len() > 0`:
  - Check CategoryAllowlist for `"lesson-learned"` — skip if absent, log error
  - Query for existing active lesson-learned entry with `topic == "retrospective/{feature_cycle}"`
  - If found: supersede (deprecate old, write new with `supersedes = old_id`)
  - Fire-and-forget via `tokio::spawn`: ONNX embed + store write
  - `context_retrospective` returns before embedding completes
- Entry fields:
  - `title`: `"Retrospective findings: {feature_cycle}"`
  - `content`: narrative summaries + recommendations (structured path); hotspot claims only (JSONL path)
  - `topic`: `"retrospective/{feature_cycle}"`
  - `category`: `"lesson-learned"`
  - `tags`: `["feature_cycle:{feature_cycle}", "hotspot_count:{n}", "source:retrospective"]`
  - `created_by`: `"cortical-implant"`
  - `trust_source`: `"system"` (0.7 trust score — correctness fix, not a boost)
- Known limitation: concurrent `context_retrospective` calls for the same feature_cycle may briefly
  produce two active lesson-learned entries. Tolerated — next call reduces to one (SR-09).

### Component 4 — Provenance Boost
- Add `pub const PROVENANCE_BOOST: f64 = 0.02` to `crates/unimatrix-engine/src/confidence.rs`
- Apply in `uds_listener.rs` ContextSearch re-ranking and `tools.rs` search handler:
  ```
  final_score = 0.85 * sim + 0.15 * conf + co_access_affinity + provenance_boost
  ```
  where `provenance_boost = PROVENANCE_BOOST if category == "lesson-learned" else 0.0`
- Does not modify stored confidence; `0.92` invariant preserved

---

## Acceptance Criteria

| AC-ID | Description | Component | Verification |
|-------|-------------|-----------|-------------|
| AC-01 | `context_retrospective` with default `evidence_limit` returns ≤3 evidence items per hotspot; total JSON payload ≤10KB for a 13-hotspot report | 1 | Integration test |
| AC-02 | `context_retrospective` with `evidence_limit = 0` returns complete evidence arrays — output is field-for-field identical to pre-col-010b output | 1 | Integration test (snapshot comparison) |
| AC-03 | `context_retrospective` with default `evidence_limit` returns `narratives: Some(...)` alongside capped evidence when the structured-events path is used; `narratives` is `None` on the JSONL path | 2 | Integration test |
| AC-04 | A `sleep_workarounds` hotspot with monotone-increasing sleep intervals (30s, 60s, 90s, 120s) produces `HotspotNarrative.sequence_pattern = Some("30s→60s→90s→120s")`. Non-monotone pattern returns `None`. | 2 | Unit test |
| AC-05 | A report with a `permission_retries` hotspot includes a `Recommendation` with non-empty `action`. A report with no recognised hotspot types produces an empty `recommendations` list. All 4 template types verified. | 2 | Unit test |
| AC-06 | After `context_retrospective` with ≥1 hotspot or recommendation, a `category:lesson-learned` entry exists in Unimatrix with `topic = "retrospective/{feature_cycle}"`, non-empty content, `trust_source = "system"`, and `embedding_dim > 0` (after background task completes). | 3 | Integration test (await embed) |
| AC-07 | Calling `context_retrospective` twice for the same feature_cycle produces exactly one active lesson-learned entry — the second supersedes the first (`status=Deprecated`, `superseded_by` set on prior entry). | 3 | Integration test |
| AC-08 | `context_search` with a query related to a prior retrospective finding (e.g., "permission retry patterns") returns the lesson-learned entry within the top 5 results. | 3 | Integration test |
| AC-09 | A lesson-learned entry and a generic convention entry with identical similarity and confidence scores: the lesson-learned entry ranks higher by exactly `PROVENANCE_BOOST = 0.02`. Verified on both the UDS ContextSearch path and the MCP `context_search` tool path. | 4 | Unit test + integration test |
| AC-10 | All existing tests pass without modification after evidence_limit addition, RetrospectiveReport type additions, and provenance boost. Existing `context_retrospective` tests that assert on exact evidence array lengths are updated to pass `evidence_limit = 0`. | All | `cargo test --workspace` |

---

## Non-Goals

- Schema migration — col-010b has no schema changes (application logic only)
- SESSIONS / INJECTION_LOG write paths — delivered in col-010 P0
- Auto-outcome session entries — delivered in col-010 P0
- `session_id: Option<String>` on `EntryRecord` — bincode positional encoding; deferred indefinitely
- Secondary index on INJECTION_LOG — full scan acceptable at current volumes
- Sophisticated narrative ML — synthesis is deterministic heuristics only
- `helpful_count` seeding on lesson-learned entries (Wilson MINIMUM_SAMPLE_SIZE=5 guard makes seeding ineffective)
- Category-specific `MINIMUM_SAMPLE_SIZE` reduction for lesson-learned

---

## Resolved Design Decisions (inherited from col-010)

| Decision | Resolution | Source ADR |
|----------|------------|-----------|
| Lesson-learned ONNX embedding | Fire-and-forget `tokio::spawn` — `context_retrospective` returns before embedding completes | col-010 ADR-004 |
| Provenance boost mechanism | `PROVENANCE_BOOST = 0.02` query-time constant — stored `0.92` invariant unchanged | col-010 ADR-005 |
| `trust_source = "system"` | All cortical-implant-generated entries use `trust_source = "system"` for correct 0.7 trust score | col-010 SPEC SEC-03 |
| Supersede race (SR-09) | Concurrent retrospective calls may briefly produce two active lesson-learned entries — tolerated, next call resolves | col-010 SPEC FR-11.6 |
| `evidence_limit = 0` backward compat | Callers requiring full evidence arrays must pass `evidence_limit = 0`; default is 3 | col-010 SPEC FR-10.1 |
| `hotspots` type unchanged | `Vec<HotspotFinding>` type is not changed — truncation is server-side only | col-010 SPEC FR-10.2 |
| `narratives` additive only | `narratives: Option<Vec<HotspotNarrative>>` with `#[serde(default, skip_serializing_if)]` — no breaking change | col-010 SPEC FR-10.2 |
| JSONL path unchanged | `build_report()` JSONL path is unmodified; `narratives = None` when JSONL used | col-010 SPEC NFR-02.1 |

---

## Constraints

- **R-09 gate (blocking)**: Audit all existing `context_retrospective` integration tests for assertions on
  exact `hotspots[].evidence` array lengths before implementing Component 1. Update tests to
  `evidence_limit = 0` or ≤3 expected count. Do not skip.
- **No embedding in hot path**: lesson-learned ONNX embedding must remain fire-and-forget; never add
  blocking ONNX to `context_retrospective` response path
- **Stored weight invariant**: `PROVENANCE_BOOST` is query-time only; stored `confidence` is never modified
- **`hotspots` type invariant**: `Vec<HotspotFinding>` type must not change
- **P0 must be merged**: col-010 P0 (SESSIONS, INJECTION_LOG, `from_structured_events()`) must be
  merged before col-010b implementation begins
- Workspace constraints: Edition 2024, MSRV 1.89

---

## Dependencies

| Dependency | Type | Reason |
|------------|------|--------|
| col-010 P0 | Hard prerequisite (merged) | `from_structured_events()`, SESSIONS, INJECTION_LOG — all required for synthesis and lesson-learned lookup |
| col-002 | Existing | `RetrospectiveReport`, `HotspotFinding`, `ObservationRecord` — extended additively |
| col-001 | Existing | Lesson-learned entry write uses the same store pathway as outcome entries |
| `unimatrix-embed` | Existing | ONNX embedding for lesson-learned entries |
| `unimatrix-engine` | Existing | `confidence.rs` — `PROVENANCE_BOOST` constant added here |

---

## Files to Modify

| File | Change |
|------|--------|
| `crates/unimatrix-observe/src/types.rs` | Add `HotspotNarrative`, `EvidenceCluster`, `Recommendation` types; add `narratives: Option<Vec<HotspotNarrative>>` and `recommendations: Vec<Recommendation>` to `RetrospectiveReport` |
| `crates/unimatrix-observe/src/structured.rs` | Extend `from_structured_events()` with narrative synthesis (clustering, sequence extraction, top-N files, injection_success_rate) |
| `crates/unimatrix-observe/src/report.rs` | Add `recommendations_for_hotspots()` covering 4 hotspot type templates |
| `crates/unimatrix-engine/src/confidence.rs` | Add `pub const PROVENANCE_BOOST: f64 = 0.02` |
| `crates/unimatrix-server/src/wire.rs` | Add `evidence_limit: Option<usize>` to `context_retrospective` request type |
| `crates/unimatrix-server/src/tools.rs` | Add evidence truncation (evidence_limit), lesson-learned fire-and-forget write, PROVENANCE_BOOST application in search re-ranking |
| `crates/unimatrix-server/src/uds_listener.rs` | Add `PROVENANCE_BOOST` to ContextSearch re-ranking path |

No new files, no schema migration.

---

## Tracking

GH Issue: TBD (create after scope approval)
Parent feature: col-010 (P0 delivered in PR #77)
Resolves: GitHub Issue #65
