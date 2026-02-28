# Pseudocode Overview: vnc-004 Server Process Reliability

## Components

| Component | File Modified | Purpose |
|-----------|--------------|---------|
| pid-guard | `src/pidfile.rs` | PidGuard RAII struct + is_unimatrix_process + handle_stale_pid_file update |
| error-path | `src/error.rs`, `src/main.rs` | DatabaseLocked variant, replace process::exit(1) |
| session-timeout | `src/main.rs` | SESSION_IDLE_TIMEOUT constant, timeout wrapper on session future |
| poison-recovery | `src/categories.rs` | Replace .expect() with .unwrap_or_else poison recovery |

## Data Flow

```
main() startup:
  1. handle_stale_pid_file()  -- now uses is_unimatrix_process() for identity check
  2. open_store_with_retry()  -- returns Err(DatabaseLocked) instead of exit(1)
  3. PidGuard::acquire()      -- flock + write PID (replaces write_pid_file)
  4. ... build server ...
  5. serve + timeout wrapper  -- SESSION_IDLE_TIMEOUT bounds the session
  6. graceful_shutdown()      -- explicit cleanup (vector dump, compact, PID remove)
  7. PidGuard::drop()         -- belt-and-suspenders PID removal + lock release
```

## Shared Types

- `PidGuard` (new struct in pidfile.rs) -- holds File + PathBuf
- `ServerError::DatabaseLocked(PathBuf)` (new variant in error.rs)
- `SESSION_IDLE_TIMEOUT: Duration` (new constant in main.rs)

## Sequencing Constraints

1. **error-path** and **pid-guard** are independent of each other
2. **session-timeout** depends on error-path (main.rs changes coexist)
3. **poison-recovery** is fully independent of all other components

## Dependencies

- `fs2` crate added to `Cargo.toml` for safe flock wrappers
- No other new dependencies

## Integration Points

- `main.rs` calls `PidGuard::acquire()` instead of `write_pid_file()`
- `main.rs` holds `_pid_guard` binding to keep guard alive through shutdown
- `shutdown.rs` Step 4 (remove_pid_file) becomes belt-and-suspenders logging only
- `LifecycleHandles` no longer needs `pid_path` field (PidGuard handles cleanup)
  - BUT: keep pid_path for belt-and-suspenders in shutdown.rs (no structural change to LifecycleHandles)
