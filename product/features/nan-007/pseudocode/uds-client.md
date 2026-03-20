# Pseudocode: uds_client.py (D5)

**Location**: `product/test/infra-001/harness/uds_client.py`

## Purpose

Provide a Python class `UnimatrixUdsClient` that connects to a running Unimatrix
daemon's MCP UDS socket via `socket.AF_UNIX`, completes the MCP `initialize` handshake,
and exposes the same 12 typed `context_*` tool methods as `UnimatrixClient`. Enables
live-path eval and integration testing without subprocess management.

Wire protocol: newline-delimited JSON (`\n`-terminated), identical to stdio MCP transport.
No length prefix (C-04). rmcp `JsonRpcMessageCodec` uses `\n` as the delimiter.

The class follows the same structure as `UnimatrixClient` in `client.py`, differing
only in transport: socket vs. subprocess pipe.

## Dependencies (all standard library)

| Module | Use |
|--------|-----|
| `socket` | `AF_UNIX SOCK_STREAM`, connect, send, recv |
| `json` | JSON-RPC serialization/deserialization |
| `pathlib.Path` | Path handling |
| `threading` | Background readline thread for timeout support |
| `time` | Deadline-based timeout |

## Constants and Exceptions

```python
DEFAULT_TIMEOUT = 10.0
MAX_SOCKET_PATH_BYTES = 103   # C-08, FR-31

class UdsClientError(Exception): pass

class UdsTimeoutError(UdsClientError):
    def __init__(self, method: str, timeout: float):
        self.method = method
        self.timeout = timeout
        super().__init__(f"Timeout after {timeout}s waiting for response to {method}")

class UdsConnectionError(UdsClientError):
    def __init__(self, socket_path: str, cause: Exception):
        self.socket_path = socket_path
        super().__init__(f"Failed to connect to {socket_path}: {cause}")

class UdsServerError(UdsClientError):
    def __init__(self, error: dict):
        super().__init__(f"MCP error: {error}")
```

## Class: `UnimatrixUdsClient`

### `__init__`

```python
def __init__(
    self,
    socket_path: str | Path,
    timeout: float = DEFAULT_TIMEOUT,
):
    self._socket_path = str(socket_path)
    self._timeout = timeout
    self._request_id = 0
    self._sock: socket.socket | None = None
    self._recv_buffer = b""
    self._initialized = False

    # Validate socket path length (C-08, FR-31):
    encoded = self._socket_path.encode("utf-8")
    if len(encoded) > MAX_SOCKET_PATH_BYTES:
        raise ValueError(
            f"socket path exceeds {MAX_SOCKET_PATH_BYTES}-byte limit "
            f"(got {len(encoded)} bytes): {self._socket_path}\n"
            f"This is an OS limit for AF_UNIX socket paths."
        )
```

### `connect`

```python
def connect(self) -> None:
    """Open AF_UNIX socket and complete MCP initialize handshake."""
    try:
        self._sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._sock.settimeout(self._timeout)
        self._sock.connect(self._socket_path)
        self._sock.settimeout(None)  # Switch to blocking mode for readline
    except (OSError, ConnectionRefusedError) as e:
        raise UdsConnectionError(self._socket_path, e)

    # Complete MCP initialize handshake (same as UnimatrixClient.initialize()):
    self._initialize()
```

### `disconnect`

```python
def disconnect(self) -> None:
    """Send MCP shutdown notification and close socket."""
    if self._sock is None:
        return
    if self._initialized:
        try:
            self._notify("notifications/shutdown", {})
        except (OSError, BrokenPipeError):
            pass
    try:
        self._sock.shutdown(socket.SHUT_RDWR)
        self._sock.close()
    except OSError:
        pass
    finally:
        self._sock = None
        self._initialized = False
```

### Context manager

```python
def __enter__(self):
    self.connect()
    return self

def __exit__(self, exc_type, exc_val, exc_tb):
    self.disconnect()
    return False  # do not suppress exceptions
```

### `_send` (private)

```python
def _send(self, message: dict) -> None:
    """Send a JSON-RPC message with newline delimiter (C-04: NO length prefix)."""
    assert self._sock is not None, "not connected"
    data = json.dumps(message, ensure_ascii=False) + "\n"
    payload = data.encode("utf-8")
    self._sock.sendall(payload)
```

### `_read_response` (private)

```python
def _read_response(self, timeout: float | None = None) -> dict:
    """Read one newline-terminated JSON-RPC response from the socket.

    Accumulates into self._recv_buffer for partial reads.
    Raises UdsTimeoutError if no complete line arrives within timeout.
    """
    assert self._sock is not None, "not connected"
    timeout = timeout or self._timeout
    deadline = time.monotonic() + timeout

    while True:
        # Check if buffer already has a complete line:
        if b"\n" in self._recv_buffer:
            line, self._recv_buffer = self._recv_buffer.split(b"\n", 1)
            line = line.strip()
            if not line:
                continue  # skip blank lines
            try:
                return json.loads(line.decode("utf-8"))
            except json.JSONDecodeError:
                continue  # skip non-JSON (tracing output)

        # Need more data:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise UdsTimeoutError("_read_response", timeout)

        self._sock.settimeout(min(remaining, 1.0))
        try:
            chunk = self._sock.recv(4096)
        except socket.timeout:
            continue
        except OSError as e:
            raise UdsClientError(f"socket recv error: {e}")
        finally:
            self._sock.settimeout(None)

        if not chunk:
            raise UdsClientError("socket closed by server during read")
        self._recv_buffer += chunk
```

### `_call` (private)

```python
def _call(
    self,
    method: str,
    params: dict | None = None,
    timeout: float | None = None,
) -> dict:
    """Send JSON-RPC request and return the matching response.

    Returns the full raw response dict. Raises UdsServerError on JSON-RPC error.
    """
    self._request_id += 1
    req_id = self._request_id

    message: dict = {"jsonrpc": "2.0", "id": req_id, "method": method}
    if params is not None:
        message["params"] = params

    self._send(message)

    timeout = timeout or self._timeout
    deadline = time.monotonic() + timeout

    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise UdsTimeoutError(method, timeout)
        raw = self._read_response(timeout=remaining)
        if raw.get("id") == req_id:
            if "error" in raw and raw["error"] is not None:
                raise UdsServerError(raw["error"])
            return raw.get("result", {})
```

### `_notify` (private)

```python
def _notify(self, method: str, params: dict | None = None) -> None:
    """Send JSON-RPC notification (no id, no response expected)."""
    message: dict = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        message["params"] = params
    self._send(message)
```

### `_initialize` (private)

```python
def _initialize(self) -> None:
    """Complete MCP initialize handshake (mirrors UnimatrixClient.initialize())."""
    result = self._call(
        "initialize",
        {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "unimatrix-uds-test-harness",
                "version": "1.0.0",
            },
        },
    )
    self._notify("notifications/initialized")
    self._initialized = True
```

### `call_tool` (private)

```python
def call_tool(
    self,
    name: str,
    arguments: dict | None = None,
    timeout: float | None = None,
) -> dict:
    """Low-level tools/call wrapper. Returns result dict."""
    return self._call(
        "tools/call",
        {"name": name, "arguments": arguments or {}},
        timeout=timeout,
    )
```

## 12 Typed Tool Methods

Mirroring `UnimatrixClient` exactly (FR-34). Each method delegates to `call_tool`.
All signatures are the same as in `client.py` — copied here for completeness:

```python
def context_search(self, query: str, *, topic=None, category=None, tags=None,
                   k=None, agent_id=None, format=None, feature=None,
                   helpful=None, timeout=None) -> dict:
    args = {"query": query}
    # populate optional keys only if not None (same pattern as client.py)
    if topic is not None: args["topic"] = topic
    if category is not None: args["category"] = category
    if tags is not None: args["tags"] = tags
    if k is not None: args["k"] = k
    if agent_id is not None: args["agent_id"] = agent_id
    if format is not None: args["format"] = format
    if feature is not None: args["feature"] = feature
    if helpful is not None: args["helpful"] = helpful
    return self.call_tool("context_search", args, timeout=timeout)

def context_store(self, content, topic, category, *, title=None, tags=None,
                  source=None, agent_id=None, format=None, timeout=None) -> dict:
    # same optional-key pattern

def context_lookup(self, *, topic=None, category=None, tags=None, id=None,
                   status=None, limit=None, agent_id=None, format=None,
                   feature=None, helpful=None) -> dict:

def context_get(self, entry_id: int, *, agent_id=None, format=None,
                feature=None, helpful=None) -> dict:

def context_correct(self, original_id, content, *, reason=None, topic=None,
                    category=None, tags=None, title=None, agent_id=None,
                    format=None) -> dict:

def context_deprecate(self, entry_id, *, reason=None, agent_id=None,
                      format=None) -> dict:

def context_status(self, *, topic=None, category=None, agent_id=None,
                   format=None, check_embeddings=None, timeout=None) -> dict:

def context_briefing(self, role, task, *, feature=None, max_tokens=None,
                     agent_id=None, format=None, helpful=None, timeout=None) -> dict:

def context_quarantine(self, entry_id, *, reason=None, action=None,
                       agent_id=None, format=None) -> dict:

def context_enroll(self, target_agent_id, trust_level, capabilities,
                   *, agent_id=None, format=None) -> dict:

def context_cycle(self, cycle_type, topic, *, keywords=None, agent_id=None,
                  format=None, timeout=None) -> dict:

def context_cycle_review(self, feature_cycle, *, agent_id=None, format=None,
                         timeout=None) -> dict:
```

Each method builds `args` by adding only non-None keyword arguments, then calls
`self.call_tool(tool_name, args, timeout=timeout)`. This pattern is identical to
`UnimatrixClient` in `client.py` — implementer should follow that code directly.

## State Machine

```
State:              DISCONNECTED → CONNECTED → INITIALIZED → DISCONNECTED
Trigger:            __init__      connect()   _initialize()   disconnect()/__exit__
socket status:      None          open socket  same            closed
_initialized:       False         False         True            False
_recv_buffer:       b""           b""           fills/drains    b""
```

## Error Handling

| Failure | Exception |
|---------|-----------|
| socket path > 103 bytes (at `__init__`) | `ValueError` with byte count and path |
| socket file does not exist | `UdsConnectionError` with socket path |
| connection refused | `UdsConnectionError` |
| read timeout | `UdsTimeoutError` with method name |
| server closes connection mid-read | `UdsClientError` |
| JSON-RPC error response | `UdsServerError` with error dict |
| `__exit__` on exception | Calls `disconnect()`; does not suppress exception |

## Key Test Scenarios

Test file: `product/test/infra-001/tests/test_eval_uds.py`
Uses `daemon_server` pytest fixture (entry #1928) for daemon lifecycle.

1. **Connection lifecycle** (AC-11): use `with UnimatrixUdsClient(path) as client:`; assert
   no explicit connect/disconnect needed; connection established and closed automatically.

2. **Tool parity** (AC-10): run `context_search(query="test")` via `UnimatrixUdsClient`
   and same via `UnimatrixClient` stdio; assert results are equivalent.

3. **All 12 typed methods callable**: call each `context_*` method; assert no exception.

4. **Socket path > 103 bytes** (R-14): `UnimatrixUdsClient.__init__("/" + "a" * 103)`;
   assert `ValueError` raised before any network call.

5. **Socket path = 103 bytes**: no ValueError raised (boundary test).

6. **Socket path = 104 bytes**: ValueError raised (R-14).

7. **Framing verification** (R-04): capture raw bytes sent; assert message ends with `\n`
   and does not start with 4 bytes of length prefix.

8. **UDS-sourced queries in query_log** (FR-35, AC-04 context): after a `context_search`
   via `UnimatrixUdsClient`, query the snapshot via `eval scenarios --retrieval-mode uds`;
   assert at least one scenario with `source="uds"` appears.

9. **Concurrent clients**: two `UnimatrixUdsClient` instances connected simultaneously;
   assert both can execute `context_status` without interference.

10. **Daemon not running**: `UnimatrixUdsClient.connect()` raises `UdsConnectionError`
    with socket path in message.

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness patterns conventions" (category: pattern) — 5 results; no Python client patterns found. UnimatrixUdsClient follows the structure of the existing UnimatrixClient in client.py (same 12 typed methods, same optional-key argument pattern). Deviation: transport layer only (socket vs. subprocess pipe); all tool method signatures are identical.
Queried: /uni-query-patterns for "block_export_sync async bridge pattern" — not applicable to this module; uds_client.py is pure Python stdlib with no async runtime. Search returned no results relevant to Python socket clients.
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — no ADRs specifically govern the Python UDS client design. C-04 (newline-delimited JSON, no length prefix) is an architecture constraint applied directly in _send(). Constraint followed.
Stored: nothing novel to store — pseudocode agents are read-only; patterns are consumed not created
