# Test Plan: pidguard-self-pid

## Component

`crates/unimatrix-server/src/infra/pidfile.rs` -- `handle_stale_pid_file` function.

Adds a self-PID check before the SIGTERM path: `if stale_pid == std::process::id()`, skip SIGTERM and reclaim directly.

## Risk Coverage

| Risk | Scenario | Test |
|------|----------|------|
| R-02 (High) | Stale PID file contains current process PID | `test_handle_stale_self_pid_reclaims_without_sigterm` |
| R-02 (High) | PID 1 reuse on container restart | `test_handle_stale_pid_file_self_pid_does_not_self_terminate` |
| R-02 (High) | Non-self PID path unchanged | `test_handle_stale_pid_file_other_pid_still_works` |

## Unit Tests

### test_handle_stale_self_pid_reclaims_without_sigterm

**Arrange**: Create a temp PID file containing the current process PID (`std::process::id()`).

**Act**: Call `handle_stale_pid_file(&path, Duration::from_secs(1))`.

**Assert**:
- Returns `Ok(true)` (resolved successfully)
- PID file still exists (left for PidGuard to reclaim, per #146)
- Process is still alive (no self-SIGTERM occurred -- verify `is_process_alive(std::process::id())` returns true)

This is the critical R-02 test. Without the self-PID guard, this test would cause the test process itself to receive SIGTERM.

### test_handle_stale_pid_file_self_pid_does_not_self_terminate

**Arrange**: Create a temp PID file containing `std::process::id()`.

**Act**: Call `handle_stale_pid_file(&path, Duration::from_secs(1))`.

**Assert**:
- Returns `Ok(true)`
- `is_process_alive(std::process::id())` is still `true` after the call
- The function did NOT enter the `terminate_and_wait` path (verified by process liveness)

### test_handle_stale_pid_file_other_pid_still_works

**Arrange**: Create a temp PID file containing a dead PID (e.g., `4000000`).

**Act**: Call `handle_stale_pid_file(&path, Duration::from_secs(1))`.

**Assert**:
- Returns `Ok(true)` (dead process detected, resolved)
- PID file still exists (per #146)

This verifies the self-PID guard does not interfere with the existing dead-process reclaim path. This test already exists (`test_handle_stale_pid_file_dead_process`) but is listed here to confirm no regression.

## Integration Tests

No new infra-001 tests. PidGuard runs before MCP server initialization. The existing `lifecycle` suite restart persistence tests exercise PidGuard reclamation end-to-end.

## Edge Cases

- **PID file contains 0**: Should return `Ok(true)` -- PID 0 is not alive. Self-PID guard does not trigger (current process PID is never 0).
- **PID file contains current PID with trailing whitespace**: `read_pid_file` trims whitespace before parsing. Self-PID check should still match.
- **Concurrent PidGuard::acquire after self-PID reclaim**: After `handle_stale_pid_file` returns `Ok(true)` for self-PID, `PidGuard::acquire` must succeed on the same path. Existing test `test_pid_guard_acquire_after_handle_stale_no_remove` covers this pattern.
