# Component: background-call-site

## Purpose

Add a call to `run_graph_inference_tick` in `run_single_tick` within
`crates/unimatrix-server/src/background.rs`. This is a two-line addition (one comment line
plus the conditional call block). No new parameters are added to any function signature.

---

## File Modified

`crates/unimatrix-server/src/background.rs`

---

## Import Required

Add to the existing imports at the top of `background.rs`:

```rust
use crate::services::nli_detection_tick::run_graph_inference_tick;
```

This mirrors the existing import:
```rust
use crate::services::nli_detection::maybe_run_bootstrap_promotion;
```

---

## Insertion Point

Find the existing bootstrap promotion call site in `run_single_tick`:

```rust
// crt-023: Bootstrap NLI promotion (one-shot, idempotent via COUNTERS marker).
// Called on every tick; fast no-op if marker already set (O(1) DB read).
// When NLI is not ready, defers silently without setting marker (FR-25).
if inference_config.nli_enabled {
    maybe_run_bootstrap_promotion(store, nli_handle, ml_inference_pool, inference_config).await;
}
```

Insert the new call **immediately after** this block (after the closing `}` of the
`maybe_run_bootstrap_promotion` if-block):

```rust
// crt-029: Background graph inference (recurring, cap-throttled via max_graph_inference_per_tick).
// Runs after bootstrap promotion so bootstrap-promoted edges are visible to the tick's
// pre-filter HashSet. Must remain after maybe_run_bootstrap_promotion (sequencing invariant).
if inference_config.nli_enabled {
    run_graph_inference_tick(store, nli_handle, vector_index, ml_inference_pool, inference_config).await;
}
```

---

## Complete Insertion (before and after)

### Before

```rust
    // crt-023: Bootstrap NLI promotion (one-shot, idempotent via COUNTERS marker).
    // Called on every tick; fast no-op if marker already set (O(1) DB read).
    // When NLI is not ready, defers silently without setting marker (FR-25).
    if inference_config.nli_enabled {
        maybe_run_bootstrap_promotion(store, nli_handle, ml_inference_pool, inference_config).await;
    }

    // Update next scheduled time
    if let Ok(mut meta) = tick_metadata.lock() {
```

### After

```rust
    // crt-023: Bootstrap NLI promotion (one-shot, idempotent via COUNTERS marker).
    // Called on every tick; fast no-op if marker already set (O(1) DB read).
    // When NLI is not ready, defers silently without setting marker (FR-25).
    if inference_config.nli_enabled {
        maybe_run_bootstrap_promotion(store, nli_handle, ml_inference_pool, inference_config).await;
    }

    // crt-029: Background graph inference (recurring, cap-throttled via max_graph_inference_per_tick).
    // Runs after bootstrap promotion so bootstrap-promoted edges are visible to the tick's
    // pre-filter HashSet. Must remain after maybe_run_bootstrap_promotion (sequencing invariant).
    if inference_config.nli_enabled {
        run_graph_inference_tick(store, nli_handle, vector_index, ml_inference_pool, inference_config).await;
    }

    // Update next scheduled time
    if let Ok(mut meta) = tick_metadata.lock() {
```

---

## Parameter Mapping

The `run_single_tick` signature already has all required parameters. No new parameters added
to `run_single_tick`, `background_tick_loop`, or `spawn_background_tick` (confirmed in
ARCHITECTURE.md §Component 4).

| `run_graph_inference_tick` param | Sourced from `run_single_tick` |
|----------------------------------|-------------------------------|
| `store` | `store: &Arc<Store>` (pass as `store` — deref if needed; match `maybe_run_bootstrap_promotion` call pattern) |
| `nli_handle` | `nli_handle: &Arc<NliServiceHandle>` (deref to `&NliServiceHandle`) |
| `vector_index` | `vector_index: &Arc<VectorIndex>` (already present, already passed to `run_single_tick`) |
| `rayon_pool` | `ml_inference_pool: &Arc<RayonPool>` (deref to `&RayonPool`) |
| `config` | `inference_config: &Arc<InferenceConfig>` (deref to `&InferenceConfig`) |

IMPLEMENTATION NOTE: The exact borrow/deref patterns should follow the existing
`maybe_run_bootstrap_promotion` call site exactly:

```rust
// Existing call (crt-023):
maybe_run_bootstrap_promotion(store, nli_handle, ml_inference_pool, inference_config).await;
// Note: parameters are Arc<T> dereferred to &T at the call site OR passed as &*arc
// Verify this against the run_single_tick signature and actual call site in background.rs
```

The new call uses the same pattern with `vector_index` added:

```rust
run_graph_inference_tick(store, nli_handle, vector_index, ml_inference_pool, inference_config).await;
```

---

## Sequencing Invariant

The tick call MUST come AFTER the `maybe_run_bootstrap_promotion` call, not before it and
not interleaved. Rationale: bootstrap promotion writes edges (bootstrap → non-bootstrap
promotion) that the tick's pre-filter `HashSet` (built in Phase 2) must already see.

If ordering is reversed, the tick's Phase 2 reads the DB before bootstrap promotion writes
new edges. Those newly-promoted edges are not in `existing_supports_pairs`, so the tick
would attempt to re-score them — wasting NLI budget. `INSERT OR IGNORE` prevents duplicates,
but the NLI call is wasted.

There is no compile-time enforcement of this ordering. It is verified by code review.

---

## Error Handling

`run_graph_inference_tick` is infallible — it returns `()`. All internal errors are logged
at `warn` or `debug` and swallowed. The `if inference_config.nli_enabled` gate means the
function is not invoked when NLI is disabled.

No timeout wrapper is required beyond the outer `TICK_TIMEOUT = 120s` on `run_single_tick`.
At default 100 pairs × ~0.5ms/pair = ~50ms NLI time, the tick completes well within budget.

---

## Key Test Scenarios

### AC-14: Tick not invoked when `nli_enabled = false`
```
config = InferenceConfig { nli_enabled: false, ..InferenceConfig::default() }
run_single_tick(..., &config, ...)  // should not call run_graph_inference_tick
// assert: no edges written, no NLI calls
```

### AC-14: Tick invoked when `nli_enabled = true` (integration test)
```
config = InferenceConfig { nli_enabled: true, ..InferenceConfig::default() }
// Seed two active entries with embeddings
// Run background tick loop (or run_single_tick directly)
// assert: run_graph_inference_tick was invoked (side effect: potentially edges written)
```

### Ordering: bootstrap promotion runs before tick
```
// Integration test:
// Seed bootstrap Contradicts edges (bootstrap_only = 1)
// After maybe_run_bootstrap_promotion, those rows change (bootstrap → non-bootstrap)
// Run tick immediately after
// assert: the tick's pre-filter sees the promoted edges (they are in existing_supports_pairs)
// (or if promotion wrote Supports, they are filtered; if it wrote Contradicts only, tick sees clean slate)
```

### Pre-merge code review checks
```bash
# Verify import is present
grep -n 'use crate::services::nli_detection_tick' crates/unimatrix-server/src/background.rs

# Verify ordering: tick call appears after bootstrap promotion call
grep -n 'maybe_run_bootstrap_promotion\|run_graph_inference_tick' crates/unimatrix-server/src/background.rs
# Expected: maybe_run_bootstrap_promotion line number < run_graph_inference_tick line number
```
