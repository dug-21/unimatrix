# Agent Report: 264-agent-1-fix

**Bug**: GH #264 — Supersession graph rebuild on every search exhausts tokio blocking thread pool
**Fix branch**: `worktree-bugfix/crt-014-graph-cache`
**Commit**: `9ff6810`

---

## Summary

Implemented the approved fix for GH #264: cached supersession entry snapshot using the `EffectivenessStateHandle` pattern from crt-018b, eliminating 4x `Store::query_by_status()` calls from the search hot path.

---

## Files Modified

- `crates/unimatrix-server/src/services/search.rs` — added `supersession_state: SupersessionStateHandle` field + updated `new()` + replaced lines 268-292 (store I/O block) with cached read
- `crates/unimatrix-server/src/services/mod.rs` — added `supersession` module, re-exported types, wired `SupersessionState::new_handle()` into `ServiceLayer::with_rate_config()`, added `supersession_state_handle()` method
- `crates/unimatrix-server/src/background.rs` — added `SupersessionState` import, `supersession_state` param to `spawn_background_tick` / `background_tick_loop` / `run_single_tick`, added rebuild step after maintenance tick
- `crates/unimatrix-server/src/main.rs` — extracted `supersession_state_handle` and passed to `spawn_background_tick`
- `crates/unimatrix-server/src/services/briefing.rs` — fixed test helper `make_briefing_service_with_effectiveness` to pass `SupersessionState::new_handle()` to `SearchService::new()`
- `product/test/infra-001/suites/test_lifecycle.py` — added `test_concurrent_search_stability` integration test (`@pytest.mark.smoke`)

## Files Created

- `crates/unimatrix-server/src/services/supersession.rs` — `SupersessionState`, `SupersessionStateHandle`, `rebuild()`, and 7 unit tests

---

## New Tests

**Unit tests in `services/supersession.rs`** (7 tests):
- `test_supersession_state_new_cold_start`
- `test_supersession_state_default_matches_new`
- `test_new_handle_readable_after_creation`
- `test_new_handle_write_then_read`
- `test_new_handle_returns_independent_handles`
- `test_poison_recovery_read_after_write_panic`
- `test_arc_clone_shares_state`

**Unit tests in `services/search.rs`** (2 tests):
- `test_search_uses_cached_supersession_state_cold_start_fallback` — verifies cold-start handle is empty and `use_fallback=true`
- `test_search_uses_cached_supersession_state_after_rebuild` — verifies search path can read a pre-populated handle and build graph without store I/O

**Integration test in `product/test/infra-001/suites/test_lifecycle.py`** (1 test, `@pytest.mark.smoke`):
- `test_concurrent_search_stability` — fires 8 search calls from parallel threads, asserts all complete within 10 seconds and all return tool-level success

---

## Verification

- `cargo build --workspace`: **PASS** (zero errors, warnings are all pre-existing)
- `cargo test --workspace`: **2516 passed, 0 failed**
- `cargo clippy --workspace -- -D warnings`: **zero new errors/warnings** in `unimatrix-server` (54 pre-existing errors in `unimatrix-observe`, unchanged)
- No `.unwrap()` / `todo!()` / `unimplemented!()` in non-test code

---

## Design Decisions

**Option A chosen** (per implementation brief): Only `all_entries` and `use_fallback` are stored in the handle. `build_supersession_graph` is called by the search path from the cloned snapshot (pure CPU, no I/O). This keeps `SupersessionGraph` (non-`Clone`) out of the handle entirely.

**Cold-start behavior**: `use_fallback: true`, `all_entries: empty`. On cold-start `build_supersession_graph(&[])` returns `Ok(empty_graph)`. `cached_use_fallback` is `true`, so `FALLBACK_PENALTY` is applied for all superseded/deprecated entries until the first background tick. This is conservative and correct — same as pre-crt-014 behavior.

**Lock ordering (R-01 compliance)**: The read lock on `supersession_state` is acquired and fully released (guard drops at end of block) before any other operation. No simultaneous locks are held.

---

## Knowledge Stewardship

- **Queried**: `/uni-query-patterns` for `unimatrix-server` — skipped per spawn prompt instruction (server unstable, do not attempt MCP calls)
- **Stored**: Skipped per spawn prompt instruction
- **Patterns discovered**:
  - `briefing.rs` contains a test helper `make_briefing_service_with_effectiveness` that directly calls `SearchService::new()` — any change to `SearchService::new()` signature must also update this test helper (non-obvious; file is in a different module from where you'd expect)
  - The `EffectivenessStateHandle` pattern re-export chain: `services/X.rs` → `pub` type → `mod.rs` pub use → binary crate access. This chain must be `pub` (not `pub(crate)`) because `spawn_background_tick` is `pub` and its parameters must match.
