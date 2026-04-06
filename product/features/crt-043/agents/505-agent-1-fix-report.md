# Agent Report: 505-agent-1-fix

**Feature:** crt-043  
**Issue:** GH #505 — server-level test seam for EmbedServiceHandle (G-02/G-03 gaps)  
**Branch:** bugfix/505-embed-handle-test-seam

## Work Completed

### G-02: embed_handle.rs

Added `set_ready_for_test(&self, adapter: Arc<EmbedAdapter>)` as a `#[cfg(test)] pub` async mutator that writes `EmbedState::Ready(adapter)` directly — mirrors `NliServiceHandle::set_ready_for_test` at nli_handle.rs:421.

Added `EmbedErrorProvider` stub (`pub(crate)`, `#[cfg(test)]`) implementing `EmbeddingProvider` with an `InferenceFailed` error on every `embed()` call. Used to test the error branch in `handle_cycle_event`'s goal embedding spawn.

Added 2 new tests to the existing `embed_handle::tests` module:
- `test_set_ready_for_test_transitions_to_ready`
- `test_embed_error_provider_returns_error`

### G-02: listener.rs — 5 unit tests

Added two helper builders to the existing `#[cfg(test)]` module:
- `make_ready_embed_service()` — Ready state backed by inline `MockEmbedProvider`
- `make_error_embed_service()` — Ready state backed by inline `EmbedErrorStub`

Both use inline `EmbeddingProvider` impls (not `unimatrix_embed::test_helpers::MockProvider` — see gotcha below).

Added 5 new unit tests:
1. `test_goal_embedding_written_after_cycle_start` — pre-inserts cycle_events row, dispatches cycle_start with goal, yields, asserts `goal_embedding IS NOT NULL`
2. `test_no_embed_task_on_empty_goal` — empty goal string → no spawn → `goal_embedding IS NULL`
3. `test_no_embed_task_on_absent_goal` — absent goal key → no spawn → `goal_embedding IS NULL`
4. `test_goal_embedding_unavailable_service_warn` — Loading handle → spawn warns → `goal_embedding IS NULL`, response is `Ack`
5. `test_goal_embedding_error_during_embed` — EmbedErrorStub → spawn warns → `goal_embedding IS NULL`, response is `Ack`

**Key design note:** Each DB-assertion test pre-inserts the cycle_events row directly via `store.insert_cycle_event()`. The INSERT in `handle_cycle_event` (Step 5) is itself fire-and-forget; pre-inserting eliminates a double race between the INSERT and embed UPDATE spawns, making the assertions deterministic.

### G-03: test_lifecycle.py

Added one `@pytest.mark.smoke` test after the concurrent search stability block:
- `test_cycle_start_goal_does_not_block_response` — wall-clock guard (< 1.0s) verifying the fire-and-forget spawn does not block the response.

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/infra/embed_handle.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/uds/listener.rs`
- `/workspaces/unimatrix/product/test/infra-001/suites/test_lifecycle.py`

## Test Results

```
test result: ok. 2776 passed; 0 failed  (unimatrix-server lib)
test result: ok. 46 passed; 0 failed    (migration integration)
test result: ok. 16 passed; 0 failed    (infra-001 import)
test result: ok. 16 passed; 0 failed    (infra-001 export)
test result: ok. 7 passed; 0 failed     (pipeline e2e)
```

Net new tests: +7 unit (2 in embed_handle, 5 in listener) + 1 integration smoke = **8 new tests**. No regressions.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entry #4174 (lesson confirming the exact gap being fixed) and entry #4068 (ADR-002 for goal embedding race resolution). Both directly relevant.
- Stored: entry #4175 "unimatrix_embed::test_helpers is NOT available in other crates' #[cfg(test)] — define inline mocks instead" via `/uni-store-pattern`

**Gotcha discovered:** `unimatrix_embed::test_helpers` is gated `#[cfg(any(test, feature = "test-support"))]` in unimatrix-embed/src/lib.rs and is NOT accessible in `#[cfg(test)]` blocks of other crates without explicitly enabling the `test-support` feature in Cargo.toml. Attempting to use it produces E0433 "configured out". Additionally, `unimatrix_embed::error` is a private module — callers must use the public re-exports `unimatrix_embed::EmbedError` and `unimatrix_embed::Result`. Stored as pattern #4175.
