# Pseudocode: Bridge Client (`bridge.rs`)

## Purpose

Implement the default no-subcommand path: a thin stdio-to-UDS byte forwarder that
connects Claude Code's stdio pipe to the running daemon's MCP socket. Contains the
auto-start sequence (FR-05) when no daemon is reachable. After connection, does only
bidirectional byte copy — no MCP parsing, no application logic.

---

## Files Affected

- **New**: `crates/unimatrix-server/src/bridge.rs`
- **Modified**: `crates/unimatrix-server/src/lib.rs` — add `pub mod bridge;`

---

## Dependencies

- `tokio::net::UnixStream` — connect to daemon socket
- `tokio::io::copy_bidirectional` — byte forwarding
- `tokio::io::{stdin, stdout}` — async stdio handles
- `std::path::Path` — socket path
- `std::process::Command` — spawn daemon for auto-start
- `std::env::current_exe()` — daemon binary path
- `std::thread::sleep` + `std::time::{Duration, Instant}` — poll loop (sync, not async)
- `unimatrix_engine::project::ProjectPaths` — paths.mcp_socket_path, paths.data_dir
- `crate::infra::pidfile::{read_pid_file, is_unimatrix_process}` — stale PID detection
- `crate::error::ServerError` — error type

---

## Constants

```
BRIDGE_CONNECT_RETRY_INTERVAL: Duration = 250ms
BRIDGE_CONNECT_TIMEOUT: Duration = 5s
BRIDGE_STALE_RETRY_DELAY: Duration = 500ms    // one retry if PID is healthy but no socket
```

---

## Function: `run_bridge`

### Signature

```
pub async fn run_bridge(paths: &ProjectPaths) -> Result<(), ServerError>
```

### Pseudocode

```
async fn run_bridge(paths: &ProjectPaths) -> Result<(), ServerError>:

    // Step 1: Try connecting to the daemon socket directly
    match try_connect(&paths.mcp_socket_path).await:
        Ok(stream) =>
            // Fast path: daemon already running, bridge immediately
            return do_bridge(stream).await
        Err(_) =>
            // Slow path: auto-start sequence
            ()

    // Step 2: Check if a healthy daemon is running (PID file check)
    // This prevents double-spawn (AC-06, FR-05 step 1)
    let pid_opt = read_pid_file(&paths.pid_path)
    if let Some(pid) = pid_opt:
        if is_unimatrix_process(pid):
            // Daemon is alive but socket not available — transient state.
            // Wait 500ms and retry once before attempting spawn.
            sleep(BRIDGE_STALE_RETRY_DELAY)
            match try_connect(&paths.mcp_socket_path).await:
                Ok(stream) => return do_bridge(stream).await
                Err(_) => ()
            // If still not available, fall through to spawn anyway
            // (socket may have been transiently unavailable)

    // Step 3: Spawn a new daemon (auto-start)
    log_path = paths.data_dir.join("unimatrix.log")
    spawn_daemon(&paths.mcp_socket_path, &paths.project_root, &log_path)?

    // Step 4: Poll for socket appearance
    start = Instant::now()
    loop:
        if paths.mcp_socket_path.exists():
            // Socket file exists — but may not be listening yet (ECONNREFUSED window)
            // Try connecting; ECONNREFUSED is retried, not treated as permanent failure
            match try_connect(&paths.mcp_socket_path).await:
                Ok(stream) => return do_bridge(stream).await
                Err(_) => ()   // ECONNREFUSED: socket not listening yet, continue polling

        if start.elapsed() >= BRIDGE_CONNECT_TIMEOUT:
            break

        sleep(BRIDGE_CONNECT_RETRY_INTERVAL)

    // Step 5: Timeout — emit actionable diagnostic
    Err(ServerError::ProjectInit(format!(
        "unimatrix daemon did not start within {}s\n\
         Check the daemon log for errors: {}\n\
         To start manually: unimatrix serve --daemon",
        BRIDGE_CONNECT_TIMEOUT.as_secs(),
        log_path.display()
    )))
```

---

## Function: `try_connect` (private)

### Pseudocode

```
async fn try_connect(socket_path: &Path) -> Result<UnixStream, io::Error>:
    tokio::net::UnixStream::connect(socket_path).await
```

---

## Function: `spawn_daemon` (private, synchronous)

### Pseudocode

```
fn spawn_daemon(
    mcp_socket_path: &Path,
    project_root: &Path,
    log_path: &Path,
) -> Result<(), ServerError>:

    exe_path = std::env::current_exe()
        .map_err(|e| ServerError::ProjectInit(format!("cannot find unimatrix binary: {e}")))?

    log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| ServerError::ProjectInit(format!("cannot open daemon log: {e}")))?

    // Build args: "serve" "--daemon" ["--project-dir" <dir>]
    // Always pass --project-dir to ensure the child initializes for the same project.
    child = std::process::Command::new(&exe_path)
        .arg("serve")
        .arg("--daemon")
        .arg("--project-dir")
        .arg(project_root)
        .stdin(Stdio::null())
        .stdout(log_file.try_clone()
            .map_err(|e| ServerError::ProjectInit(format!("log file clone: {e}")))?)
        .stderr(log_file)
        .spawn()
        .map_err(|e| ServerError::ProjectInit(format!("failed to spawn daemon: {e}")))?

    // Do NOT wait — daemon runs independently
    drop(child)
    Ok(())
```

---

## Function: `do_bridge` (private, async)

### Pseudocode

```
async fn do_bridge(stream: UnixStream) -> Result<(), ServerError>:

    // Convert tokio stdio to AsyncRead + AsyncWrite
    let mut stdin  = tokio::io::stdin()
    let mut stdout = tokio::io::stdout()

    // Split UDS stream into halves
    let (mut stream_read, mut stream_write) = stream.into_split()

    // Bidirectional copy: stdin -> daemon, daemon -> stdout
    // Returns when either side closes (bridge client stdin EOF, or daemon closes session)
    match tokio::io::copy_bidirectional(&mut stdin, &mut stream).await:
        // Note: copy_bidirectional takes (&mut A, &mut B) where both implement
        // AsyncRead + AsyncWrite. UnixStream implements both.
        // Alternatively: tokio::io::copy_bidirectional(&mut stdin_stdout_pair, &mut stream)
        // The exact calling convention depends on how tokio::io::stdin()/stdout() combine.
        //
        // Implementation note: tokio::io::stdin() and tokio::io::stdout() are separate
        // types. Use tokio::io::copy_bidirectional with a combined handle or use two
        // separate copy tasks:
        //
        //   Task A: tokio::io::copy(&mut stdin,  &mut stream_write)
        //   Task B: tokio::io::copy(&mut stream_read, &mut stdout)
        //   tokio::join!(task_a, task_b) or select! — either side closing ends bridging

        Ok((bytes_from_client, bytes_from_daemon)) =>
            tracing::debug!(
                bytes_in = bytes_from_client,
                bytes_out = bytes_from_daemon,
                "bridge session ended"
            )
        Err(e) =>
            // Connection error — this is expected when daemon closes session (cap reached, etc.)
            tracing::debug!(error = %e, "bridge copy error (may be normal on session close)")

    Ok(())
```

### Implementation Note: Two-copy approach

Since `tokio::io::stdin()` returns `Stdin` and `tokio::io::stdout()` returns `Stdout`
(separate types), the simplest correct approach uses two tasks racing to completion:

```
async fn do_bridge(stream: UnixStream) -> Result<(), ServerError>:
    let (mut stream_rx, mut stream_tx) = stream.into_split()
    let mut stdin  = tokio::io::stdin()
    let mut stdout = tokio::io::stdout()

    // Race: either side closing ends both copies
    tokio::select!:
        r = tokio::io::copy(&mut stdin, &mut stream_tx) =>
            tracing::debug!("stdin closed; bridge session ended")
        r = tokio::io::copy(&mut stream_rx, &mut stdout) =>
            tracing::debug!("daemon stream closed; bridge session ended")

    Ok(())
```

The `select!` approach ends copying when the first side closes. This is the correct
behavior: when Claude Code closes stdin (session end), bridge exits. When daemon closes the
stream (session cap, shutdown), bridge also exits.

---

## Integration Notes

### Call site in `main.rs` (no-subcommand path)

```
None => {
    // Default invocation: bridge mode (vnc-005)
    // Async path: tokio runtime required for copy_bidirectional
    tokio_main_bridge(cli)
}
```

```
#[tokio::main]
async fn tokio_main_bridge(cli: Cli) -> Result<(), Box<dyn std::error::Error>>:
    let filter = if cli.verbose { "debug" } else { "warn" }  // quiet by default in bridge mode
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)  // bridge never writes to stdout (MCP traffic only)
        .init()

    let paths = project::ensure_data_directory(cli.project_dir.as_deref(), None)
        .map_err(|e| ServerError::ProjectInit(e.to_string()))?

    bridge::run_bridge(&paths).await?
    Ok(())
```

### C-10: Hook path must remain before bridge dispatch

In `main.rs`, the `Command::Hook` arm must be matched BEFORE the `None` arm that calls the
bridge. The bridge is async; the hook path is sync. The match structure is:

```
match cli.command:
    Some(Command::Hook { event }) =>   // SYNC — first, before any async
    Some(Command::Export { .. }) =>    // SYNC
    Some(Command::Import { .. }) =>    // SYNC
    Some(Command::Version) =>          // SYNC
    Some(Command::ModelDownload) =>    // SYNC
    Some(Command::Stop) =>             // SYNC (vnc-005)
    Some(Command::Serve { .. }) =>     // ASYNC dispatch (vnc-005)
    None =>                            // ASYNC bridge (vnc-005) — LAST
```

No Tokio runtime is initialized before the `Hook` arm is reached (C-10).

---

## Key Test Scenarios

1. **Bridge connects to running daemon** (AC-03) — start daemon; invoke bridge; assert MCP
   `initialize` request is forwarded and response received.

2. **Bridge exits when stdin closes** — pipe MCP `initialize`; close stdin; assert bridge
   process exits 0 within 2s.

3. **Bridge exits when daemon closes stream** — fill session cap; attempt bridge; assert
   bridge exits non-zero (stream closed by daemon).

4. **Auto-start: bridge spawns daemon and connects** (AC-05) — no daemon running; pipe MCP
   `initialize`; assert response received within 8s; assert `unimatrix-mcp.sock` was created.

5. **Auto-start does not double-spawn a healthy daemon** (AC-06) — start daemon; invoke
   bridge; assert only one daemon process exists after bridge connects.

6. **Auto-start timeout emits log path** (AC-15, R-08) — stub binary that sleeps instead
   of creating socket; assert bridge exits 1 within 7s; assert stderr contains log path
   string.

7. **Stale PID check prevents spawn when daemon is alive** (R-04) — start daemon; simulate
   PID file pointing to a dead process; invoke bridge; assert bridge connects to the live
   daemon without spawning a second one.

8. **ECONNREFUSED during poll window is retried** (edge case from RISK-TEST-STRATEGY) —
   the socket file is created before `listen()` is called in the daemon; assert bridge
   retries ECONNREFUSED and eventually connects.

9. **Bridge log tracing does not write to stdout** — assert no bytes written to stdout
   from tracing during a bridge session (MCP traffic only on stdout).
