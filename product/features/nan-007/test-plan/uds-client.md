# Test Plan: `uds_client.py` (D5)

**Component**: `product/test/infra-001/harness/uds_client.py`
**Class under test**: `UnimatrixUdsClient`
**AC coverage**: AC-10 (tool parity), AC-11 (context manager)
**Risk coverage**: R-04 (framing mismatch), R-07 (offline/live separation), R-14 (path length validation)

---

## Implementation Note

`uds_client.py` and `tests/test_eval_uds.py` together constitute the D5 deliverable. The test file is not merely a wrapper — it is the acceptance evidence for AC-10 and AC-11.

The client must mirror `UnimatrixClient` (in `harness/client.py`) in API surface. The primary difference is transport setup: `UnimatrixUdsClient` uses `socket.AF_UNIX SOCK_STREAM` instead of a subprocess pipe. Wire framing is identical: newline-delimited JSON (no length prefix).

---

## Unit Tests (Python, no daemon required)

Location: `product/test/infra-001/tests/test_eval_uds.py` (sections marked `# --- unit-style ---`)

These tests can run without a daemon by exercising the client's validation logic only.

### Test: `test_uds_path_too_long_rejected`

**Purpose**: R-14 (FR-31) — socket path of 104 bytes raises `ValueError` before `connect()`.
**Arrange**: Construct a string path of exactly 104 bytes (e.g., `"/tmp/" + "a" * 99`).
**Act**: `UnimatrixUdsClient(path)` or `client.connect()`.
**Assert**: `ValueError` is raised with a descriptive message mentioning the 103-byte limit. No socket `connect()` syscall is made.
**Risk**: R-14

### Test: `test_uds_path_exactly_103_accepted`

**Purpose**: R-14 — path of exactly 103 bytes does not raise a validation error.
**Arrange**: Path string of 103 bytes.
**Act**: Instantiate `UnimatrixUdsClient(path)`.
**Assert**: No `ValueError` raised (socket `connect()` will fail later if the socket doesn't exist, but that is a different error class).
**Risk**: R-14 boundary

### Test: `test_uds_path_1_byte_accepted`

**Purpose**: Short paths pass validation without error.
**Arrange**: Path = `"/s"` (2 bytes).
**Assert**: No `ValueError` from length validation.
**Risk**: R-14 lower boundary

---

## Integration Tests (Python, daemon required via `daemon_server` fixture)

Location: `product/test/infra-001/tests/test_eval_uds.py`

All tests below require the `daemon_server` fixture which starts the unimatrix daemon and yields socket paths.

### Test: `test_uds_connection_lifecycle`

**Purpose**: FR-32 — `connect()` and `disconnect()` work correctly.
**Arrange**: Get `mcp_socket_path` from `daemon_server` fixture.
**Act**:
1. `client = UnimatrixUdsClient(mcp_socket_path)`.
2. `client.connect()`.
3. Assert connection is established (no exception).
4. `client.disconnect()`.
**Assert**: No exception at any step. After `disconnect()`, the client is in a clean state.
**Risk**: FR-32

### Test: `test_uds_context_manager`

**Purpose**: AC-11 — context manager handles connect/disconnect automatically.
**Act**:
```python
with UnimatrixUdsClient(mcp_socket_path) as client:
    resp = client.context_status()
    # assert resp is valid
```
**Assert**: Connection established on `__enter__` (no prior `connect()` call). `context_status()` succeeds. Socket closed on `__exit__` without explicit `disconnect()` call.
**Risk**: AC-11

### Test: `test_uds_framing_newline_delimited`

**Purpose**: R-04 — raw bytes sent by the client are newline-delimited JSON (no 4-byte prefix).
**Arrange**: Intercept the `_send` method or use a loopback socket to capture raw bytes.
**Act**: Call `client.context_status()` (or send a manual `initialize` request).
**Assert**:
- First byte of the sent message is `b'{'` (JSON object open), not a 4-byte integer length.
- Message ends with `b'\n'`.
- The full message is valid JSON when stripped of the trailing `\n`.
**Risk**: R-04 (Critical)

### Test: `test_uds_tool_parity_search`

**Purpose**: AC-10 — `UnimatrixUdsClient.context_search()` returns the same result as `UnimatrixClient.context_search()` for the same query against the same daemon.
**Arrange**: Start daemon. Store one entry via `UnimatrixClient` stdio. Then search via both clients.
**Act**:
1. Store entry: `stdio_client.context_store("content text", "test", "pattern")`.
2. Search via UDS: `uds_client.context_search("content text", k=1)`.
3. Search via stdio: `stdio_client.context_search("content text", k=1)`.
**Assert**: Both responses contain the same entry ID in the result. Result count, entry content, and topic match.
**Risk**: AC-10

### Test: `test_uds_all_12_tools_callable`

**Purpose**: AC-10 (FR-34) — all 12 `context_*` typed methods are present and callable without protocol error.
**Arrange**: Daemon with some pre-stored entries (use `daemon_server` populated fixture if available, otherwise store entries in the test).
**Act**: Call each of the 12 typed methods with valid minimal arguments:
1. `context_search(query="test", k=1)`
2. `context_store("content", "topic", "pattern", agent_id="human")`
3. `context_lookup(category="pattern")`
4. `context_get(entry_id=<stored_id>)`
5. `context_correct(<id>, "corrected content")`
6. `context_deprecate(<id>)`
7. `context_status()`
8. `context_briefing("tester", "verify tools")`
9. `context_quarantine(<id>)` (admin fixture needed — use `admin_server` variant if available)
10. `context_enroll("test-agent", "observer", ["search"])` (admin)
11. `context_cycle("start", "test-topic")`
12. `context_cycle_review("nan-007")`
**Assert**: Each call returns an `MCPResponse` with no `error` field set. No `TimeoutError` raised.
**Risk**: AC-10

### Test: `test_uds_concurrent_clients`

**Purpose**: FR-35 — multiple UDS clients can connect simultaneously to the same daemon.
**Arrange**: Get `mcp_socket_path` from `daemon_server`.
**Act**: Open 3 `UnimatrixUdsClient` instances simultaneously (using `threading.Thread`). Each performs a `context_status()` call.
**Assert**: All 3 calls complete without error. All return valid responses. No socket contention or hang.
**Risk**: FR-35

### Test: `test_uds_query_logged_as_source_uds`

**Purpose**: FR-35 — UDS-sourced queries appear with `source="uds"` in `query_log`.
**Arrange**: Daemon with a snapshot-capable project dir. Execute a `context_search` via `UnimatrixUdsClient`.
**Act**:
1. `uds_client.context_search("uds test query", k=3)`.
2. Inspect the `query_log` table in the live database (via direct SQLite read or via `context_status`).
**Assert**: At least one `query_log` row with `query` text `"uds test query"` has `source = "uds"`.
**Risk**: FR-35

### Test: `test_uds_socket_not_found_connection_error`

**Purpose**: Failure mode — socket file does not exist → descriptive error, not opaque OS error.
**Act**: `UnimatrixUdsClient("/tmp/does_not_exist_ever.sock").connect()`.
**Assert**: Raises a `ConnectionError` (or `OSError`) with the socket path in the message. Not a bare `FileNotFoundError` without context.
**Risk**: Failure mode table in RISK-TEST-STRATEGY.md

### Test: `test_uds_daemon_close_connection_propagated`

**Purpose**: Failure mode — if the daemon closes the connection mid-session, `IOError` is propagated to the caller.
**Act**: Connect, then forcibly close the daemon (or use a mock). Attempt another tool call.
**Assert**: Raises `IOError` or `BrokenPipeError`. Context manager `__exit__` is called and does not raise a secondary exception.
**Risk**: Failure mode

---

## Integration Test Expectations (through MCP interface)

`UnimatrixUdsClient` behavior is fully visible through the MCP interface. Every tool call result is the same response the daemon would return over stdio. The parity test (`test_uds_tool_parity_search`) is the primary integration-level assertion.

The framing test (`test_uds_framing_newline_delimited`) is critical because a length-prefix bug would cause all 12 tool calls to fail with an opaque connection reset — the parity test alone would not distinguish a framing bug from a tool-level bug.

---

## Edge Cases from Risk Strategy

- Socket path exactly 103 bytes: must pass. 104 bytes: must fail with descriptive `ValueError`.
- Unicode characters in tool arguments: `json.dumps` must emit UTF-8, not ASCII-escaped `\uXXXX` sequences (unless the receiving end is tolerant — document behavior).
- Connection to wrong socket (hook socket path vs MCP socket path): the MCP `initialize` handshake will fail with a framing error. The client must surface this as a meaningful error, not hang.
- Context manager `__exit__` called with an exception: must not raise a secondary exception.

---

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness testing patterns edge cases" — found entries #1204 (Test Plan Must Cross-Reference Pseudocode for Edge-Case Behavior Assertions), #157 (Test infrastructure is cumulative), #229 (Tester Role Duties)
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — found ADR-001 (UDS framing: newline-delimited JSON, no length prefix), ADR-004 (uds_client.py lives in infra-001/harness/)
Queried: /uni-query-patterns for "integration test harness patterns infra" — found entries #238 (Testing Infrastructure Convention), #748 (TestHarness Server Integration Pattern), #157 (Test infrastructure is cumulative)
Stored: nothing novel to store — test plan agents are read-only; patterns are consumed not created
