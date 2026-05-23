//! Health check CLI subcommand.
//!
//! Checks daemon liveness by connecting to the MCP UDS socket.
//! Used by Docker `HEALTHCHECK` directive. Sync path (no tokio runtime)
//! per ADR-003. Exit 0 = healthy, exit 1 = unhealthy.

use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::project;

/// Run the health check: resolve ProjectPaths, connect to MCP UDS socket.
///
/// Returns 0 on success (healthy), 1 on failure (unhealthy).
/// Follows the `run_stop` pattern for testability — caller in main.rs
/// calls `std::process::exit(code)`.
///
/// Sync path, no tokio runtime (ADR-003, procedure #1192).
pub fn run(project_dir: Option<&Path>) -> i32 {
    // Step 1: Resolve ProjectPaths using the same function as serve.
    // In the container: project_dir = Some("/data"), HOME = "/data".
    // Produces the same mcp_socket_path as the daemon (SR-11 / R-03 mitigation).
    let paths = match project::ensure_data_directory(project_dir, None) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("unhealthy: failed to resolve project paths: {e}");
            return 1;
        }
    };

    let socket_path = &paths.mcp_socket_path;

    // Step 2: Check if socket file exists on the filesystem.
    if !socket_path.exists() {
        eprintln!(
            "unhealthy: MCP socket not found at {}",
            socket_path.display()
        );
        return 1;
    }

    // Step 3: Attempt sync UDS connect.
    // Unix domain socket connect() to a listening socket either succeeds
    // immediately or fails with ECONNREFUSED. The HEALTHCHECK directive's
    // --timeout=5s provides the outer guard. No additional timeout wrapper
    // is needed for local socket connect (ADR-003).
    match UnixStream::connect(socket_path) {
        Ok(_stream) => {
            // Connection succeeded — daemon accept loop is responsive.
            // Drop the stream immediately (no data exchange needed).
            // No stdout output on success (FR-5.7).
            0
        }
        Err(e) => {
            eprintln!(
                "unhealthy: cannot connect to MCP socket at {}: {e}",
                socket_path.display()
            );
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    /// Test: health returns 1 when no socket file exists (no daemon running).
    #[test]
    fn test_health_returns_error_when_no_socket() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Pass a temp dir as project_dir — no daemon is running, no socket file.
        let code = run(Some(tmp.path()));
        assert_eq!(code, 1, "health must return 1 when no socket exists");
    }

    /// Test: socket path is deterministic — same inputs produce the same path (R-03).
    #[test]
    fn test_health_socket_path_matches_serve() {
        let tmp = tempfile::TempDir::new().unwrap();
        let paths_a = project::ensure_data_directory(Some(tmp.path()), None).unwrap();
        let paths_b = project::ensure_data_directory(Some(tmp.path()), None).unwrap();
        assert_eq!(
            paths_a.mcp_socket_path, paths_b.mcp_socket_path,
            "both calls must resolve the same mcp_socket_path (R-03)"
        );
    }

    /// Test: health returns 0 when a live UDS listener is accepting connections.
    #[test]
    fn test_health_run_success_on_live_socket() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tempfile::TempDir::new().unwrap();
        let paths = project::ensure_data_directory(Some(tmp.path()), Some(base.path())).unwrap();

        // Spawn a UnixListener at the expected mcp_socket_path.
        let listener = UnixListener::bind(&paths.mcp_socket_path).unwrap();

        // Accept connections in a background thread so the connect succeeds.
        let handle = std::thread::spawn(move || {
            let _conn = listener.accept();
        });

        let code = run_with_base(tmp.path(), base.path());
        assert_eq!(code, 0, "health must return 0 when daemon socket is live");

        // Clean up the listener thread.
        let _ = handle.join();
    }

    /// Test: health returns 1 when socket file exists but no listener is bound
    /// (simulates a stale socket or nonresponsive daemon).
    #[test]
    fn test_health_run_timeout_on_nonresponsive_socket() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tempfile::TempDir::new().unwrap();
        let paths = project::ensure_data_directory(Some(tmp.path()), Some(base.path())).unwrap();

        // Create a regular file at the socket path (not an actual socket).
        // connect() will fail with an appropriate error.
        std::fs::write(&paths.mcp_socket_path, b"not-a-socket").unwrap();

        let code = run_with_base(tmp.path(), base.path());
        assert_eq!(
            code, 1,
            "health must return 1 when socket exists but connect fails"
        );
    }

    /// Helper: run health check with explicit base_dir for test isolation.
    ///
    /// The public `run()` function uses `ensure_data_directory(project_dir, None)`
    /// which defaults base_dir to `~/.unimatrix`. Tests need an isolated base_dir
    /// to avoid interference with the real installation.
    fn run_with_base(project_dir: &Path, base_dir: &Path) -> i32 {
        let paths = match project::ensure_data_directory(Some(project_dir), Some(base_dir)) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("unhealthy: failed to resolve project paths: {e}");
                return 1;
            }
        };

        let socket_path = &paths.mcp_socket_path;

        if !socket_path.exists() {
            eprintln!(
                "unhealthy: MCP socket not found at {}",
                socket_path.display()
            );
            return 1;
        }

        match UnixStream::connect(socket_path) {
            Ok(_stream) => 0,
            Err(e) => {
                eprintln!(
                    "unhealthy: cannot connect to MCP socket at {}: {e}",
                    socket_path.display()
                );
                1
            }
        }
    }
}
