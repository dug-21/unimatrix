# Pseudocode: Stop Subcommand (`main.rs` additions)

## Purpose

Add `unimatrix stop` as a synchronous subcommand (no Tokio runtime) that reads the PID
file, verifies the process via `is_unimatrix_process`, sends SIGTERM, and polls for exit.
Also covers the other `main.rs` additions for vnc-005: the `Serve` subcommand variant,
the `--daemon-child` hidden flag, and the dispatch routing changes.

See ADR-006 for the design rationale.

---

## Files Affected

- **Modified**: `crates/unimatrix-server/src/main.rs`

---

## Dependencies

- `crate::infra::pidfile::{read_pid_file, is_unimatrix_process, terminate_and_wait}` —
  all existing; no new functions needed
- `crate::infra::daemon::{run_daemon_launcher, prepare_daemon_child}` — new (Wave 1)
- `crate::bridge::run_bridge` — new (Wave 2)
- `unimatrix_engine::project::ensure_data_directory` — existing
- `std::process::exit` — for stop subcommand exit codes (synchronous path)

---

## `Cli` Struct Changes

### New hidden flag (C-01 / ADR-001)

```
/// Internal flag: used by run_daemon_launcher to spawn the daemon child.
/// Not shown in help output; not intended for direct user invocation (R-17).
#[arg(long, hide = true)]
daemon_child: bool,
```

This is a top-level `Cli` field (not inside a subcommand) so the child process can parse it
regardless of which subcommand path it takes.

---

## `Command` Enum Changes

### New variants

```
/// Start the server (daemon or stdio mode).
Serve {
    /// Run as a detached background daemon.
    #[arg(long)]
    daemon: bool,

    /// Run in foreground stdio mode (pre-vnc-005 behavior).
    #[arg(long)]
    stdio: bool,
},

/// Stop the running background daemon.
///
/// Synchronous path, no tokio runtime.
Stop,
```

### Mutual exclusivity note

`--daemon` and `--stdio` are not declared `conflicts_with` each other in clap, but the
dispatch logic handles the ambiguity: if both are provided, `daemon` takes precedence and
an informational warning is logged. If neither is provided under `serve`, print usage help
and exit non-zero.

---

## `main()` Dispatch Routing

### Full updated dispatch structure

```
fn main() -> Result<(), Box<dyn std::error::Error>>:

    // Install panic hook (unchanged)
    std::panic::set_hook(...)

    let cli = Cli::parse()

    // C-10: Hook and other sync subcommands MUST be dispatched BEFORE
    // any async code or Tokio runtime initialization.
    // The match on cli.command must run in this order.

    match cli.command:
        Some(Command::Hook { event }) =>
            // SYNC — no tokio (ADR-002 from vnc-001; R-13 regression gate)
            return unimatrix_server::uds::hook::run(event, cli.project_dir)

        Some(Command::Export { output }) =>
            // SYNC
            return unimatrix_server::export::run_export(...)

        Some(Command::Import { input, skip_hash_validation, force }) =>
            // SYNC
            return unimatrix_server::import::run_import(...)

        Some(Command::Version) =>
            // SYNC
            return handle_version(cli.project_dir)

        Some(Command::ModelDownload) =>
            // SYNC
            return handle_model_download()

        Some(Command::Stop) =>
            // SYNC — no tokio (ADR-006)
            // run_stop returns an exit code; main() calls std::process::exit
            let code = run_stop(cli.project_dir)
            std::process::exit(code)

        Some(Command::Serve { daemon: true, .. }) =>
            // Daemon path — MAY be sync (launcher) or async (child)
            // C-01: if --daemon-child, call prepare_daemon_child() here BEFORE
            // entering tokio_main_daemon (which initializes the Tokio runtime).
            if cli.daemon_child:
                // We are the child process: setsid() then fall through to async
                unimatrix_server::infra::daemon::prepare_daemon_child()?
                return tokio_main_daemon(cli)
            else:
                // We are the launcher: synchronous spawn + poll
                let paths = compute_paths_sync(&cli.project_dir)?
                return unimatrix_server::infra::daemon::run_daemon_launcher(&paths)
                    .map_err(Into::into)

        Some(Command::Serve { stdio: true, .. }) | Some(Command::Serve { daemon: false, stdio: false }) =>
            // Stdio mode (or bare `serve` with no flags — treat as stdio for backward compat)
            return tokio_main_stdio(cli)

        None =>
            // No subcommand: bridge mode (vnc-005 default)
            return tokio_main_bridge(cli)
```

### `compute_paths_sync` helper (private)

```
fn compute_paths_sync(project_dir: &Option<PathBuf>) -> Result<ProjectPaths, Box<dyn Error>>:
    project::ensure_data_directory(project_dir.as_deref(), None)
        .map_err(|e| ServerError::ProjectInit(e.to_string()).into())
```

Used in the launcher path only — avoids initializing the full server just to get paths.

---

## Function: `run_stop`

### Signature

```
fn run_stop(project_dir: Option<PathBuf>) -> i32   // returns exit code
```

### Pseudocode

```
fn run_stop(project_dir: Option<PathBuf>) -> i32:

    // Step 1: Resolve project paths (synchronous — same as hook path)
    let paths = match project::ensure_data_directory(project_dir.as_deref(), None):
        Ok(p) => p
        Err(e):
            eprintln!("error: failed to resolve project paths: {e}")
            return 1

    // Step 2: Read PID file
    let pid = match pidfile::read_pid_file(&paths.pid_path):
        Some(p) => p
        None:
            eprintln!("no unimatrix daemon running for this project (no PID file)")
            return 1

    // Step 3: Verify it is a unimatrix process (R-04 on macOS: fallback to is_process_alive)
    if !pidfile::is_unimatrix_process(pid):
        eprintln!(
            "stale PID file: process {} is not a unimatrix daemon (or has exited)",
            pid
        )
        return 1

    // Step 4: Send SIGTERM and wait (ADR-006: 15s timeout to accommodate graceful shutdown)
    let stopped = pidfile::terminate_and_wait(pid, Duration::from_secs(15))

    // Step 5: Report result
    if stopped:
        println!("unimatrix daemon stopped (PID {})", pid)
        return 0   // exit code 0: daemon stopped
    else:
        eprintln!(
            "daemon (PID {}) did not stop within 15 seconds",
            pid
        )
        return 2   // exit code 2: timeout (ADR-006 exit code specification)
```

### Exit Code Summary

| Code | Condition |
|---|---|
| 0 | Daemon stopped successfully |
| 1 | No daemon running, no PID file, or stale PID |
| 2 | Daemon did not exit within 15-second timeout |

---

## `tokio_main_daemon` — Daemon Async Entry Point (new)

This is a new `#[tokio::main]` function for the daemon child path. It replaces the current
`tokio_main(cli)` in the daemon child case. The daemon path differs from the stdio path in:
- Does NOT call `server.serve(stdio())` — uses the MCP acceptor instead
- Uses a `CancellationToken` as the daemon lifetime signal
- Calls `graceful_shutdown` after the daemon token fires (not after transport close)
- Builds `LifecycleHandles` with the two new fields (`mcp_socket_guard`, `mcp_acceptor_handle`)

```
#[tokio::main]
async fn tokio_main_daemon(cli: Cli) -> Result<(), Box<dyn std::error::Error>>:

    // Initialize tracing (daemon: log to stderr, which is redirected to log file by launcher)
    let filter = if cli.verbose { "debug" } else { "info" }
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init()

    tracing::info!("starting unimatrix daemon")

    // Initialize project paths
    let paths = project::ensure_data_directory(cli.project_dir.as_deref(), None)
        .map_err(|e| ServerError::ProjectInit(e.to_string()))?

    tracing::info!(
        project_root = %paths.project_root.display(),
        project_hash = %paths.project_hash,
        data_dir = %paths.data_dir.display(),
        mcp_socket = %paths.mcp_socket_path.display(),
        "daemon project initialized"
    )

    // Handle stale PID file (same as current tokio_main)
    handle_stale_pid_file_if_present(&paths)

    // Open database with retry (same helper as current tokio_main)
    let store = open_store_with_retry(&paths.db_path)?

    // Acquire PID guard
    let _pid_guard = acquire_pid_guard(&paths.pid_path)   // logs warn if fails, continues

    // Handle stale hook IPC socket
    uds_listener::handle_stale_socket(&paths.socket_path)?

    // [All the same subsystem initialization as current tokio_main:
    //  vector_index, embed_handle, registry, audit, store adapters, categories,
    //  adapt_service, session_registry, pending_entries_analysis, usage_dedup, services]
    // ... (same initialization as current tokio_main lines 217-283)

    // Build the server (same as current tokio_main lines 305-318)
    let mut server = UnimatrixServer::new(...)
    server.pending_entries_analysis = Arc::clone(&pending_entries_analysis)
    server.session_registry = Arc::clone(&session_registry)

    // Extract state handles (same as current tokio_main lines 321-333)

    // Start UDS hook IPC listener (same as current tokio_main)
    let (uds_handle, socket_guard) = uds_listener::start_uds_listener(...).await?

    // Spawn background tick (same as current tokio_main)
    let tick_handle = unimatrix_server::background::spawn_background_tick(...)

    // Create daemon CancellationToken (NEW)
    let daemon_token = CancellationToken::new()

    // Start MCP acceptor (NEW — requires Wave 2 mcp_listener.rs)
    let (mcp_acceptor_handle, mcp_socket_guard) =
        uds::mcp_listener::start_mcp_uds_listener(
            &paths.mcp_socket_path,
            server.clone(),
            daemon_token.clone(),
        ).await?

    // Signal handler: cancel daemon token on SIGTERM/SIGINT (NEW pattern)
    let signal_token = daemon_token.clone()
    tokio::spawn(async move {
        shutdown::shutdown_signal().await
        tracing::info!("received shutdown signal; cancelling daemon token")
        signal_token.cancel()
    })

    // Build LifecycleHandles with new fields (UPDATED)
    let lifecycle_handles = LifecycleHandles {
        store,
        vector_index,
        vector_dir: paths.vector_dir.clone(),
        registry,
        audit,
        adapt_service,
        data_dir: paths.data_dir.clone(),
        mcp_socket_guard: Some(mcp_socket_guard),    // NEW
        mcp_acceptor_handle: Some(mcp_acceptor_handle),  // NEW
        socket_guard: Some(socket_guard),
        uds_handle: Some(uds_handle),
        tick_handle: Some(tick_handle),
        services: Some(services),
    }

    tracing::info!("unimatrix daemon ready")

    // Wait for daemon token to be cancelled (SIGTERM/SIGINT)
    // ADR-002 / C-04: This is the daemon lifetime boundary.
    // Session EOF (QuitReason::Closed) does NOT cancel this token.
    daemon_token.cancelled().await
    tracing::info!("daemon token cancelled; beginning graceful shutdown")

    // ONLY call site for graceful_shutdown in daemon path (C-05)
    shutdown::graceful_shutdown(lifecycle_handles).await?
    tracing::info!("unimatrix daemon exited cleanly")
    Ok(())
```

---

## `tokio_main_stdio` — Stdio Async Entry Point (refactored from current `tokio_main`)

The current `tokio_main` becomes `tokio_main_stdio`. Its structure remains identical to
the current code with one change: the `LifecycleHandles` constructor must now supply
the two new fields with `None` values:

```
let lifecycle_handles = LifecycleHandles {
    ...existing fields...
    mcp_socket_guard: None,      // stdio mode has no MCP socket
    mcp_acceptor_handle: None,   // stdio mode has no MCP acceptor
    socket_guard: Some(socket_guard),
    ...
}
```

The existing signal handler + `running.waiting().await` + `graceful_shutdown` sequence is
unchanged. `QuitReason::Closed` (stdin EOF) still triggers `graceful_shutdown` in stdio mode.
This is the R-12 regression gate.

---

## Key Test Scenarios

1. **`unimatrix stop` exits 0 when daemon stops** (AC-10) — start daemon; `run_stop`;
   assert exit code 0; assert daemon PID gone within 10s.

2. **`unimatrix stop` exits 1 when no daemon** (AC-11) — no daemon; `run_stop`; assert
   exit code 1; assert stderr contains "no daemon" or "no PID file".

3. **`unimatrix stop` exits 1 on stale PID** — write a PID file with a non-unimatrix PID;
   `run_stop`; assert exit code 1; assert stderr contains "stale".

4. **`unimatrix stop` exits 2 on timeout** — start daemon that ignores SIGTERM; `run_stop`;
   assert exit code 2 within 17s; assert stderr contains "did not stop within".

5. **`--daemon-child` is hidden from help** (R-17) — run `unimatrix --help` and
   `unimatrix serve --help`; assert `--daemon-child` does not appear.

6. **Hook dispatch is sync (no Tokio init)** (R-13) — time `unimatrix hook SessionStart`;
   assert wall clock < 50ms.

7. **`serve --stdio` behaves as pre-vnc-005 default** (AC-12, R-12) — pipe MCP
   `initialize`; close stdin; assert process exits 0; assert `unimatrix-mcp.sock` was NOT
   created.

8. **`serve --daemon` starts daemon and launcher exits 0** (AC-01) — `serve --daemon`;
   assert launcher exits 0 within 5s; assert `unimatrix-mcp.sock` exists; assert daemon
   PID alive.

9. **`serve --daemon` rejected if daemon already running** (AC-07) — start daemon; invoke
   `serve --daemon` a second time; assert exit code non-zero; assert only one daemon in
   process table.

10. **No subcommand invokes bridge mode** — `unimatrix` with no args and a running daemon;
    assert bridge connects and MCP `initialize` succeeds.
