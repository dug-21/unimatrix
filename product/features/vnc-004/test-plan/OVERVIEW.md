# Test Plan Overview: vnc-004 Server Process Reliability

## Test Strategy

All tests are unit tests within the `unimatrix-server` crate. No new integration test suites are needed -- the changes are internal process lifecycle fixes that don't alter MCP protocol behavior.

Integration smoke tests and the `protocol` + `tools` suites will be run in Stage 3c to verify no regressions.

## Risk-to-Test Mapping

| Risk ID | Risk Description | Component | Test Functions | Priority |
|---------|-----------------|-----------|---------------|----------|
| R-01 | PidGuard drop fails to remove PID file | pid-guard | test_pid_guard_drop_removes_file, test_pid_guard_drop_already_removed_no_panic | Medium |
| R-02 | flock not supported on filesystem | pid-guard | test_pid_guard_acquire_error_propagated | Medium |
| R-03 | /proc/cmdline parsing edge cases | pid-guard | test_is_unimatrix_process_* (5 tests) | Medium |
| R-04 | Session timeout kills active session | session-timeout | (documented assumption -- rmcp session future behavior) | Low |
| R-05 | DatabaseLocked error message leaks paths | error-path | test_database_locked_display, test_database_locked_error_data | Low |
| R-06 | Poisoned RwLock recovery | poison-recovery | test_poison_recovery_* (4 tests) | Low |
| R-07 | PidGuard and graceful_shutdown race | pid-guard | test_remove_nonexistent_pid_file_is_silent (existing) | Low |
| R-08 | PID recycling window | pid-guard | test_pid_guard_second_acquire_fails | Medium |
| R-09 | Different project dirs sharing PID file | pid-guard | (existing test_hash_different_paths covers this) | Low |
| R-10 | flock blocks indefinitely without LOCK_NB | pid-guard | test_pid_guard_second_acquire_fails (verifies non-blocking) | Medium |

## Cross-Component Test Dependencies

- pid-guard tests require `fs2` crate (dev-dependencies already has `tempfile`)
- error-path tests are self-contained (no filesystem needed)
- poison-recovery tests use std::panic::catch_unwind to poison the lock
- session-timeout has no unit-testable behavior (timeout is tested via integration)

## AC Verification Plan

| AC-ID | Verification | Test/Method |
|-------|-------------|-------------|
| AC-01 | grep -r "process::exit" crates/unimatrix-server/src/ returns no matches | Stage 3c grep check |
| AC-02 | PID file always cleaned up | test_pid_guard_drop_removes_file, test_pid_guard_drop_on_scope_exit |
| AC-03 | Identity verification before SIGTERM | test_handle_stale_not_unimatrix_removes_without_sigterm |
| AC-04 | Advisory lock prevents TOCTOU | test_pid_guard_second_acquire_fails |
| AC-05 | Session timeout triggers shutdown | Integration test (Stage 3c) |
| AC-06 | No panics from lock poisoning | test_poison_recovery_validate, test_poison_recovery_add_category, test_poison_recovery_list_categories |
