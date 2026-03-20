# Test Plan: `hook_client.py` (D6)

**Component**: `product/test/infra-001/harness/hook_client.py`
**Class under test**: `UnimatrixHookClient`
**AC coverage**: AC-12 (ping), AC-13 (session lifecycle), AC-14 (payload size limit)
**Risk coverage**: R-05 (framing byte order), R-07 (live/offline separation), R-13 (size guard fires after send)

---

## Implementation Note

`hook_client.py` and `tests/test_eval_hooks.py` together constitute the D6 deliverable. The hook IPC wire protocol uses **4-byte big-endian length prefix + JSON body** (C-05, `unimatrix_engine::wire`). This is distinct from the MCP UDS framing used by D5. Tests must not confuse the two protocols.

`UnimatrixHookClient` accepts `socket_path` as a constructor argument. The `daemon_server` fixture yields `socket_path` which is `ProjectPaths.socket_path` — the hook IPC socket at `{data_dir}/unimatrix.sock`. The MCP socket is `ProjectPaths.mcp_socket_path` — they must not be swapped.

---

## Unit Tests (Python, no daemon required)

These tests exercise client-side validation logic only.

### Test: `test_hook_oversized_payload_rejected_before_send`

**Purpose**: AC-14, R-13 — payload exceeding `MAX_PAYLOAD_SIZE` (1 MiB = 1,048,576 bytes) raises `ValueError` before any socket write.
**Arrange**: Construct a payload of exactly 1,048,577 bytes (e.g., `"x" * 1_048_577`). Mock the socket's `send` / `sendall` method to detect if it is called.
**Act**: Call any `UnimatrixHookClient` method with this oversized payload (e.g., `pre_tool_use` with a large `input` dict).
**Assert**:
- `ValueError` is raised.
- Error message is descriptive (mentions 1 MiB limit or `MAX_PAYLOAD_SIZE`).
- The mocked socket `send` method was NOT called (zero bytes sent).
**Risk**: R-13, AC-14

### Test: `test_hook_payload_exactly_at_limit_accepted`

**Purpose**: Boundary — payload of exactly 1,048,576 bytes does not raise `ValueError`.
**Arrange**: Construct a payload of exactly `MAX_PAYLOAD_SIZE` bytes.
**Act**: Call `_send` with this payload.
**Assert**: No `ValueError` raised from size guard. (Socket connect may fail if daemon not running — that is a different exception class.)
**Risk**: R-13 boundary

### Test: `test_hook_framing_big_endian_byte_order`

**Purpose**: R-05 — write framing uses big-endian 4-byte length prefix, not little-endian.
**Arrange**: Create a known payload: `b'{"type":"Ping"}'` (15 bytes). Capture the raw bytes that `_send` would write.
**Act**: Call `struct.pack('>I', 15)` and compare against the first 4 bytes that the client would emit.
**Assert**:
- First 4 bytes are `b'\x00\x00\x00\x0f'` (big-endian representation of 15).
- NOT `b'\x0f\x00\x00\x00'` (little-endian).
- Full wire bytes are `b'\x00\x00\x00\x0f' + b'{"type":"Ping"}'`.
**Risk**: R-05 (Critical)

### Test: `test_hook_read_framing_big_endian`

**Purpose**: R-05 — response reading uses big-endian for the 4-byte length header.
**Arrange**: Construct a mock socket that returns `b'\x00\x00\x00\x10' + b'{"type":"Pong"}'` (16-byte body).
**Act**: Call the internal `_recv` or `_read_response` method.
**Assert**: Parses correctly and returns an object with `type == "Pong"`.
**Risk**: R-05

---

## Integration Tests (Python, daemon required via `daemon_server` fixture)

Location: `product/test/infra-001/tests/test_eval_hooks.py`

All tests require the `daemon_server` fixture. Fixture yields `socket_path` (hook IPC) and `mcp_socket_path` (for UDS verification).

### Test: `test_hook_ping_pong`

**Purpose**: AC-12, R-05 — `ping()` returns a `HookResponse` with `type = "Pong"` within timeout.
**Arrange**: Get `socket_path` from `daemon_server`.
**Act**:
```python
client = UnimatrixHookClient(socket_path)
response = client.ping()
```
**Assert**: `response.type == "Pong"` (or equivalent field name per `HookResponse` definition). No timeout exception. No connection error.
**Risk**: AC-12, R-05

### Test: `test_hook_session_lifecycle`

**Purpose**: AC-13 — `session_start` + `session_stop` round-trip succeeds without error.
**Arrange**: `session_id = "test-session-123"`, `feature_cycle = "nan-007"`, `agent_role = "tester"`.
**Act**:
1. `start_resp = client.session_start(session_id, feature_cycle, agent_role)`.
2. `stop_resp = client.session_stop(session_id, "completed")`.
**Assert**: Both calls return `HookResponse` objects. No exception. `start_resp` and `stop_resp` indicate success (non-error type field).
**Risk**: AC-13

### Test: `test_hook_session_visible_in_status`

**Purpose**: AC-13 — session record is visible in the database after `session_stop`, verifiable via `context_status`.
**Arrange**: Use both `UnimatrixHookClient(socket_path)` and `UnimatrixUdsClient(mcp_socket_path)`.
**Act**:
1. `client.session_start("sess-visibility", "nan-007", "tester")`.
2. `client.session_stop("sess-visibility", "completed")`.
3. `status = uds_client.context_status()`.
**Assert**: The status response contains information about the session `"sess-visibility"` (or the count of sessions is incremented). The exact field depends on what `context_status` returns for sessions — assert non-zero session count or session ID present.
**Risk**: AC-13

### Test: `test_hook_session_keywords_populated`

**Purpose**: FR-40 (col-022) — keywords field is populated for sessions created via hook client.
**Arrange**: `session_start` with a `feature_cycle` that triggers keyword population (per col-022 behavior).
**Act**:
1. `client.session_start("sess-keywords", "nan-007", "tester")`.
2. `client.session_stop("sess-keywords", "completed")`.
3. Query session keywords via `uds_client.context_cycle_review("nan-007")` or equivalent.
**Assert**: Response contains keywords field for the session. Keywords are non-empty strings.
**Risk**: FR-40

### Test: `test_hook_pre_post_tool_use`

**Purpose**: FR-37 — `pre_tool_use` and `post_tool_use` calls succeed.
**Arrange**: Active session `"sess-tools"` started.
**Act**:
1. `client.session_start("sess-tools", "nan-007", "tester")`.
2. `client.pre_tool_use("sess-tools", "context_search", {"query": "test"})`.
3. `client.post_tool_use("sess-tools", "context_search", 100, "result snippet")`.
4. `client.session_stop("sess-tools", "completed")`.
**Assert**: All 4 calls return `HookResponse` objects without exceptions.
**Risk**: FR-37

### Test: `test_hook_oversized_payload_rejected_before_send_integration`

**Purpose**: AC-14 integration-level — oversized payload raises `ValueError` before socket send even with real daemon running.
**Arrange**: Connect `UnimatrixHookClient` to live daemon.
**Act**: Construct a payload of 1,048,577 bytes and attempt any method call.
**Assert**: `ValueError` raised before any bytes sent. Client is still usable for `ping()` after the rejection.
**Risk**: R-13, AC-14

### Test: `test_hook_oversized_payload_client_still_usable`

**Purpose**: R-13 — after a size-guard rejection, the client is still usable.
**Arrange**: Connect to live daemon.
**Act**:
1. Attempt oversized payload → `ValueError` raised.
2. `client.ping()`.
**Assert**: `ping()` returns `HookResponse` with `type == "Pong"`. No socket corruption.
**Risk**: R-13

### Test: `test_hook_malformed_json_payload`

**Purpose**: FR-40 — sending a malformed JSON payload (via `send_raw` if exposed) is handled gracefully.
**Arrange**: Connect to live daemon. Manually send `struct.pack('>I', 3) + b"abc"` (not valid JSON).
**Assert**: Server returns an error response (not a hang or connection reset). Client remains usable for subsequent valid calls.
**Risk**: FR-40 (failure mode)

### Test: `test_hook_wrong_socket_produces_error`

**Purpose**: Ensure connecting to the MCP socket (wrong socket type) with hook framing produces a meaningful error.
**Arrange**: Supply `mcp_socket_path` (wrong socket) to `UnimatrixHookClient`.
**Act**: `client.ping()`.
**Assert**: An exception is raised (connection reset, framing error, or timeout). It is NOT a silent success.
**Risk**: Integration risk (two-socket / two-framing from RISK-TEST-STRATEGY.md)

---

## Integration Test Expectations (through hook IPC interface)

The hook client exercises the `unimatrix_engine::wire` protocol path. Observable effects:
- Session records in the database (verified via `context_status` through UDS client).
- Session keywords in `context_cycle_review` output.
- No writes to the analytics tables from eval-mode sessions (relevant when used alongside `EvalServiceLayer`).

The two-socket / two-framing integration risk requires explicit testing: `test_hook_wrong_socket_produces_error` guards against accidentally swapping socket paths.

---

## Edge Cases from Risk Strategy

- Payload exactly at 1 MiB limit: accepted (no `ValueError`).
- Payload at 1 MiB + 1 byte: raises `ValueError` before send.
- After a size-guard rejection, client socket is unmodified (no partial write), so subsequent valid calls succeed.
- Unicode in session IDs or feature cycle strings: must be encoded as valid UTF-8 JSON.
- Partial read on response (daemon killed mid-response): `IOError` propagated to caller. Context manager or explicit cleanup must close the socket.

---

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness testing patterns edge cases" — found entries #1204 (Test Plan Must Cross-Reference Pseudocode for Edge-Case Behavior Assertions), #157 (Test infrastructure is cumulative), #229 (Tester Role Duties)
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — found ADR-001 (hook client framing: 4-byte BE length prefix, distinct from UDS newline framing), ADR-004 (hook_client.py lives in infra-001/harness/)
Queried: /uni-query-patterns for "integration test harness patterns infra" — found entries #238 (Testing Infrastructure Convention), #748 (TestHarness Server Integration Pattern), #129 (Concrete assertions)
Stored: nothing novel to store — test plan agents are read-only; patterns are consumed not created
