# Agent Report: crt-019-agent-4-confidence-state

## Status: COMPLETE

## Files Modified

- `crates/unimatrix-server/src/services/confidence.rs` — MODIFIED (was ~57 lines, now ~240 lines)
- `crates/unimatrix-server/src/services/mod.rs` — wired handle, added `confidence_state_handle()` accessor, re-exported types
- `crates/unimatrix-server/src/services/search.rs` — added `confidence_state: ConfidenceStateHandle` field + updated `new()` signature
- `crates/unimatrix-server/src/services/status.rs` — added `confidence_state: ConfidenceStateHandle` field + updated `new()` signature
- `crates/unimatrix-server/src/background.rs` — threaded `ConfidenceStateHandle` through `spawn_background_tick` → `background_tick_loop` → `run_single_tick` → `StatusService::new`
- `crates/unimatrix-server/src/main.rs` — extract handle via `services.confidence_state_handle()`, pass to `spawn_background_tick`
- `crates/unimatrix-server/src/services/briefing.rs` — updated test fixture `SearchService::new` call to include default `ConfidenceState` handle

## Tests

- **8 passed, 0 failed** (`cargo test -p unimatrix-server services::confidence`)
- Tests cover: initial observed_spread=0.1471 (R-06), initial weight=0.18375 > floor 0.15, cold-start priors alpha0=beta0=3.0, four-field atomic update, write-read roundtrip, concurrent reads, weight_not_zero, clone independence

## Implementation Notes

### ConfidenceState visibility
The `ConfidenceState` struct and `ConfidenceStateHandle` type alias are `pub` (not `pub(crate)`) because `main.rs` lives in the binary crate separate from the lib crate. `pub(crate)` would prevent `main.rs` from using the type in `spawn_background_tick`'s signature. A `pub fn confidence_state_handle()` method on `ServiceLayer` exposes the handle cleanly.

### recompute() engine call site dependency
`ConfidenceService::recompute()` captures `alpha0`/`beta0` from `ConfidenceState` before `spawn_blocking` as the pseudocode requires. However, the actual `compute_confidence` call uses the current 2-arg engine signature (pre-crt-019). The `_ = (alpha0, beta0)` line documents the captured values are ready; the call site upgrades to 4-arg once the confidence-formula-engine agent lands the engine signature change.

### Blast radius
Adding `ConfidenceStateHandle` to `StatusService::new` required threading it through `background.rs` (3 function signatures: `spawn_background_tick`, `background_tick_loop`, `run_single_tick`) and `main.rs`. This was necessary to preserve the architectural invariant that the background tick and search path share the same `Arc<RwLock<ConfidenceState>>`.

### Pre-existing flaky test
`unimatrix-vector::index::tests::test_compact_search_consistency` fails intermittently (non-deterministic HNSW compaction). No changes were made to `unimatrix-vector`. Confirmed pre-existing by running the test on a clean stash.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — not executed (Unimatrix server unavailable in this worktree context; proceeded without)
- Stored: entry noted for future storage — "pub(crate) on server structs/types doesn't work for `main.rs` calls since the binary is a separate crate from the lib. Types shared between lib and bin must be `pub`. Use `ServiceLayer` accessor methods as the clean boundary."

Nothing novel beyond this visibility pattern was discovered.
