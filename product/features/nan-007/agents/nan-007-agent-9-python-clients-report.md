# Agent Report: nan-007-agent-9-python-clients

**Component**: D5 (UnimatrixUdsClient) + D6 (UnimatrixHookClient) — Python
**Branch**: feature/nan-007
**Commit**: c0bd99e

---

## Files Created

| File | Lines | Description |
|------|-------|-------------|
| `product/test/infra-001/harness/uds_client.py` | 430 | UnimatrixUdsClient — MCP over AF_UNIX, newline-delimited JSON |
| `product/test/infra-001/harness/hook_client.py` | 270 | UnimatrixHookClient — hook IPC over AF_UNIX, 4-byte BE length prefix |
| `product/test/infra-001/tests/conftest.py` | 13 | sys.path bootstrap for tests/ directory |
| `product/test/infra-001/tests/test_eval_uds.py` | 340 | D5 test suite (unit + integration) |
| `product/test/infra-001/tests/test_eval_hooks.py` | 370 | D6 test suite (unit + integration) |

**Modified**:
- `product/test/infra-001/pytest.ini` — registered `integration` marker to silence PytestUnknownMarkWarning

---

## Test Results

Unit tests (no daemon): **38 passed, 0 failed, 14 integration deselected**

```
tests/test_eval_uds.py   — 16 unit tests (6 path validation, 7 framing, 2 context manager, 1 socket not found)
tests/test_eval_hooks.py — 22 unit tests (6 size guard, 5 framing/BE, 3 response dataclass, 3 connection, 5 typed methods)
```

Integration tests: 14 collected but skipped (require live daemon with `daemon_server` fixture).

---

## AC Coverage

| AC | Tests |
|----|-------|
| AC-10 (tool parity) | test_uds_tool_parity_search, test_uds_all_12_tools_callable |
| AC-11 (context manager) | test_uds_context_manager, test_context_manager_protocol |
| AC-12 (ping/pong) | test_hook_ping_pong |
| AC-13 (session lifecycle) | test_hook_session_lifecycle, test_hook_session_visible_in_status |
| AC-14 (payload size limit) | test_oversized_payload_rejected_before_send, test_pre_tool_use_large_input_rejected, test_hook_oversized_payload_rejected_before_send_integration |

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-04 (UDS framing) | test_send_newline_delimited, test_send_no_length_prefix |
| R-05 (hook framing byte order) | test_send_uses_big_endian_length_prefix, test_be_header_differs_from_le, test_recv_reads_big_endian_header |
| R-13 (size guard before send) | test_oversized_payload_rejected_before_send, test_client_still_usable_after_size_rejection |
| R-14 (UDS path length) | test_uds_path_too_long_rejected, test_uds_path_exactly_103_accepted, test_uds_path_validation_uses_utf8_byte_count |

---

## Design Decisions Followed

- C-04: UnimatrixUdsClient._send emits `json.dumps(msg) + "\n"` — no length prefix.
- C-05: UnimatrixHookClient._send uses `struct.pack('>I', len(payload)) + payload`.
- C-08: Path validation in `__init__` using `len(path.encode('utf-8')) > 103` — before any syscall.
- AC-14 / R-13: Size guard in `_send()` raises before `sendall()` — socket state unmodified on rejection.
- Pseudocode followed exactly; no deviations.

---

## Issues / Blockers

None. All unit tests pass. Integration tests require a daemon_server fixture that must be provided by the daemon implementation agent (supplying `socket_path` and `mcp_socket_path` keys).

The `daemon_server` fixture does not yet exist in the test harness — integration tests will be skipped until it is added (referenced as entry #1928 in Unimatrix).

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for "Python socket client UDS framing patterns" (category: pattern) — found entry #2582 (MCP UDS transport uses newline-delimited JSON) and #2603 (two-socket/two-framing test patterns). Applied both.
- Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — found ADR-001 through ADR-005. C-04, C-05, C-08 constraints applied directly.
- Stored: entry #2616 "Two-socket / two-framing test patterns for dual-socket daemon harnesses" via /uni-store-pattern — captured mock socket patterns, BytesIO recv stream technique, pytest.ini marker registration, and socket-swap guard test pattern that were not previously documented.
