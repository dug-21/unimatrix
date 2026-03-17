# Test Plan: Stop Subcommand (`main.rs` — `run_stop`)

Component responsibility: synchronous (no Tokio runtime) `unimatrix stop` subcommand.
Reads PID file, calls `is_unimatrix_process(pid)`, calls `terminate_and_wait(pid, 15s)`.
Exit codes: 0 = stopped, 1 = no daemon/stale, 2 = timeout.

Also covers:
- `--daemon-child` flag hidden from help (R-17)
- Hook IPC unaffected by daemon (AC-13, R-13)
- Hook path does not initialize Tokio runtime (R-13)

Risk coverage: R-13, R-17.
Additional ACs: AC-10, AC-11, AC-13.
RV items: RV-03, RV-04.

---

## Unit Tests

### T-STOP-U-01: `run_stop` returns exit code 0 when daemon stopped successfully (AC-10)
**Arrange**: Start a running daemon; provide its PID and PID file path to `run_stop`.
**Act**: Call `run_stop(paths)`.
**Assert**:
- Returns `0` (exit code).
- Daemon PID is no longer alive (`kill -0` returns ESRCH) within 10 seconds.

### T-STOP-U-02: `run_stop` returns exit code 1 when no PID file present (AC-11)
**Arrange**: No PID file at `paths.pid_file_path`.
**Act**: Call `run_stop(paths)`.
**Assert**:
- Returns `1`.
- stderr (or the error output channel) contains a human-readable message such as
  "no daemon running" or "no PID file found".

### T-STOP-U-03: `run_stop` returns exit code 1 when PID file is stale (AC-11)
**Arrange**: Write a PID file with a PID that `is_unimatrix_process` returns false for
(e.g., a dead process or a non-unimatrix process).
**Act**: Call `run_stop(paths)`.
**Assert**:
- Returns `1`.
- stderr message indicates "stale PID" or "no daemon was found".
- No SIGTERM is sent (no kill attempt for a non-unimatrix process).

### T-STOP-U-04: `run_stop` returns exit code 2 on timeout (15s)
**Arrange**: Start a process that ignores SIGTERM (e.g., a process that catches SIGTERM
and does nothing). Write its PID to the PID file.
**Act**: Call `run_stop(paths)` (which calls `terminate_and_wait(pid, 15s)`).
**Assert**:
- Returns `2` after approximately 15 seconds.
- stderr message contains "timeout" or "did not stop within".

### T-STOP-U-05: `run_stop` contains no Tokio runtime initialization (R-13)
**Risk**: R-13
**Arrange**: Code review / grep.
**Act**: `grep -n 'tokio\|Runtime\|#\[tokio' crates/unimatrix-server/src/main.rs`
Filter results to the `Command::Stop` match arm and `run_stop` function.
**Assert**: No Tokio-related symbols appear in the stop subcommand dispatch path.
The function is entirely synchronous.

### T-STOP-U-06: `--daemon-child` is hidden from help output (RV-03, R-17)
**Risk**: R-17
**Arrange**: After implementation.
**Act**:
1. Capture `unimatrix --help` output.
2. Capture `unimatrix serve --help` output.
**Assert**:
- Neither output contains the string `daemon-child`.
- `#[arg(hide = true)]` is present on the `daemon_child` field in the clap struct
  (grep verification).
**Test form**: Shell command capturing help output; grep for absence.

---

## Integration Tests

### T-STOP-I-01: `unimatrix stop` exits 0, daemon gone within 10s (AC-10)
**Arrange**: Start daemon; record PID; verify alive.
**Act**: Invoke `unimatrix stop`.
**Assert**:
- `unimatrix stop` exits with code 0.
- `kill -0 {daemon_pid}` fails within 10 seconds (ESRCH — process gone).

### T-STOP-I-02: `unimatrix stop` exits non-zero when no daemon running (AC-11)
**Arrange**: Ensure no daemon running; no PID file.
**Act**: Invoke `unimatrix stop`.
**Assert**:
- Exit code is non-zero (1 expected).
- stderr contains a message indicating no daemon was found.

### T-STOP-I-03: Hook IPC unaffected by daemon (AC-13, R-13)
**Arrange**: Start daemon.
**Act**: Invoke `unimatrix hook SessionStart --feature test-001`.
**Assert**:
- Exit code 0.
- `unimatrix.sock` is present under `~/.unimatrix/{hash}/` and is served by the daemon.
- The hook IPC protocol is unchanged (existing hook integration tests still pass).

### T-STOP-I-04: Hook subcommand wall-clock time under 50ms (RV-04, R-13)
**Risk**: R-13
**Arrange**: Start daemon.
**Act**: Time `unimatrix hook SessionStart --feature test-001` with `time` command;
repeat 5 times and take the median.
**Assert**: Median wall-clock time is under 50ms.
**Note**: If CI environment is slow, use 100ms as the tolerance threshold and flag
values approaching 50ms. The key assertion is that no Tokio thread-pool initialization
is occurring (which would add ~200-500ms on first invocation).

### T-STOP-I-05: Hook path Tokio init check (R-13)
**Risk**: R-13
**Arrange**: Inspect `main.rs` for the `Command::Hook` dispatch path.
**Act**: `grep -n 'tokio\|Runtime' crates/unimatrix-server/src/main.rs`
Filter to lines above the `Command::Hook` match arm.
**Assert**: No Tokio initialization appears before `Command::Hook` is dispatched.
The hook dispatch must be reachable without entering any async context.
**Test form**: Code review + grep in Stage 3c.

---

## Edge Cases

### T-STOP-E-01: `unimatrix stop` while daemon is mid-graceful-shutdown
**Arrange**: Send SIGTERM to daemon manually; immediately run `unimatrix stop`.
**Act**: Observe exit code and timing.
**Assert**: `unimatrix stop` exits 0 if the daemon exits within its 15s timeout window.
No panic, no "stale PID" false positive (PID file may still exist during shutdown).

### T-STOP-E-02: `unimatrix stop` stale PID file from previous SIGKILL
**Arrange**: Kill daemon with SIGKILL (leaves PID file). Start new daemon.
**Act**: Invoke `unimatrix stop` (targets the new daemon, PID file was overwritten).
**Assert**: Exits 0; new daemon is stopped. Old PID file was overwritten by new
daemon's PidGuard.

### T-STOP-E-03: Double `unimatrix stop` (second invocation after daemon already stopped)
**Arrange**: Start daemon; run `unimatrix stop` (first invocation stops daemon).
**Act**: Run `unimatrix stop` again.
**Assert**: Second invocation exits 1 with "no daemon running" message.
No panic, no attempt to send SIGTERM to a dead PID.

---

## Assertions Summary

| Test | AC/RV | Risk |
|------|-------|------|
| T-STOP-U-01 (stop returns 0) | AC-10 | — |
| T-STOP-U-02 (no PID file → exit 1) | AC-11 | — |
| T-STOP-U-03 (stale PID → exit 1) | AC-11 | — |
| T-STOP-U-04 (timeout → exit 2) | — | — |
| T-STOP-U-05 (no Tokio in stop path) | — | R-13 |
| T-STOP-U-06 (--daemon-child hidden from help) | RV-03 | R-17 |
| T-STOP-I-01 (unimatrix stop exits 0) | AC-10 | — |
| T-STOP-I-02 (no daemon → non-zero) | AC-11 | — |
| T-STOP-I-03 (hook IPC unaffected) | AC-13 | R-13 |
| T-STOP-I-04 (hook wall-clock < 50ms) | RV-04 | R-13 |
| T-STOP-I-05 (no Tokio before hook dispatch) | — | R-13 |
