# Test Plan: MCP Session Acceptor (`uds/mcp_listener.rs`)

Component responsibility: bind `unimatrix-mcp.sock` (0600), run an accept loop that
spawns a per-session task for each incoming `UnixStream`, enforce
`MAX_CONCURRENT_SESSIONS = 32`, sweep finished handles with `retain(is_finished)` on
every iteration, and cleanly break the loop when `shutdown_token` is cancelled.

Risk coverage: R-02, R-06, R-09, R-10, R-11, R-14.
Additional ACs: AC-02, AC-14, AC-16, AC-20.
RV items: RV-02, RV-05, RV-08.

---

## Unit Tests

### T-LISTEN-U-01: `start_mcp_uds_listener` binds socket with 0600 permissions (AC-02, R-06)
**Arrange**: Call `start_mcp_uds_listener(path, server, token)` with a temp path.
**Act**: Inspect the created socket file permissions immediately after the call returns.
**Assert**:
- File exists at the given path.
- `std::fs::metadata(path).unwrap().permissions().mode() & 0o777 == 0o600`.
- Group-read bit (0o040) is zero. Other-read bit (0o004) is zero.
**Note**: Permissions must be set BEFORE `accept()` is called. This is verifiable
because `start_mcp_uds_listener` returns only after the socket is bound and listening;
the test inspects permissions before any connection is attempted.

### T-LISTEN-U-02: Socket path length validation — boundary values (RV-05, R-14)
**Arrange**: Unit test `validate_socket_path_length(path: &Path) -> Result<(), ServerError>`.
**Act**:
1. Path of exactly 103 bytes → `Ok(())`.
2. Path of exactly 104 bytes → implementation-defined (Linux: `Ok`; macOS: fail).
   Assert it does not panic; returns a `Result`.
3. Path of 107 bytes → `Err(...)` containing "path too long" or equivalent message.
4. Path of 50 bytes → `Ok(())`.
**Assert**: Boundary conditions produce the correct `Ok`/`Err` without panic.

### T-LISTEN-U-03: `retain(is_finished)` sweep bounds handle Vec size (RV-02, R-10)
**Arrange**: Construct a mock acceptor loop that tracks `session_handles: Vec<JoinHandle>`.
Spawn 100 tasks; immediately mark them as finished via `abort()` + `await`.
**Act**: After all 100 tasks complete, run one iteration of the accept loop (which
triggers `retain`).
**Assert**: `session_handles.len() <= 1` after the retain sweep — not 100.
**Test form**: Tokio unit test with `#[tokio::test]`.

### T-LISTEN-U-04: Session count enforced — 33rd connection immediately dropped (AC-20, R-11)
**Arrange**: Start `start_mcp_uds_listener` with a real `UnixListener` in a test.
Open 32 concurrent `UnixStream::connect` tasks; await all accepted.
**Act**: Open a 33rd `UnixStream::connect`.
**Assert**:
- The 33rd stream connects at the OS level (OS-level accept queue) but is immediately
  dropped by the daemon — the reading end sees EOF or a closed stream.
- The existing 32 sessions remain connected and responsive (send a ping, receive pong).
- Daemon logs a `warn!` message for the dropped connection.

### T-LISTEN-U-05: `shutdown_token` cancellation breaks accept loop (R-02)
**Arrange**: Start `start_mcp_uds_listener` with a real listener and a `CancellationToken`.
**Act**: Cancel the token; await the returned `JoinHandle`.
**Assert**:
- The acceptor task completes (does not hang).
- The `JoinHandle` returns without error.
- No new connections are accepted after the token is cancelled.

### T-LISTEN-U-06: Session spawned just before shutdown token fires is tracked (R-02)
**Arrange**: Start acceptor; prepare an incoming connection timed to arrive simultaneously
with token cancellation.
**Act**: Cancel the token and connect concurrently.
**Assert**: Either the connection is rejected before spawning a task, OR the task is
added to `session_handles` and subsequently joined. In either case, graceful_shutdown
sees no untracked Arc clone. No panic from `Arc::try_unwrap`.
**Note**: This is a concurrency stress test. Use `tokio::time::pause()` and manual
wakers or `sleep(Duration::from_millis(1))` injection points to exercise the race.

---

## Integration Tests (AC-level)

### T-LISTEN-I-01: MCP socket permissions are 0600 after daemon start (AC-02, R-06)
**Arrange**: Start daemon in integration test environment.
**Act**: `stat -c '%a' ~/.unimatrix/{hash}/unimatrix-mcp.sock` (or Python `oct(stat.S_IMODE(...))`).
**Assert**: Mode is `0o600`. Test must capture permissions before any bridge connects.

### T-LISTEN-I-02: Concurrent sessions — no data corruption (AC-14, R-05 partial)
**Arrange**: Start daemon; open 4 concurrent bridge connections.
**Act**: Each bridge calls `context_store` with a unique `topic` key, then
`context_get` for that same key.
**Assert**:
- All 4 `context_get` calls return the correct entry.
- No SQLite error or panic appears in daemon logs.
- All 4 sessions complete successfully.

### T-LISTEN-I-03: Stale MCP socket unlinked at daemon startup (AC-16, R-09)
**Arrange**: Create a plain file (not a socket) at `~/.unimatrix/{hash}/unimatrix-mcp.sock`.
No listening process.
**Act**: Invoke `unimatrix serve --daemon`.
**Assert**:
- Launcher exits 0.
- New socket is bound at the same path.
- A bridge connecting after launcher exit receives a valid MCP initialize response.

### T-LISTEN-I-04: Session cap — 33rd connection rejected, 32 unaffected (AC-20, R-11)
**Arrange**: Start daemon; open 32 concurrent bridge connections (32 separate
`UnixStream::connect` calls, each kept open without EOF).
**Act**: Attempt a 33rd `UnixStream::connect`; attempt to send an MCP initialize request.
**Assert**:
- 33rd connection receives immediate EOF (or ECONNREFUSED from the daemon closing it).
- The 32 existing sessions remain active; a context_search to one returns a valid response.
- Daemon log contains a `warn!`-level message about the dropped connection.

### T-LISTEN-I-05: After SIGKILL, both socket files exist; next daemon cleans up (RV-08, R-09, R-16)
**Arrange**: Start daemon; kill with SIGKILL (no graceful shutdown).
**Assert** (immediately after kill):
- `unimatrix-mcp.sock` exists on disk.
- `unimatrix.sock` exists on disk.
**Act**: Start a new daemon.
**Assert**:
- New daemon starts successfully (launcher exits 0).
- Both sockets are cleaned up and re-bound.
- A new bridge connection receives a valid MCP initialize response.

### T-LISTEN-I-06: Hook IPC socket permissions unchanged after adding second socket (R-06 partial)
**Arrange**: Start daemon.
**Act**: `stat ~/.unimatrix/{hash}/unimatrix.sock`.
**Assert**: Hook socket mode is still `0o600`. The addition of the MCP socket did not
change the hook socket's permissions.

---

## Edge Cases

### T-LISTEN-E-01: Session task panic does not kill other sessions
**Arrange**: Start daemon with a test-injected session handler that panics after
accepting the first tool call.
**Act**: Send a tool call to the first session (triggers panic); send a tool call to
a second session.
**Assert**: Second session receives a valid response. Daemon does not exit. The
JoinHandle for the first session returns `Err(JoinError::panicked(...))`, logged at
error level.
**Note**: Requires a test-only injection point or a real tool call that can be made
to panic (e.g., malformed binary input — but MCP parser guards against this).
In practice this is partially a code-review assertion that session task panics are
caught at the JoinHandle level, not propagated to the daemon token.

### T-LISTEN-E-02: 100 sequential connect/disconnect cycles do not grow handle Vec (RV-02, R-10)
**Arrange**: Start daemon acceptor in test harness.
**Act**: Open connection, close it immediately (EOF), repeat 100 times sequentially.
**Assert**: After all 100 cycles, `session_handles.len()` is at most 1 (or 0 if the
last handle was reaped). Not 100.
**Measurement**: Capture `session_handles.len()` via a test-exposed counter or by
inspecting the Vec after the last retain sweep.

### T-LISTEN-E-03: ECONNREFUSED during daemon startup poll window is retried (edge case, bridge side)
**Arrange**: Sequence a scenario where the socket file appears (path exists) before
`listen()` is called (TOCTOU window).
**Act**: Bridge attempts to connect during the window.
**Assert**: Bridge retries the connection (does not treat ECONNREFUSED as permanent
failure during the poll window) and eventually connects successfully.
**Note**: This is partially a bridge concern; the listener must call `listen()` before
signaling readiness. Documented here as an integration boundary.

---

## Assertions Summary

| Test | AC/RV | Risk |
|------|-------|------|
| T-LISTEN-U-01 (0600 permissions) | AC-02 | R-06 |
| T-LISTEN-U-02 (path length validation) | RV-05 | R-14 |
| T-LISTEN-U-03 (retain bounds Vec) | RV-02 | R-10 |
| T-LISTEN-U-04 (33rd dropped) | AC-20 | R-11 |
| T-LISTEN-U-05 (shutdown token breaks loop) | — | R-02 |
| T-LISTEN-U-06 (spawn race tracked or rejected) | — | R-02 |
| T-LISTEN-I-01 (stat 0600 integration) | AC-02 | R-06 |
| T-LISTEN-I-02 (concurrent sessions no corruption) | AC-14 | R-05 |
| T-LISTEN-I-03 (stale socket unlinked) | AC-16 | R-09 |
| T-LISTEN-I-04 (session cap + 32 unaffected) | AC-20 | R-11 |
| T-LISTEN-I-05 (SIGKILL, then clean restart) | RV-08 | R-09, R-16 |
| T-LISTEN-E-02 (100 cycles, Vec bounded) | RV-02 | R-10 |
