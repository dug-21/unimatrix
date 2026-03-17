# Pseudocode: Daemonizer (`infra/daemon.rs`)

## Purpose

Implement the spawn-new-process daemonization pattern (ADR-001). Two functions:

1. `run_daemon_launcher` — called from `main.rs` when `serve --daemon` is given WITHOUT
   `--daemon-child`. Spawns the child process with `--daemon-child` appended, then polls
   `mcp_socket_path` until the socket appears or timeout elapses.

2. `prepare_daemon_child` — called from `main.rs` when `--daemon-child` IS present.
   Calls `nix::unistd::setsid()` synchronously, before any Tokio runtime is initialized
   (C-01). Returns `Ok(())` to let `tokio_main_daemon` proceed.

Both functions are synchronous (`fn`, not `async fn`). No Tokio.

---

## Files Affected

- **New**: `crates/unimatrix-server/src/infra/daemon.rs`
- **Modified**: `crates/unimatrix-server/src/infra/mod.rs` — add `pub mod daemon;`

---

## Dependencies

- `std::process::Command` — spawn child
- `std::env::current_exe()` — resolve binary path
- `std::fs` — poll socket path existence
- `std::thread::sleep` — poll interval
- `std::time::{Duration, Instant}` — timeout
- `nix::unistd::setsid` — terminal detach (already in Cargo.toml)
- `unimatrix_engine::project::ProjectPaths` — path resolution
- `crate::error::ServerError` — error type

---

## Constants

```
DAEMON_SOCKET_POLL_INTERVAL: Duration = 100ms
DAEMON_SOCKET_POLL_TIMEOUT: Duration = 5s
```

---

## Function: `run_daemon_launcher`

### Signature

```
pub fn run_daemon_launcher(paths: &ProjectPaths) -> Result<(), ServerError>
```

### Pseudocode

```
fn run_daemon_launcher(paths: &ProjectPaths) -> Result<(), ServerError>:

    // Step 1: Resolve log file path (append mode; create if absent)
    log_path = paths.data_dir.join("unimatrix.log")

    // Step 2: Open log file in append mode
    log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        PROPAGATE as ServerError::ProjectInit

    // Step 3: Resolve current executable path
    exe_path = std::env::current_exe()
        PROPAGATE as ServerError::ProjectInit

    // Step 4: Collect original args (forward all except --daemon-child)
    // The child re-parses from scratch; we just need to forward structural args
    // like --project-dir if present. Pass "serve" and "--daemon" explicitly
    // rather than forwarding argv to avoid duplicating --daemon.
    //
    // Child command: unimatrix serve --daemon --daemon-child [--project-dir <dir>]
    child_args = vec!["serve", "--daemon", "--daemon-child"]
    if paths.project_root != detected_default_project_root():
        // Only pass --project-dir if it differs from what auto-detection would find.
        // This is conservative — always pass it to avoid misdetection.
        child_args.push("--project-dir")
        child_args.push(paths.project_root.to_str())

    // Step 5: Spawn child with redirected I/O
    child = std::process::Command::new(&exe_path)
        .args(&child_args)
        .stdin(Stdio::null())             // daemon reads no stdin
        .stdout(log_file.try_clone()?)    // daemon logs to file
        .stderr(log_file)                 // both streams to same log file
        .spawn()
        PROPAGATE as ServerError::ProjectInit("failed to spawn daemon child: {e}")

    // Note: child handle is dropped here. We do NOT wait on it.
    // The child process is now running independently.
    drop(child)

    // Step 6: Poll for MCP socket appearance
    start = Instant::now()
    loop:
        if paths.mcp_socket_path.exists():
            return Ok(())

        if start.elapsed() >= DAEMON_SOCKET_POLL_TIMEOUT:
            break

        sleep(DAEMON_SOCKET_POLL_INTERVAL)

    // Step 7: Timeout — report failure with log path for investigation
    Err(ServerError::ProjectInit(format!(
        "daemon did not start within {}s; check log at {}",
        DAEMON_SOCKET_POLL_TIMEOUT.as_secs(),
        log_path.display()
    )))
```

### Error Propagation

| Error | Condition | ServerError variant |
|---|---|---|
| `current_exe()` fails | rare; binary not accessible | `ProjectInit` |
| log file open fails | permissions problem on data dir | `ProjectInit` |
| `Command::spawn()` fails | binary not executable | `ProjectInit` |
| poll timeout | daemon init too slow or crashed | `ProjectInit` |

---

## Function: `prepare_daemon_child`

### Signature

```
pub fn prepare_daemon_child() -> Result<(), ServerError>
```

### Pseudocode

```
fn prepare_daemon_child() -> Result<(), ServerError>:

    // C-01: setsid() MUST be called before ANY Tokio runtime initialization.
    // This function is called in main() BEFORE tokio_main_daemon() is entered.
    // No async code, no Runtime::new(), no #[tokio::main] runs before this call.

    #[cfg(unix)]
    {
        nix::unistd::setsid()
            .map_err(|e| ServerError::ProjectInit(
                format!("setsid() failed: {e}")
            ))?
        // setsid() successful: process is now session leader, detached from terminal
    }

    #[cfg(not(unix))]
    {
        // Windows: daemon mode is not supported (C-12 / NFR-08)
        return Err(ServerError::ProjectInit(
            "daemon mode is not supported on Windows; use 'serve --stdio'".to_string()
        ))
    }

    Ok(())
```

### Notes

- `nix::unistd::setsid()` is a safe Rust wrapper over the `setsid(2)` syscall. No
  unsafe code. Satisfies `#![forbid(unsafe_code)]` (C-03).
- On Linux and macOS, `setsid()` always succeeds if the calling process is not already
  a process group leader (it won't be, since the parent just spawned us with `Command`).
- The `#[cfg(not(unix))]` arm runs on Windows. Windows is documented as not supported
  for daemon mode; this provides a clear error instead of a silent failure.

---

## Integration Notes

### Call sites in `main.rs`

```
// In main() — synchronous, before match on cli.command:
if cli.daemon_child {
    // C-01: setsid must happen before tokio_main_daemon enters the #[tokio::main] context
    infra::daemon::prepare_daemon_child()?;
    // Fall through to tokio_main_daemon below
}

// In the Serve { daemon: true, .. } match arm:
if !cli.daemon_child {
    // Launcher path: spawn child, poll socket, exit
    return infra::daemon::run_daemon_launcher(&paths);
}
// Else: we are the child; fall through to tokio_main_daemon
```

### Windows guard in `main.rs`

```
// In Serve { daemon: true, .. } arm:
#[cfg(not(unix))]
{
    eprintln!("error: 'serve --daemon' is not supported on Windows");
    eprintln!("       use 'unimatrix serve --stdio' instead");
    std::process::exit(1);
}
```

---

## Key Test Scenarios

1. **Launcher polls until socket appears** — launch `run_daemon_launcher` with a test helper
   that creates `mcp_socket_path` after 500ms; assert function returns `Ok(())`.

2. **Launcher times out and returns descriptive error** — launch with no socket creation;
   assert `Err` within 6s; assert error message contains log file path string.

3. **`prepare_daemon_child` returns Ok on Unix** — call directly in a test; assert `Ok(())`.

4. **`prepare_daemon_child` returns Err on Windows** — compile with `cfg(not(unix))`; assert
   error message contains "Windows".

5. **Child is truly detached** — integration: spawn a daemon, verify `setsid()` was called
   by confirming the child's session ID differs from the test process's session ID (via
   `nix::unistd::getsid()`). This is a process-level test.

6. **`--daemon-child` is hidden from `--help` output** — run `unimatrix serve --help`;
   assert `--daemon-child` does not appear in the output (R-17).
