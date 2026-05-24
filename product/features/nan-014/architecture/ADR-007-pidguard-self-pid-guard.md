## ADR-007: PidGuard Self-PID Guard Against Container Restart Self-Termination

### Context

PidGuard manages daemon exclusivity via a PID file and flock. When the daemon starts, `handle_stale_pid_file` checks if a previous PID file exists, reads the PID, calls `is_unimatrix_process(pid)` to verify via `/proc/{pid}/cmdline`, and sends SIGTERM if it's a running unimatrix process.

In containers, PID 1 is always the main process. When a container restarts with a retained named volume:

1. The stale PID file contains `1` (written by the previous container).
2. The new container starts. PID 1 is the new `unimatrix` process.
3. `handle_stale_pid_file` reads `1` from the file.
4. `is_unimatrix_process(1)` checks `/proc/1/cmdline` → matches `unimatrix` → returns `true`.
5. Code sends `SIGTERM` to PID 1 → **self-termination**.

This is not container-specific in principle — any scenario where a stale PID file contains the current process's own PID triggers the same bug. Containers make it deterministic because PID 1 is always reused.

### Decision

Add a self-PID check in `handle_stale_pid_file` before the SIGTERM path:

```rust
if stale_pid == std::process::id() {
    // Stale PID file references our own process — reclaim directly.
    // This happens on container restart where PID 1 is always reused.
    return Ok(RecoveryAction::Reclaimed);
}
```

This check is placed BEFORE the `is_unimatrix_process` call. If the stale PID equals the current process's PID, the file is reclaimed without any signal. The rationale: a process cannot be "stale" relative to itself.

### Consequences

- **Easier**: Eliminates the self-termination scenario on container restart. One `if` statement, no behavioral change to any other path.
- **Easier**: General correctness improvement — works for any PID reuse scenario, not just containers.
- **Easier**: No changes to `is_unimatrix_process`, `PidGuard::acquire`, or `flock` logic.
- **Harder**: None. This is a pure bug fix with no tradeoffs.
