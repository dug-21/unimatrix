# Pseudocode: pid-guard

## Purpose

Add PidGuard RAII struct for unified PID file lifecycle (flock + write + cleanup on drop), add is_unimatrix_process() for identity verification, and update handle_stale_pid_file() to use identity check before SIGTERM.

## File: crates/unimatrix-server/src/pidfile.rs

### New import

```
use fs2::FileExt;  // provides try_lock_exclusive() on std::fs::File
use std::path::PathBuf;
```

### New struct: PidGuard

```
/// RAII guard for PID file lifecycle: flock + PID write + cleanup on drop.
pub struct PidGuard {
    /// Open file handle holding the exclusive advisory lock.
    file: std::fs::File,
    /// Path to the PID file (for removal on drop).
    path: PathBuf,
}
```

### PidGuard::acquire(path: &Path) -> io::Result<Self>

```
pub fn acquire(path: &Path) -> io::Result<PidGuard>:
    // Create or open the PID file
    file = File::create(path)?     // creates or truncates

    // Acquire non-blocking exclusive flock
    file.try_lock_exclusive()?     // fs2 method; returns Err on EWOULDBLOCK

    // Write current PID to the file
    write!(file, "{}\n", std::process::id())?

    // Flush to ensure PID is visible to other processes
    file.flush()?

    return Ok(PidGuard { file, path: path.to_path_buf() })
```

### PidGuard::drop

```
impl Drop for PidGuard:
    fn drop(&mut self):
        // Remove PID file; log warning on failure (not panic)
        if let Err(e) = fs::remove_file(&self.path):
            if e.kind() != io::ErrorKind::NotFound:
                // Log warning but do not panic in drop
                tracing::warn!(error = %e, path = %self.path.display(), "failed to remove PID file on drop")

        // Lock is released automatically when self.file is dropped (closes fd)
```

### New function: is_unimatrix_process(pid: u32) -> bool

```
/// Check whether a PID belongs to a unimatrix-server process.
///
/// On Linux: reads /proc/{pid}/cmdline and checks for "unimatrix-server".
/// On non-Linux Unix: falls back to kill -0 (existence check only).
/// Returns false for non-existent processes.

#[cfg(target_os = "linux")]
pub fn is_unimatrix_process(pid: u32) -> bool:
    let cmdline_path = format!("/proc/{pid}/cmdline")
    match fs::read(&cmdline_path):
        Ok(bytes):
            if bytes.is_empty():
                return false  // kernel thread or zombie
            // /proc/pid/cmdline is null-separated
            // Check if any argument contains "unimatrix-server"
            // Convert bytes to string, replacing nulls with spaces for matching
            let cmdline = String::from_utf8_lossy(&bytes)
            // Split on null bytes and check each argument
            for arg in cmdline.split('\0'):
                // Match the binary name, handling full paths like /usr/bin/unimatrix-server
                if arg ends with "unimatrix-server" or arg == "unimatrix-server":
                    return true
                // Also check if the arg contains unimatrix-server as a path component
                // Use Path to extract filename
                if Path::new(arg).file_name() == Some("unimatrix-server"):
                    return true
            return false
        Err(_):
            return false  // process exited or /proc not readable

#[cfg(all(unix, not(target_os = "linux")))]
pub fn is_unimatrix_process(pid: u32) -> bool:
    // Fall back to existence check via kill -0
    is_process_alive(pid)

#[cfg(not(unix))]
pub fn is_unimatrix_process(_pid: u32) -> bool:
    false
```

### Modified function: handle_stale_pid_file

Signature unchanged: `pub fn handle_stale_pid_file(pid_path: &Path, terminate_timeout: Duration) -> io::Result<bool>`

```
pub fn handle_stale_pid_file(pid_path, terminate_timeout):
    pid = match read_pid_file(pid_path):
        Some(pid) => pid,
        None => return Ok(true)  // No PID file or unreadable

    if !is_process_alive(pid):
        tracing::info!(pid, "removing stale PID file (process is dead)")
        remove_pid_file(pid_path)
        return Ok(true)

    // Process is alive — check if it's actually unimatrix-server
    if !is_unimatrix_process(pid):
        tracing::info!(pid, "PID is alive but not unimatrix-server; removing stale PID file")
        remove_pid_file(pid_path)
        return Ok(true)

    // Process is alive AND is unimatrix-server — SIGTERM it
    tracing::info!(pid, "stale unimatrix-server process detected, sending SIGTERM")
    if terminate_and_wait(pid, terminate_timeout):
        tracing::info!(pid, "stale process exited after SIGTERM")
        remove_pid_file(pid_path)
        Ok(true)
    else:
        tracing::warn!(pid, "stale process did not exit within timeout")
        Ok(false)
```

## Error Handling

- `PidGuard::acquire` returns `io::Error` on: file create failure, flock failure (EWOULDBLOCK), write failure
- `PidGuard::drop` never panics: logs warnings on removal failure
- `is_unimatrix_process` never panics: returns false on any read failure

## Key Test Scenarios

1. PidGuard::acquire succeeds on a fresh path, PID file contains current PID
2. PidGuard::drop removes the PID file
3. Second PidGuard::acquire on same path fails immediately (non-blocking)
4. PidGuard::drop on already-removed file does not panic
5. is_unimatrix_process returns true for current process (on Linux)
6. is_unimatrix_process returns false for PID 0
7. is_unimatrix_process returns false for non-existent PID
8. handle_stale_pid_file does NOT SIGTERM when PID is alive but not unimatrix-server
