# Risk-Based Test Strategy: vnc-004

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | PidGuard drop fails to remove PID file (permission error, read-only fs) | High | Low | Medium |
| R-02 | flock not supported on filesystem (NFS, some container overlayfs) | High | Low | Medium |
| R-03 | `/proc/{pid}/cmdline` parsing fails on unexpected format (null-separated, empty, truncated) | Med | Med | Medium |
| R-04 | Session timeout kills active session (tool call in flight when timeout fires) | High | Low | Medium |
| R-05 | DatabaseLocked error message leaks internal paths in production | Low | Med | Low |
| R-06 | Poisoned RwLock recovery returns inconsistent data (partial HashSet insert) | Med | Low | Low |
| R-07 | PidGuard flock and graceful_shutdown PID removal race on concurrent exit paths | Med | Low | Low |
| R-08 | PID recycling window — between reading stale PID and checking /proc, PID could be recycled to a new unimatrix-server instance | High | Low | Medium |
| R-09 | Two instances started with different `--project-dir` flags share the same PID file | High | Low | Medium |
| R-10 | flock acquisition blocks indefinitely if LOCK_NB flag is accidentally omitted | High | Low | Medium |

## Risk-to-Scenario Mapping

### R-01: PidGuard drop fails to remove PID file
**Severity**: High
**Likelihood**: Low
**Impact**: Next startup finds stale PID file and attempts unnecessary SIGTERM/recovery. Not fatal — the flock-based check recovers — but adds startup latency.

**Test Scenarios**:
1. PidGuard drop when PID file was already removed by another path (double-remove idempotent)
2. PidGuard drop on a path that doesn't exist (tempdir cleaned up first)

**Coverage Requirement**: Unit test PidGuard drop in normal and edge-case scenarios. Verify no panic on failed removal.

### R-02: flock not supported on filesystem
**Severity**: High
**Likelihood**: Low
**Impact**: `PidGuard::acquire` fails with an I/O error, server cannot start. This is a hard failure on unsupported filesystems.

**Test Scenarios**:
1. `PidGuard::acquire` returns `Err` when flock fails — verify error is propagated, not panicked
2. Verify the error message is actionable (mentions flock and filesystem)

**Coverage Requirement**: Unit test error path of `PidGuard::acquire`. The flock-unsupported case is hard to simulate in CI but the error propagation path must be tested.

### R-03: /proc/cmdline parsing edge cases
**Severity**: Medium
**Likelihood**: Medium
**Impact**: False negative — identity check fails, treats a real unimatrix-server process as non-unimatrix, removes PID file instead of SIGTERMing.

**Test Scenarios**:
1. `/proc/{pid}/cmdline` contains null-separated args (standard Linux format)
2. `/proc/{pid}/cmdline` contains the full path `/usr/bin/unimatrix-server`
3. `/proc/{pid}/cmdline` is empty (kernel thread or zombie)
4. `/proc/{pid}/cmdline` does not exist (process exited between check and read)
5. `/proc/{pid}/cmdline` contains `unimatrix-server` as a substring of another binary name (e.g., `not-unimatrix-server`)

**Coverage Requirement**: Unit tests for `is_unimatrix_process` with mocked `/proc` content (or test helper that writes to temp files with cmdline format).

### R-04: Session timeout kills active session
**Severity**: High
**Likelihood**: Low
**Impact**: Client receives an error mid-tool-call. Data loss if a write was in progress.

**Test Scenarios**:
1. Verify timeout only fires when `running.waiting()` is blocking (no active tool calls)
2. Verify timeout triggers graceful shutdown (vector dump, compact, PID cleanup all execute)

**Coverage Requirement**: Integration test that verifies the timeout path triggers full graceful shutdown sequence. The "active session not interrupted" property relies on rmcp's session future behavior — document as assumption.

### R-05: DatabaseLocked error message leaks paths
**Severity**: Low
**Likelihood**: Medium
**Impact**: Internal filesystem paths visible in error output. Low severity because the server runs locally.

**Test Scenarios**:
1. Verify `DatabaseLocked` Display and ErrorData formatting include the path and hint

**Coverage Requirement**: Unit test for Display and ErrorData conversion.

### R-06: Poisoned RwLock recovery returns inconsistent data
**Severity**: Medium
**Likelihood**: Low
**Impact**: CategoryAllowlist might have a partially-inserted category. For a `HashSet<String>`, `insert` is atomic at the Rust level — either the key is in the set or it isn't. No partial state is possible.

**Test Scenarios**:
1. Poison the lock by panicking in a write closure, then verify recovery returns valid data
2. Verify `validate`, `add_category`, and `list_categories` all recover from poisoned state

**Coverage Requirement**: Unit tests that deliberately poison the RwLock and verify recovery.

### R-07: PidGuard drop and graceful_shutdown race
**Severity**: Medium
**Likelihood**: Low
**Impact**: Double removal of PID file. Already idempotent via `remove_pid_file` handling NotFound.

**Test Scenarios**:
1. Call `remove_pid_file` on an already-removed path — no error, no panic

**Coverage Requirement**: Existing test `test_remove_nonexistent_pid_file_is_silent` covers this.

### R-08: PID recycling window
**Severity**: High
**Likelihood**: Low
**Impact**: Could SIGTERM a newly-started legitimate unimatrix-server instance that reused the PID. The flock approach largely eliminates this — if the new instance holds the flock, we won't reach the SIGTERM path.

**Test Scenarios**:
1. Verify that when flock is held by another process, the startup path reads PID and checks identity before any SIGTERM decision

**Coverage Requirement**: Integration test with two PidGuard instances on the same path — second acquire must fail with clear error.

### R-09: Different project dirs sharing PID file
**Severity**: High
**Likelihood**: Low
**Impact**: Two servers for different projects could interfere. Current design: PID file is per-project (`~/.unimatrix/{hash}/unimatrix.pid`), so different `--project-dir` values produce different PID file paths.

**Test Scenarios**:
1. Verify `ProjectPaths` produces different `pid_path` for different project roots

**Coverage Requirement**: Existing `test_hash_different_paths` partially covers this. Add explicit PID path isolation test.

### R-10: flock blocks indefinitely without LOCK_NB
**Severity**: High
**Likelihood**: Low
**Impact**: Server hangs at startup waiting for a lock that may never be released.

**Test Scenarios**:
1. Verify `PidGuard::acquire` uses non-blocking lock (`try_lock_exclusive`, not `lock_exclusive`)
2. Verify immediate error return when lock is held

**Coverage Requirement**: Unit test: acquire two PidGuards on the same file — second must fail immediately (not block).

## Integration Risks

| Risk | Components | Scenario |
|------|-----------|----------|
| PidGuard ownership transfer | `main.rs` <-> `shutdown.rs` | PidGuard created in `main()`, must survive through `graceful_shutdown()` and only drop after shutdown completes. If moved into `LifecycleHandles`, drop order matters. |
| Error variant exhaustiveness | `error.rs` <-> `main.rs` | Adding `DatabaseLocked` to `ServerError` requires matching in `Display`, `ErrorData::from`. Missing arm = compile error (good), but the `ErrorData` mapping must be added explicitly. |
| flock + PID file ordering | `pidfile.rs` | Must acquire flock BEFORE writing PID. If PID is written first, another process could read the PID before the lock is held, creating a brief window of inconsistency. |

## Edge Cases

| Edge Case | Expected Behavior |
|-----------|-------------------|
| PID file path contains spaces or special chars | `fs2::FileExt` operates on `File` handles, not paths. Path handling is in `std::fs::File::create`. |
| PID file parent directory doesn't exist | `ensure_data_directory` creates it. If it's somehow missing, `PidGuard::acquire` returns `Err`. |
| PID = 0 in stale PID file | `is_unimatrix_process(0)` returns false (PID 0 is kernel). Remove stale file. |
| PID = current process ID in stale PID file | This means we're reading our own stale PID from a previous run. Identity check finds us. Should not SIGTERM self. The flock path handles this — we already hold the lock if this is our PID. |
| Concurrent read/write of CategoryAllowlist | RwLock handles concurrency. Poison recovery means even panics don't permanently break the lock. |
| `/proc` mounted with hidepid=2 | Can't read other processes' cmdline. `is_unimatrix_process` falls back to kill-0 behavior (returns true if process exists, can't verify identity). |

## Security Risks

| Risk | Assessment |
|------|------------|
| PID file as attack vector | Low. PID file is in `~/.unimatrix/{hash}/` with user-only permissions (created by `fs::write` with default umask). An attacker with write access to the home directory already has full compromise. |
| SIGTERM to wrong process | Medium (pre-fix), mitigated by Fix 3 (identity verification). Residual risk on non-Linux platforms where identity check falls back to existence-only. |
| flock bypass | Low. Advisory locks can be bypassed by processes that don't use flock. Acceptable because the PID file is only used by unimatrix-server, not by arbitrary programs. |
| Session timeout as DoS | Low. An attacker controlling the system clock could trigger premature timeouts, but clock manipulation requires root. |

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| flock acquisition fails (unsupported fs) | `PidGuard::acquire` returns `Err`. Server logs error with actionable hint and exits via normal error path. |
| PID file removal fails on drop | PidGuard logs warning, continues drop. Next startup handles stale file via flock + identity check. |
| Session timeout fires during active operation | Graceful shutdown waits for the 100ms flush pause. In-flight operations complete or fail at the rmcp level. |
| DatabaseLocked after retries | Server returns error to main, PidGuard drops (if acquired), clean exit with non-zero status. |
| Lock poisoning in CategoryAllowlist | Recovery via `into_inner()`. Operations continue with the data from before the panic. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 | R-02, R-10 | `fs2` crate provides safe flock wrappers; non-blocking lock prevents hangs |
| SR-02 | R-03 | Fallback chain: /proc/cmdline -> kill-0. Platform coverage matrix documented in architecture. |
| SR-03 | R-04 | 30-minute default timeout. Timeout wraps session future, not individual tool calls. |
| SR-04 | R-07 | Single PidGuard struct owns both flock and file. Double-remove is idempotent. |
| SR-05 | R-04 | ADR-002 chose simple timeout over watchdog. rmcp internals not needed. |
| SR-06 | R-07 | Existing `remove_pid_file` handles NotFound silently. |
| SR-07 | R-05 | `DatabaseLocked` variant added to `ServerError` with proper Display/ErrorData. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 0 | 0 scenarios |
| Medium | 5 (R-01, R-02, R-03, R-08, R-10) | 12 scenarios |
| Low | 5 (R-04, R-05, R-06, R-07, R-09) | 8 scenarios |
| **Total** | **10** | **20 scenarios** |
