# Agent Report: col-025-rework-listener-tests

**Agent ID**: col-025-rework-listener-tests
**Task**: Add two missing Gate 3c unit tests to listener.rs

---

## Work Completed

Added two tests to `crates/unimatrix-server/src/uds/listener.rs` in the existing `#[cfg(test)] mod tests` block:

### Test 1: `test_subagent_start_goal_wins_over_nonempty_prompt_snippet` (AC-12 / Gate 3c scenario 3)

Location: after `test_subagent_start_goal_present_routes_to_index_briefing`, before the existing `test_subagent_start_non_subagent_source_skips_goal_branch`.

Setup:
- Register session `"sai-02-wins"` with `current_goal = Some("my goal")`.
- Dispatch `HookRequest::ContextSearch` with `source = "SubagentStart"` and a non-empty `query` (simulating a non-empty `prompt_snippet`).

Assert:
- `logs_contain("col-025: SubagentStart goal-present branch")` — confirms goal branch entered.
- `logs_contain("my goal")` — confirms goal text used, not prompt_snippet content.

### Test 2: `test_resume_db_error_degrades_to_none_with_warn` (R-03 / AC-15 / Gate 3c scenario 7)

Setup:
- Create a store, then call `store.write_pool_server().close().await` to close the pool before dispatch.
- Dispatch `HookRequest::SessionRegister` with `feature = Some("col-025-db-error")`.

Assert:
- `matches!(resp, HookResponse::Ack)` — registration succeeds despite DB error.
- `state.current_goal == None` — degraded to None.
- `logs_contain("col-025: goal resume lookup failed")` — warn emitted.

Both tests use `#[tracing_test::traced_test]` for log capture.

---

## Test Results

```
running 2 tests
test uds::listener::tests::test_resume_db_error_degrades_to_none_with_warn ... ok
test uds::listener::tests::test_subagent_start_goal_wins_over_nonempty_prompt_snippet ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 1970 filtered out; finished in 0.08s
```

Both Gate 3c required tests pass.

---

## Files Modified

- `crates/unimatrix-server/src/uds/listener.rs` — 122 lines added (two new tests + corrected doc comment on existing `test_subagent_start_non_subagent_source_skips_goal_branch`)

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for unimatrix-server -- skipped (task is purely additive test code; patterns already visible in surrounding tests in same file)
- Stored: nothing novel to store — the `write_pool_server().close().await` technique to force pool errors in tests is a sound approach but the pool-closure pattern for test error injection is not specific enough to this crate to warrant a stored pattern entry (it is a standard sqlx idiom).
