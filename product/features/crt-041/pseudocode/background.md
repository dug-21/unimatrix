# Pseudocode: background

## Purpose

Modify `crates/unimatrix-server/src/background.rs` to wire `run_graph_enrichment_tick`
into `run_single_tick` after `run_graph_inference_tick`. Also register the new module
in `services/mod.rs`.

Two files are modified:
- `crates/unimatrix-server/src/services/mod.rs` — add `pub(crate) mod graph_enrichment_tick;`
- `crates/unimatrix-server/src/background.rs` — add import and call site

## Modification 1: `services/mod.rs`

Add the new module registration following the existing pattern (currently ~line 26):

```
// BEFORE (existing):
pub(crate) mod co_access_promotion_tick;

// AFTER (add alongside existing):
pub(crate) mod co_access_promotion_tick;
pub(crate) mod graph_enrichment_tick;   // crt-041
```

No `pub use` needed — `run_graph_enrichment_tick` is only called from `background.rs`.

## Modification 2: `background.rs` — import

The existing import for `run_co_access_promotion_tick` is at line ~43:
```
use crate::services::co_access_promotion_tick::run_co_access_promotion_tick;
```

Add alongside it:
```
use crate::services::graph_enrichment_tick::run_graph_enrichment_tick;   // crt-041
```

## Modification 3: `background.rs` — call site in `run_single_tick`

The current tick sequence ends with `run_graph_inference_tick` (currently ~lines 769-780):

```rust
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

// Update next scheduled time
if let Ok(mut meta) = tick_metadata.lock() {
    meta.next_scheduled = Some(now_secs() + tick_interval_secs);
}
```

Insert `run_graph_enrichment_tick` call AFTER `run_graph_inference_tick` and BEFORE the
"Update next scheduled time" block:

```rust
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

// --- Graph enrichment tick (crt-041) ---
// S1 (tag co-occurrence) and S2 (vocabulary) run every tick.
// S8 (search co-retrieval) runs every `s8_batch_interval_ticks` ticks.
// Must run AFTER run_graph_inference_tick. New edges are visible to PPR
// at the NEXT tick's TypedGraphState::rebuild (one-tick delay).
run_graph_enrichment_tick(store, inference_config, current_tick as u64).await;

// Update next scheduled time
if let Ok(mut meta) = tick_metadata.lock() {
    meta.next_scheduled = Some(now_secs() + tick_interval_secs);
}
```

## Modification 4: Update tick-ordering invariant comment

The existing tick-ordering comment at ~line 661:

```
// Tick ordering invariant (non-negotiable):
// compaction → promotion → graph-rebuild
//   → contradiction_scan (if embed adapter ready, every CONTRADICTION_SCAN_INTERVAL_TICKS)
//   → extraction_tick → structural_graph_tick (always)
//
// Do not reorder these steps. The contradiction scan runs BEFORE graph inference so that
// the contradiction_cache reflects the current entry set before Informs edges accumulate.
```

Update to include `run_graph_enrichment_tick`:

```
// Tick ordering invariant (non-negotiable):
// compaction → promotion → graph-rebuild
//   → contradiction_scan (if embed adapter ready, every CONTRADICTION_SCAN_INTERVAL_TICKS)
//   → extraction_tick → structural_graph_tick / run_graph_inference_tick (always)
//   → run_graph_enrichment_tick (S1/S2 always, S8 every s8_batch_interval_ticks) [crt-041]
//
// Do not reorder these steps. The contradiction scan runs BEFORE graph inference so that
// the contradiction_cache reflects the current entry set before Informs edges accumulate.
// run_graph_enrichment_tick must run AFTER run_graph_inference_tick: new edges from this
// tick are not visible to PPR until the NEXT tick's TypedGraphState::rebuild.
```

## `current_tick` Type Note

The `run_single_tick` function signature must expose `current_tick` to pass to
`run_graph_enrichment_tick`. Verify whether `current_tick` is already available as a `u32`
or `u64` variable at the call site.

From reading background.rs, `current_tick` appears as a `u32` (used in
`is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS)` at ~line 675 and
`run_co_access_promotion_tick(store, inference_config, current_tick)` at ~line 550
where `run_co_access_promotion_tick` takes `current_tick: u32`).

`run_graph_enrichment_tick` takes `current_tick: u64` for the modulo with
`s8_batch_interval_ticks: u32` (u32 % u32 is fine; u64 % u64 also fine).

Call site: pass `current_tick as u64` if local variable is u32. This is safe; u32 fits
in u64 with no loss.

## Function Signature (unchanged)

No changes to `run_single_tick`'s own signature. This is an internal call addition only.

## Error Handling

`run_graph_enrichment_tick` is infallible (returns `()`). No error propagation from the
new call site. If S1, S2, or S8 encounter errors they log at `warn!` internally.

## Key Test Scenarios

### T-BG-01: tick ordering — enrichment runs after inference
Integration test that instruments the tick and asserts `run_graph_enrichment_tick` is
called after `run_graph_inference_tick` completes within the same tick.
(Implementation-level test; may require extracting the order-verification mock from the
existing test infrastructure.)

### T-BG-02: S8 gate — skipped on non-multiple ticks
With `s8_batch_interval_ticks = 5`, run ticks 1, 2, 3, 4. Assert zero S8 edges written.
Run tick 5. Assert S8 edges ARE written (if audit_log has qualifying rows).

### T-BG-03: S8 gate — fires on tick 0
With `s8_batch_interval_ticks = 10`, `current_tick = 0`.
`0 % 10 == 0` → S8 runs. Assert S8 is invoked on the first tick.

### T-BG-04: compile-time verification
`cargo build --workspace` must succeed with the new import and call site.
No clippy warnings from the new lines.

## Notes for Delivery Agent

1. `current_tick` in `run_single_tick` is the variable passed to existing tick functions.
   Confirm its type by checking the signature of `run_co_access_promotion_tick` call site.
   Pass `current_tick as u64` if u32.

2. The `inference_config` at the call site is `&Arc<InferenceConfig>` or `&InferenceConfig`
   — check how it is dereferenced for the existing `run_graph_inference_tick` call. Use
   the same dereference pattern for `run_graph_enrichment_tick`.

3. Do not add a `tokio::time::timeout` wrapper around `run_graph_enrichment_tick`. The
   existing `TICK_TIMEOUT` wrapper is only applied to ML inference tasks (`extraction_tick`,
   `TypedGraphState::rebuild`, `PhaseFreqTable::rebuild`). S1/S2/S8 are pure SQL and
   expected to complete in <500ms (NFR-03). If a future ticket adds a timeout, that is
   a separate concern.
