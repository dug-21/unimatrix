# Pseudocode: hook_client.py (D6)

**Location**: `product/test/infra-001/harness/hook_client.py`

## Purpose

Provide a Python class `UnimatrixHookClient` that connects to the Unimatrix daemon's
hook IPC socket via `socket.AF_UNIX` and sends synthetic `HookRequest` messages using
the 4-byte big-endian length-prefix + JSON body wire protocol defined in
`unimatrix_engine::wire`. Enables observation pipeline testing and GNN training
data generation (W3-1 foundation).

Wire protocol (C-05, FR-38):
- Write: `struct.pack('>I', len(payload)) + payload`
- Read: read 4 bytes → `struct.unpack('>I', header)[0]` → read N bytes → parse JSON

Socket path: `ProjectPaths.socket_path` = `{data_dir}/unimatrix.sock` (hook IPC).
This is distinct from `ProjectPaths.mcp_socket_path` (MCP UDS, used by `UnimatrixUdsClient`).

## Dependencies (all standard library)

| Module | Use |
|--------|-----|
| `socket` | `AF_UNIX SOCK_STREAM`, connect, send, recv |
| `struct` | `pack('>I', ...)` / `unpack('>I', ...)` for BE length prefix |
| `json` | Payload serialization/deserialization |
| `pathlib.Path` | Path handling |

## Constants and Types

```python
import struct, socket, json
from pathlib import Path
from dataclasses import dataclass

DEFAULT_TIMEOUT = 10.0
MAX_PAYLOAD_SIZE = 1_048_576  # 1 MiB — matches unimatrix_engine::wire (C-05, AC-14)

class HookClientError(Exception): pass

class HookTimeoutError(HookClientError):
    def __init__(self, op: str, timeout: float):
        super().__init__(f"Timeout after {timeout}s during {op}")

class HookPayloadTooLargeError(HookClientError):
    """Raised before any send when payload exceeds MAX_PAYLOAD_SIZE (AC-14, R-13)."""
    def __init__(self, size: int):
        super().__init__(
            f"payload size {size} bytes exceeds MAX_PAYLOAD_SIZE={MAX_PAYLOAD_SIZE} bytes; "
            f"refusing to send"
        )

class HookConnectionError(HookClientError):
    def __init__(self, socket_path: str, cause: Exception):
        super().__init__(f"Failed to connect to hook socket {socket_path}: {cause}")

@dataclass
class HookResponse:
    """Typed wrapper for a hook IPC response."""
    type: str           # e.g. "Pong", "SessionStarted", "SessionStopped", etc.
    raw: dict           # full deserialized JSON body

    @classmethod
    def from_dict(cls, data: dict) -> "HookResponse":
        return cls(type=data.get("type", "Unknown"), raw=data)
```

## Class: `UnimatrixHookClient`

### `__init__`

```python
def __init__(
    self,
    socket_path: str | Path,
    timeout: float = DEFAULT_TIMEOUT,
):
    self._socket_path = str(socket_path)
    self._timeout = timeout
    self._sock: socket.socket | None = None
```

### `connect`

```python
def connect(self) -> None:
    """Open AF_UNIX SOCK_STREAM to the hook IPC socket."""
    try:
        self._sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._sock.settimeout(self._timeout)
        self._sock.connect(self._socket_path)
    except (OSError, ConnectionRefusedError) as e:
        self._sock = None
        raise HookConnectionError(self._socket_path, e)
```

### `disconnect`

```python
def disconnect(self) -> None:
    """Close the hook socket."""
    if self._sock is None:
        return
    try:
        self._sock.shutdown(socket.SHUT_RDWR)
        self._sock.close()
    except OSError:
        pass
    finally:
        self._sock = None
```

### Context manager

```python
def __enter__(self):
    self.connect()
    return self

def __exit__(self, exc_type, exc_val, exc_tb):
    self.disconnect()
    return False
```

### `_send` (private)

```python
def _send(self, payload: bytes) -> None:
    """Frame payload with 4-byte BE length prefix and send (C-05, FR-38).

    Raises HookPayloadTooLargeError BEFORE any send if len > MAX_PAYLOAD_SIZE (AC-14, R-13).
    """
    assert self._sock is not None, "not connected"

    # Size guard: client-side enforcement BEFORE any write (AC-14, R-13)
    if len(payload) > MAX_PAYLOAD_SIZE:
        raise HookPayloadTooLargeError(len(payload))

    # Frame: 4-byte big-endian length + payload
    header = struct.pack(">I", len(payload))
    self._sock.sendall(header + payload)
```

### `_recv` (private)

```python
def _recv(self, timeout: float | None = None) -> dict:
    """Read one framed response: 4-byte BE length header + JSON body.

    Raises HookTimeoutError on read timeout.
    Raises HookClientError on socket close or framing error.
    """
    assert self._sock is not None, "not connected"
    timeout = timeout or self._timeout

    self._sock.settimeout(timeout)
    try:
        # Read exactly 4 bytes for the length header:
        header = self._recv_exactly(4, timeout)
        if len(header) < 4:
            raise HookClientError("connection closed by server before length header")

        # Unpack big-endian unsigned 32-bit integer:
        body_len = struct.unpack(">I", header)[0]

        # Read exactly body_len bytes:
        body = self._recv_exactly(body_len, timeout)
        if len(body) < body_len:
            raise HookClientError(
                f"connection closed mid-body (expected {body_len} bytes, got {len(body)})"
            )

        return json.loads(body.decode("utf-8"))

    except socket.timeout:
        raise HookTimeoutError("_recv", timeout)
    finally:
        self._sock.settimeout(None)
```

### `_recv_exactly` (private)

```python
def _recv_exactly(self, n: int, timeout: float) -> bytes:
    """Read exactly n bytes, accumulating across partial reads."""
    buf = b""
    deadline = time.monotonic() + timeout
    while len(buf) < n:
        remaining_time = deadline - time.monotonic()
        if remaining_time <= 0:
            raise socket.timeout
        self._sock.settimeout(remaining_time)
        chunk = self._sock.recv(n - len(buf))
        if not chunk:
            return buf  # connection closed; caller checks length
        buf += chunk
    return buf
```

### `_request` (private)

```python
def _request(self, request: dict, timeout: float | None = None) -> HookResponse:
    """Serialize request, send with length prefix, receive framed response."""
    payload = json.dumps(request, ensure_ascii=False).encode("utf-8")
    self._send(payload)  # raises HookPayloadTooLargeError if too large
    raw = self._recv(timeout=timeout)
    return HookResponse.from_dict(raw)
```

## 5 Typed Methods (FR-37)

### `ping`

```python
def ping(self, timeout: float | None = None) -> HookResponse:
    """Send Ping request; expect Pong response (AC-12)."""
    return self._request({"type": "Ping"}, timeout=timeout)
```

### `session_start`

```python
def session_start(
    self,
    session_id: str,
    feature_cycle: str,
    agent_role: str,
    timeout: float | None = None,
) -> HookResponse:
    """Send SessionStart hook event (AC-13)."""
    return self._request(
        {
            "type": "SessionStart",
            "session_id": session_id,
            "feature_cycle": feature_cycle,
            "agent_role": agent_role,
        },
        timeout=timeout,
    )
```

### `session_stop`

```python
def session_stop(
    self,
    session_id: str,
    outcome: str,
    timeout: float | None = None,
) -> HookResponse:
    """Send SessionStop hook event (AC-13)."""
    return self._request(
        {
            "type": "SessionStop",
            "session_id": session_id,
            "outcome": outcome,
        },
        timeout=timeout,
    )
```

### `pre_tool_use`

```python
def pre_tool_use(
    self,
    session_id: str,
    tool: str,
    input: dict,
    timeout: float | None = None,
) -> HookResponse:
    """Send PreToolUse hook event."""
    return self._request(
        {
            "type": "PreToolUse",
            "session_id": session_id,
            "tool": tool,
            "input": input,
        },
        timeout=timeout,
    )
```

### `post_tool_use`

```python
def post_tool_use(
    self,
    session_id: str,
    tool: str,
    response_size: int,
    response_snippet: str,
    timeout: float | None = None,
) -> HookResponse:
    """Send PostToolUse hook event."""
    return self._request(
        {
            "type": "PostToolUse",
            "session_id": session_id,
            "tool": tool,
            "response_size": response_size,
            "response_snippet": response_snippet,
        },
        timeout=timeout,
    )
```

## Wire Protocol Notes

The hook IPC wire format is defined in `unimatrix_engine::wire`. Key invariants:

1. Write: 4-byte big-endian uint32 length of UTF-8 JSON body, then the body bytes.
2. Read: 4-byte BE header → body length → read exactly that many bytes → parse JSON.
3. JSON keys must match `HookRequest` enum variant names in `wire.rs`.
4. The client must confirm field names against `wire.rs` before implementing.
   Confirmed field names as of nan-007: `type`, `session_id`, `feature_cycle`,
   `agent_role`, `outcome`, `tool`, `input`, `response_size`, `response_snippet`.
   The `type` field maps to Rust enum variants: `Ping`, `SessionStart`, `SessionStop`,
   `PreToolUse`, `PostToolUse`.

## UTF-8 Note (RISK-TEST-STRATEGY edge case)

`json.dumps(request, ensure_ascii=False)` emits proper UTF-8. If `ensure_ascii=True`
(the default), non-ASCII characters are escaped as `\uXXXX`, which is valid JSON and
decoded correctly by Rust's `serde_json`. Either works for ASCII-only content;
`ensure_ascii=False` is preferred for readable output in test fixtures.

## State Machine

```
State:    DISCONNECTED → CONNECTED → DISCONNECTED
Trigger:  __init__      connect()    disconnect().__exit__
socket:   None          AF_UNIX       None
```

No `_initialized` state: the hook socket does not require a handshake. Each method
call is a stateless request/response cycle.

## Error Handling

| Failure | Exception | Timing |
|---------|-----------|--------|
| Payload > MAX_PAYLOAD_SIZE | `HookPayloadTooLargeError` | Before any write (AC-14) |
| Socket file does not exist | `HookConnectionError` | At `connect()` |
| Connection refused | `HookConnectionError` | At `connect()` |
| Read timeout | `HookTimeoutError` | During `_recv` |
| Connection closed mid-read | `HookClientError` | During `_recv` |
| JSON parse error on response | `json.JSONDecodeError` propagated | During `_recv` |

## Key Test Scenarios

Test file: `product/test/infra-001/tests/test_eval_hooks.py`
Uses `daemon_server` pytest fixture (entry #1928).

1. **Ping/Pong** (AC-12): `hook_client.ping()` returns `HookResponse` with `type == "Pong"`
   within timeout.

2. **Session lifecycle** (AC-13): `session_start` → `session_stop`; then verify session
   record visible via `UnimatrixUdsClient.context_status()`.

3. **Pre/post tool use**: send `pre_tool_use` + `post_tool_use` around a session; assert
   no error and response is well-formed.

4. **Oversized payload** (AC-14, R-13): construct `pre_tool_use` with input dict that
   serializes to > 1,048,576 bytes; assert `HookPayloadTooLargeError` raised before any
   send (mock or inspect socket to confirm zero bytes written).

5. **Payload at 1,048,576 bytes exactly**: assert NOT rejected (boundary).

6. **Payload at 1,048,577 bytes**: assert `HookPayloadTooLargeError` raised.

7. **Framing correctness** (R-05): capture raw bytes sent for a known `ping()` call;
   assert first 4 bytes are `struct.pack('>I', len(b'{"type":"Ping"}'))` (big-endian).

8. **Big-endian byte order**: manually compare against `struct.pack('<I', ...)` (LE) and
   assert they differ; assert the client uses BE.

9. **Session keywords** (FR-40, col-022): after a session with tool use events, call
   `context_status` and assert keywords are populated in session record.

10. **Invalid payload rejection**: send a `pre_tool_use` with a crafted large `response_snippet`
    that pushes total payload over 1 MiB; assert `HookPayloadTooLargeError`.

11. **Daemon not listening**: `hook_client.connect()` raises `HookConnectionError` with
    socket path in message.

12. **After oversized rejection, client still usable**: after `HookPayloadTooLargeError`,
    call `ping()` with a valid payload; assert `Pong` response (R-13: no partial-write state).

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness patterns conventions" (category: pattern) — 5 results; no Python hook client patterns found. hook_client.py is a new pattern (4-byte BE framing over AF_UNIX for synthetic hook injection). No existing codebase pattern to deviate from.
Queried: /uni-query-patterns for "block_export_sync async bridge pattern" — not applicable; hook_client.py is pure Python stdlib. The BE framing protocol is defined in unimatrix_engine::wire and must match exactly (C-05). Queried to confirm no Python bridging pattern exists that would apply here — none found.
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — no ADRs specifically govern the Python hook client. C-05 (4-byte BE length prefix) is the binding constraint, applied in _send() and _recv(). Constraint followed exactly.
Stored: nothing novel to store — pseudocode agents are read-only; patterns are consumed not created
