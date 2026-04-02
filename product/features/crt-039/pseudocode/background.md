# background.rs — Tick Orchestrator Pseudocode
# crt-039: Remove `nli_enabled` gate; add ordering invariant comment; label contradiction scan

## Purpose

`background.rs` owns `run_single_tick`, which calls all tick steps in invariant order.
crt-039 makes three changes to this file — all comment or structural-wrapper changes with
zero behavioral change to any step except the removal of the outer `nli_enabled` gate:

1. Add tick ordering invariant comment before the contradiction scan block.
2. Rename/label the contradiction scan block as a named independent tick step.
3. Remove the `if inference_config.nli_enabled { ... }` wrapper around `run_graph_inference_tick`.

No new functions, no new imports, no type changes.

## Modified Region: Contradiction Scan Block + Graph Inference Tick

### Before (lines ~659–769 of background.rs)

```
// GH #278 fix: Contradiction scan — runs every CONTRADICTION_SCAN_INTERVAL_TICKS ticks
if current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS) {
    if let Ok(adapter) = embed_service.get_adapter().await {
        ... [scan body — unchanged] ...
    }
}

// 2. Extraction tick (with timeout, #236)
match tokio::time::timeout(TICK_TIMEOUT, extraction_tick(...)).await {
    ... [extraction body — unchanged] ...
}

// crt-029: Background graph inference (recurring, cap-throttled via max_graph_inference_per_tick).
if inference_config.nli_enabled {
    run_graph_inference_tick(store, nli_handle, vector_index, ml_inference_pool, inference_config).await;
}
```

### After (pseudocode)

```
// Tick ordering invariant (non-negotiable):
// compaction → promotion → graph-rebuild
//   → contradiction_scan (if embed adapter ready, every CONTRADICTION_SCAN_INTERVAL_TICKS)
//   → extraction_tick → structural_graph_tick (always)
//
// Do not reorder these steps. The contradiction scan runs BEFORE graph inference so that
// the contradiction_cache reflects the current entry set before Informs edges accumulate.

// --- Contradiction scan (independent tick step) ---
// Gated on: embed adapter availability AND tick-interval (CONTRADICTION_SCAN_INTERVAL_TICKS).
// Runs independently of the structural graph tick below.
// O(N) ONNX inference — interval gate prevents per-tick CPU spike.
// BEHAVIORAL CHANGE: none. Comment and label additions only (NFR-07 zero-diff constraint).
if current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS) {
    if let Ok(adapter) = embed_service.get_adapter().await {
        ... [existing scan body — ZERO behavioral change: no condition, bracket, or assignment changes] ...
    }
}

// 2. Extraction tick (with timeout, #236)
match tokio::time::timeout(TICK_TIMEOUT, extraction_tick(...)).await {
    ... [existing extraction body — unchanged] ...
}

// --- Structural graph tick (always) ---
// Phase 4b (structural Informs HNSW scan) runs unconditionally.
// Phase 8 (NLI Supports) is internally gated by get_provider() inside run_graph_inference_tick.
// The outer `if inference_config.nli_enabled` gate is removed (crt-039 FR-01, ADR-001).
run_graph_inference_tick(
    store,
    nli_handle,
    vector_index,
    ml_inference_pool,
    inference_config,
)
.await;
```

## Error Handling

No error handling changes in this file. `run_graph_inference_tick` is infallible (returns `()`).
The contradiction scan and extraction tick retain their existing match arms unchanged.

## Key Constraint: Zero Behavioral Change to Contradiction Scan (R-09)

The only permitted changes to the contradiction scan block are:
- Adding a comment header naming it as an independent tick step.
- Adding an inline comment about its condition semantics.

Prohibited:
- Changing the condition (`is_multiple_of(...)`, `get_adapter().await`, `&&` operand order).
- Moving the block relative to other tick steps.
- Adding or removing any inner branch or assignment.

Verification: `git diff` on the contradiction scan block shows ONLY line additions (new
comment lines), zero deletions, zero condition mutations.

## Key Test Scenarios

These tests must pass without modification (AC-06, AC-07):
- All existing contradiction scan tests (behavioral identity confirmed by green CI).
- All existing tick-level integration tests that assert graph edge writes (validates ordering).

New tests covered by other components (TC-01, TC-02) exercise the removed `nli_enabled` gate
indirectly: if the gate were still present, TC-01 would find zero Informs edges.

No new tests required for background.rs itself — the only testable behavior change is the
unconditional call to `run_graph_inference_tick`, which is covered by TC-01 and TC-02 in
nli_detection_tick.rs tests.
