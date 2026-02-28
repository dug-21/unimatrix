# Test Plan: pid-guard

## Unit Tests

All tests in `crates/unimatrix-server/src/pidfile.rs` module `tests`.

### test_pid_guard_acquire_writes_pid

- Arrange: Create tempdir, compute PID file path
- Act: PidGuard::acquire(path)
- Assert: File exists, contents match current PID (std::process::id())
- Risks: None (happy path)

### test_pid_guard_drop_removes_file

- Arrange: Create tempdir, acquire PidGuard
- Act: Drop the PidGuard (let it go out of scope via inner block)
- Assert: PID file no longer exists
- Risks: R-01

### test_pid_guard_drop_already_removed_no_panic

- Arrange: Acquire PidGuard, then manually remove the PID file via fs::remove_file
- Act: Drop the PidGuard
- Assert: No panic (drop handles NotFound gracefully)
- Risks: R-01

### test_pid_guard_second_acquire_fails

- Arrange: Acquire first PidGuard on a path (keep it alive)
- Act: Try PidGuard::acquire on the same path
- Assert: Second acquire returns Err (io::Error)
- Assert: First PidGuard still valid (file still exists with first PID)
- Risks: R-08, R-10 (verifies non-blocking and immediate failure)

### test_pid_guard_acquire_error_propagated

- Arrange: Use a path in a non-existent directory (e.g., /nonexistent/dir/test.pid)
- Act: PidGuard::acquire(path)
- Assert: Returns Err(io::Error) with appropriate kind
- Risks: R-02 (error propagation path)

### test_is_unimatrix_process_current_pid (Linux only)

- Arrange: Get current process ID
- Act: is_unimatrix_process(std::process::id())
- Assert: Returns true (current process IS unimatrix-server in test context -- the test binary contains "unimatrix" in its path)
- Note: In test context, the binary is the test runner. The cmdline will contain the test binary path which includes "unimatrix-server" in the crate path. If this proves unreliable, the test can be adjusted to verify the function reads /proc correctly.
- Risks: R-03

### test_is_unimatrix_process_dead_pid (Linux only)

- Arrange: Use PID 4000000 (very high, unlikely to exist)
- Act: is_unimatrix_process(4000000)
- Assert: Returns false
- Risks: R-03

### test_is_unimatrix_process_pid_zero

- Arrange: PID 0 (kernel)
- Act: is_unimatrix_process(0)
- Assert: Returns false
- Risks: R-03

### test_is_unimatrix_process_pid_one

- Arrange: PID 1 (init/systemd -- definitely not unimatrix-server)
- Act: is_unimatrix_process(1)
- Assert: Returns false
- Risks: R-03

### test_handle_stale_not_unimatrix_removes_without_sigterm

- Arrange: Write PID file with PID 1 (init -- alive but not unimatrix-server)
- Act: handle_stale_pid_file(path, timeout)
- Assert: Returns Ok(true), PID file is removed
- Note: PID 1 is always alive (init) but is_unimatrix_process(1) returns false, so it should be treated as stale
- Risks: R-03, AC-03

### test_handle_stale_dead_process_still_works

- Arrange: Write PID file with PID 4000000 (dead process)
- Act: handle_stale_pid_file(path, timeout)
- Assert: Returns Ok(true), PID file removed
- Note: Verifies existing behavior is preserved after adding identity check
- Risks: R-07 (regression check)

## Existing Tests to Keep

All existing tests in pidfile::tests remain valid:
- test_write_and_read_pid_file
- test_read_missing_pid_file_returns_none
- test_read_invalid_pid_file_returns_none
- test_read_empty_pid_file_returns_none
- test_remove_pid_file
- test_remove_nonexistent_pid_file_is_silent
- test_current_process_is_alive
- test_dead_pid_is_not_alive
- test_handle_stale_pid_file_no_file
- test_handle_stale_pid_file_dead_process
- test_handle_stale_pid_file_invalid_contents
- test_write_pid_file_overwrites
