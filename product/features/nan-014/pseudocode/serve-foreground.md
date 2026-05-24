# serve-foreground: --foreground Flag on Serve Subcommand

## Purpose

Run the full daemon stack (UDS listener, tick loop, ML inference, signal handling) as PID 1 without fork/setsid process detachment. Container-native execution mode. Per ADR-001: `tokio_main_daemon` IS the shared core. `--foreground` calls it directly; `--daemon` adds setsid before it.

## Modified File

**File**: `crates/unimatrix-server/src/main.rs`

### 1. Command Enum Change

Add `foreground` field to existing `Serve` variant:

```
enum Command {
    // ... existing variants unchanged ...

    Serve {
        /// Run as a detached background daemon.
        #[arg(long)]
        daemon: bool,

        /// Run in foreground stdio mode (pre-vnc-005 default behavior).
        #[arg(long)]
        stdio: bool,

        /// Run as PID 1 foreground process (container mode).
        /// Identical to daemon but without fork/setsid.
        #[arg(long, conflicts_with_all = ["daemon", "stdio"])]
        foreground: bool,
    },

    // ... rest unchanged ...
}
```

### 2. Match Arm Addition

Add new match arm in `main()` for the foreground case. This arm must be placed BEFORE the existing `daemon: true` and `daemon: false` arms to ensure correct pattern matching.

```
fn main():
    // ... existing code ...

    match cli.command:
        // ... existing sync arms (Hook, Export, Import, Version, etc.) ...

        // NEW: Foreground mode — direct tokio_main_daemon call (ADR-001).
        // No launcher, no child spawn, no setsid. PID 1 container mode.
        Some(Command::Serve { foreground: true, .. }):
            return tokio_main_daemon(cli)

        // EXISTING: Daemon mode — unchanged.
        Some(Command::Serve { daemon: true, .. }):
            if cli.daemon_child:
                unimatrix_server::infra::daemon::prepare_daemon_child()?
                return tokio_main_daemon(cli)
            else:
                let paths = compute_paths_sync(&cli.project_dir)?
                unimatrix_server::infra::daemon::run_daemon_launcher(&paths)?
                return Ok(())

        // EXISTING: Stdio mode — unchanged.
        Some(Command::Serve { daemon: false, .. }):
            return tokio_main_stdio(cli)

        // ... rest unchanged ...
```

### Key Behavioral Notes

**What foreground mode does**:
- Calls `tokio_main_daemon(cli)` directly — the exact same function called by `--daemon-child` after setsid.
- The full daemon stack initializes: tracing, project paths, config loading, PidGuard, database, vector index, embedding service, NLI service, UDS listeners (hook + MCP), background tick, signal handlers, graceful shutdown.
- Tracing output goes to stderr (same as daemon mode).

**What foreground mode skips**:
- `run_daemon_launcher` (no child process spawn)
- `prepare_daemon_child` (no setsid — process stays attached to container's init)
- No stdout/stderr redirection to log file

**What is NOT modified**:
- `tokio_main_daemon` — zero changes to this function
- `tokio_main_stdio` — zero changes
- `tokio_main_bridge` — zero changes
- `prepare_daemon_child` — zero changes
- `run_daemon_launcher` — zero changes
- `shutdown::shutdown_signal` — already registers SIGTERM/SIGINT explicitly, works as PID 1

**Signal handling**: PID 1 in a container does not receive default signal handling. The existing `shutdown::shutdown_signal()` explicitly registers SIGTERM and SIGINT via `tokio::signal::unix::signal(SignalKind::terminate())`, which works correctly for PID 1. No changes needed.

**PidGuard**: The self-PID guard (pidguard-self-pid component) must be in place. On container restart, `handle_stale_pid_file` would otherwise SIGTERM PID 1 (self). With the guard, it detects `stale_pid == std::process::id()` and reclaims safely.

## Error Handling

No new error paths. `tokio_main_daemon` already returns `Result<(), Box<dyn Error>>`. All errors propagate to `main()` and exit the process with the error message on stderr.

## Key Test Scenarios

1. **Clap mutual exclusion**: `Cli::try_parse_from(["unimatrix", "serve", "--foreground", "--daemon"])` returns `Err`. Same for `--foreground --stdio`. Validates clap `conflicts_with_all`.

2. **Foreground flag default is false**: `Cli::try_parse_from(["unimatrix", "serve", "--daemon"])` succeeds with `foreground: false`. Existing `--daemon` dispatch is unaffected.

3. **Foreground-only parse**: `Cli::try_parse_from(["unimatrix", "serve", "--foreground"])` succeeds with `foreground: true, daemon: false, stdio: false`.

4. **Regression: existing daemon tests pass unchanged**: All existing daemon integration tests must pass with zero modifications. The `--daemon` match arm is unchanged.

5. **Regression: existing stdio tests pass unchanged**: The `--stdio` / bare `serve` match arm is unchanged.

6. **Foreground dispatches to tokio_main_daemon**: This is an integration-level check. Start `unimatrix serve --foreground --project-dir /tmp/test` and verify the MCP UDS socket is created, the process responds to SIGTERM with graceful shutdown.
