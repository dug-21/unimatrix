# pidguard-self-pid: PidGuard Self-PID Guard

## Purpose

Prevent self-termination when `handle_stale_pid_file` encounters a PID file containing the current process's own PID. This happens deterministically on container restart: the stale PID file on the named volume says `1`, and the new container's PID 1 IS the new unimatrix process. Without this guard, the code reads PID 1, confirms it is unimatrix via `/proc/1/cmdline`, and sends SIGTERM to itself.

General correctness fix (not container-specific) per ADR-007.

## Modified Function

**File**: `crates/unimatrix-server/src/infra/pidfile.rs`

**Function**: `handle_stale_pid_file`

**Current signature** (unchanged):
```
pub fn handle_stale_pid_file(pid_path: &Path, terminate_timeout: Duration) -> io::Result<bool>
```

### Pseudocode

```
pub fn handle_stale_pid_file(pid_path, terminate_timeout):
    pid = read_pid_file(pid_path)
    if pid is None:
        return Ok(true)  // No PID file — nothing to do

    // NEW: Self-PID guard (ADR-007).
    // If the stale PID equals our own PID, the file references us.
    // This happens on container restart where PID 1 is always reused.
    // A process cannot be "stale" relative to itself — reclaim directly.
    // Placed BEFORE is_process_alive to avoid any signal to self.
    if pid == std::process::id():
        tracing::info!(pid, "stale PID file references current process; reclaiming")
        return Ok(true)  // PidGuard::acquire will flock and overwrite

    // --- existing code below, unchanged ---
    if not is_process_alive(pid):
        tracing::info!(pid, "stale PID file found (process is dead); PidGuard will reclaim")
        return Ok(true)

    if not is_unimatrix_process(pid):
        tracing::info!(pid, "PID is alive but not unimatrix; PidGuard will reclaim")
        return Ok(true)

    tracing::info!(pid, "stale unimatrix process detected, sending SIGTERM")
    if terminate_and_wait(pid, terminate_timeout):
        tracing::info!(pid, "stale process exited after SIGTERM; PidGuard will reclaim")
        Ok(true)
    else:
        tracing::warn!(pid, "stale process did not exit within timeout")
        Ok(false)
```

### Key Detail

The self-PID check is placed BEFORE `is_process_alive(pid)`. This ordering matters because:
1. `is_process_alive(our_pid)` returns `true` (we are alive).
2. `is_unimatrix_process(our_pid)` returns `true` (we ARE unimatrix).
3. The code would then SIGTERM our own PID — self-termination.

By checking `pid == std::process::id()` first, we short-circuit before any of those checks run.

## Error Handling

No new error paths. The function's return type (`io::Result<bool>`) is unchanged. The self-PID case returns `Ok(true)` (resolved), same as the "process is dead" case.

## Key Test Scenarios

1. **Self-PID detection**: Write a PID file containing `std::process::id()`. Call `handle_stale_pid_file`. Assert it returns `Ok(true)` without sending any signal. Verify the function does NOT call `is_unimatrix_process` or `terminate_and_wait` (the short-circuit means the process is not killed).

2. **PID 1 simulation**: Write `1` to a PID file. In a test where `std::process::id() != 1`, this falls through to the existing `is_process_alive` check (PID 1 is init, alive but not unimatrix). When `std::process::id() == 1` (container), the self-PID guard triggers.

3. **Non-self PID still works**: Write a non-existent PID (e.g., 99999) to the file. Call `handle_stale_pid_file`. Assert it returns `Ok(true)` via the `is_process_alive` dead-process path. Confirms the guard does not interfere with normal stale-PID handling.

4. **Regression: existing daemon tests pass unchanged**: The `--daemon` path calls `handle_stale_pid_file` during startup. All existing daemon integration tests must pass with zero modifications.
