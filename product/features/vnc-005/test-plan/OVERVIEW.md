# vnc-005 Test Plan Overview

## Overall Test Strategy

vnc-005 introduces daemon mode: a persistent background server, a UDS MCP session
acceptor, a stdio-to-UDS bridge, and two refactored modules (shutdown decoupling +
accumulator restructuring). The test strategy covers three levels:

1. **Unit tests** (`cargo test --workspace`): Pure-logic components testable without
   a live server. Covers `PendingEntriesAnalysis` methods, socket path validation,
   `run_stop` exit-code logic, and static invariants (call-site grep checks).

2. **Integration tests — infra-001 harness**: The existing harness runs the
   `unimatrix-server` binary over stdio. vnc-005 adds `unimatrix serve --stdio` as
   the explicit stdio mode; existing infra-001 tests remain valid against
   `serve --stdio`. New daemon-mode integration tests require shell-level test
   helpers (spawn daemon, connect bridge) and are added to `suites/test_lifecycle.py`
   and a new `suites/test_daemon.py`.

3. **Shell-level / process tests**: ACs that require a real daemon process (daemon
   start/stop, socket file checks, concurrent bridge connections) are exercised as
   integration scenarios either in infra-001 or as standalone shell assertions in
   Stage 3c.

The test plan is **risk-driven**: every Critical and High risk has at least one
dedicated test expectation. Coverage is traced through the risk-to-test mapping
table below.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Test File(s) |
|---------|----------|--------------|--------------|
| R-01 | Critical | server_refactor (shutdown), shutdown | `server_refactor.md`, `shutdown.md` |
| R-02 | Critical | mcp_listener, shutdown | `mcp_listener.md`, `shutdown.md` |
| R-03 | Critical | shutdown, server_refactor | `shutdown.md`, `server_refactor.md` |
| R-04 | Critical | daemonizer, bridge | `daemonizer.md`, `bridge.md` |
| R-05 | High | server_refactor (accumulator) | `server_refactor.md` |
| R-06 | High | mcp_listener | `mcp_listener.md` |
| R-07 | High | server_refactor | `server_refactor.md` |
| R-08 | High | bridge | `bridge.md` |
| R-09 | High | mcp_listener, daemonizer | `mcp_listener.md`, `daemonizer.md` |
| R-10 | High | mcp_listener | `mcp_listener.md` |
| R-11 | Med | mcp_listener | `mcp_listener.md` |
| R-12 | Critical | shutdown, stop_cmd | `shutdown.md`, `stop_cmd.md` |
| R-13 | Med | stop_cmd | `stop_cmd.md` |
| R-14 | Med | mcp_listener | `mcp_listener.md` |
| R-15 | Med | server_refactor (accumulator) | `server_refactor.md` |
| R-16 | Med | shutdown | `shutdown.md` |
| R-17 | Low | stop_cmd | `stop_cmd.md` |
| R-18 | Med | server_refactor (accumulator) | `server_refactor.md` |

---

## AC-to-Test Mapping

| AC-ID | Verification Method | Component Test File |
|-------|--------------------|--------------------|
| AC-01 | shell + process assertions | `daemonizer.md` |
| AC-02 | shell: stat socket mode | `mcp_listener.md` |
| AC-03 | integration: bridge connect + MCP initialize | `bridge.md` |
| AC-04 | integration: daemon survival after bridge disconnect | `shutdown.md` |
| AC-05 | integration: auto-start + bridge | `bridge.md` |
| AC-06 | integration: no duplicate spawn | `bridge.md` |
| AC-07 | shell: second daemon attempt exits non-zero | `daemonizer.md` |
| AC-08 | integration: SIGTERM graceful shutdown + persistence | `shutdown.md` |
| AC-09 | integration: SIGINT graceful shutdown | `shutdown.md` |
| AC-10 | integration: `unimatrix stop` exits 0 | `stop_cmd.md` |
| AC-11 | shell: `unimatrix stop` no daemon → non-zero | `stop_cmd.md` |
| AC-12 | integration: `serve --stdio` exits on stdin close | `shutdown.md` |
| AC-13 | shell: hook IPC unaffected | `stop_cmd.md` |
| AC-14 | integration: concurrent sessions, no corruption | `mcp_listener.md` |
| AC-15 | integration: bridge timeout stderr contains log path | `bridge.md` |
| AC-16 | integration: stale socket unlinked at startup | `mcp_listener.md` |
| AC-17 | integration: cross-session accumulation | `server_refactor.md` |
| AC-18 | integration: drain clears bucket | `server_refactor.md` |
| AC-19 | grep: no Tokio init before setsid | `daemonizer.md` |
| AC-20 | integration: 33rd session rejected, others unaffected | `mcp_listener.md` |

### RV Items (Additional Verification Requirements)

| RV-ID | Test File |
|-------|-----------|
| RV-01 | `shutdown.md` |
| RV-02 | `mcp_listener.md` |
| RV-03 | `stop_cmd.md` |
| RV-04 | `stop_cmd.md` |
| RV-05 | `mcp_listener.md` |
| RV-06 | `server_refactor.md` |
| RV-07 | `shutdown.md` |
| RV-08 | `mcp_listener.md` |
| RV-09 | `shutdown.md` |
| RV-10 | `shutdown.md` |
| RV-11 | `server_refactor.md` |
| RV-12 | `server_refactor.md` |

---

## Cross-Component Test Dependencies

1. `shutdown.md` tests depend on `mcp_listener` (must accept sessions before testing
   session-end behavior) and `server_refactor` (Arc clone count invariant).
2. `bridge.md` tests depend on `daemonizer` (daemon must start before bridge can
   connect) and `mcp_listener` (MCP socket must be bound).
3. `server_refactor.md` accumulator tests are partially standalone (unit-level) and
   partially depend on a live daemon (multi-session AC-17/AC-18 path).
4. `stop_cmd.md` tests depend on a running daemon (daemonizer + mcp_listener).

---

## Integration Harness Plan

### Which Existing Suites Apply

| Suite | Relevance to vnc-005 | Run Requirement |
|-------|----------------------|-----------------|
| `smoke` | All features — mandatory gate | MUST run |
| `protocol` | vnc-005 changes MCP transport surface (UDS); existing stdio protocol tests exercise the same MCP protocol layer via `serve --stdio` | MUST run — regression gate for MCP protocol compliance |
| `tools` | All 12 tools work through the server that is now shared across sessions; regression coverage for tool behavior unchanged | MUST run |
| `lifecycle` | `context_store` → `context_retrospective` flows exercise accumulator; restart persistence exercises shutdown/startup | MUST run |
| `edge_cases` | Unicode/boundary value tests exercise the accumulator key paths (feature_cycle strings) and concurrent ops | MUST run |
| `security` | `CallerId::UdsSession` exemption boundary (R-07) surfaces as a security concern; existing security tests exercise rate limiting | SHOULD run |
| `confidence` | No direct change; run as regression | SHOULD run |
| `contradiction` | No direct change; run as regression | SHOULD run |
| `volume` | Accumulator at scale; large bucket scenarios | SHOULD run |

Minimum gate: `pytest -m smoke` must pass before Gate 3c.
Recommended: run all suites via `python -m pytest suites/ -v --timeout=60`.

### Binary Invocation Note

The existing infra-001 harness invokes the binary as `unimatrix-server`. After
vnc-005, `unimatrix` (no subcommand) becomes bridge mode. The harness MUST invoke
`unimatrix serve --stdio` (or keep using `unimatrix-server` if the binary name is
unchanged) to exercise the stdio MCP path. Check `conftest.py` server fixture for the
invocation pattern and update if needed.

### Existing Suite Gaps (vnc-005 New Behavior)

The following daemon-mode behaviors are NOT covered by any existing infra-001 test:

| Gap | New Test Location |
|-----|------------------|
| Daemon process start/stop lifecycle | New: `suites/test_daemon.py` |
| Bridge auto-start and connection | New: `suites/test_daemon.py` |
| Daemon survival after client disconnect (AC-04) | New: `suites/test_daemon.py` |
| Concurrent multi-session store+retrieve (AC-14) | New: `suites/test_daemon.py` |
| Cross-session accumulator accumulation (AC-17/18) | New: `suites/test_daemon.py` |
| Session concurrency cap enforcement (AC-20) | New: `suites/test_daemon.py` |
| Socket file cleanup after SIGTERM (RV-07) | New: `suites/test_daemon.py` |
| `unimatrix stop` subcommand exit codes | New: `suites/test_daemon.py` |

### New Integration Tests to Add: `suites/test_daemon.py`

All tests in `test_daemon.py` require shell-level daemon management. They use a new
`daemon_server` fixture (scope: function) that:
- Spawns `unimatrix serve --daemon` and polls for `unimatrix-mcp.sock`
- Provides the socket path and daemon PID
- Tears down by calling `unimatrix stop` or `kill -SIGTERM pid` + waiting for exit

```python
# Naming convention:
def test_daemon_start_creates_socket(daemon_server): ...
def test_daemon_survive_client_disconnect(daemon_server): ...
def test_daemon_concurrent_sessions_no_corruption(daemon_server): ...
def test_daemon_stop_subcommand_exits_zero(daemon_server): ...
def test_daemon_session_cap_33rd_rejected(daemon_server): ...
def test_daemon_accumulator_cross_session(daemon_server): ...
def test_daemon_accumulator_drain_clears_bucket(daemon_server): ...
def test_daemon_socket_cleanup_after_sigterm(daemon_server): ...
def test_bridge_auto_start_no_daemon(tmp_project): ...
def test_bridge_timeout_stderr_contains_log_path(tmp_project): ...
def test_daemon_stale_socket_unlinked_at_startup(tmp_project): ...
def test_daemon_duplicate_rejected(daemon_server): ...
```

These tests require a real daemon process and cannot be run in the existing
`server` fixture (which uses stdio transport directly). They are shell/process-level
tests within the pytest harness.

### Fixture Design for New Tests

```python
# New fixtures needed in conftest.py:
@pytest.fixture
def daemon_server(tmp_path):
    """Start a daemon, yield (socket_path, pid, log_path), stop on teardown."""
    ...

@pytest.fixture
def tmp_project(tmp_path):
    """Isolated project directory with no running daemon."""
    ...
```

### Tests Requiring Real Daemon vs. Unit Harness

| Test Scenario | Requires Real Daemon? | Why |
|---------------|----------------------|-----|
| AC-04 daemon survival after disconnect | Yes | Must observe process liveness after session EOF |
| AC-14 concurrent sessions | Yes | Requires multiple simultaneous UDS connections |
| AC-17/18 cross-session accumulator | Yes | Requires two sequential bridge sessions |
| AC-20 session cap enforcement | Yes | Requires 32 live connections |
| R-10 handle Vec growth | Yes | Requires 100 sequential connect/disconnect cycles |
| AC-01 daemon start/detach | Yes | Process-level spawn and socket existence check |
| PendingEntriesAnalysis unit tests | No | Pure in-memory struct methods |
| socket path length validation | No | Pure function, no process needed |
| `--daemon-child` hidden flag | No | Binary CLI help output check |
| RV-09 single graceful_shutdown call site | No | Grep/static check |
| RV-11 C-07 comment present | No | Grep/static check |
