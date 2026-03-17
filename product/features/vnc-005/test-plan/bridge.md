# Test Plan: Bridge Client (`bridge.rs`)

Component responsibility: default no-subcommand path. Attempt `UnixStream::connect`
to `unimatrix-mcp.sock`. If connected, enter `copy_bidirectional(stdin, uds_stream)`.
If not connected, run auto-start sequence: check PID via `is_unimatrix_process`, spawn
`unimatrix serve --daemon` if stale, poll for socket at 250ms intervals for 5s, then
bridge. On timeout, write error including log file path to stderr and exit 1.

Risk coverage: R-04, R-08.
Additional ACs: AC-03, AC-05, AC-06, AC-15.

---

## Unit Tests

### T-BRIDGE-U-01: `run_bridge` connects to existing socket and proxies MCP traffic (AC-03)
**Arrange**: Start a real daemon (or mock UDS server). Socket exists at the expected path.
**Act**: Invoke `run_bridge(socket_path, log_path)` with piped MCP `initialize` JSON.
**Assert**:
- `run_bridge` returns `Ok(())` after stdin closes.
- A valid MCP `initialize` response was written to stdout before stdin closed.
- No daemon was spawned (no new process).

### T-BRIDGE-U-02: `run_bridge` exits with error when socket never appears (AC-15, R-08)
**Risk**: R-08
**Arrange**: Configure `run_bridge` to point to a socket path that will never be
created. Provide a valid `log_path`.
**Act**: Invoke `run_bridge(nonexistent_socket_path, log_path)`.
**Assert**:
- Returns `Err(...)` or writes to stderr + exits non-zero.
- The timeout fires within approximately 5–7 seconds (not immediately, not > 10s).
- stderr output contains the `log_path` string.
- stderr output contains a human-readable explanation (e.g., "timed out", "failed to
  start", or equivalent — not a raw OS error code).

### T-BRIDGE-U-03: Auto-start does not spawn daemon when healthy daemon already running (AC-06, R-04)
**Risk**: R-04
**Arrange**: Start daemon (daemon is healthy; PID file present;
`is_unimatrix_process(pid)` returns true). Count processes before.
**Act**: Invoke `run_bridge` (bridge path: connects to existing socket).
**Assert**:
- Bridge connects successfully.
- Process count for unimatrix-server is unchanged (no duplicate spawn).
- `is_unimatrix_process` was called (verifiable via mock or log inspection).

### T-BRIDGE-U-04: Auto-start spawns daemon when no daemon running (AC-05, R-04)
**Risk**: R-04
**Arrange**: No daemon running; socket path does not exist; PID file absent.
**Act**: Invoke `run_bridge` with piped MCP `initialize`.
**Assert**:
- A `unimatrix serve --daemon` child process is spawned.
- Socket appears within 5 seconds.
- Bridge connects and returns a valid `initialize` response.
- Total wall time is under 8 seconds.

### T-BRIDGE-U-05: Auto-start respects stale PID detection — spawns when PID is stale (R-04)
**Risk**: R-04
**Arrange**: Write a PID file containing a PID that belongs to a non-unimatrix process
(or a dead process). Socket does not exist.
**Act**: Invoke `run_bridge`.
**Assert**:
- `is_unimatrix_process(stale_pid)` returns false (verified via log or mock).
- Bridge spawns a fresh daemon.
- Bridge connects successfully.

### T-BRIDGE-U-06: Bridge exit propagates daemon-side stream close to Claude Code (integration risk)
**Arrange**: Start daemon; connect bridge; after bridging starts, close the daemon-side
UDS stream (simulate daemon dropping the session).
**Act**: Observe bridge behavior.
**Assert**:
- Bridge exits with a non-zero code (not silent hang).
- Bridge propagates the close to its own stdout (Claude Code sees stdin closed).
**Note**: This tests the `copy_bidirectional` EOF propagation behavior. If the daemon
closes the stream (e.g., session cap hit), the bridge must not hang.

### T-BRIDGE-U-07: Bridge carries no Unimatrix capabilities (C-06)
**Risk**: C-06 (security constraint)
**Arrange**: Inspect bridge source for any capability-bearing fields or auth tokens.
**Act**: Code review / grep: `grep -n 'capability\|UDS_CAPABILITIES\|CallerId' crates/unimatrix-server/src/bridge.rs`
**Assert**: No capability fields or auth token construction in bridge code.
Bridge is pure byte-copy with no application logic beyond auto-start.
**Test form**: Code review in Stage 3c.

---

## Integration Tests

### T-BRIDGE-I-01: `unimatrix` (no subcommand) connects to running daemon (AC-03)
**Arrange**: Start daemon.
**Act**: Pipe `{"jsonrpc":"2.0","method":"initialize","id":1,"params":{...}}` to
`unimatrix` (no subcommand).
**Assert**: stdout contains a valid MCP `initialize` response (parseable JSON with
`result.protocolVersion`).

### T-BRIDGE-I-02: Auto-start from cold (no daemon) — gets valid response within 8s (AC-05)
**Arrange**: No daemon running; no socket file.
**Act**: Pipe MCP `initialize` to `unimatrix` (no subcommand). Record start time.
**Assert**:
- Valid `initialize` response received.
- Total time (from invoke to response) under 8 seconds.
- `unimatrix-mcp.sock` exists after the test.

### T-BRIDGE-I-03: Auto-start stale PID — no duplicate daemon (AC-06, R-04)
**Arrange**: Start daemon; record its PID. Stop daemon gracefully (socket and PID file
removed). Write a fake PID file with a recycled non-unimatrix PID.
**Act**: Invoke `unimatrix` (bridge).
**Assert**:
- Bridge spawns one new daemon.
- Only one daemon process exists after bridge connects.

### T-BRIDGE-I-04: Timeout stderr message includes log file path (AC-15, R-08)
**Arrange**: Replace the daemon binary with a stub that sleeps forever and never
creates `unimatrix-mcp.sock`. Record the expected log file path.
**Act**: Invoke `unimatrix` (bridge); capture stderr. Start a 7-second timer.
**Assert**:
- Bridge exits 1 within 7 seconds.
- stderr contains the expected log file path string.
- No orphaned stub daemon process remains after the bridge exits.

### T-BRIDGE-I-05: After timeout, no orphaned daemon process (R-08)
**Arrange**: Same setup as T-BRIDGE-I-04.
**Assert**: After bridge exits 1, process table contains no orphaned stub/daemon
process spawned during auto-start.
**Note**: The stub process started by auto-start should be left running (it's a
background process), but if the stub is designed to exit, verify this. The key
point is no runaway daemon accumulates file descriptors.

---

## Edge Cases

### T-BRIDGE-E-01: ECONNREFUSED during poll window is retried (not treated as permanent failure)
**Arrange**: Create the socket file (via `touch`) before the daemon calls `listen()`.
Simulate a 200ms window between file creation and listen.
**Act**: Bridge polls and encounters ECONNREFUSED.
**Assert**: Bridge retries after 250ms; eventually connects once listen() is called.
Bridge does not exit with a connection error during the poll window.

### T-BRIDGE-E-02: Bridge exit 0 when stdin closes normally
**Arrange**: Start daemon; connect bridge; send MCP `initialize`; close stdin.
**Assert**: Bridge process exits with code 0.

### T-BRIDGE-E-03: Bridge on Windows prints clear error and exits non-zero (C-12)
**Note**: This is a platform-specific test, not runnable in the Linux CI environment.
Document as: "Bridge mode on Windows must return a non-zero exit code with message
'daemon mode not supported on Windows'. `serve --stdio` must continue to work on
Windows." Verification is manual or via a Windows CI job if one is added.

---

## Assertions Summary

| Test | AC/RV | Risk |
|------|-------|------|
| T-BRIDGE-U-02 (timeout, stderr has log path) | AC-15 | R-08 |
| T-BRIDGE-U-03 (no duplicate spawn, healthy daemon) | AC-06 | R-04 |
| T-BRIDGE-U-04 (auto-start from cold) | AC-05 | R-04 |
| T-BRIDGE-U-05 (stale PID → new daemon) | — | R-04 |
| T-BRIDGE-U-07 (no capabilities in bridge, code review) | — | C-06 |
| T-BRIDGE-I-01 (bridge connects, MCP response) | AC-03 | — |
| T-BRIDGE-I-02 (cold auto-start within 8s) | AC-05 | R-04 |
| T-BRIDGE-I-03 (stale PID, no duplicate) | AC-06 | R-04 |
| T-BRIDGE-I-04 (timeout stderr log path) | AC-15 | R-08 |
