# Gate Bugfix Report: bugfix-505

> Gate: Bugfix Validation
> Date: 2026-04-06
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed (not just symptoms) | PASS | `set_ready_for_test` injects a Ready-state handle; the Ok(adapter) branch is now reachable in unit tests |
| No todo!/unimplemented!/TODO/FIXME placeholders | PASS | None in diff |
| All tests pass | PASS | 2776 unit tests passed, 0 failed |
| No new clippy warnings in diff | PASS | Clippy errors present in workspace are pre-existing (auth.rs:113, event_queue.rs — not in changed files) |
| No unsafe code introduced | PASS | No `unsafe` in any changed file |
| Fix is minimal (no unrelated changes) | PASS | 493 lines added, all test infrastructure and new tests; no production logic modified |
| New tests would have caught the original bug | PASS | `test_goal_embedding_written_after_cycle_start` directly exercises the previously unreachable Ok(adapter) branch |
| Integration smoke tests passed | PASS | 23/23 smoke tests pass; `test_cycle_start_goal_does_not_block_response` is new (G-03) |
| xfail markers have corresponding GH Issues | PASS | No xfail markers added |
| Knowledge Stewardship — investigator | PASS | Entry #4174 stored: "Service handle fire-and-forget spawn paths require stub provider to be unit-testable" (lesson-learned) |
| Knowledge Stewardship — rust-dev | PASS | Entry #4175 stored: "unimatrix_embed::test_helpers is NOT available in other crates' #[cfg(test)]" (pattern) |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**: The diagnosed root cause was that `EmbedServiceHandle` initializes in `Loading` state, so `get_adapter()` returns `Err(EmbedNotReady)` in unit tests without ONNX. The `Ok(adapter)` branch inside the fire-and-forget spawn was permanently unreachable.

The fix adds `set_ready_for_test()` to `EmbedServiceHandle` (`embed_handle.rs` lines 224-228, `#[cfg(test)]`) and a `pub(crate) EmbedErrorProvider` stub (lines 256-278). This gives the test module in `listener.rs` a seam to construct Ready-state handles backed by `MockEmbedProvider` or `EmbedErrorStub` without requiring ONNX.

The `make_ready_embed_service()` and `make_error_embed_service()` test helpers in `listener.rs` (lines 7677-7692) call `set_ready_for_test()`, making the Ok(adapter) and embed-error branches both reachable.

This mirrors the existing `NliServiceHandle::set_ready_for_test` pattern — correct approach.

### No Stubs/Placeholders

**Status**: PASS

**Evidence**: No `todo!()`, `unimplemented!()`, `TODO`, or `FIXME` strings appear in the diff. All test functions are fully implemented with real assertions.

### All Tests Pass

**Status**: PASS

**Evidence**: `cargo test --workspace` reports 2776 passed, 0 failed across all crates. The five new unit tests (`test_goal_embedding_written_after_cycle_start`, `test_no_embed_task_on_empty_goal`, `test_no_embed_task_on_absent_goal`, `test_goal_embedding_unavailable_service_warn`, `test_goal_embedding_error_during_embed`) and two `embed_handle` tests (`test_set_ready_for_test_transitions_to_ready`, `test_embed_error_provider_returns_error`) all pass.

### No New Clippy Warnings in Diff

**Status**: PASS

**Evidence**: `cargo clippy --workspace -- -D warnings` produces errors in `unimatrix-engine/src/auth.rs:113` (pre-existing `collapsible_if`, confirmed in spawn prompt) and `unimatrix-engine/src/event_queue.rs` (also pre-existing). Neither `embed_handle.rs` nor `listener.rs` appear in the clippy output. The diff introduces no new warnings.

### No Unsafe Code

**Status**: PASS

**Evidence**: No `unsafe` blocks in any of the three changed files.

### Fix is Minimal

**Status**: PASS

**Evidence**: The commit (`734b0d98`) adds 493 lines, all within `#[cfg(test)]` gates or the Python test suite. Zero changes to production code paths. No unrelated refactors or feature additions.

### New Tests Would Have Caught the Original Bug

**Status**: PASS

**Evidence**: `test_goal_embedding_written_after_cycle_start` uses `make_ready_embed_service()` (backed by `MockEmbedProvider`) and asserts `goal_embedding IS NOT NULL` after cycle start. Without the `set_ready_for_test` seam, the handle would remain `Loading`, the embed spawn would hit `Err(EmbedNotReady)`, and the assertion would fail — which is precisely the original symptom. G-03 (`test_cycle_start_goal_does_not_block_response`) verifies the timing contract that was never written.

### Integration Smoke Tests

**Status**: PASS

**Evidence**: Tester reports 23/23 smoke tests pass. The new `@pytest.mark.smoke` test `test_cycle_start_goal_does_not_block_response` (elapsed < 1.0s) is included in that count.

**Minor observation (WARN)**: Line 938 of `test_lifecycle.py` contains the assertion `assert "error" not in str(result).lower() or result is not None`. The `or result is not None` clause makes the disjunction trivially true whenever `result` is not None (which it always will be on a successful call). The timing assertion on line 937 carries the real validation weight; line 938 is effectively dead. Not a blocker — the smoke test's primary purpose (non-blocking response under 1.0s) is validated by line 937 — but the second assert could be simplified to `assert result is not None` for clarity.

### xfail Markers

**Status**: PASS

**Evidence**: No `pytest.mark.xfail` markers added anywhere in the diff.

### Knowledge Stewardship — Investigator

**Status**: PASS

**Evidence**: Entry #4174 confirmed active: "Service handle fire-and-forget spawn paths require stub provider to be unit-testable" (category: lesson-learned, tags: caused_by_feature:crt-043, embed-handle, fire-and-forget, gate-3c, stub, test-seam, testing, unimatrix-server).

### Knowledge Stewardship — Rust-Dev

**Status**: PASS

**Evidence**: Entry #4175 confirmed active: "unimatrix_embed::test_helpers is NOT available in other crates' #[cfg(test)] — define inline mocks instead" (category: pattern, tags: cfg-test, cross-crate, crt-043, embed, gotcha, mock, testing, unimatrix-embed).

## Knowledge Stewardship

- Queried: entries #4174 and #4175 via context_get to verify investigator and rust-dev stewardship compliance
- nothing novel to store -- gate result is feature-specific; no recurring cross-feature failure pattern identified in this fix
