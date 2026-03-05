# Architecture: col-013 Extraction Rule Engine

## Overview

col-013 introduces three architectural subsystems:

1. **Extraction pipeline** -- `ExtractionRule` trait + 5 rules + quality gate, living in `unimatrix-observe`
2. **Background maintenance tick** -- `tokio::spawn` + interval timer in `unimatrix-server`, replacing the manual `maintain=true` path
3. **CRT refactors** -- targeted changes in `unimatrix-engine` (trust_score), `unimatrix-server` (contradiction extraction, status reporting, maintenance relocation)

Plus a **type migration** -- shared observation types move from `unimatrix-observe` to `unimatrix-core`.

## Architecture Decisions

### ADR-001: Extraction Rules in unimatrix-observe with Store Dependency

**Context:** Extraction rules need access to both observation data (`ObservationRecord`) and the knowledge store (`Store`) for cross-referencing (e.g., dead knowledge detection requires checking entry access patterns). Currently, unimatrix-observe has no dependency on unimatrix-store (per col-012 ADR-001). The human approved placing extraction rules in unimatrix-observe.

**Decision:** Add `unimatrix-store` as a dependency of `unimatrix-observe`. Create an `extraction` module alongside the existing `detection` module.

**Rationale:**
- Extraction rules are conceptually parallel to detection rules -- both analyze observation data
- Colocation avoids creating a new crate for ~300 lines of extraction code
- The dependency direction (observe -> store) is one-way and does not create cycles (store has no dependency on observe)
- The existing `ObservationSource` trait remains the abstraction for detection rules; extraction rules take `&Store` directly since they need richer store operations

**Consequences:**
- unimatrix-observe gains a build-time dependency on unimatrix-store (and transitively rusqlite)
- The `#![doc]` and lib.rs comment about "no dependency on unimatrix-store" must be updated
- Detection rules remain storage-independent; only extraction rules use `&Store`

### ADR-002: Observation Types to unimatrix-core

**Context:** `ObservationRecord`, `HookType`, `ParsedSession`, `ObservationStats` are defined in `unimatrix-observe::types` and used by unimatrix-server for UDS event handling and now by extraction rules. Future crates (crt-007, crt-008) will also consume these types.

**Decision:** Move `ObservationRecord`, `HookType`, `ParsedSession`, `ObservationStats` to `unimatrix-core`. Re-export them from `unimatrix-observe` for backward compatibility.

**Rationale:**
- unimatrix-core is the shared foundation crate (already depends on store, vector, embed)
- Making observation types available from core avoids forcing every consumer to depend on unimatrix-observe
- Re-exports preserve the existing public API of unimatrix-observe

**Consequences:**
- ~80-100 lines of import changes across ~14 files
- unimatrix-core gains `serde_json` dependency (ObservationRecord.input is `Option<serde_json::Value>`)
- No logic changes

### ADR-003: Background Tick Architecture

**Context:** Maintenance operations (confidence refresh, co-access cleanup, HNSW compaction, session GC) are currently triggered by `maintain=true` on `context_status`. The extraction pipeline also needs periodic triggers. Both should run automatically.

**Decision:** Single `tokio::spawn` launched at server startup, running a `tokio::time::interval` loop. Each tick:
1. Checks what maintenance is needed (lightweight reads)
2. Dispatches maintenance work via `spawn_blocking` tasks
3. Runs extraction pipeline on new observations (since last tick)
4. Records tick metadata (timestamp, duration, items processed)

The tick interval is configurable (default 15 minutes for the initial implementation, adjustable based on observed cost). The interval is short enough for responsive extraction but long enough that per-run cost is negligible.

**Rationale:**
- 15 minutes balances responsiveness with efficiency -- at negligible per-run cost (per human guidance to optimize for small incremental runs)
- A single coordinator avoids multiple timers competing for resources
- `spawn_blocking` isolates CPU-bound work from the async runtime
- The tick function is a standalone async function, easily testable in isolation

**Architecture:**

```
tokio::spawn(background_tick_loop)
    |
    +-- interval.tick().await  (every 15 min)
    |
    +-- check_maintenance_needed()  (async, lightweight SQL reads)
    |
    +-- spawn_blocking(maintenance_tick)  (confidence, co-access, compaction, session GC)
    |
    +-- spawn_blocking(extraction_tick)  (run rules on new observations, quality gate, store entries)
    |
    +-- update_tick_metadata()  (last_run, next_scheduled, items processed)
```

**Consequences:**
- `StatusService::run_maintenance()` body moves to a `maintenance_tick()` function callable from the background loop
- `context_status` `maintain` parameter silently ignored
- StatusReport gains `last_maintenance_run` and `next_maintenance_scheduled` fields
- Server startup must launch the background task after all subsystems are initialized

### ADR-004: Extraction Watermark Pattern

**Context:** Extraction rules need to process observations accumulated since the last run, not the entire table each time (addresses SR-02).

**Decision:** Track a `last_processed_observation_id` watermark (the maximum `observations.id` processed in the previous tick). Each extraction tick queries `SELECT ... FROM observations WHERE id > ?` to get only new observations.

**Rationale:**
- The `observations.id` column is an autoincrement primary key, providing a natural monotonic ordering
- Watermark queries are index-bound and O(new_rows), not O(total_rows)
- The watermark is stored in-memory (resets on server restart, which means the first tick after restart processes all observations -- acceptable for correctness, and bounded by the 90-day retention window)

**Consequences:**
- Extraction rules see only incremental data per tick
- Cross-feature validation still works because it queries the Store for historical patterns, not just the new observation batch
- On server restart, a full scan occurs once (bounded by retention)

### ADR-005: Quality Gate Pipeline Order

**Context:** The quality gate runs 6 checks per proposed entry. Some checks are cheap (rate limit, content validation) and some expensive (embedding + cosine similarity, contradiction check). Order matters for efficiency.

**Decision:** Pipeline order (cheapest rejections first):
1. Rate limit check (in-memory counter, O(1))
2. Content validation (min length, category allowlist, O(1))
3. Cross-feature validation (SQL count query, O(1))
4. Confidence floor check (computed from entry metadata, O(1))
5. Near-duplicate check (embed + HNSW search, O(embedding_time + log(n)))
6. Point-of-insertion contradiction check (embed + HNSW search + heuristic, O(embedding_time + k*log(n)))

**Rationale:** Steps 1-4 reject invalid entries before the expensive embedding-based checks (5-6). This ensures the common case (rate limited, single-feature data, low-confidence) is filtered cheaply.

### ADR-006: Single-Entry Contradiction Check Extraction

**Context:** The quality gate needs a point-of-insertion contradiction check for a single proposed entry against existing knowledge. The existing `scan_contradictions()` iterates over all active entries. We need the inner logic (check one entry against neighbors) as a standalone function.

**Decision:** Extract `check_entry_contradiction()` from `scan_contradictions()` in `infra/contradiction.rs`. The new function takes a single entry's content, embeds it, searches HNSW for neighbors, and runs the conflict heuristic against each. Returns `Option<ContradictionPair>` (the highest-scoring conflict, if any).

**Signature:**
```rust
pub fn check_entry_contradiction(
    content: &str,
    title: &str,
    store: &Store,
    vector_store: &dyn VectorStore,
    embed_adapter: &dyn EmbedService,
    config: &ContradictionConfig,
) -> Result<Option<ContradictionPair>, ServerError>
```

**Rationale:**
- Reuses the existing `conflict_heuristic()` function unchanged
- `scan_contradictions()` can be refactored to call `check_entry_contradiction()` for each entry (reducing code duplication)
- The extraction is purely mechanical -- the inner loop body of `scan_contradictions()` becomes the new function

## Component Diagram

```
unimatrix-core  (gains ObservationRecord, HookType, ParsedSession, ObservationStats)
      |
      v
unimatrix-observe  (gains unimatrix-store dep, extraction module)
  |-- detection/  (21 detection rules, unchanged)
  |-- extraction/
  |     |-- mod.rs       (ExtractionRule trait, run_extraction_pipeline(), quality_gate())
  |     |-- knowledge_gap.rs
  |     |-- implicit_convention.rs
  |     |-- dead_knowledge.rs
  |     |-- recurring_friction.rs
  |     |-- file_dependency.rs
  |-- types.rs  (re-exports from unimatrix-core, plus extraction-specific types)
  |-- source.rs (ObservationSource trait, unchanged)
      |
      v
unimatrix-server
  |-- background.rs      (background_tick_loop, maintenance_tick, extraction_tick)
  |-- services/status.rs (run_maintenance -> maintenance_tick, context_status read-only)
  |-- infra/contradiction.rs (check_entry_contradiction extracted)
  |-- server.rs           (spawn background tick at startup)

unimatrix-engine
  |-- confidence.rs       (trust_score adds "auto" -> 0.35)
```

## Data Flow

### Extraction Pipeline (per tick)

```
1. Query observations WHERE id > last_watermark
2. Group by session_id -> feature_cycle (via SESSIONS table)
3. For each ExtractionRule:
     rule.evaluate(observations, store) -> Vec<ProposedEntry>
4. For each ProposedEntry:
     quality_gate(entry) -> Accept | Reject(reason)
5. Accepted entries: store via Store API with trust_source="auto"
6. Update watermark to max(processed observation ids)
```

### Maintenance Tick (per tick)

```
1. Co-access cleanup (>30 day pairs)
2. Confidence refresh (batch 100 stale entries)
3. HNSW compaction (if stale_ratio > 10%)
4. Session GC (timed-out sessions)
5. Observation cleanup (>90 day retention)
```

### context_status Changes

**Before (col-012):**
- `maintain=true` triggers run_maintenance()
- StatusReport includes maintenance results

**After (col-013):**
- `maintain` parameter silently ignored
- StatusReport gains:
  - `last_maintenance_run: Option<u64>` (epoch seconds)
  - `next_maintenance_scheduled: Option<u64>` (epoch seconds)
  - `extraction_stats: ExtractionStats` (entries_extracted, entries_rejected, last_run, rules_fired)
  - `coherence_by_source: HashMap<String, f64>` (per-trust_source lambda)
- All maintenance operations run via background tick

## Integration Points

| Component | Change Type | Description |
|-----------|-------------|-------------|
| `unimatrix-core` | Type addition | ObservationRecord, HookType, ParsedSession, ObservationStats moved here |
| `unimatrix-observe` | New module + dep | `extraction/` module, unimatrix-store dependency |
| `unimatrix-engine/confidence.rs` | 1-line change | trust_score: `"auto" => 0.35` |
| `unimatrix-server/infra/contradiction.rs` | Function extraction | `check_entry_contradiction()` extracted from `scan_contradictions()` |
| `unimatrix-server/services/status.rs` | Refactor | `run_maintenance()` body -> `maintenance_tick()`, StatusReport gains fields |
| `unimatrix-server/background.rs` | New file | Background tick loop, maintenance dispatch, extraction dispatch |
| `unimatrix-server/server.rs` | Startup change | Launch background tick task |
| `unimatrix-server/mcp/tools.rs` | Parameter change | `maintain` parameter ignored |

## Risk Mitigations

| Risk | Mitigation |
|------|-----------|
| SR-01 (tick starvation) | Background tick is async; only work dispatch uses spawn_blocking. Tick itself cannot be starved. |
| SR-02 (observation table growth) | Watermark pattern (ADR-004) ensures O(new_rows) queries. 90-day retention cleanup. |
| SR-03 (quality gate cost) | Cheap checks first (ADR-005). Rate limit caps at 10/hour. |
| SR-05 (crate coupling) | ADR-001 documents the trade-off. Detection rules remain storage-independent. |
| SR-07 (silent maintenance failure) | `last_maintenance_run` in StatusReport. Log at INFO level. 2x interval warning. |
| SR-08 (write contention) | Same spawn_blocking + store locking pattern as existing writes. |
| SR-09 (low-quality entries) | Quality gate pipeline. trust_score 0.35 naturally ranks lower. Cross-feature validation. |
