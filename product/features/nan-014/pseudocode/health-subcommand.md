# health-subcommand: Health CLI Subcommand

## Purpose

Check daemon liveness by connecting to the MCP UDS socket. Used by Docker `HEALTHCHECK` directive. Sync path (no tokio runtime) per procedure #1192 and ADR-003. Exit 0 = healthy, exit 1 = unhealthy.

## New File

**File**: `crates/unimatrix-server/src/health.rs`

### Module Registration

Add `pub mod health;` in `crates/unimatrix-server/src/lib.rs` alongside existing `pub mod export;`, `pub mod import;`.

### Function Signature

```
/// Run the health check: resolve ProjectPaths, connect to MCP UDS socket.
/// Exit 0 on success, exit 1 on failure. Sync path, no tokio runtime.
///
/// Follows sync CLI subcommand pattern (procedure #1192):
/// - No tokio runtime
/// - Uses ensure_data_directory for path resolution
/// - Returns Result; caller in main.rs handles exit code
pub fn run(project_dir: Option<&Path>) -> Result<(), Box<dyn std::error::Error>>
```

### Pseudocode

```
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use crate::project;

/// Connect timeout for the MCP UDS socket health check.
const HEALTH_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

pub fn run(project_dir: Option<&Path>) -> Result<(), Box<dyn std::error::Error>>:
    // Step 1: Resolve ProjectPaths using the same function as serve --foreground.
    // In the container: project_dir = Some("/data"), HOME = "/data".
    // Produces the same mcp_socket_path as the daemon (SR-11 mitigation).
    let paths = project::ensure_data_directory(project_dir, None)
        .map_err(|e| format!("failed to resolve project paths: {e}"))?

    let socket_path = &paths.mcp_socket_path

    // Step 2: Check if socket file exists on the filesystem.
    if not socket_path.exists():
        eprintln!("unhealthy: MCP socket not found at {}", socket_path.display())
        std::process::exit(1)

    // Step 3: Attempt sync UDS connect with timeout.
    // UnixStream does not have a connect_timeout method in std.
    // Use set_nonblocking + connect + poll pattern, or accept that
    // connect() on a UDS is effectively instantaneous (local IPC).
    //
    // For robustness: set a 3-second overall timeout.
    // Implementation approach: spawn a thread for the connect,
    // join with timeout. Or use nix::sys::socket for SO_RCVTIMEO.
    //
    // Simplest correct approach: UnixStream::connect is blocking
    // but for local sockets returns immediately (accept or ECONNREFUSED).
    // The 3s timeout guards against a socket file that exists but
    // the daemon's accept loop is stalled (backpressure).
    match connect_with_timeout(socket_path, HEALTH_CONNECT_TIMEOUT):
        Ok(_stream):
            // Connection succeeded — daemon accept loop is responsive.
            // Drop the stream immediately (no data exchange needed).
            // Print nothing to stdout on success (FR-5.7).
            Ok(())
        Err(e):
            eprintln!("unhealthy: cannot connect to MCP socket at {}: {e}", socket_path.display())
            std::process::exit(1)


/// Connect to a Unix socket with a timeout.
///
/// Uses a background thread + join_timeout approach since std::os::unix::net::UnixStream
/// does not expose connect_timeout directly.
fn connect_with_timeout(path: &Path, timeout: Duration) -> std::io::Result<UnixStream>:
    let path_owned = path.to_path_buf()

    let handle = std::thread::spawn(move ||:
        UnixStream::connect(&path_owned)
    )

    match handle.join_timeout(timeout):  // Note: join does not have timeout in std.
        // IMPLEMENTATION NOTE: std::thread::JoinHandle does not have join_timeout.
        // Alternative approaches:
        //   (a) Use crossbeam::thread or std::sync::mpsc with recv_timeout.
        //   (b) Use nix to set SO_SNDTIMEO on the socket after connect.
        //   (c) Accept that UDS connect is instantaneous and just call
        //       UnixStream::connect directly without a timeout wrapper.
        //
        // RECOMMENDED: approach (c). Unix domain socket connect() to a
        // listening socket either succeeds immediately or fails with
        // ECONNREFUSED. The 3s timeout in the HEALTHCHECK directive
        // (--timeout=5s) provides the outer guard. The connect itself
        // does not block for seconds on a local socket.
        //
        // Use direct connect:
    UnixStream::connect(path)
```

### Recommended Simplified Implementation

Given that UDS connect is effectively instantaneous:

```
pub fn run(project_dir: Option<&Path>) -> Result<(), Box<dyn std::error::Error>>:
    let paths = project::ensure_data_directory(project_dir, None)
        .map_err(|e| format!("failed to resolve project paths: {e}"))?

    let socket_path = &paths.mcp_socket_path

    if not socket_path.exists():
        eprintln!("unhealthy: MCP socket not found at {}", socket_path.display())
        std::process::exit(1)

    match UnixStream::connect(socket_path):
        Ok(_):
            // Healthy. No stdout output (FR-5.7).
            Ok(())
        Err(e):
            eprintln!("unhealthy: {e}")
            std::process::exit(1)
```

## Modified File: main.rs

### Command Enum Addition

```
enum Command {
    // ... existing variants ...

    /// Check daemon liveness via UDS socket connect.
    Health,
}
```

### Match Arm Addition

Add in the sync dispatch section of `main()`, alongside Hook, Export, etc.:

```
Some(Command::Health):
    // Sync path: NO tokio (procedure #1192).
    // run() returns Ok on success (exit 0). On failure, run() calls
    // process::exit(1) internally after printing diagnostic to stderr.
    return unimatrix_server::health::run(cli.project_dir.as_deref())
```

## Error Handling

| Condition | Behavior |
|-----------|----------|
| ProjectPaths resolution fails | Error message to stderr, process exits with error via `?` propagation |
| Socket file does not exist | `eprintln!("unhealthy: ...")`, `exit(1)` |
| Socket exists but connect refused | `eprintln!("unhealthy: ...")`, `exit(1)` |
| Socket exists, connect succeeds | Return `Ok(())`, process exits 0 |

No panics. No `.unwrap()`. The health check is a fire-and-forget binary probe.

## Key Test Scenarios

1. **Socket exists, daemon running**: Create a real UDS listener on a temp path. Set up ProjectPaths pointing to it. Call `health::run`. Assert returns `Ok(())`.

2. **Socket missing**: Point ProjectPaths at a nonexistent socket path. Call `health::run`. Assert it calls `process::exit(1)`. (Test via a subprocess or by refactoring to return an error enum instead of calling exit directly.)

3. **Socket exists, no listener (ECONNREFUSED)**: Create the socket file but no listener. Call `health::run`. Assert failure (exit 1).

4. **Path consistency with serve --foreground**: Call `ensure_data_directory(Some("/tmp/test-data"), None)` twice. Assert `mcp_socket_path` is identical both times. This validates SR-11: health and serve resolve the same socket.

5. **Container path resolution**: With `HOME=/tmp/fake-home`, call `ensure_data_directory(Some("/tmp/test-data"), None)`. Assert `mcp_socket_path` is under `/tmp/fake-home/.unimatrix/{hash}/`. This validates the container path chain.

### Testability Note

The `std::process::exit(1)` calls make unit testing difficult. The implementation agent should consider either:
- (a) Returning a result enum and having the main.rs match arm call `exit(1)` — matches the `run_stop` pattern.
- (b) Using `#[cfg(test)]` to skip the exit in tests.

Option (a) is recommended for consistency with `run_stop`:

```
pub fn run(project_dir: Option<&Path>) -> i32:
    // ... logic ...
    // Return 0 for healthy, 1 for unhealthy.
```

Then in main.rs:
```
Some(Command::Health):
    let code = unimatrix_server::health::run(cli.project_dir.as_deref())
    std::process::exit(code)
```
