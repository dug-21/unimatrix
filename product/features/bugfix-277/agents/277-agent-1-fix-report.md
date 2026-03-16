# Agent 277-agent-1-fix Report

## Task
Fix GH #277: hot-path MCP handlers using bare `spawn_blocking` with no timeout, causing indefinite hangs when background tick holds `Mutex<Connection>`.

## Files Modified

- `crates/unimatrix-server/src/infra/timeout.rs` — added regression test
- `crates/unimatrix-server/src/services/search.rs` — import + lines 224, 461
- `crates/unimatrix-server/src/services/store_ops.rs` — import + lines 116, 187 + error chain
- `crates/unimatrix-server/src/services/store_correct.rs` — import + lines 52, 83 + error chain
- `crates/unimatrix-server/src/services/status.rs` — import + lines 208, 432, 460, 491, 632, 660, 674
- `crates/unimatrix-server/src/mcp/tools.rs` — lines 1352, 1411, 1447, 1665, 1692, 1833, 1847, 1861

## New Tests

- `test_handler_times_out_when_mutex_held_by_background_tick` — in `infra/timeout.rs`. Spawns a background thread that holds a `Mutex<()>` for 2 seconds, then runs `spawn_blocking_with_timeout` with a 50ms deadline. Asserts `Err` with "timed out" message rather than hanging.

## Tests

1317 passed; 0 failed (unimatrix-server unit tests)
All integration test suites also pass (16 + 16 + 7).

## Issues

None. The key subtlety: `spawn_blocking_with_timeout` returns `Result<T, ServerError>` — the outer error type changes from `JoinError` to `ServerError`. Two-level error chains that previously did `.map_err(|JoinError| ...)?.map_err(|inner| ...)?` needed the first map updated to handle `ServerError`. Fire-and-forget spawns (`let _ = spawn_blocking(...)` for usage recording, confidence seeding, supersession chain) were correctly left unchanged per the approved approach.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` via spawn prompt context — entries #1367 (spawn_blocking_with_timeout pattern), #1369 (MCP Tool 6-Step Handler Pipeline)
- Stored: entry #1700 "spawn_blocking_with_timeout: outer error type is ServerError, not JoinError" via `/uni-store-pattern` — this is the gotcha that bites during migration: the outer error type changes from `JoinError` to `ServerError`, requiring updated `map_err` chains at every wrapped callsite.
