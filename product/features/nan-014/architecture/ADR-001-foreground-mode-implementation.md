## ADR-001: Foreground Mode as Direct tokio_main_daemon Call

### Context

Containers expect PID 1 to be the main process. The existing `serve --daemon` path uses a launcher/child pattern: the launcher spawns a child process with `--daemon-child`, the child calls `prepare_daemon_child()` (setsid) to detach from the controlling terminal, then enters `tokio_main_daemon()` which initializes the full server stack (UDS listener, tick loop, ML inference, signal handling, graceful shutdown).

SR-06 (High severity) flags that touching the daemon startup path risks breaking the existing `serve --daemon` mode. The risk assessment recommends extracting shared daemon logic into a `run_daemon_core()` function called by both paths.

However, examining the code reveals that `tokio_main_daemon` already IS the shared core. The daemon-specific behavior (setsid) happens BEFORE `tokio_main_daemon` is called, in `main()`:

```
--daemon path:  main() → prepare_daemon_child() [setsid] → tokio_main_daemon()
--foreground:   main() → tokio_main_daemon()  [no setsid, no launcher]
```

Extracting a separate `run_daemon_core()` function would be a refactor with no behavioral difference — it would contain the exact same code as `tokio_main_daemon`. The risk is lower by NOT refactoring: the existing, tested `tokio_main_daemon` function is called identically by both paths.

### Decision

Add a `foreground: bool` field to the `Serve` subcommand. When `--foreground` is set:

1. Skip the launcher path entirely (no `run_daemon_launcher`, no child spawn).
2. Skip `prepare_daemon_child()` (no setsid — process stays attached to the container's init).
3. Call `tokio_main_daemon(cli)` directly.

No refactoring of `tokio_main_daemon`. No new function extraction. The match arm in `main()` is:

```rust
Some(Command::Serve { foreground: true, .. }) => {
    return tokio_main_daemon(cli);
}
```

The `--foreground` and `--daemon` flags are mutually exclusive (enforced by clap `conflicts_with`). `--foreground` and `--stdio` are also mutually exclusive.

Signal handling requires no changes. The existing `shutdown::shutdown_signal()` explicitly registers SIGTERM and SIGINT handlers via `tokio::signal::unix::signal(SignalKind::terminate())`. PID 1 in a container does not receive default signal behavior, but explicitly registered handlers work correctly.

PidGuard requires a self-PID guard (ADR-007). On container restart with a retained named volume, the stale PID file contains `1`. In the new container, PID 1 IS the new `unimatrix` process. Without a guard, `handle_stale_pid_file` would read PID 1, confirm via `/proc/1/cmdline` that it's unimatrix, and send SIGTERM to PID 1 — self-termination. ADR-007 specifies the fix: if `stale_pid == std::process::id()`, skip SIGTERM and reclaim the PID file directly.

### Consequences

- **Easier**: Zero blast radius to existing `--daemon` path. No code in `tokio_main_daemon` or `daemon.rs` is modified. The `--daemon` path continues to work identically.
- **Easier**: Minimal code change — one new clap field, one new match arm (~3 lines).
- **Easier**: Testing: `--foreground` uses the same `tokio_main_daemon` that is already tested by daemon integration tests.
- **Harder**: If `tokio_main_daemon` later needs container-specific behavior (e.g., different config loading), the split point is inside the function rather than at a clean extraction boundary. Acceptable: such changes are additive and can be gated on a `foreground` field on `Cli` or an environment variable.
