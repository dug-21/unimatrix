"""UnimatrixUdsClient — MCP over AF_UNIX, newline-delimited JSON (D5).

Connects to a running Unimatrix daemon's MCP UDS socket, completes the MCP
initialize handshake, and exposes the same 12 typed context_* tool methods
as UnimatrixClient in client.py. Wire framing: newline-delimited JSON with
no length prefix (C-04). Identical to stdio MCP transport framing.
"""

import json
import socket
import time
from pathlib import Path
from typing import Any


DEFAULT_TIMEOUT = 10.0
MAX_SOCKET_PATH_BYTES = 103  # C-08, FR-31: OS limit for AF_UNIX socket paths


# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------


class UdsClientError(Exception):
    """Base exception for UDS client errors."""


class UdsTimeoutError(UdsClientError):
    """Raised when no complete response arrives within the timeout."""

    def __init__(self, method: str, timeout: float):
        self.method = method
        self.timeout = timeout
        super().__init__(
            f"Timeout after {timeout}s waiting for response to {method}"
        )


class UdsConnectionError(UdsClientError):
    """Raised when the socket connection cannot be established."""

    def __init__(self, socket_path: str, cause: Exception):
        self.socket_path = socket_path
        super().__init__(f"Failed to connect to {socket_path}: {cause}")


class UdsServerError(UdsClientError):
    """Raised when the MCP server returns a JSON-RPC error response."""

    def __init__(self, error: dict):
        self.error = error
        super().__init__(f"MCP error: {error}")


# ---------------------------------------------------------------------------
# Client
# ---------------------------------------------------------------------------


class UnimatrixUdsClient:
    """MCP client over AF_UNIX socket with newline-delimited JSON framing.

    Connects to a running Unimatrix daemon's MCP UDS socket, completes the
    MCP initialize handshake, and exposes 12 typed context_* tool methods.

    Wire protocol: newline-delimited JSON (no length prefix). Identical to
    the stdio MCP transport used by UnimatrixClient (C-04).

    Usage (context manager, recommended)::

        with UnimatrixUdsClient("/path/to/unimatrix-mcp.sock") as client:
            resp = client.context_search("query text", k=5)

    Usage (manual)::

        client = UnimatrixUdsClient("/path/to/unimatrix-mcp.sock")
        client.connect()
        try:
            resp = client.context_status()
        finally:
            client.disconnect()
    """

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

        # Validate socket path length before any network call (C-08, FR-31).
        encoded = self._socket_path.encode("utf-8")
        if len(encoded) > MAX_SOCKET_PATH_BYTES:
            raise ValueError(
                f"socket path exceeds {MAX_SOCKET_PATH_BYTES}-byte limit "
                f"(got {len(encoded)} bytes): {self._socket_path}\n"
                f"This is an OS limit for AF_UNIX socket paths."
            )

    # -- Lifecycle ---------------------------------------------------------

    def connect(self) -> None:
        """Open AF_UNIX socket and complete MCP initialize handshake."""
        try:
            self._sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self._sock.settimeout(self._timeout)
            self._sock.connect(self._socket_path)
            self._sock.settimeout(None)  # Switch to blocking for readline
        except (OSError, ConnectionRefusedError) as e:
            if self._sock is not None:
                try:
                    self._sock.close()
                except OSError:
                    pass
                self._sock = None
            raise UdsConnectionError(self._socket_path, e)

        self._initialize()

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
            self._recv_buffer = b""

    # -- Context manager ---------------------------------------------------

    def __enter__(self) -> "UnimatrixUdsClient":
        self.connect()
        return self

    def __exit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> bool:
        self.disconnect()
        return False  # do not suppress exceptions

    # -- Private wire methods ----------------------------------------------

    def _send(self, message: dict) -> None:
        """Send a JSON-RPC message with newline delimiter (C-04: no length prefix)."""
        assert self._sock is not None, "not connected"
        data = json.dumps(message, ensure_ascii=False) + "\n"
        payload = data.encode("utf-8")
        self._sock.sendall(payload)

    def _read_response(self, timeout: float | None = None) -> dict:
        """Read one newline-terminated JSON-RPC message from the socket.

        Accumulates partial reads into self._recv_buffer. Skips blank lines
        and non-JSON data (e.g. tracing output). Raises UdsTimeoutError if no
        complete line arrives within timeout.
        """
        assert self._sock is not None, "not connected"
        timeout = timeout if timeout is not None else self._timeout
        deadline = time.monotonic() + timeout

        while True:
            # Check if buffer already contains a complete line.
            if b"\n" in self._recv_buffer:
                line, self._recv_buffer = self._recv_buffer.split(b"\n", 1)
                line = line.strip()
                if not line:
                    continue  # skip blank lines
                try:
                    return json.loads(line.decode("utf-8"))
                except json.JSONDecodeError:
                    continue  # skip non-JSON (e.g. tracing output)

            # Need more data from the socket.
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

    def _call(
        self,
        method: str,
        params: dict | None = None,
        timeout: float | None = None,
    ) -> dict:
        """Send JSON-RPC request and return the result from the matching response.

        Raises UdsServerError if the server returns an error response.
        Raises UdsTimeoutError if the timeout expires.
        """
        self._request_id += 1
        req_id = self._request_id

        message: dict = {"jsonrpc": "2.0", "id": req_id, "method": method}
        if params is not None:
            message["params"] = params

        self._send(message)

        effective_timeout = timeout if timeout is not None else self._timeout
        deadline = time.monotonic() + effective_timeout

        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise UdsTimeoutError(method, effective_timeout)
            raw = self._read_response(timeout=remaining)
            if raw.get("id") == req_id:
                if "error" in raw and raw["error"] is not None:
                    raise UdsServerError(raw["error"])
                return raw.get("result", {})

    def _notify(self, method: str, params: dict | None = None) -> None:
        """Send JSON-RPC notification (no id, no response expected)."""
        message: dict = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self._send(message)

    def _initialize(self) -> None:
        """Complete MCP initialize handshake (mirrors UnimatrixClient.initialize())."""
        self._call(
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

    # -- Low-level tool call -----------------------------------------------

    def call_tool(
        self,
        name: str,
        arguments: dict | None = None,
        timeout: float | None = None,
    ) -> dict:
        """Low-level tools/call wrapper. Returns the raw result dict."""
        return self._call(
            "tools/call",
            {"name": name, "arguments": arguments or {}},
            timeout=timeout,
        )

    # -- 12 Typed tool methods (FR-34, AC-10) ------------------------------

    def context_search(
        self,
        query: str,
        *,
        topic: str | None = None,
        category: str | None = None,
        tags: list[str] | None = None,
        k: int | None = None,
        agent_id: str | None = None,
        format: str | None = None,
        feature: str | None = None,
        helpful: bool | None = None,
        timeout: float | None = None,
    ) -> dict:
        args: dict[str, Any] = {"query": query}
        if topic is not None:
            args["topic"] = topic
        if category is not None:
            args["category"] = category
        if tags is not None:
            args["tags"] = tags
        if k is not None:
            args["k"] = k
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        if feature is not None:
            args["feature"] = feature
        if helpful is not None:
            args["helpful"] = helpful
        return self.call_tool("context_search", args, timeout=timeout)

    def context_store(
        self,
        content: str,
        topic: str,
        category: str,
        *,
        title: str | None = None,
        tags: list[str] | None = None,
        source: str | None = None,
        agent_id: str | None = None,
        format: str | None = None,
        timeout: float | None = None,
    ) -> dict:
        args: dict[str, Any] = {
            "content": content,
            "topic": topic,
            "category": category,
        }
        if title is not None:
            args["title"] = title
        if tags is not None:
            args["tags"] = tags
        if source is not None:
            args["source"] = source
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        return self.call_tool("context_store", args, timeout=timeout)

    def context_lookup(
        self,
        *,
        topic: str | None = None,
        category: str | None = None,
        tags: list[str] | None = None,
        id: int | None = None,
        status: str | None = None,
        limit: int | None = None,
        agent_id: str | None = None,
        format: str | None = None,
        feature: str | None = None,
        helpful: bool | None = None,
    ) -> dict:
        args: dict[str, Any] = {}
        if topic is not None:
            args["topic"] = topic
        if category is not None:
            args["category"] = category
        if tags is not None:
            args["tags"] = tags
        if id is not None:
            args["id"] = id
        if status is not None:
            args["status"] = status
        if limit is not None:
            args["limit"] = limit
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        if feature is not None:
            args["feature"] = feature
        if helpful is not None:
            args["helpful"] = helpful
        return self.call_tool("context_lookup", args)

    def context_get(
        self,
        entry_id: int,
        *,
        agent_id: str | None = None,
        format: str | None = None,
        feature: str | None = None,
        helpful: bool | None = None,
    ) -> dict:
        args: dict[str, Any] = {"id": entry_id}
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        if feature is not None:
            args["feature"] = feature
        if helpful is not None:
            args["helpful"] = helpful
        return self.call_tool("context_get", args)

    def context_correct(
        self,
        original_id: int,
        content: str,
        *,
        reason: str | None = None,
        topic: str | None = None,
        category: str | None = None,
        tags: list[str] | None = None,
        title: str | None = None,
        agent_id: str | None = None,
        format: str | None = None,
    ) -> dict:
        args: dict[str, Any] = {"original_id": original_id, "content": content}
        if reason is not None:
            args["reason"] = reason
        if topic is not None:
            args["topic"] = topic
        if category is not None:
            args["category"] = category
        if tags is not None:
            args["tags"] = tags
        if title is not None:
            args["title"] = title
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        return self.call_tool("context_correct", args)

    def context_deprecate(
        self,
        entry_id: int,
        *,
        reason: str | None = None,
        agent_id: str | None = None,
        format: str | None = None,
    ) -> dict:
        args: dict[str, Any] = {"id": entry_id}
        if reason is not None:
            args["reason"] = reason
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        return self.call_tool("context_deprecate", args)

    def context_status(
        self,
        *,
        topic: str | None = None,
        category: str | None = None,
        agent_id: str | None = None,
        format: str | None = None,
        check_embeddings: bool | None = None,
        timeout: float | None = None,
    ) -> dict:
        args: dict[str, Any] = {}
        if topic is not None:
            args["topic"] = topic
        if category is not None:
            args["category"] = category
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        if check_embeddings is not None:
            args["check_embeddings"] = check_embeddings
        return self.call_tool("context_status", args, timeout=timeout)

    def context_briefing(
        self,
        role: str,
        task: str,
        *,
        feature: str | None = None,
        max_tokens: int | None = None,
        agent_id: str | None = None,
        format: str | None = None,
        helpful: bool | None = None,
        timeout: float | None = None,
    ) -> dict:
        args: dict[str, Any] = {"role": role, "task": task}
        if feature is not None:
            args["feature"] = feature
        if max_tokens is not None:
            args["max_tokens"] = max_tokens
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        if helpful is not None:
            args["helpful"] = helpful
        return self.call_tool("context_briefing", args, timeout=timeout)

    def context_quarantine(
        self,
        entry_id: int,
        *,
        reason: str | None = None,
        action: str | None = None,
        agent_id: str | None = None,
        format: str | None = None,
    ) -> dict:
        args: dict[str, Any] = {"id": entry_id}
        if reason is not None:
            args["reason"] = reason
        if action is not None:
            args["action"] = action
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        return self.call_tool("context_quarantine", args)

    def context_enroll(
        self,
        target_agent_id: str,
        trust_level: str,
        capabilities: list[str],
        *,
        agent_id: str | None = None,
        format: str | None = None,
    ) -> dict:
        args: dict[str, Any] = {
            "target_agent_id": target_agent_id,
            "trust_level": trust_level,
            "capabilities": capabilities,
        }
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        return self.call_tool("context_enroll", args)

    def context_cycle(
        self,
        cycle_type: str,
        topic: str,
        *,
        keywords: list[str] | None = None,
        agent_id: str | None = None,
        format: str | None = None,
        timeout: float | None = None,
    ) -> dict:
        args: dict[str, Any] = {"type": cycle_type, "topic": topic}
        if keywords is not None:
            args["keywords"] = keywords
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        return self.call_tool("context_cycle", args, timeout=timeout)

    def context_cycle_review(
        self,
        feature_cycle: str,
        *,
        agent_id: str | None = None,
        format: str | None = None,
        timeout: float | None = None,
    ) -> dict:
        args: dict[str, Any] = {"feature_cycle": feature_cycle}
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        return self.call_tool("context_cycle_review", args, timeout=timeout)
