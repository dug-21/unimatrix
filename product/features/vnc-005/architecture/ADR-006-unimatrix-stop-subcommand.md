## ADR-006: `unimatrix stop` Interacts with PID File via Existing terminate_and_wait

### Context

The scope document (OQ-06 → In scope) and AC-12 specify:

> `unimatrix stop` sends SIGTERM to the daemon (reads PID file) and exits 0 when the
> daemon has exited, non-zero if no daemon was running.

The `pidfile.rs` module already provides:
- `read_pid_file(path) -> Option<u32>`
- `is_unimatrix_process(pid) -> bool`
- `terminate_and_wait(pid, timeout) -> bool`

`terminate_and_wait` handles the full stop sequence: SIGTERM → poll 250ms intervals →
SIGKILL escalation if timeout expires. This is the same function used by
`handle_stale_pid_file` when a live stale process is detected at startup.

Two design questions:

1. **What PID file path does `stop` use?** The `stop` subcommand must compute the
   same project paths as the daemon (project hash, data directory). It uses the same
   `project::ensure_data_directory` call with the `--project-dir` override if provided.
   This is consistent with how `hook` subcommands locate the socket.

2. **Does `stop` wait for the daemon to fully exit?** AC-12 says "exits 0 when the
   daemon has exited." `terminate_and_wait` polls until dead or timeout. The timeout
   for `stop` should be 15 seconds (longer than the internal `STALE_PROCESS_TIMEOUT`
   of 10s used at startup) to give `graceful_shutdown` time to complete vector dump,
   adapt save, and DB compaction.

   The concern: `graceful_shutdown` is async and includes DB compaction. If the daemon
   is mid-tick (15-minute maintenance cycle), compaction may take a few seconds. A
   15-second timeout accommodates this without blocking the user excessively.

3. **Does `stop` use tokio?** The `hook` subcommand runs without Tokio (ADR-002 from
   vnc-001 context, entry #243). The `stop` subcommand also needs no Tokio: it reads a
   file, sends a signal, and polls. All operations are synchronous. `std::thread::sleep`
   is sufficient for the poll loop (same implementation as `terminate_and_wait`).

### Decision

`unimatrix stop` is a synchronous subcommand (no Tokio runtime) that:

1. Computes `ProjectPaths` via `project::ensure_data_directory`.
2. Reads `paths.pid_path` via `pidfile::read_pid_file`.
3. If no PID file or unreadable: prints "no unimatrix daemon running for this project"
   to stderr, exits 1.
4. Calls `is_unimatrix_process(pid)`. If false: prints "PID {pid} is not a unimatrix
   process (stale PID file)" to stderr, exits 1.
5. Calls `terminate_and_wait(pid, Duration::from_secs(15))`.
6. If true (daemon exited): prints "unimatrix daemon stopped" to stdout, exits 0.
7. If false (daemon did not exit): prints "daemon did not stop within 15 seconds" to
   stderr, exits 2. (Exit 2 distinguishes "no daemon" from "daemon refused to stop"
   for scripting.)

The `stop` subcommand is added to the `Command` enum in `main.rs` alongside `Hook`,
`Export`, `Import`, `Version`, and `ModelDownload`. All of these are synchronous; `stop`
follows the same pattern.

The `--project-dir` flag from `Cli` is respected (it is a top-level flag inherited by
all subcommands via clap's `from_global` or the common `Cli` struct).

### Consequences

Easier:
- Zero new logic: reuses `pidfile::terminate_and_wait` and `is_unimatrix_process`.
- No Tokio runtime in the stop path — fast startup, consistent with hook path.
- Exit codes are distinct: 0 (stopped), 1 (no daemon / stale), 2 (timeout). Scripts
  can distinguish all three states.

Harder:
- The 15-second timeout means `unimatrix stop` can block the terminal for up to 15
  seconds in the worst case (daemon hung during shutdown). This is acceptable for a
  manual ops command; automated scripts should use `|| true` if they don't care about
  the outcome.
- On macOS (non-Linux Unix), `is_unimatrix_process` falls back to `is_process_alive`
  (no `/proc` check). This means a non-unimatrix process that happens to have reused
  the PID will receive a SIGTERM from `stop`. This is an existing limitation of the
  PID file mechanism, not a new issue introduced by vnc-005.
