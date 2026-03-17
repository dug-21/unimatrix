# Test Plan: Daemonizer (`infra/daemon.rs`)

Component responsibility: spawn a new child process with `--daemon-child` flag,
redirect child stdout/stderr to the log file, call `nix::unistd::setsid()` in the
child before Tokio starts, then poll for `unimatrix-mcp.sock` in the launcher.

Risk coverage: R-01 (partial), R-04, R-09 (partial), R-12 (partial), R-19 (AC-19).

---

## Unit Tests

### T-DAEMON-U-01: `prepare_daemon_child` — setsid called before runtime init
**Risk**: R-12 (SR-02 compliance — AC-19)
**Arrange**: Call `prepare_daemon_child()` in isolation in a test binary (not in the
main tokio runtime).
**Act**: Invoke `prepare_daemon_child()`.
**Assert**: Returns `Ok(())`. No panic. The process is now a session leader (verify
via `nix::unistd::getsid(Pid::this())` returns the current PID).
**Note**: This test must run in a separate test binary or `#[test]` with no tokio
attribute, since the function is synchronous and must precede any async init.

### T-DAEMON-U-02: `run_daemon_launcher` — no Tokio init in launcher path
**Risk**: R-12 (AC-19), C-01
**Arrange**: Static check via grep.
**Act**: `grep -n 'tokio::runtime\|#\[tokio::main\]\|Runtime::new' crates/unimatrix-server/src/infra/daemon.rs crates/unimatrix-server/src/main.rs`
**Assert**: Zero matches before the `--daemon-child` dispatch point. The launcher
function contains only synchronous code (`std::process::Command`, file operations,
polling loop with `std::thread::sleep`).
**Test form**: Automated grep in Stage 3c.

### T-DAEMON-U-03: Launcher polls MCP socket with correct interval and timeout
**Risk**: R-08 (partial — launcher side)
**Arrange**: Unit test of the polling loop logic in isolation by mocking the file
existence check.
**Act**: Simulate socket appearing at poll interval 3 (after ~750ms if 250ms interval).
**Assert**: `run_daemon_launcher` returns `Ok(())` as soon as the socket path exists;
does not poll past 5 seconds.

### T-DAEMON-U-04: Launcher returns error on timeout (socket never appears)
**Risk**: R-08
**Arrange**: Configure launcher to poll a path that never exists.
**Act**: Call `run_daemon_launcher` with a non-existent socket path.
**Assert**: Returns `Err(ServerError::...)` after approximately 5 seconds. The error
message contains the log file path string.

### T-DAEMON-U-05: Child process stdout/stderr redirected to log file
**Risk**: R-15 (operational diagnostic quality)
**Arrange**: Integration-level check.
**Act**: Start daemon; write a known log message at `warn!` level; check the log file.
**Assert**: Log file at `~/.unimatrix/{hash}/unimatrix.log` exists and contains the
expected message. Log is opened in append mode (prior content is preserved).

---

## Integration Tests (AC-level)

### T-DAEMON-I-01: Daemon start creates socket, launcher exits 0 (AC-01)
**Risk**: R-04, R-09
**Arrange**: No daemon running; isolated project directory.
**Act**: Invoke `unimatrix serve --daemon`; wait up to 5 seconds.
**Assert**:
- Launcher exits with code 0.
- `unimatrix-mcp.sock` exists under `~/.unimatrix/{hash}/`.
- `kill -0 $(cat unimatrix.pid)` exits 0 (daemon is alive).
- PID file contains the child process PID, not the launcher PID.

### T-DAEMON-I-02: Second daemon invocation exits non-zero (AC-07)
**Risk**: R-04
**Arrange**: Daemon already running.
**Act**: Invoke `unimatrix serve --daemon` a second time.
**Assert**:
- Exit code is non-zero.
- Exactly one daemon process exists in the process table (check via PID file).
- stderr (or stdout) contains a message indicating a daemon is already running.

### T-DAEMON-I-03: Daemon child creates fresh Tokio runtime after setsid (AC-19)
**Risk**: R-04, C-01
**Arrange**: Code review / grep.
**Act**: `grep -n 'tokio\|Runtime' crates/unimatrix-server/src/infra/daemon.rs`
**Assert**: No Tokio-related symbols appear in `run_daemon_launcher` or in code paths
executed before `nix::unistd::setsid()` in `prepare_daemon_child`. The `#[tokio::main]`
or equivalent must only be reached after the `--daemon-child` branch completes
`prepare_daemon_child`.

### T-DAEMON-I-04: Daemon log file created in append mode
**Risk**: operational (FR-15)
**Arrange**: Start daemon; stop it; start daemon again.
**Act**: Check log file after second restart.
**Assert**: Log file exists; content from first run is preserved (not truncated).

### T-DAEMON-I-05: Stale MCP socket unlinked at daemon startup via `--daemon-child` path (AC-16)
**Risk**: R-09
**Arrange**: Create a stale `unimatrix-mcp.sock` file (no listening process).
**Act**: Invoke `unimatrix serve --daemon`.
**Assert**:
- Launcher exits 0.
- A new socket exists at the path and accepts connections.
- No error about "address already in use" in daemon log.
**Note**: This verifies `handle_stale_socket` is applied in the `--daemon-child` path,
not just in the explicit `serve --daemon` launcher.

---

## Edge Cases

### T-DAEMON-E-01: `--daemon-child` flag is hidden from help (RV-03 partial, R-17)
**Assert**: `unimatrix --help` output does not contain the string `daemon-child`.
`unimatrix serve --help` output does not contain the string `daemon-child`.
**Test form**: Parse CLI help output in Stage 3c shell test.

### T-DAEMON-E-02: Daemon directory created if not present
**Arrange**: Fresh system with no `~/.unimatrix/{hash}/` directory.
**Act**: Invoke `unimatrix serve --daemon`.
**Assert**: Directory is created; daemon starts successfully.

### T-DAEMON-E-03: Home directory too long triggers clear error at socket bind
**Risk**: R-14 (partial — the validation is in mcp_listener; daemonizer is the caller)
**Arrange**: Construct `ProjectPaths` with `data_dir` such that the resulting
`unimatrix-mcp.sock` path exceeds 107 bytes.
**Act**: Attempt daemon startup.
**Assert**: Daemon exits non-zero with a message containing "path too long" or
equivalent; no cryptic OS bind error is surfaced.

---

## Assertions Summary

| Test | AC/RV | Risk |
|------|-------|------|
| T-DAEMON-U-02 (grep) | AC-19 | R-12 |
| T-DAEMON-U-04 (timeout returns Err) | — | R-08 |
| T-DAEMON-I-01 (start exits 0, socket exists) | AC-01 | R-04, R-09 |
| T-DAEMON-I-02 (second start non-zero) | AC-07 | R-04 |
| T-DAEMON-I-03 (no tokio before setsid) | AC-19 | R-04 |
| T-DAEMON-I-05 (stale socket unlinked) | AC-16 | R-09 |
| T-DAEMON-E-01 (--daemon-child hidden) | RV-03 | R-17 |
