# Test Plan: Shutdown Signal Router (`infra/shutdown.rs`)

Component responsibility: decouple session-end from daemon-end; add
`mcp_socket_guard: Option<SocketGuard>` and `mcp_acceptor_handle: Option<JoinHandle<()>>`
to `LifecycleHandles`; ensure `graceful_shutdown` is called exactly once, only from
the daemon token cancellation path, never from `QuitReason::Closed` in a session task.

Risk coverage: R-01, R-02, R-03, R-12, R-16.
Additional ACs: AC-04, AC-08, AC-09, AC-12.
RV items: RV-01, RV-07, RV-09, RV-10.

---

## Unit Tests (Static / Grep)

### T-SHUT-U-01: Exactly one `graceful_shutdown` call site exists (RV-09, R-03)
**Risk**: R-03
**Arrange**: After implementation.
**Act**: `grep -rn 'graceful_shutdown' crates/unimatrix-server/src/`
**Assert**:
- Exactly one match.
- The match is NOT inside any session task spawn closure (i.e., not inside code
  reachable from `waiting().await` returning `QuitReason::Closed`).
- The match IS reachable only from the daemon token cancellation branch.
**Test form**: Automated grep in Stage 3c. Report the exact call site line.

### T-SHUT-U-02: `QuitReason::Closed` branch does NOT call `graceful_shutdown` (R-03)
**Risk**: R-03
**Arrange**: Inspect the session task body (the code that calls `waiting().await`).
**Act**: `grep -A5 'QuitReason::Closed' crates/unimatrix-server/src/`
**Assert**: The `Closed` arm contains only task cleanup (drop, log) — no call to
`graceful_shutdown`, no cancellation of the daemon token.
**Test form**: Code review + grep in Stage 3c.

### T-SHUT-U-03: `LifecycleHandles` has `mcp_socket_guard` and `mcp_acceptor_handle` fields
**Arrange**: Inspect the struct definition.
**Act**: `grep -A20 'struct LifecycleHandles' crates/unimatrix-server/src/infra/shutdown.rs`
**Assert**: Both fields are present with correct types:
- `mcp_socket_guard: Option<SocketGuard>`
- `mcp_acceptor_handle: Option<tokio::task::JoinHandle<()>>`

### T-SHUT-U-04: Drop ordering — mcp_socket_guard dropped before PidGuard (R-16, integration)
**Risk**: R-16
**Arrange**: Inspect `graceful_shutdown` body.
**Act**: Confirm the drop sequence in code:
1. `mcp_acceptor_handle` joined or aborted.
2. `mcp_socket_guard` dropped.
3. `socket_guard` (hook IPC) dropped.
4. All `Arc<Store>` holders (ServiceLayer, etc.) dropped.
5. `PidGuard` dropped last.
**Assert**: No explicit `drop(pid_guard)` appears before `drop(mcp_socket_guard)` or
`drop(socket_guard)`. If implicit drop ordering is relied upon, field declaration order
in `LifecycleHandles` is verified (Rust drops struct fields in declaration order).
**Test form**: Code review in Stage 3c.

---

## Integration Tests

### T-SHUT-I-01: Daemon survives client disconnect (AC-04, R-03)
**Risk**: R-03
**Arrange**: Start daemon; connect a bridge; send MCP `initialize`; note daemon PID.
**Act**: Close the bridge stdin (simulate Claude Code session end / EOF).
**Assert**:
- Wait 2 seconds; `kill -0 {daemon_pid}` exits 0 (daemon still alive).
- Open a second bridge; send MCP `initialize`.
- Second bridge receives a valid `initialize` response.
- Daemon's uptime or tick counter shows no reset (state preserved).
**This is the primary regression test for R-03.**

### T-SHUT-I-02: Three sequential client disconnects — daemon survives all (R-03)
**Risk**: R-03
**Arrange**: Start daemon.
**Act**: Open bridge A → initialize → close. Open bridge B → initialize → close.
Open bridge C → initialize → close.
**Assert**: After each disconnect, daemon is still alive (`kill -0`). After all three,
daemon responds to a fourth bridge connection's `initialize` request.

### T-SHUT-I-03: SIGTERM triggers graceful shutdown and process exits (AC-08, R-01)
**Risk**: R-01
**Arrange**: Start daemon; call `context_store` for one entry; wait for ack response.
**Act**: Send SIGTERM to the daemon PID.
**Assert**:
- Daemon exits within 30 seconds.
- Exit code is 0.
- Start a fresh daemon; call `context_get` for the stored entry.
- Entry is retrievable (DB was flushed before exit — vector dump + DB compaction ran).

### T-SHUT-I-04: SIGINT triggers the same graceful shutdown as SIGTERM (AC-09, R-01)
**Risk**: R-01
**Arrange**: Same setup as T-SHUT-I-03.
**Act**: Send SIGINT instead of SIGTERM.
**Assert**: Same assertions as T-SHUT-I-03 — daemon exits cleanly, data persists.

### T-SHUT-I-05: Graceful shutdown with 4 concurrent sessions (AC-08, RV-01, R-01)
**Risk**: R-01
**Arrange**: Start daemon; open 4 concurrent bridge connections; all sending MCP
`initialize`.
**Act**: Send SIGTERM to daemon while all 4 sessions are active.
**Assert**:
- All 4 session task handles are joined before `graceful_shutdown` is called.
- Daemon exits within 30 seconds.
- `Arc::strong_count(&store) == 1` immediately before `graceful_shutdown` executes
  (verify via test-only instrumentation or post-hoc inference from successful shutdown).
**Note**: The `Arc::strong_count` assertion (RV-01) requires either a test-only
counter or a unit-level test harness (T-SERVER-U-02). At integration level, "shutdown
completes without panic" is the observable proxy for the invariant.

### T-SHUT-I-06: Graceful shutdown — both socket files removed (RV-07, R-16)
**Risk**: R-16
**Arrange**: Start daemon.
**Act**: Send SIGTERM; wait for daemon to exit.
**Assert**:
- `unimatrix-mcp.sock` does NOT exist.
- `unimatrix.sock` does NOT exist.
- PID file does NOT exist.
All three are cleaned up by the end of graceful_shutdown.

### T-SHUT-I-07: `unimatrix serve --stdio` exits when stdin closes (AC-12, R-12)
**Risk**: R-12 — primary regression gate
**Arrange**: Invoke `unimatrix serve --stdio` with piped MCP `initialize`.
**Act**: Close stdin after `initialize` response is received.
**Assert**:
- Process exits within 5 seconds.
- Exit code is 0.
- No `unimatrix-mcp.sock` file was created.
- Exit is triggered by `QuitReason::Closed` → graceful shutdown on the stdio path.

### T-SHUT-I-08: `unimatrix serve --stdio` exits on SIGTERM (RV-10, R-12)
**Risk**: R-12
**Arrange**: Invoke `unimatrix serve --stdio`; send MCP `initialize`.
**Act**: Send SIGTERM to the `serve --stdio` process.
**Assert**:
- Process runs graceful shutdown sequence and exits within 30 seconds.
- Exit code is 0.
- Data stored during the session is persisted (same assertion as T-SHUT-I-03).

### T-SHUT-I-09: `serve --stdio` and a running daemon do not conflict (R-12)
**Risk**: R-12
**Arrange**: Start daemon; then start a separate `unimatrix serve --stdio` process.
**Act**: Send MCP `initialize` to the stdio process; close its stdin.
**Assert**:
- Stdio process exits cleanly.
- Daemon remains running (no shared socket conflict — stdio mode creates no
  `unimatrix-mcp.sock`).

### T-SHUT-I-10: Shutdown with session in mid-tool-call completes within 30s (NFR-06)
**Risk**: R-01 (partial — edge case: session holds Arc during long operation)
**Arrange**: Start daemon; open session; trigger a tool call that involves
`spawn_blocking` (e.g., `context_store` with embedding).
**Act**: Send SIGTERM during the blocking operation.
**Assert**:
- Daemon exits within 30 seconds.
- Session join timeout fires if the blocking task does not complete; daemon exits
  anyway with a warn log.

---

## Assertions Summary

| Test | AC/RV | Risk |
|------|-------|------|
| T-SHUT-U-01 (single graceful_shutdown call site) | RV-09 | R-03 |
| T-SHUT-U-02 (Closed branch no shutdown call) | — | R-03 |
| T-SHUT-I-01 (daemon survives disconnect) | AC-04 | R-03 |
| T-SHUT-I-02 (3 sequential disconnects) | AC-04 | R-03 |
| T-SHUT-I-03 (SIGTERM exits + data persists) | AC-08 | R-01 |
| T-SHUT-I-04 (SIGINT same as SIGTERM) | AC-09 | R-01 |
| T-SHUT-I-05 (shutdown with 4 concurrent sessions) | AC-08, RV-01 | R-01, R-02 |
| T-SHUT-I-06 (both sockets removed after SIGTERM) | RV-07 | R-16 |
| T-SHUT-I-07 (stdio exits on stdin close) | AC-12 | R-12 |
| T-SHUT-I-08 (stdio exits on SIGTERM) | RV-10 | R-12 |
