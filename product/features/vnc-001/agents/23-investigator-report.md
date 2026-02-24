# Bug Investigation Report: 23-investigator

## Bug Summary
When a new `unimatrix-server` process is launched while a previous instance still holds the redb database lock (stale process, unclean shutdown, or concurrent launch during MCP reconnect), the new instance exits immediately with code 1. The client (Claude Code) sees "Failed to reconnect to unimatrix."

## Root Cause Analysis

### Code Path Trace
- `main.rs:main()` -> `project::ensure_data_directory()` -> `Store::open(&paths.db_path)` -> redb `Database::create()` -> `DatabaseAlreadyOpen` error
- `main.rs:61` matches `StoreError::Database(redb::DatabaseError::DatabaseAlreadyOpen)` -> prints error to stderr -> `std::process::exit(1)`

### Why It Fails
1. **No stale process detection**: There is no mechanism to detect whether a previous server process is still running. redb uses file-level locking, which persists as long as the process holding the lock is alive.
2. **No stale process cleanup**: When a previous process is alive but stale (orphaned from its MCP client), there is no way for the new process to signal it to shut down.
3. **Immediate exit on lock conflict**: The `DatabaseAlreadyOpen` handler calls `std::process::exit(1)` immediately with no retry. There is no attempt to wait for the lock to be released (e.g., after the old process receives SIGTERM from its own shutdown path).
4. **No PID file**: There is no PID file written during startup, so there is no way to identify which process holds the lock without resorting to `lsof`.

The shutdown module (`shutdown.rs`) already handles SIGTERM properly (dump vector, compact DB, exit), but there is no mechanism for a new server instance to trigger that shutdown in the old instance.

## Affected Files and Functions
| File | Function | Role in Bug |
|------|----------|-------------|
| `crates/unimatrix-server/src/main.rs:58-71` | `main()` | DB open error handler exits immediately on `DatabaseAlreadyOpen` |
| `crates/unimatrix-server/src/shutdown.rs` | `graceful_shutdown()` | No PID file cleanup on exit |
| `crates/unimatrix-server/src/project.rs` | `ProjectPaths` | No PID file path in project paths |

## Proposed Fix Approach

### 1. Add PID file path to ProjectPaths (`project.rs`)
- Add `pid_path: PathBuf` field to `ProjectPaths` struct, set to `{data_dir}/unimatrix.pid`

### 2. Create a PID file module (`pidfile.rs`)
- `write_pid_file(path)` — writes current PID to file atomically
- `read_pid_file(path)` — reads PID from file, returns None if missing/invalid
- `remove_pid_file(path)` — removes PID file (called on shutdown)
- `is_process_alive(pid)` — checks if PID is alive via `kill(pid, 0)` (Unix) or equivalent
- `terminate_stale_process(pid, timeout)` — sends SIGTERM, waits up to timeout for exit, returns whether the process exited

### 3. Add stale process handling to main.rs startup
Before `Store::open()`:
1. Check for `unimatrix.pid` in data_dir
2. If PID file exists and process is alive: send SIGTERM, wait up to 5s
3. If PID file exists and process is dead: remove stale PID file
4. After stale process handling, write current PID to file

### 4. Add retry loop on DatabaseAlreadyOpen
Replace the immediate `process::exit(1)` with a retry loop:
- Up to 3 attempts with 1-second backoff
- This handles the race where SIGTERM was sent but the old process hasn't fully released the lock yet
- If all retries exhausted, then exit with the existing error message

### 5. Clean up PID file on shutdown (`shutdown.rs`)
- After graceful shutdown completes, remove the PID file
- Pass the PID file path into `LifecycleHandles`

### Why This Fix
- PID file is the standard Unix mechanism for single-instance process management
- SIGTERM is already handled by the existing shutdown module, so we are reusing existing infrastructure
- Retry loop is a safety net for the race between SIGTERM delivery and lock release
- The fix is minimal: adds one new module, modifies three existing files, uses no new dependencies

## Risk Assessment
- **Blast radius**: Low. Changes are confined to process lifecycle (startup/shutdown). No changes to MCP protocol handling, tool implementations, or data storage logic.
- **Regression risk**: Low. The PID file mechanism is additive. If PID file operations fail (permissions, missing dir), the server should fall back to the existing behavior (attempt open, fail if locked). The retry loop adds at most 3 seconds of startup delay in the worst case.
- **Confidence**: High. The root cause is clearly visible in `main.rs:61-68` — the `DatabaseAlreadyOpen` handler has no recovery path. The fix follows a well-established pattern (PID files + SIGTERM).
- **Platform note**: `kill(pid, 0)` and SIGTERM are Unix-specific. The crate already has `#[cfg(unix)]` blocks in `shutdown.rs` for signal handling. The PID file module should use the same pattern, with a graceful fallback on non-Unix (skip stale process termination, keep retry loop).

## Missing Test
The following test scenarios should have existed:
1. **PID file lifecycle**: write PID -> read PID -> process exits -> stale detection -> removal
2. **Stale process detection**: PID file with dead PID is correctly identified as stale
3. **Retry on DatabaseAlreadyOpen**: When the lock is released during retry window, the server starts successfully
4. **PID file cleanup on shutdown**: After graceful shutdown, PID file is removed

The existing test `test_open_already_open_returns_database_error` in `crates/unimatrix-store/src/db.rs:140` verifies the error is correctly produced, but there is no test for recovery from that error.

## Reproduction Scenario
Deterministic:
1. Start `unimatrix-server` (process A holds the DB lock)
2. Attempt to start a second `unimatrix-server` pointing at the same project (process B)
3. Process B hits `DatabaseAlreadyOpen` at `main.rs:61` and exits with code 1
4. Any MCP client that launched process B sees a connection failure
