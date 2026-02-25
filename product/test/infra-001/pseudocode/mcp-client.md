# Pseudocode: C2 — MCP Client Library

## File: `harness/client.py`

## Classes

### UnimatrixClient

```python
import json
import os
import signal
import subprocess
import tempfile
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


DEFAULT_TIMEOUT = 10.0  # seconds per call
SHUTDOWN_TIMEOUT = 5.0  # seconds for graceful shutdown
SIGTERM_TIMEOUT = 3.0   # seconds after SIGTERM before SIGKILL


@dataclass
class MCPResponse:
    """Parsed JSON-RPC response."""
    id: int | None
    result: dict | None
    error: dict | None
    raw: dict


class ClientError(Exception):
    """Base exception for client errors."""
    pass


class TimeoutError(ClientError):
    """Raised when a call exceeds its timeout."""
    def __init__(self, method: str, timeout: float):
        self.method = method
        self.timeout = timeout
        super().__init__(f"Timeout after {timeout}s waiting for response to {method}")


class ServerDied(ClientError):
    """Raised when the server process exits unexpectedly."""
    def __init__(self, returncode: int, stderr: str):
        self.returncode = returncode
        self.stderr_output = stderr
        super().__init__(f"Server exited with code {returncode}")


class UnimatrixClient:
    def __init__(self, binary_path: str | Path, project_dir: str | Path | None = None,
                 timeout: float = DEFAULT_TIMEOUT):
        """
        Spawn unimatrix-server as subprocess.

        Args:
            binary_path: Path to unimatrix-server binary
            project_dir: Override project directory (default: create temp dir)
            timeout: Default timeout for all calls in seconds
        """
        self._binary_path = str(binary_path)
        self._timeout = timeout
        self._request_id = 0
        self._stderr_lines: list[str] = []
        self._stderr_lock = threading.Lock()
        self._initialized = False

        # Create project dir if not provided
        if project_dir is None:
            self._temp_dir = tempfile.mkdtemp(prefix="unimatrix-test-")
            self._project_dir = self._temp_dir
        else:
            self._temp_dir = None
            self._project_dir = str(project_dir)

        # Spawn server subprocess
        env = os.environ.copy()
        env["RUST_LOG"] = env.get("RUST_LOG", "info")

        self._process = subprocess.Popen(
            [self._binary_path, "--project-dir", self._project_dir],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        )

        # Start stderr drain thread to prevent deadlock (R-09)
        self._stderr_thread = threading.Thread(
            target=self._drain_stderr, daemon=True
        )
        self._stderr_thread.start()

    def _drain_stderr(self):
        """Continuously read stderr to prevent buffer deadlock."""
        # Read line-by-line from stderr until EOF
        for line_bytes in iter(self._process.stderr.readline, b""):
            try:
                line = line_bytes.decode("utf-8", errors="replace").rstrip()
            except Exception:
                line = repr(line_bytes)
            with self._stderr_lock:
                self._stderr_lines.append(line)
        # EOF reached — process has closed stderr

    def _next_id(self) -> int:
        """Monotonically increasing request ID."""
        self._request_id += 1
        return self._request_id

    def _send(self, message: dict):
        """Send JSON-RPC message to server stdin."""
        data = json.dumps(message) + "\n"
        self._process.stdin.write(data.encode("utf-8"))
        self._process.stdin.flush()

    def _read_response(self, timeout: float | None = None) -> dict:
        """
        Read one JSON-RPC response from server stdout.

        Reads line-by-line. Skips non-JSON lines (server may emit tracing).
        Raises TimeoutError if no valid JSON within timeout.
        Raises ServerDied if process exits before responding.
        """
        timeout = timeout or self._timeout
        deadline = time.monotonic() + timeout

        while time.monotonic() < deadline:
            # Check if process is still alive
            if self._process.poll() is not None:
                raise ServerDied(self._process.returncode, self.get_stderr())

            # Use select/poll or timeout on readline
            # Set a short per-read timeout to allow deadline checking
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break

            # Read one line with timeout via threading
            line = self._readline_with_timeout(min(remaining, 1.0))
            if line is None:
                continue  # timeout on this read, check deadline

            line = line.strip()
            if not line:
                continue

            # Try to parse as JSON
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                # Not JSON — likely server tracing output, skip
                continue

        raise TimeoutError("read_response", timeout)

    def _readline_with_timeout(self, timeout: float) -> str | None:
        """Read one line from stdout with timeout using a background thread."""
        result = [None]
        def reader():
            try:
                result[0] = self._process.stdout.readline()
            except Exception:
                pass

        t = threading.Thread(target=reader, daemon=True)
        t.start()
        t.join(timeout=timeout)

        if result[0] is not None:
            return result[0].decode("utf-8", errors="replace")
        return None

    def _call(self, method: str, params: dict | None = None,
              timeout: float | None = None) -> MCPResponse:
        """Send JSON-RPC request and wait for matching response."""
        req_id = self._next_id()
        message = {
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
        }
        if params is not None:
            message["params"] = params

        self._send(message)

        # Read responses until we get one matching our ID
        timeout = timeout or self._timeout
        deadline = time.monotonic() + timeout

        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(method, timeout)
            raw = self._read_response(timeout=remaining)

            # Match by ID
            if raw.get("id") == req_id:
                return MCPResponse(
                    id=raw.get("id"),
                    result=raw.get("result"),
                    error=raw.get("error"),
                    raw=raw,
                )
            # Else: notification or response to different ID, skip

    def _notify(self, method: str, params: dict | None = None):
        """Send JSON-RPC notification (no id, no response expected)."""
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self._send(message)

    # ── MCP Lifecycle ────────────────────────────────────────

    def initialize(self, timeout: float | None = None) -> MCPResponse:
        """
        Complete MCP initialize handshake.

        1. Send initialize request with client capabilities
        2. Receive initialize response with server capabilities
        3. Send initialized notification
        """
        response = self._call("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "unimatrix-test-harness",
                "version": "1.0.0",
            }
        }, timeout=timeout or self._timeout)

        if response.error:
            raise ClientError(f"Initialize failed: {response.error}")

        # Send initialized notification
        self._notify("notifications/initialized")
        self._initialized = True
        return response

    def shutdown(self):
        """
        Graceful shutdown sequence:
        1. Send shutdown request (if initialized)
        2. Wait for response
        3. If no response, SIGTERM
        4. If still alive after SIGTERM_TIMEOUT, SIGKILL
        5. Wait for process to exit
        """
        if self._process.poll() is not None:
            return  # already dead

        # Step 1: Try MCP shutdown if initialized
        if self._initialized:
            try:
                self._send({
                    "jsonrpc": "2.0",
                    "id": self._next_id(),
                    "method": "shutdown",
                    "params": {},
                })
                # Wait briefly for process to exit
                try:
                    self._process.wait(timeout=SHUTDOWN_TIMEOUT)
                    return
                except subprocess.TimeoutExpired:
                    pass
            except (BrokenPipeError, OSError):
                pass  # process already dead or pipe broken

        # Step 2: SIGTERM
        if self._process.poll() is None:
            try:
                self._process.terminate()
                self._process.wait(timeout=SIGTERM_TIMEOUT)
                return
            except subprocess.TimeoutExpired:
                pass

        # Step 3: SIGKILL
        if self._process.poll() is None:
            self._process.kill()
            self._process.wait(timeout=5.0)

    def get_stderr(self) -> str:
        """Return accumulated server stderr output."""
        with self._stderr_lock:
            return "\n".join(self._stderr_lines)

    @property
    def pid(self) -> int | None:
        """Server process PID (None if exited)."""
        if self._process.poll() is None:
            return self._process.pid
        return None

    @property
    def project_dir(self) -> str:
        """Server's project directory path."""
        return self._project_dir

    # ── Context Manager ──────────────────────────────────────

    def __enter__(self):
        self.initialize()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.shutdown()
        return False

    # ── Tool Wrappers ────────────────────────────────────────

    def call_tool(self, name: str, arguments: dict | None = None,
                  timeout: float | None = None) -> MCPResponse:
        """Low-level tool call. Builds tools/call envelope."""
        return self._call("tools/call", {
            "name": name,
            "arguments": arguments or {},
        }, timeout=timeout)

    def list_tools(self, timeout: float | None = None) -> MCPResponse:
        """List available MCP tools."""
        return self._call("tools/list", {}, timeout=timeout)

    def send_raw(self, method: str, params: dict | None = None,
                 timeout: float | None = None) -> MCPResponse:
        """Send arbitrary JSON-RPC request (for protocol testing)."""
        return self._call(method, params, timeout=timeout)

    def send_raw_bytes(self, data: bytes):
        """Send raw bytes to stdin (for malformed input testing)."""
        self._process.stdin.write(data)
        self._process.stdin.flush()

    # ── Typed Tool Methods ───────────────────────────────────

    def context_store(self, content: str, topic: str, category: str, *,
                      title: str | None = None, tags: list[str] | None = None,
                      source: str | None = None, agent_id: str | None = None,
                      format: str | None = None) -> MCPResponse:
        args = {"content": content, "topic": topic, "category": category}
        if title is not None: args["title"] = title
        if tags is not None: args["tags"] = tags
        if source is not None: args["source"] = source
        if agent_id is not None: args["agent_id"] = agent_id
        if format is not None: args["format"] = format
        return self.call_tool("context_store", args)

    def context_search(self, query: str, *, topic: str | None = None,
                       category: str | None = None, tags: list[str] | None = None,
                       k: int | None = None, agent_id: str | None = None,
                       format: str | None = None, feature: str | None = None,
                       helpful: bool | None = None) -> MCPResponse:
        args: dict[str, Any] = {"query": query}
        if topic is not None: args["topic"] = topic
        if category is not None: args["category"] = category
        if tags is not None: args["tags"] = tags
        if k is not None: args["k"] = k
        if agent_id is not None: args["agent_id"] = agent_id
        if format is not None: args["format"] = format
        if feature is not None: args["feature"] = feature
        if helpful is not None: args["helpful"] = helpful
        return self.call_tool("context_search", args)

    def context_lookup(self, *, topic: str | None = None,
                       category: str | None = None, tags: list[str] | None = None,
                       id: int | None = None, status: str | None = None,
                       limit: int | None = None, agent_id: str | None = None,
                       format: str | None = None, feature: str | None = None,
                       helpful: bool | None = None) -> MCPResponse:
        args: dict[str, Any] = {}
        if topic is not None: args["topic"] = topic
        if category is not None: args["category"] = category
        if tags is not None: args["tags"] = tags
        if id is not None: args["id"] = id
        if status is not None: args["status"] = status
        if limit is not None: args["limit"] = limit
        if agent_id is not None: args["agent_id"] = agent_id
        if format is not None: args["format"] = format
        if feature is not None: args["feature"] = feature
        if helpful is not None: args["helpful"] = helpful
        return self.call_tool("context_lookup", args)

    def context_get(self, entry_id: int, *, agent_id: str | None = None,
                    format: str | None = None, feature: str | None = None,
                    helpful: bool | None = None) -> MCPResponse:
        args: dict[str, Any] = {"id": entry_id}
        if agent_id is not None: args["agent_id"] = agent_id
        if format is not None: args["format"] = format
        if feature is not None: args["feature"] = feature
        if helpful is not None: args["helpful"] = helpful
        return self.call_tool("context_get", args)

    def context_correct(self, original_id: int, content: str, *,
                        reason: str | None = None, topic: str | None = None,
                        category: str | None = None, tags: list[str] | None = None,
                        title: str | None = None, agent_id: str | None = None,
                        format: str | None = None) -> MCPResponse:
        args: dict[str, Any] = {"original_id": original_id, "content": content}
        if reason is not None: args["reason"] = reason
        if topic is not None: args["topic"] = topic
        if category is not None: args["category"] = category
        if tags is not None: args["tags"] = tags
        if title is not None: args["title"] = title
        if agent_id is not None: args["agent_id"] = agent_id
        if format is not None: args["format"] = format
        return self.call_tool("context_correct", args)

    def context_deprecate(self, entry_id: int, *, reason: str | None = None,
                          agent_id: str | None = None,
                          format: str | None = None) -> MCPResponse:
        args: dict[str, Any] = {"id": entry_id}
        if reason is not None: args["reason"] = reason
        if agent_id is not None: args["agent_id"] = agent_id
        if format is not None: args["format"] = format
        return self.call_tool("context_deprecate", args)

    def context_status(self, *, topic: str | None = None,
                       category: str | None = None, agent_id: str | None = None,
                       format: str | None = None,
                       check_embeddings: bool | None = None) -> MCPResponse:
        args: dict[str, Any] = {}
        if topic is not None: args["topic"] = topic
        if category is not None: args["category"] = category
        if agent_id is not None: args["agent_id"] = agent_id
        if format is not None: args["format"] = format
        if check_embeddings is not None: args["check_embeddings"] = check_embeddings
        return self.call_tool("context_status", args)

    def context_briefing(self, role: str, task: str, *,
                         feature: str | None = None,
                         max_tokens: int | None = None,
                         agent_id: str | None = None,
                         format: str | None = None,
                         helpful: bool | None = None) -> MCPResponse:
        args: dict[str, Any] = {"role": role, "task": task}
        if feature is not None: args["feature"] = feature
        if max_tokens is not None: args["max_tokens"] = max_tokens
        if agent_id is not None: args["agent_id"] = agent_id
        if format is not None: args["format"] = format
        if helpful is not None: args["helpful"] = helpful
        return self.call_tool("context_briefing", args)

    def context_quarantine(self, entry_id: int, *,
                           reason: str | None = None,
                           action: str | None = None,
                           agent_id: str | None = None,
                           format: str | None = None) -> MCPResponse:
        args: dict[str, Any] = {"id": entry_id}
        if reason is not None: args["reason"] = reason
        if action is not None: args["action"] = action
        if agent_id is not None: args["agent_id"] = agent_id
        if format is not None: args["format"] = format
        return self.call_tool("context_quarantine", args)
```

## Error Handling

- All calls enforce timeout. Timeout raises `TimeoutError` with method name and duration.
- If server process exits mid-call, `ServerDied` raised with return code and stderr.
- If stdin pipe breaks (server crashed), `BrokenPipeError` propagated.
- Shutdown sequence is defensive: catches all exceptions, always attempts kill.

## Thread Safety

- One stderr drain thread per client instance (daemon thread).
- `_stderr_lines` protected by `_stderr_lock`.
- `_readline_with_timeout` uses per-read thread to avoid blocking.
- Client is NOT thread-safe for concurrent tool calls (no response multiplexing).
  Tests call one tool at a time per client instance.

## Key Design Decisions

- Line-based JSON parsing: server writes newline-delimited JSON-RPC (rmcp default).
- Non-JSON lines skipped: server tracing may appear on stdout before JSON responses.
- Monotonic request IDs: simple integer counter, matched on response.
- Context manager wraps initialize + shutdown for clean resource management.
- Temp dir created if project_dir not provided (for standalone testing).
