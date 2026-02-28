# Specification: vnc-004 Server Process Reliability

## Objective

Eliminate cascading process lifecycle failures in the MCP server that leave the system in an unrecoverable state after connection drops, by replacing `process::exit(1)` with proper error propagation, implementing RAII-based PID file management with advisory locking, adding process identity verification before SIGTERM, bounding session lifetime with a timeout, and recovering from lock poisoning instead of panicking.

## Functional Requirements

### FR-01: Error propagation replaces process::exit

When the database is locked by another process after exhausting retry attempts, `open_store_with_retry` must return `Err(ServerError::DatabaseLocked(path))` instead of calling `std::process::exit(1)`. The error message must include the database path and a hint for the operator.

### FR-02: RAII PID file lifecycle

A `PidGuard` struct must manage the PID file's complete lifecycle:
- On construction: acquire an exclusive advisory lock on the PID file, then write the current PID.
- On drop: remove the PID file. The lock is released when the file handle closes.
- Construction must fail if another instance holds the lock (`EWOULDBLOCK` / `LOCK_NB`).

### FR-03: Process identity verification

Before sending SIGTERM to a PID read from a stale PID file, the server must verify the process is a unimatrix-server instance:
- On Linux: read `/proc/{pid}/cmdline` and check for `unimatrix-server` in the command line.
- On non-Linux Unix: fall back to `kill -0` (existence check only, no identity verification).
- If the PID exists but is NOT a unimatrix-server process: treat the PID file as stale, remove it, do NOT send SIGTERM.

### FR-04: Advisory file lock enforcement

The PID file must be protected by an exclusive advisory lock (`flock(LOCK_EX | LOCK_NB)`):
- If the lock is acquired: the server is the sole instance. Write PID and proceed.
- If the lock fails: another instance is running. The startup flow should read the PID, verify identity, and potentially SIGTERM the stale instance (existing behavior).
- The lock must be released automatically on process exit (normal, error, panic, SIGKILL).

### FR-05: Session idle timeout

The MCP session must be bounded by a configurable idle timeout:
- Default timeout: 30 minutes (1800 seconds).
- When the timeout expires, the server initiates graceful shutdown (vector dump, compact, PID cleanup).
- The timeout applies to the session future (`running.waiting()`), not to individual tool calls.
- Active tool call processing is NOT interrupted by the timeout — the timeout only fires when the session future would otherwise block indefinitely.

### FR-06: Panic-safe lock acquisition

`CategoryAllowlist` methods that acquire `RwLock` must recover from poisoned locks:
- Replace `.expect("category lock poisoned")` with `.unwrap_or_else(|e| e.into_inner())`.
- This applies to `validate()`, `add_category()`, and `list_categories()`.
- The recovered data must be used normally — a poisoned lock indicates a prior panic, not data corruption.

## Non-Functional Requirements

### NFR-01: No unsafe code

All changes must compile under `#![forbid(unsafe_code)]`. Advisory locking must use safe wrappers (e.g., `fs2` crate).

### NFR-02: Backward compatibility

The happy path (single instance, clean startup, clean shutdown) must behave identically before and after these changes. No MCP protocol changes, no schema changes.

### NFR-03: Startup recovery time

After a `SIGKILL` of a previous instance, a new server instance must be able to start within 10 seconds (covering PID file cleanup + flock acquisition + database open).

### NFR-04: No data loss

The session timeout must trigger the full graceful shutdown sequence (vector dump, adaptation state save, database compact) — not an abrupt termination.

## Acceptance Criteria

| AC-ID | Description | Verification Method |
|-------|-------------|---------------------|
| AC-01 | No `process::exit` in server code | grep |
| AC-02 | PID file always cleaned up on normal exit, error exit, and panic | test |
| AC-03 | Stale PID detection verifies process identity — never SIGTERMs a non-unimatrix process | test |
| AC-04 | Advisory file lock prevents TOCTOU race — two simultaneous startups don't both proceed past PID check | test |
| AC-05 | Zombie server detection — server exits gracefully if stdio transport is broken but process lingers | test |
| AC-06 | No panics from lock poisoning — `CategoryAllowlist` recovers instead of crashing | test |

## Domain Models

### PidGuard

A RAII guard that owns the PID file lifecycle. Created after successful database open, dropped when `main()` returns (normally or via error/panic).

**States**:
- `Acquired` — file locked, PID written. Normal operating state.
- `Dropped` — file removed, lock released. Terminal state (happens automatically).

### Stale PID Resolution

The resolution flow when a PID file exists at startup:

1. **No PID file** -> Proceed to lock acquisition.
2. **PID file exists, flock available** -> Previous instance crashed (SIGKILL). Remove stale file, proceed.
3. **PID file exists, flock held** -> Live instance. Read PID, verify identity, SIGTERM if unimatrix-server, wait for lock release.
4. **PID file exists, PID is not unimatrix-server** -> PID recycled. Remove stale file, proceed.

### Session Lifecycle

```
startup -> PidGuard acquired -> server running -> [timeout | session close | signal] -> graceful shutdown -> PidGuard dropped
                                                                                              |
                                                                                    error return from main() -> PidGuard dropped
```

## User Workflows

### Normal operation

1. MCP client starts unimatrix-server.
2. Server acquires PID guard (flock + PID write).
3. Server serves tool calls over stdio.
4. Client disconnects (session close).
5. Server runs graceful shutdown, PidGuard drops, resources released.

### Recovery from crash

1. Previous server instance was SIGKILLed.
2. New instance starts, finds stale PID file.
3. Attempts flock — succeeds (kernel released old lock).
4. Removes stale PID file, acquires new PidGuard.
5. Opens database successfully.

### Recovery from zombie

1. Previous server's stdio pipe broke but process lingers.
2. Session timeout fires after 30 minutes of idle.
3. Graceful shutdown runs, PidGuard drops.
4. New instance can start immediately.

### Concurrent startup race

1. Two server instances start simultaneously.
2. First acquires flock on PID file — wins.
3. Second fails flock acquisition, detects existing instance.
4. Second reads PID, verifies identity, optionally SIGTERMs.
5. Only one instance proceeds to serve.

## Constraints

- `#![forbid(unsafe_code)]` in the server crate.
- No new modules — changes fit in existing files plus the new `PidGuard` struct in `pidfile.rs`.
- `fs2` crate is the only new dependency.
- No MCP protocol changes. No schema changes. No new redb tables.
- Session timeout constant is compile-time for now (future: config file in vnc-004 Config Externalization).

## Dependencies

| Dependency | Version | Purpose |
|-----------|---------|---------|
| `fs2` | latest stable | Safe `flock` wrappers for advisory file locking |

All other dependencies are unchanged.

## NOT in Scope

- Agent permission/capability errors (#46) — separate authorization issue.
- Config file externalization (original vnc-004 scope in product vision) — separate feature.
- Cross-platform Windows support for process identity — Windows is not a target platform.
- Automatic server restart — the MCP client (Claude Code) handles restart.
- Dynamic session timeout adjustment based on activity — use a fixed timeout for simplicity.
