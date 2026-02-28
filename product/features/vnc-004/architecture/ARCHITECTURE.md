# Architecture: vnc-004 Server Process Reliability

## System Overview

vnc-004 hardens the MCP server's process lifecycle to eliminate cascading failures when stdio connections drop. The changes are confined to `crates/unimatrix-server/` and affect five existing modules plus one new error variant. No new crates, no schema changes, no MCP protocol changes.

The core architectural principle is **defense in depth**: each fix is independently valuable, but together they close a cascade path where one failure (broken pipe) leads to a second (zombie process), which leads to a third (database lock held indefinitely), which blocks all future instances.

## Component Breakdown

### Component 1: PidGuard (RAII lifecycle manager)

**Location**: `crates/unimatrix-server/src/pidfile.rs`
**Responsibility**: Unified ownership of the PID file's lifecycle — write, lock, and cleanup.

```rust
pub struct PidGuard {
    /// Open file handle holding the flock.
    file: std::fs::File,
    /// Path to the PID file (for removal on drop).
    path: PathBuf,
}
```

`PidGuard` acquires an exclusive advisory lock (`flock(LOCK_EX | LOCK_NB)`) on the PID file, writes the current PID, and removes the file on drop. This single struct replaces three separate operations (write, lock, remove) that previously had no shared ownership.

**Design decisions**:
- The flock is held for the process lifetime via the open `File` handle in the struct. When the process exits (normally, on error, or on panic), the file handle closes, releasing the lock. SIGKILL also releases the lock (kernel closes all fds).
- Uses `fs2` crate for safe `flock` wrappers (no unsafe code needed, preserving `#![forbid(unsafe_code)]`). See ADR-001.
- The explicit `remove_pid_file` call in `graceful_shutdown` becomes redundant but remains for belt-and-suspenders logging.

### Component 2: Process Identity Verification

**Location**: `crates/unimatrix-server/src/pidfile.rs`
**Responsibility**: Determine whether a PID belongs to a unimatrix-server process before sending SIGTERM.

```rust
pub fn is_unimatrix_process(pid: u32) -> bool
```

On Linux, reads `/proc/{pid}/cmdline` and checks if any argument contains `unimatrix-server`. On non-Linux Unix, falls back to `kill -0` (existence check only). Returns `false` for non-existent processes.

**Integration**: Replaces `is_process_alive` in `handle_stale_pid_file`. The stale PID handler now has three outcomes:
1. PID dead -> remove PID file, proceed.
2. PID alive, IS unimatrix-server -> SIGTERM and wait.
3. PID alive, NOT unimatrix-server -> remove PID file (stale), proceed. Do NOT SIGTERM.

### Component 3: Error Path Correction

**Location**: `crates/unimatrix-server/src/main.rs`, `crates/unimatrix-server/src/error.rs`
**Responsibility**: Replace `std::process::exit(1)` with proper error propagation.

New error variant:
```rust
pub enum ServerError {
    // ... existing variants ...
    /// Database is locked by another process after exhausting retries.
    DatabaseLocked(PathBuf),
}
```

`open_store_with_retry` returns `Err(ServerError::DatabaseLocked(path))` instead of calling `process::exit(1)`. This allows destructors (including `PidGuard::drop`) to run.

### Component 4: Session Timeout

**Location**: `crates/unimatrix-server/src/main.rs`
**Responsibility**: Prevent zombie servers by bounding session idle time.

Wraps the `running.waiting()` future with `tokio::time::timeout`. If the session is idle beyond the threshold (default: 30 minutes, configurable via constant), the timeout expires and the normal shutdown path executes.

```rust
const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

let waiting = async {
    let _ = tokio::time::timeout(SESSION_IDLE_TIMEOUT, running.waiting()).await;
};
```

**Design decision**: This uses the simplest possible approach — a timeout on the session future. It does NOT attempt to detect broken pipes, monitor stdout health, or inspect rmcp internals. The rationale is that a server with no activity for 30 minutes has no clients and should release its resources. Active sessions reset the timeout implicitly because rmcp's session future only completes when the transport closes. See ADR-002.

### Component 5: Panic-Safe Lock Acquisition

**Location**: `crates/unimatrix-server/src/categories.rs`
**Responsibility**: Recover from poisoned `RwLock` instead of panicking.

Replace three `.expect("category lock poisoned")` calls with `.unwrap_or_else(|e| e.into_inner())`. A poisoned `RwLock` means a previous writer panicked mid-update, but the data inside is still structurally valid. For a category allowlist (a `HashSet<String>`), partial writes leave the set in a consistent state — either the insert happened or it didn't.

## Component Interactions

```
main() startup:
    1. handle_stale_pid_file() -- uses is_unimatrix_process() for identity check
    2. open_store_with_retry() -- returns Err(DatabaseLocked) instead of exit(1)
    3. PidGuard::acquire()     -- flock + write PID (new unified step)
    4. ... build server ...
    5. serve + timeout wrapper  -- SESSION_IDLE_TIMEOUT bounds the session
    6. graceful_shutdown()      -- explicit cleanup (vector dump, compact, PID remove)
    7. PidGuard::drop()        -- belt-and-suspenders PID removal + lock release

Error/panic path:
    main() returns Err → PidGuard::drop() runs → PID file removed, flock released
    panic in any task → PidGuard::drop() runs during unwind
    SIGKILL → kernel closes fds → flock released (PID file remains, cleaned by next startup)
```

## Technology Decisions

| Decision | Choice | Rationale | ADR |
|----------|--------|-----------|-----|
| flock implementation | `fs2` crate | Safe wrappers, no unsafe code, well-maintained | ADR-001 |
| Session timeout approach | `tokio::time::timeout` on session future | Simplest approach, no rmcp internals, no watchdog task | ADR-002 |
| Poison recovery strategy | `unwrap_or_else(\|e\| e.into_inner())` | Category data is structurally safe after partial writes | N/A (straightforward) |

## Integration Points

| Integration | Current State | Change |
|-------------|--------------|--------|
| `main.rs` <-> `pidfile.rs` | Separate `write_pid_file` call | `PidGuard::acquire()` replaces write; guard held in `main()` scope |
| `main.rs` <-> `error.rs` | `process::exit(1)` bypasses error type | New `DatabaseLocked` variant, normal error propagation |
| `shutdown.rs` <-> `pidfile.rs` | Explicit `remove_pid_file` in step 4 | Remains as logging point; PidGuard drop is the real cleanup |
| `main.rs` <-> rmcp | `running.waiting()` blocks indefinitely | Wrapped in `tokio::time::timeout` |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `PidGuard::acquire(path: &Path) -> io::Result<Self>` | New public constructor | `pidfile.rs` |
| `PidGuard::drop(&mut self)` | Removes PID file + releases flock | `pidfile.rs` |
| `is_unimatrix_process(pid: u32) -> bool` | New public function (replaces `is_process_alive` usage) | `pidfile.rs` |
| `ServerError::DatabaseLocked(PathBuf)` | New error variant | `error.rs` |
| `SESSION_IDLE_TIMEOUT: Duration` | New constant | `main.rs` |
| `remove_pid_file(path: &Path)` | Existing, unchanged | `pidfile.rs` |
| `handle_stale_pid_file(pid_path, timeout) -> io::Result<bool>` | Existing signature, updated internals | `pidfile.rs` |

## Files to Create/Modify

| File | Action | Summary |
|------|--------|---------|
| `src/pidfile.rs` | Modify | Add `PidGuard`, `is_unimatrix_process`, update `handle_stale_pid_file` |
| `src/main.rs` | Modify | Use `PidGuard`, replace `process::exit(1)`, add session timeout |
| `src/error.rs` | Modify | Add `DatabaseLocked` variant + Display/ErrorData impls |
| `src/shutdown.rs` | Modify | Remove explicit PID removal (or keep as log-only) |
| `src/categories.rs` | Modify | Replace `.expect()` with poison recovery |
| `Cargo.toml` | Modify | Add `fs2` dependency |

## Constraints

- `#![forbid(unsafe_code)]` — no unsafe code in the server crate. `fs2` provides safe flock wrappers.
- No MCP protocol changes. No schema changes. No new tables.
- All fixes must be backward compatible — existing behavior preserved for the happy path.
- Session timeout must not kill active sessions under normal operation.
