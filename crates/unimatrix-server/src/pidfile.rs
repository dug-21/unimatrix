//! PID file management for single-instance enforcement.
//!
//! Provides functions to write, read, and remove a PID file, detect stale
//! processes, and terminate them so a new server instance can acquire the
//! database lock.
//!
//! `PidGuard` provides RAII-based lifecycle management: advisory locking (flock),
//! PID file write, and cleanup on drop.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;

/// RAII guard for PID file lifecycle: flock + PID write + cleanup on drop.
///
/// Acquires an exclusive advisory lock on the PID file, writes the current
/// process ID, and removes the file when dropped. The lock is released
/// automatically when the file handle closes (on drop, process exit, or SIGKILL).
pub struct PidGuard {
    /// Open file handle holding the exclusive advisory lock.
    _file: File,
    /// Path to the PID file (for removal on drop).
    path: PathBuf,
}

impl PidGuard {
    /// Acquire an exclusive advisory lock on the PID file and write the current PID.
    ///
    /// Uses non-blocking `flock(LOCK_EX | LOCK_NB)` via the `fs2` crate.
    /// Returns `Err` if the lock is held by another process or on I/O failure.
    pub fn acquire(path: &Path) -> io::Result<Self> {
        let mut file = File::create(path)?;
        file.try_lock_exclusive()?;
        write!(file, "{}\n", std::process::id())?;
        file.flush()?;
        Ok(PidGuard {
            _file: file,
            path: path.to_path_buf(),
        })
    }
}

impl Drop for PidGuard {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.path) {
            if e.kind() != io::ErrorKind::NotFound {
                tracing::warn!(
                    error = %e,
                    path = %self.path.display(),
                    "failed to remove PID file on drop"
                );
            }
        }
        // Lock is released automatically when self._file is dropped (fd closes).
    }
}

/// Write the current process ID to a PID file.
///
/// Creates or overwrites the file at `path` with the current PID followed by a
/// newline. The write is not atomic (the file is small and loss is harmless).
pub fn write_pid_file(path: &Path) -> io::Result<()> {
    let pid = std::process::id();
    fs::write(path, format!("{pid}\n"))
}

/// Read a PID from a PID file.
///
/// Returns `None` if the file does not exist, is empty, or contains a
/// non-numeric value.
pub fn read_pid_file(path: &Path) -> Option<u32> {
    let contents = fs::read_to_string(path).ok()?;
    contents.trim().parse::<u32>().ok()
}

/// Remove a PID file if it exists.
///
/// Silently ignores "not found" errors.
pub fn remove_pid_file(path: &Path) {
    if let Err(e) = fs::remove_file(path)
        && e.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(error = %e, path = %path.display(), "failed to remove PID file");
    }
}

/// Check whether a process with the given PID is alive.
///
/// Uses `kill(pid, 0)` on Unix. Returns `false` on non-Unix platforms.
#[cfg(unix)]
pub fn is_process_alive(pid: u32) -> bool {
    // SAFETY: `libc::kill` with signal 0 only checks existence — it sends no
    // signal. However, this module lives inside a `#![forbid(unsafe_code)]`
    // crate, so we use the nix crate approach via std::process::Command
    // instead: send signal 0 via the `kill` command.
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub fn is_process_alive(_pid: u32) -> bool {
    // Cannot check process liveness portably; assume dead so the retry loop
    // handles the lock conflict if the process is actually alive.
    false
}

/// Check whether a PID belongs to a unimatrix-server process.
///
/// On Linux: reads `/proc/{pid}/cmdline` and checks if any argument's
/// filename is `unimatrix-server`.
/// On non-Linux Unix: falls back to `kill -0` (existence check only).
/// Returns `false` for non-existent processes.
#[cfg(target_os = "linux")]
pub fn is_unimatrix_process(pid: u32) -> bool {
    let cmdline_path = format!("/proc/{pid}/cmdline");
    let bytes = match fs::read(&cmdline_path) {
        Ok(b) => b,
        Err(_) => return false, // process exited or /proc not readable
    };

    if bytes.is_empty() {
        return false; // kernel thread or zombie
    }

    // /proc/pid/cmdline is null-separated. Check each argument's filename.
    let cmdline = String::from_utf8_lossy(&bytes);
    for arg in cmdline.split('\0') {
        if arg.is_empty() {
            continue;
        }
        // Extract the filename component from the argument (handles full paths
        // like /usr/bin/unimatrix-server).
        if let Some(name) = Path::new(arg).file_name() {
            if name == "unimatrix-server" {
                return true;
            }
        }
    }

    false
}

#[cfg(all(unix, not(target_os = "linux")))]
pub fn is_unimatrix_process(pid: u32) -> bool {
    // Fall back to existence check — no /proc on macOS/BSD.
    is_process_alive(pid)
}

#[cfg(not(unix))]
pub fn is_unimatrix_process(_pid: u32) -> bool {
    false
}

/// Send SIGTERM to a process and wait for it to exit.
///
/// Polls `is_process_alive` every 250 ms up to `timeout`. Returns `true` if
/// the process exited within the timeout, `false` otherwise.
///
/// On non-Unix platforms this is a no-op that returns `false`.
#[cfg(unix)]
pub fn terminate_and_wait(pid: u32, timeout: std::time::Duration) -> bool {
    // Send SIGTERM via the `kill` command (avoids unsafe libc call).
    let sent = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !sent {
        // Could not send signal — process may already be gone.
        return !is_process_alive(pid);
    }

    let start = std::time::Instant::now();
    let poll_interval = std::time::Duration::from_millis(250);

    while start.elapsed() < timeout {
        if !is_process_alive(pid) {
            return true;
        }
        std::thread::sleep(poll_interval);
    }

    !is_process_alive(pid)
}

#[cfg(not(unix))]
pub fn terminate_and_wait(_pid: u32, _timeout: std::time::Duration) -> bool {
    false
}

/// Handle a stale PID file found at startup.
///
/// If the PID file exists:
/// - If the recorded process is dead, removes the stale PID file.
/// - If the recorded process is alive, sends SIGTERM and waits up to
///   `terminate_timeout` for it to exit.
///
/// Returns `Ok(true)` if the stale process was resolved (dead or terminated),
/// `Ok(false)` if the process is still alive after the timeout, or an `Err`
/// on I/O failures unrelated to "not found".
pub fn handle_stale_pid_file(
    pid_path: &Path,
    terminate_timeout: std::time::Duration,
) -> io::Result<bool> {
    let pid = match read_pid_file(pid_path) {
        Some(pid) => pid,
        None => return Ok(true), // No PID file or unreadable — nothing to do.
    };

    if !is_process_alive(pid) {
        tracing::info!(pid, "removing stale PID file (process is dead)");
        remove_pid_file(pid_path);
        return Ok(true);
    }

    // Process is alive — verify it is actually a unimatrix-server instance
    // before sending SIGTERM. If it is not, treat the PID file as stale.
    if !is_unimatrix_process(pid) {
        tracing::info!(
            pid,
            "PID is alive but not unimatrix-server; removing stale PID file"
        );
        remove_pid_file(pid_path);
        return Ok(true);
    }

    tracing::info!(pid, "stale unimatrix-server process detected, sending SIGTERM");
    if terminate_and_wait(pid, terminate_timeout) {
        tracing::info!(pid, "stale process exited after SIGTERM");
        remove_pid_file(pid_path);
        Ok(true)
    } else {
        tracing::warn!(pid, "stale process did not exit within timeout");
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_read_pid_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.pid");

        write_pid_file(&path).unwrap();
        let pid = read_pid_file(&path);
        assert_eq!(pid, Some(std::process::id()));
    }

    #[test]
    fn test_read_missing_pid_file_returns_none() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.pid");
        assert_eq!(read_pid_file(&path), None);
    }

    #[test]
    fn test_read_invalid_pid_file_returns_none() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bad.pid");
        fs::write(&path, "not-a-number\n").unwrap();
        assert_eq!(read_pid_file(&path), None);
    }

    #[test]
    fn test_read_empty_pid_file_returns_none() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("empty.pid");
        fs::write(&path, "").unwrap();
        assert_eq!(read_pid_file(&path), None);
    }

    #[test]
    fn test_remove_pid_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.pid");
        fs::write(&path, "12345\n").unwrap();
        assert!(path.exists());

        remove_pid_file(&path);
        assert!(!path.exists());
    }

    #[test]
    fn test_remove_nonexistent_pid_file_is_silent() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.pid");
        // Should not panic or error.
        remove_pid_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn test_current_process_is_alive() {
        assert!(is_process_alive(std::process::id()));
    }

    #[cfg(unix)]
    #[test]
    fn test_dead_pid_is_not_alive() {
        // PID 0 is special (kernel); use a very high PID unlikely to exist.
        // The kill -0 command will fail for a nonexistent PID.
        assert!(!is_process_alive(4_000_000));
    }

    #[test]
    fn test_handle_stale_pid_file_no_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("no.pid");
        let result =
            handle_stale_pid_file(&path, std::time::Duration::from_secs(1)).unwrap();
        assert!(result);
    }

    #[test]
    fn test_handle_stale_pid_file_dead_process() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("stale.pid");
        // Write a PID that definitely doesn't exist.
        fs::write(&path, "4000000\n").unwrap();

        let result =
            handle_stale_pid_file(&path, std::time::Duration::from_secs(1)).unwrap();
        assert!(result);
        // PID file should have been removed.
        assert!(!path.exists());
    }

    #[test]
    fn test_handle_stale_pid_file_invalid_contents() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bad.pid");
        fs::write(&path, "garbage\n").unwrap();

        let result =
            handle_stale_pid_file(&path, std::time::Duration::from_secs(1)).unwrap();
        assert!(result);
    }

    #[test]
    fn test_write_pid_file_overwrites() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.pid");
        fs::write(&path, "99999\n").unwrap();

        write_pid_file(&path).unwrap();
        let pid = read_pid_file(&path);
        assert_eq!(pid, Some(std::process::id()));
    }

    // --- PidGuard tests ---

    #[test]
    fn test_pid_guard_acquire_writes_pid() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.pid");

        let _guard = PidGuard::acquire(&path).unwrap();

        // File should exist and contain current PID.
        let contents = fs::read_to_string(&path).unwrap();
        let pid: u32 = contents.trim().parse().unwrap();
        assert_eq!(pid, std::process::id());
    }

    #[test]
    fn test_pid_guard_drop_removes_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.pid");

        {
            let _guard = PidGuard::acquire(&path).unwrap();
            assert!(path.exists());
        }
        // Guard dropped — file should be removed.
        assert!(!path.exists());
    }

    #[test]
    fn test_pid_guard_drop_already_removed_no_panic() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.pid");

        let guard = PidGuard::acquire(&path).unwrap();
        // Manually remove the file before drop.
        fs::remove_file(&path).unwrap();
        // Drop should not panic (handles NotFound gracefully).
        drop(guard);
    }

    #[test]
    fn test_pid_guard_second_acquire_fails() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.pid");

        let _first = PidGuard::acquire(&path).unwrap();
        // Second acquire on the same path should fail immediately (non-blocking).
        let result = PidGuard::acquire(&path);
        assert!(result.is_err(), "second acquire should fail");
    }

    #[test]
    fn test_pid_guard_acquire_error_on_bad_path() {
        let path = Path::new("/nonexistent-dir-xyz/impossible/test.pid");
        let result = PidGuard::acquire(path);
        assert!(result.is_err(), "acquire on bad path should fail");
    }

    // --- is_unimatrix_process tests ---

    #[cfg(target_os = "linux")]
    #[test]
    fn test_is_unimatrix_process_dead_pid() {
        // Very high PID unlikely to exist.
        assert!(!is_unimatrix_process(4_000_000));
    }

    #[test]
    fn test_is_unimatrix_process_pid_zero() {
        // PID 0 is kernel — not unimatrix-server.
        assert!(!is_unimatrix_process(0));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_is_unimatrix_process_pid_one() {
        // PID 1 is init/systemd — definitely not unimatrix-server.
        assert!(!is_unimatrix_process(1));
    }

    // --- handle_stale_pid_file identity check tests ---

    #[cfg(unix)]
    #[test]
    fn test_handle_stale_not_unimatrix_removes_without_sigterm() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("stale.pid");
        // PID 1 is init — alive but not unimatrix-server.
        fs::write(&path, "1\n").unwrap();

        let result =
            handle_stale_pid_file(&path, std::time::Duration::from_secs(1)).unwrap();
        assert!(result, "should resolve stale PID for non-unimatrix process");
        assert!(!path.exists(), "PID file should be removed");
    }

    #[test]
    fn test_handle_stale_dead_process_still_works() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("stale.pid");
        // Dead process — same as existing test but verifies no regression after
        // adding identity check.
        fs::write(&path, "4000000\n").unwrap();

        let result =
            handle_stale_pid_file(&path, std::time::Duration::from_secs(1)).unwrap();
        assert!(result);
        assert!(!path.exists());
    }
}
