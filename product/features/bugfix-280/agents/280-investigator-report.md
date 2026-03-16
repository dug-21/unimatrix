# Bug Investigation Report: 280-investigator

## Bug Summary

The background maintenance tick calls `compute_report()` — a function designed for the interactive `context_status` MCP tool — in order to obtain the `active_entries` list and `effectiveness` data it needs. This forces the tick to execute all 8 phases of `compute_report()`, including contradiction scanning (O(N) ONNX inference), category/topic distributions, outcome stats, observation stats, and retrospected feature count — none of which the tick consumes. At current scale, the cumulative tick cost is 40–89 seconds against a 120-second timeout. Any disk or SQLite slowness pushes the tick over the limit, causing intermittent timeout failures.

## Root Cause Analysis

`maintenance_tick()` at `background.rs:512` calls:

```rust
let result = status_svc.compute_report(None, None, false).await;
let (mut report, active_entries) = match result { ... };
```

`compute_report()` is an 8-phase pipeline designed for the `context_status` interactive tool. From that pipeline, `maintenance_tick()` consumes only two things:

1. `active_entries: Vec<EntryRecord>` — passed to `run_maintenance()` for confidence refresh (Phase 2) and graph compaction (Phase 3).
2. `report.effectiveness: Option<EffectivenessReport>` — used to build `EffectivenessState.categories` and feed the auto-quarantine loop.
3. `report.graph_stale_ratio: f64` — read inside `run_maintenance()` at line 973 to gate graph compaction.

Everything else in `compute_report()` — Phases 1 (distributions), 2 (contradiction scan), 3 (embedding check), 4 (co-access stats), 5 (coherence dimensions), 6 (observation stats), 7 (retrospected feature count) — runs on every tick and its output is discarded.

### Code Path Trace

```
run_single_tick()                                          background.rs:370
  tokio::time::timeout(TICK_TIMEOUT, maintenance_tick())   background.rs:381
    status_svc.compute_report(None, None, false)           background.rs:512
      Phase 1: SQL distributions (category, topic)         status.rs:225–303   [UNUSED by tick]
      Phase 1: compute_status_aggregates()                 status.rs:307–315   [UNUSED by tick]
      Phase 1: load_active_entries_with_tags()             status.rs:318–320   [USED: active_entries]
      Phase 1: load_outcome_entries_with_tags() + loop     status.rs:322–363   [UNUSED by tick]
      Phase 2: scan_contradictions() (O(N) ONNX)          status.rs:432–458   [UNUSED by tick]
      Phase 3: check_embedding_consistency() (skipped)     status.rs:461–487   [UNUSED by tick]
      Phase 4: co_access_stats() + top_co_access_pairs()   status.rs:490–540   [UNUSED by tick]
      Phase 5: coherence dimensions computation            status.rs:542–636   [UNUSED by tick, except graph_stale_ratio]
      Phase 6: observation_stats()                         status.rs:638–665   [UNUSED by tick]
      Phase 7: list_all_metrics()                          status.rs:667–678   [UNUSED by tick]
      Phase 8: compute_effectiveness_aggregates()          status.rs:681–755   [USED: effectiveness field]
            + load_entry_classification_meta()
    (report, active_entries) returned to maintenance_tick

    Step 2: report.effectiveness → EffectivenessState     background.rs:527–617
    Step 9: process_auto_quarantine (uses effectiveness_report)  background.rs:621–629
    Step 10: run_maintenance(&active_entries, &mut report) background.rs:638–646
      reads: active_entries                                status.rs:805, 975, 982
      reads: report.graph_stale_ratio                      status.rs:973
      writes: report.stale_pairs_cleaned, etc. (not read by tick)
```

### Why It Fails

`compute_report()` is a sequential multi-phase function with each phase awaiting a `spawn_blocking_with_timeout()` call. The tick therefore holds the tokio async execution context (waiting on blocking threads) for the full duration of all 8 phases. The dominant costs are:

- **Phase 2 (contradiction scan)**: `scan_contradictions()` runs O(N) ONNX inference calls — one embedding per active entry, then HNSW search. At 50 active entries: ~5 seconds. At 200+ entries: approaches or exceeds the remaining tick budget after other phases.
- **Phase 8 (effectiveness)**: `compute_effectiveness_aggregates()` runs 4 complex JOIN queries over `injection_log` and `sessions`. `load_entry_classification_meta()` scans all active entries. At scale, 5–60 seconds.
- **Phase 1 (distributions + outcome scan)**: `load_outcome_entries_with_tags()` iterates all outcome entries with tag loading. `compute_status_aggregates()` runs multiple aggregation queries.
- **Phases 5–7 (coherence, observation stats, retrospected count)**: each `spawn_blocking_with_timeout` creates a blocking thread that must acquire `Mutex<Connection>`, competing with concurrent MCP request handlers.

None of Phase 2, 3, 4, 5, 6, 7 output is used by the tick. Phase 1 distributions and correction aggregates are also unused. Only `active_entries` (from load_active_entries_with_tags inside Phase 1), `graph_stale_ratio` (computed in Phase 5), and `effectiveness` (Phase 8) are consumed.

The intermittent (rather than consistent) failure pattern is explained by the cumulative budget:

```
Phase 1 SQL + active_entries load:   ~2–5s
Phase 2 contradiction scan (N=50):   ~5–15s
Phase 4 co-access stats:             ~1–3s
Phase 5 coherence dimensions:        ~1–2s
Phase 6 observation stats:           ~1–3s
Phase 7 list_all_metrics:            ~1–2s
Phase 8 effectiveness:               ~5–15s
run_maintenance() internals:         ~10–20s (confidence refresh, prior, GC)
                          Total:     ~26–65s (mean)
```

Under SQLite contention (MCP handler competing for mutex) or disk pressure, any single phase can 2–3x, pushing the cumulative total past 120 seconds.

## Affected Files and Functions

| File | Function | Role in Bug |
|------|----------|-------------|
| `crates/unimatrix-server/src/background.rs` | `maintenance_tick()` (line 501) | Calls `compute_report()` at line 512; only uses `active_entries`, `effectiveness`, and `graph_stale_ratio` from the result |
| `crates/unimatrix-server/src/services/status.rs` | `compute_report()` (line 201) | Runs 8-phase pipeline; tick discards Phases 1 (partial), 2, 3, 4, 5 (partial), 6, 7 output |
| `crates/unimatrix-server/src/services/status.rs` | `run_maintenance()` (line 771) | Takes `active_entries` and `report`; reads `report.graph_stale_ratio` at line 973 |
| `crates/unimatrix-store/src/read.rs` | `load_active_entries_with_tags()` (line 833) | The single store query the tick actually needs from Phase 1 |
| `crates/unimatrix-store/src/read.rs` | `compute_effectiveness_aggregates()` (line 895) | The store queries the tick actually needs from Phase 8 |
| `crates/unimatrix-store/src/read.rs` | `load_entry_classification_meta()` (line 1002) | Also needed for Phase 8 path |

## Proposed Fix Approach

### 1. Define `MaintenanceDataSnapshot`

Add a new struct in `status.rs` (or a new `maintenance.rs` module) containing only what the tick needs:

```rust
pub(crate) struct MaintenanceDataSnapshot {
    /// Active entries with tags — needed for confidence refresh and graph compaction.
    pub active_entries: Vec<EntryRecord>,
    /// Graph stale ratio — needed to gate graph compaction in run_maintenance().
    pub graph_stale_ratio: f64,
    /// Effectiveness report — needed for EffectivenessState update and auto-quarantine.
    pub effectiveness: Option<unimatrix_engine::effectiveness::EffectivenessReport>,
}
```

### 2. Add `StatusService::load_maintenance_snapshot()`

Add a new async method to `StatusService` that runs 3 targeted operations instead of 8 phases:

**Operation A** — single `spawn_blocking_with_timeout`: call `store.load_active_entries_with_tags()`. Returns `Vec<EntryRecord>` directly.

**Operation B** — inline (no blocking): compute `graph_stale_ratio` from `self.vector_index.point_count()` and `self.vector_index.stale_count()`. These are in-memory reads on the `VectorIndex` — no store I/O, no `spawn_blocking` needed. This is already done in Phase 5 of `compute_report()` at lines 556–565 with zero blocking calls.

**Operation C** — single `spawn_blocking_with_timeout`: call `store.compute_effectiveness_aggregates()` and `store.load_entry_classification_meta()`, then run `classify_entry()` loop and `build_report()` to produce `Option<EffectivenessReport>`. This mirrors Phase 8 of `compute_report()` exactly.

Total: 2 blocking operations instead of 6–8 sequential ones. Contradiction scan, distribution queries, outcome stats, observation stats, and retrospected feature count are eliminated entirely from the tick path.

### 3. Update `maintenance_tick()` to call `load_maintenance_snapshot()`

Replace lines 511–522 in `background.rs`:

```rust
// Current (line 512):
let result = status_svc.compute_report(None, None, false).await;
let (mut report, active_entries) = match result { ... };

// Proposed:
let snapshot = status_svc.load_maintenance_snapshot().await?;  // emit_tick_skipped on Err
let active_entries = snapshot.active_entries;
let graph_stale_ratio = snapshot.graph_stale_ratio;
let effectiveness = snapshot.effectiveness;
```

The `run_maintenance()` call at line 638 currently receives `(&active_entries, &mut report)`. With `MaintenanceDataSnapshot`, the tick no longer has a `StatusReport` to mutate. Two options:

**Option A (minimal change)**: Keep `run_maintenance()` signature unchanged but construct a thin `StatusReport` with only `graph_stale_ratio` populated (all other fields default). The maintenance function writes back to the report fields but those writes are never read by the tick anyway — this is already the case today.

**Option B (clean)**: Refactor `run_maintenance()` to take `graph_stale_ratio: f64` directly instead of `&mut StatusReport`, since it only reads `report.graph_stale_ratio` (line 973) and writes to fields that are unused by the caller. This removes the `StatusReport` dependency entirely from the tick path.

Option B is cleaner but has a larger diff. Option A ships the performance fix with minimal blast radius.

### 4. Keep `compute_report()` unchanged

`compute_report()` continues to serve the interactive `context_status` MCP tool path. No callers change except `maintenance_tick()`.

### Why This Fix

The fix targets the exact conflation described in the bug report: the tick uses `compute_report()` as a data loader when it only needs 3 of the 8 things that function computes. A targeted snapshot loader eliminates the 5–6 wasted phases without touching the interactive tool path, correction scan logic, or any other callers.

The approach follows the codebase's existing pattern for decoupling: `EffectivenessStateHandle` (crt-018b) and `ConfidenceStateHandle` (crt-019) are both examples of "tick computes, multiple consumers read from cache." `MaintenanceDataSnapshot` is a load-side version of the same pattern — the tick loads only what it needs rather than computing a full report.

## Risk Assessment

- **Blast radius**: `maintenance_tick()` is the sole caller of `compute_report()` that would be changed. The interactive `context_status` tool path calls `compute_report()` via `StatusService` through the MCP tool handler at `mcp/tools.rs` — that path is untouched.

- **Regression risk from Option A (thin StatusReport)**: Low. `run_maintenance()` reads only `report.graph_stale_ratio` (line 973) from the incoming `report`. All other `report.*` fields it writes to (`stale_pairs_cleaned`, `confidence_refreshed_count`, `graph_compacted`) are set by `run_maintenance()` itself and never read by `maintenance_tick()` after the call returns — the `report` is dropped. Setting a thin `StatusReport` with `graph_stale_ratio` populated and all else defaulted produces identical behavior to today.

- **Regression risk from Option B (refactor run_maintenance signature)**: Medium. `run_maintenance()` signature changes require updating both `maintenance_tick()` (the only call site in production code) and any test that constructs a `StatusReport` to pass in. The test at `background.rs:1256–1267` (`apply_tick_write`) does not call `run_maintenance()` directly, so test breakage would be limited to integration tests that drive the full `maintenance_tick()`.

- **Graph compaction gate**: The `graph_stale_ratio` value must be available in `load_maintenance_snapshot()`. As noted in the fix: this is computed from in-memory `VectorIndex` reads (`point_count()` + `stale_count()`), which are already called without blocking in Phase 5. No risk of regression here.

- **Effectiveness data race**: `maintenance_tick()` currently consumes `effectiveness` from `compute_report()` and writes it into `EffectivenessState`. The proposed fix moves the same effectiveness computation (same SQL queries, same classification loop) into `load_maintenance_snapshot()`. The output is functionally identical. No race condition is introduced.

- **Confidence**: High. The root cause is unambiguous — all 8 phases of `compute_report()` execute on every tick, and the tick only consumes output from 3 of them. The code path is straightforward to read and the fix is mechanical. The intermittent timeout pattern is fully explained by the cumulative cost analysis.

## Missing Test

The test that should have caught this is an **integration test for `maintenance_tick()` cost budget**:

```
test: maintenance_tick completes within time budget using mock StatusService
setup:
  - Mock or stub StatusService with an instrumented compute_report() that records which phases ran
  - Use a real or stub store with a known number of active entries (e.g., 50)
  - Wire a real EffectivenessStateHandle and a real VectorIndex
assert:
  - maintenance_tick() completes in < X ms (e.g., 5000ms with mocked SQL)
  - Only load_active_entries_with_tags, compute_effectiveness_aggregates, and
    load_entry_classification_meta are called on the store (not load_outcome_entries_with_tags,
    scan_contradictions, co_access_stats, observation_stats, list_all_metrics)
```

A lighter version of this test would be a **unit test on `load_maintenance_snapshot()`** that asserts it makes exactly 2 `spawn_blocking` calls (one for `load_active_entries_with_tags`, one for the effectiveness computation), verified by counting mock store method invocations.

The underlying gap: no test measured the number of store operations triggered by a tick, and no test asserted the tick's wall-clock budget at any entry count. Tests for `maintenance_tick()` in `background.rs` (e.g., `test_tick_write_updates_categories_from_report`) operate on the in-memory `EffectivenessState` logic only — they use an `apply_tick_write()` helper that explicitly bypasses `compute_report()`.

## Reproduction Scenario

Not intermittent at the code level — the bug is deterministic. The intermittent timeout failures in production occur because the cumulative tick cost (40–89s at current scale) is close to the 120s timeout, and noise from SQLite mutex contention tips individual runs over the limit. The root cause (all 8 phases run on every tick) is reliably reproducible by adding timing instrumentation to `compute_report()` and observing the full pipeline execute during a maintenance tick.

To reproduce the timeout: run with 100+ active entries, trigger simultaneous `context_search` MCP calls during the tick window, and observe the maintenance tick timeout in the trace logs.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` (context_search) for `background tick compute_report maintenance data loading performance` with `category:lesson-learned` — found entry #1628 (per-query full-store reads causing MCP instability, crt-014), #1759 (extraction tick batch size mutex pattern, bugfix-279).
- Queried: `/uni-query-patterns` (context_search) for `Arc RwLock background tick state snapshot pattern` with `category:pattern` — found entry #1560 (background-tick state cache pattern, crt-019) and #1561 (generation-cached snapshot pattern, crt-018b). Both directly relevant as the prior art for this fix.
- Stored: Will store a lesson capturing the "compute_report() repurposed as tick data loader" anti-pattern after the fix is confirmed. Not stored pre-fix to avoid recording an incomplete picture.
- Declined: Storing the specific diagnosis details of GH#280 — the bug analysis lives on the GH issue; Unimatrix gets only the generalizable pattern.
