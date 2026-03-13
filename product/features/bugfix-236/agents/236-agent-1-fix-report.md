# Agent Report: 236-agent-1-fix

## Task
Fix MCP server reliability issues (#236): ghost process after SIGTERM, background tick mutex contention, and missing request-level timeouts.

## Files Modified
- `/workspaces/unimatrix/.claude/worktrees/agent-a7668203/crates/unimatrix-server/src/main.rs` -- Restructured shutdown to use cancellation token pattern
- `/workspaces/unimatrix/.claude/worktrees/agent-a7668203/crates/unimatrix-server/src/infra/shutdown.rs` -- Simplified graceful_shutdown (removed server future param), made shutdown_signal pub
- `/workspaces/unimatrix/.claude/worktrees/agent-a7668203/crates/unimatrix-server/src/infra/pidfile.rs` -- SIGKILL escalation in terminate_and_wait
- `/workspaces/unimatrix/.claude/worktrees/agent-a7668203/crates/unimatrix-server/src/background.rs` -- 2-minute timeout on maintenance_tick and extraction_tick
- `/workspaces/unimatrix/.claude/worktrees/agent-a7668203/crates/unimatrix-server/src/infra/mod.rs` -- Added timeout module
- `/workspaces/unimatrix/.claude/worktrees/agent-a7668203/crates/unimatrix-server/src/infra/timeout.rs` -- New: spawn_blocking_with_timeout utility
- `/workspaces/unimatrix/.claude/worktrees/agent-a7668203/crates/unimatrix-server/src/mcp/tools.rs` -- Applied spawn_blocking_with_timeout to context_retrospective handler

## New Tests
- `test_spawn_blocking_with_timeout_returns_result` -- happy path
- `test_spawn_blocking_with_timeout_on_timeout` -- timeout behavior
- `test_spawn_blocking_with_timeout_on_panic` -- panic handling
- `test_spawn_blocking_with_timeout_string_result` -- generic type support
- `test_mcp_handler_timeout_is_30s` -- constant value validation

## Test Results
- 1190 passed (1180 lib + 10 bin), 0 failed
- All pre-existing tests continue to pass

## Fix Details

### Fix 1: Ghost Process (Primary)
**Problem**: After SIGTERM, `graceful_shutdown()` used `tokio::select!` to race `running.waiting()` against the signal. On signal, the `waiting()` future was dropped, which dropped the RunningService. The Drop impl triggers async cancellation via a DropGuard, but by then the tokio runtime is shutting down and the blocking stdin reader is never closed.

**Solution**: Get the `cancellation_token()` from RunningService before calling `waiting()`. Spawn a separate task that monitors SIGTERM/SIGINT and calls `token.cancel()`. This causes the rmcp service loop to exit cleanly (including transport close), and `waiting()` returns normally with `QuitReason::Cancelled`. The `graceful_shutdown` function is simplified to no longer accept a server future parameter.

### Fix 2: Background Tick Timeout
**Problem**: maintenance_tick and extraction_tick could hold the Store mutex for extended periods (especially graph compaction, confidence refresh), blocking MCP request handlers.

**Solution**: Both ticks wrapped in `tokio::time::timeout(Duration::from_secs(120), ...)`. If a tick exceeds 2 minutes, it is aborted and logged. Work is idempotent and retries next cycle.

### Fix 3: Handler-Level Timeouts
**Problem**: MCP tool handlers using `spawn_blocking` had no timeout. If the Store mutex was contended, clients waited indefinitely.

**Solution**: Created `spawn_blocking_with_timeout` utility in `infra/timeout.rs` with 30-second default. Applied to 5 direct `spawn_blocking` calls in `context_retrospective` handler (observations load, cached metrics, historical metrics, metrics store, observation cleanup). Fire-and-forget calls (usage recording, confidence seeding) are NOT wrapped per the constraint.

### Bonus: SIGKILL Escalation
**Problem**: `terminate_and_wait` only sent SIGTERM. If the process didn't exit (ghost process scenario), the stale process detection path returned `false` and startup continued without cleanup.

**Solution**: After SIGTERM timeout expires, send SIGKILL as last resort with 500ms wait for kernel cleanup.

## Issues
None. All changes are within scope and all tests pass.

## Knowledge Stewardship
- Queried: Entries #731 (fire-and-forget pattern), #735 (spawn_blocking pool saturation), #770 (non-reentrant mutex deadlock), #771 (blocking lock_conn), #667 (lock-then-mutate pattern) -- all provided by the spawn prompt
- Declined to store: The rmcp cancellation_token pattern is specific to this shutdown fix and documented in the code comments. Not generalizable enough for a Unimatrix entry since it's a one-time structural fix.
