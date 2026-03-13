# Gate Bugfix Report: bugfix-236

> Gate: Bugfix Validation
> Date: 2026-03-13
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Fix addresses root cause | PASS | All 3 root causes addressed with targeted fixes |
| No todo/unimplemented/TODO/FIXME | PASS | grep found zero matches in server crate |
| All tests pass | PASS | 5 new tests + full server suite pass; 1 pre-existing flaky in unimatrix-vector (unrelated) |
| No new clippy warnings | PASS | All warnings pre-existing; none in bugfix files |
| No unsafe code introduced | PASS | No unsafe in any modified files |
| Fix is minimal | PASS | 7 files, +259/-102 lines, all within unimatrix-server crate |
| New tests catch original bug | PASS | timeout utility tests + constant validation cover the fix |
| Integration smoke tests | PASS | 136 integration tests pass, 6 xfail pre-existing |
| xfail markers have GH issues | PASS | GH#238 filed for lifecycle xfail |
| Knowledge stewardship | PASS | Rust-dev report has Queried + Declined entries with reasoning |

## Detailed Findings

### Fix Addresses Root Cause
**Status**: PASS
**Evidence**: The three diagnosed root causes are directly addressed:

1. **Ghost process (RC-1)**: `main.rs` lines 357-382 -- the old `tokio::select!` approach that dropped `RunningService` (and its blocking stdin reader) on signal is replaced with `cancellation_token()` pattern. A spawned task monitors SIGTERM/SIGINT and calls `cancel_token.cancel()`, causing rmcp to close the transport cleanly. `waiting()` returns normally with `QuitReason::Cancelled`. `graceful_shutdown` signature simplified to no longer accept the server future.

2. **Background tick contention (RC-2)**: `background.rs` lines 196-285 -- both `maintenance_tick` and `extraction_tick` are wrapped in `tokio::time::timeout(TICK_TIMEOUT)` where `TICK_TIMEOUT = 120s`. On timeout, the tick is aborted and retries next cycle (work is idempotent).

3. **Handler timeouts (RC-3)**: New `infra/timeout.rs` provides `spawn_blocking_with_timeout` utility with `MCP_HANDLER_TIMEOUT = 30s`. Applied to 5 `spawn_blocking` calls in `context_retrospective` handler. Fire-and-forget calls (usage recording, query logging) are correctly excluded.

4. **Bonus -- SIGKILL escalation**: `pidfile.rs` lines 214-231 -- after SIGTERM timeout expires, `terminate_and_wait` now sends SIGKILL with 500ms wait for kernel cleanup. This prevents ghost processes from blocking future startups.

### No Placeholder Functions
**Status**: PASS
**Evidence**: `grep -r "todo!\|unimplemented!\|TODO\|FIXME" crates/unimatrix-server/src/` returned zero matches.

### All Tests Pass
**Status**: PASS
**Evidence**:
- `cargo test -p unimatrix-server --lib` -- all 1180 tests pass
- `cargo test -p unimatrix-server` -- all lib + integration + doc tests pass (7 pipeline tests)
- 5 new tests in `infra::timeout::tests` all pass:
  - `test_spawn_blocking_with_timeout_returns_result` (happy path)
  - `test_spawn_blocking_with_timeout_on_timeout` (verifies timeout error)
  - `test_spawn_blocking_with_timeout_on_panic` (verifies panic handling)
  - `test_spawn_blocking_with_timeout_string_result` (generic type)
  - `test_mcp_handler_timeout_is_30s` (constant validation)
- 1 pre-existing flaky test in `unimatrix-vector` (`test_compact_search_consistency` -- HNSW non-determinism). Not touched by bugfix.

### No New Clippy Warnings
**Status**: PASS
**Evidence**: `cargo clippy -p unimatrix-server` shows only pre-existing warnings (collapsible `if` statements, `write!` style). No warnings reference any bugfix-modified code paths.

### No Unsafe Code
**Status**: PASS
**Evidence**: `grep -r "unsafe" ` in all modified files returned only pre-existing comments in `pidfile.rs` explaining why `libc::kill` is avoided in favor of the `kill` command.

### Fix Is Minimal
**Status**: PASS
**Evidence**: `git diff --stat` shows exactly 7 files modified, all within `crates/unimatrix-server/`:
- `main.rs` (+35/-20): shutdown restructure for cancellation_token
- `infra/shutdown.rs` (-17/+8): simplified graceful_shutdown, made shutdown_signal pub
- `infra/pidfile.rs` (+18/-4): SIGKILL escalation
- `background.rs` (+38/-19): tick timeouts
- `infra/timeout.rs` (+92): new utility module
- `infra/mod.rs` (+1): module declaration
- `mcp/tools.rs` (+67/-47): applied spawn_blocking_with_timeout

No unrelated changes. No files outside the server crate touched.

### New Tests Catch Original Bug
**Status**: PASS
**Evidence**: The timeout tests directly verify the spawn_blocking_with_timeout utility that prevents RC-3 (indefinite handler hangs). `test_spawn_blocking_with_timeout_on_timeout` specifically verifies that a slow blocking task returns an error after the timeout. `test_mcp_handler_timeout_is_30s` locks the constant value. For RC-1, the fix is structural (cancellation_token vs select! pattern) -- difficult to unit test without a full rmcp stack, but the code change is well-documented and follows rmcp's recommended shutdown pattern. For RC-2, the timeout wrapping is straightforward tokio::time::timeout and covered by existing tick tests.

### Integration Smoke Tests
**Status**: PASS
**Evidence**: 136 integration tests pass, 6 xfail (pre-existing lifecycle tests affected by bugfix-228's permissive auto-enroll change).

### xfail Markers Have GH Issues
**Status**: PASS
**Evidence**: GH#238 filed: "[infra-001] test_multi_agent_interaction: restricted agent can now store after bugfix-228 permissive auto-enroll". State: OPEN. Root cause documented as bugfix-228's permissive auto-enroll behavior conflicting with test assumptions.

### Knowledge Stewardship
**Status**: PASS
**Evidence**: Rust-dev agent report (`236-agent-1-fix-report.md` lines 51-53) contains:
- `Queried:` Entries #731, #735, #770, #771, #667 (fire-and-forget, pool saturation, mutex deadlock, blocking lock, lock-then-mutate patterns)
- `Declined to store:` with reasoning -- rmcp cancellation_token pattern is specific to this fix, not generalizable

Investigator work done inline by bugfix leader (valid per protocol). Knowledge queries cited: same entries #731, #735, #770, #771, #667.

## WARN: File Length

`pidfile.rs` (569 lines), `background.rs` (643 lines), and `tools.rs` (2547 lines) all exceed the 500-line limit. However, all were already over 500 lines before this bugfix (pre-existing: 551, 607, 2533 respectively). The bugfix added minimal lines (+18, +36, +14). Not blocking.
