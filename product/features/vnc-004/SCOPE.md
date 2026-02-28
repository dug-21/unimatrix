# vnc-004: Server Process Reliability

**Issue:** #52 — MCP server connection drops and fails to recover
**Phase:** Vinculum (MCP server infrastructure)
**Type:** Bug fix

## Problem Statement

The Unimatrix MCP server suffers from cascading process lifecycle failures during extended sessions. When the stdio connection drops (client timeout, pipe break, session restart), the server either lingers as a zombie holding the database lock, or exits without cleaning up its PID file. Subsequent server instances then fail to acquire the database lock, exhaust their retry budget, and hard-exit via `std::process::exit(1)` — which itself skips cleanup. This cascade leaves the system in a state where no server can start without manual intervention.

### Observed Symptoms

- `MCP error -32000: Connection closed` on tool calls
- Server becomes unavailable: `No such tool available: mcp__unimatrix__context_store`
- Multiple stale server processes accumulate (2-3 instances visible in `ps aux`)
- PID file persists after crash, pointing to dead or recycled PIDs
- Restarting the binary doesn't restore MCP tool availability

### Root Causes (from #52 investigation)

| # | Cause | Location | Severity |
|---|-------|----------|----------|
| 1 | `std::process::exit(1)` bypasses all shutdown (no PID cleanup, no vector dump) | `main.rs:201` | **High** |
| 2 | PID file only removed during graceful shutdown; not cleaned on crash/SIGKILL | `shutdown.rs:85` | **High** |
| 3 | No process identity verification — `kill -0` checks PID existence, not that it's unimatrix-server; could SIGTERM an unrelated process on PID recycling | `pidfile.rs:44-56` | **Medium** |
| 4 | No advisory file lock — PID file check has TOCTOU race; two servers can both pass stale check | `pidfile.rs:113-137` | **Medium** |
| 5 | Broken stdout doesn't close stdin — server may hang on stdin read while unable to write responses, holding DB lock indefinitely | `main.rs:163` (rmcp internals) | **Medium** |
| 6 | RwLock `.expect()` panics crash the server without graceful shutdown | `categories.rs:38,53,59` | **Low** |

**Out of scope:** Agent permission/capability errors (#46) — creates perceived reliability issues but is a separate authorization problem being tracked independently.

## Proposed Fix

### Fix 1: Replace `process::exit(1)` with proper error return

**File:** `main.rs:179-209` (`open_store_with_retry`)

Return a `ServerError` instead of calling `std::process::exit(1)`. This allows `main()` to propagate the error through its normal return path. While `main()` returning an error doesn't run `graceful_shutdown()` either, it does run destructors and allows Rust's normal cleanup. More importantly, this unblocks Fix 2 — if we add PID-file-on-drop cleanup, it will actually execute.

```
Before: std::process::exit(1)
After:  return Err(ServerError::DatabaseLocked(db_path))
```

### Fix 2: PID file cleanup via drop guard

**File:** new struct in `pidfile.rs`

Create a `PidGuard` RAII struct that removes the PID file on drop. This ensures cleanup on:
- Normal shutdown (already works)
- Error returns from `main()`
- Panics (drop runs during unwind)

The only case it doesn't cover is `SIGKILL` — but that's unavoidable and already handled by the stale PID detection at startup.

```rust
pub struct PidGuard { path: PathBuf }

impl PidGuard {
    pub fn write(path: PathBuf) -> io::Result<Self> { ... }
}

impl Drop for PidGuard {
    fn drop(&mut self) { remove_pid_file(&self.path); }
}
```

The guard is created in `main()` after DB lock acquisition and held for the process lifetime. The explicit `remove_pid_file` call in `graceful_shutdown` becomes redundant but can remain as belt-and-suspenders.

### Fix 3: Process identity verification

**File:** `pidfile.rs` — replace `is_process_alive`

On Linux, read `/proc/{pid}/cmdline` and verify it contains `unimatrix-server`. Fall back to bare `kill -0` on non-Linux Unix (macOS doesn't have `/proc`). This prevents SIGTERMing an unrelated process that reused the PID.

```rust
pub fn is_unimatrix_process(pid: u32) -> bool {
    // 1. Check /proc/{pid}/cmdline for "unimatrix-server"
    // 2. Fall back to kill -0 if /proc not available
}
```

Update `handle_stale_pid_file` to use this instead of `is_process_alive`. If the PID exists but is NOT a unimatrix-server process, treat the PID file as stale (remove it, don't SIGTERM).

### Fix 4: Advisory file lock (`flock`)

**File:** `pidfile.rs` — new `PidFileLock` or integrate into `PidGuard`

Use `flock(2)` (via the `fs2` crate or raw `libc`) on the PID file for atomic single-instance enforcement. This eliminates the TOCTOU race between reading the PID file and opening the database.

Flow:
1. Try `flock(LOCK_EX | LOCK_NB)` on the PID file
2. If lock acquired: we're the only instance. Write our PID into the file.
3. If lock fails (`EWOULDBLOCK`): another instance holds it. Read PID, verify identity, optionally SIGTERM.

The lock is automatically released when the process exits (including crashes), solving the stale-lock problem that PID files alone can't handle.

### Fix 5: Stdio health watchdog

**File:** new module or addition to `shutdown.rs`

Detect broken stdout proactively instead of waiting for rmcp to notice. Spawn a background task that periodically writes a no-op byte to stderr (which is safe — stderr is for logging). The real check: if rmcp's session future hasn't completed but tool calls start failing with transport errors, initiate shutdown.

Simpler alternative: wrap the `running.waiting()` future with a timeout. If the session has been idle for longer than a configurable threshold (e.g., 30 minutes), initiate graceful shutdown and release the DB lock. This prevents zombie servers from holding the lock indefinitely.

```rust
let waiting = async {
    let _ = tokio::time::timeout(SESSION_IDLE_TIMEOUT, running.waiting()).await;
};
```

**Note:** This needs care — we don't want to kill active sessions. The timeout should be long (30+ min) and only guard against the "broken pipe but stdin still open" zombie case.

### Fix 6: Panic-safe lock acquisition

**File:** `categories.rs`

Replace `.expect("lock poisoned")` with `.unwrap_or_else(|e| e.into_inner())` to recover from poisoned locks instead of panicking. A poisoned RwLock means a writer panicked, but the data is still accessible. For a category allowlist, recovery is safe.

## Affected Files

| File | Changes |
|------|---------|
| `crates/unimatrix-server/src/main.rs` | Replace `exit(1)`, use `PidGuard`, add session timeout |
| `crates/unimatrix-server/src/pidfile.rs` | `PidGuard`, `flock`, process identity check |
| `crates/unimatrix-server/src/shutdown.rs` | Remove explicit PID removal (guard handles it) |
| `crates/unimatrix-server/src/error.rs` | Add `DatabaseLocked` variant |
| `crates/unimatrix-server/src/categories.rs` | Poison recovery |
| `crates/unimatrix-server/Cargo.toml` | Possibly add `fs2` or `libc` for `flock` |

## Acceptance Criteria

1. **No `process::exit` in server code** — all exit paths go through normal Rust error propagation
2. **PID file always cleaned up** — on normal exit, error exit, and panic (SIGKILL is the only exception, handled by stale detection)
3. **Stale PID detection verifies process identity** — never SIGTERMs a non-unimatrix process
4. **Advisory file lock prevents TOCTOU race** — two simultaneous startups don't both proceed past PID check
5. **Zombie server detection** — server exits gracefully if stdio transport is broken but process lingers
6. **No panics from lock poisoning** — `CategoryAllowlist` recovers instead of crashing

## Testing Strategy

- **Unit tests:** `PidGuard` drop behavior, process identity parsing, flock acquire/release
- **Integration tests:** Simulate stale PID with wrong process identity, concurrent startup race, DB lock retry with proper error return
- **Manual validation:** Kill server with SIGKILL, verify next startup recovers cleanly

## Dependencies

- Possibly `fs2` crate for cross-platform `flock` (or use `libc` directly since we're Unix-only in practice)
- No Unimatrix schema changes
- No MCP protocol changes

## Tracking

- Bug Report: #52
- Implementation: #53
