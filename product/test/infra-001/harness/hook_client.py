"""UnimatrixHookClient — hook IPC over AF_UNIX, 4-byte BE length prefix (D6).

Connects to the Unimatrix daemon's hook IPC socket and sends synthetic
HookRequest messages using the wire protocol defined in unimatrix_engine::wire.

Wire framing (C-05, FR-38):
  Send: struct.pack('>I', len(payload)) + payload
  Recv: read 4 bytes -> struct.unpack('>I', header)[0] -> read N bytes -> parse JSON

This is distinct from the MCP UDS framing (newline-delimited JSON) used by
UnimatrixUdsClient. Do not swap socket paths between the two clients.
"""

import json
import socket
import struct
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any


DEFAULT_TIMEOUT = 10.0
MAX_PAYLOAD_SIZE = 1_048_576  # 1 MiB — matches unimatrix_engine::wire (C-05, AC-14)


# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------


class HookClientError(Exception):
    """Base exception for hook client errors."""


class HookTimeoutError(HookClientError):
    """Raised when a socket read exceeds the timeout."""

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
    """Raised when the socket connection cannot be established."""

    def __init__(self, socket_path: str, cause: Exception):
        super().__init__(f"Failed to connect to hook socket {socket_path}: {cause}")


# ---------------------------------------------------------------------------
# Response type
# ---------------------------------------------------------------------------


@dataclass
class HookResponse:
    """Typed wrapper for a hook IPC response."""

    type: str   # e.g. "Pong", "SessionStarted", "SessionStopped", etc.
    raw: dict   # full deserialized JSON body

    @classmethod
    def from_dict(cls, data: dict) -> "HookResponse":
        return cls(type=data.get("type", "Unknown"), raw=data)


# ---------------------------------------------------------------------------
# Client
# ---------------------------------------------------------------------------


class UnimatrixHookClient:
    """Hook IPC client over AF_UNIX with 4-byte big-endian length-prefix framing.

    Connects to the Unimatrix daemon's hook socket (ProjectPaths.socket_path,
    e.g. ``{data_dir}/unimatrix.sock``). This is distinct from the MCP socket
    (ProjectPaths.mcp_socket_path) used by UnimatrixUdsClient.

    No MCP handshake is required — hook socket connections are stateless
    request/response pairs.

    Usage (context manager, recommended)::

        with UnimatrixHookClient("/path/to/unimatrix.sock") as client:
            response = client.ping()
            assert response.type == "Pong"

    Usage (manual)::

        client = UnimatrixHookClient("/path/to/unimatrix.sock")
        client.connect()
        try:
            client.session_start("sid", "nan-007", "tester")
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
        self._sock: socket.socket | None = None

    # -- Lifecycle ---------------------------------------------------------

    def connect(self) -> None:
        """Open AF_UNIX SOCK_STREAM connection to the hook IPC socket."""
        try:
            self._sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self._sock.settimeout(self._timeout)
            self._sock.connect(self._socket_path)
        except (OSError, ConnectionRefusedError) as e:
            if self._sock is not None:
                try:
                    self._sock.close()
                except OSError:
                    pass
            self._sock = None
            raise HookConnectionError(self._socket_path, e)

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

    # -- Context manager ---------------------------------------------------

    def __enter__(self) -> "UnimatrixHookClient":
        self.connect()
        return self

    def __exit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> bool:
        self.disconnect()
        return False

    # -- Private wire methods ----------------------------------------------

    def _send(self, payload: bytes) -> None:
        """Frame payload with 4-byte BE length prefix and send (C-05, FR-38).

        Raises HookPayloadTooLargeError BEFORE any send if
        len(payload) > MAX_PAYLOAD_SIZE (AC-14, R-13). This guarantees zero
        bytes are written to the socket on rejection, keeping the socket
        usable for subsequent calls.
        """
        assert self._sock is not None, "not connected"

        # Size guard fires before any socket write (AC-14, R-13).
        if len(payload) > MAX_PAYLOAD_SIZE:
            raise HookPayloadTooLargeError(len(payload))

        header = struct.pack(">I", len(payload))
        self._sock.sendall(header + payload)

    def _recv_exactly(self, n: int, timeout: float) -> bytes:
        """Read exactly n bytes from the socket, accumulating partial reads."""
        buf = b""
        deadline = time.monotonic() + timeout
        while len(buf) < n:
            remaining_time = deadline - time.monotonic()
            if remaining_time <= 0:
                raise socket.timeout
            self._sock.settimeout(remaining_time)  # type: ignore[union-attr]
            chunk = self._sock.recv(n - len(buf))  # type: ignore[union-attr]
            if not chunk:
                return buf  # connection closed; caller checks length
            buf += chunk
        return buf

    def _recv(self, timeout: float | None = None) -> dict:
        """Read one framed response: 4-byte BE length header + JSON body.

        Raises HookTimeoutError on read timeout.
        Raises HookClientError on socket close or framing error.
        """
        assert self._sock is not None, "not connected"
        effective_timeout = timeout if timeout is not None else self._timeout

        self._sock.settimeout(effective_timeout)
        try:
            header = self._recv_exactly(4, effective_timeout)
            if len(header) < 4:
                raise HookClientError(
                    "connection closed by server before length header"
                )

            body_len = struct.unpack(">I", header)[0]

            body = self._recv_exactly(body_len, effective_timeout)
            if len(body) < body_len:
                raise HookClientError(
                    f"connection closed mid-body "
                    f"(expected {body_len} bytes, got {len(body)})"
                )

            return json.loads(body.decode("utf-8"))

        except socket.timeout:
            raise HookTimeoutError("_recv", effective_timeout)
        finally:
            self._sock.settimeout(None)

    def _request(self, request: dict, timeout: float | None = None) -> HookResponse:
        """Serialize request, send with length prefix, receive framed response."""
        payload = json.dumps(request, ensure_ascii=False).encode("utf-8")
        self._send(payload)  # raises HookPayloadTooLargeError if too large
        raw = self._recv(timeout=timeout)
        return HookResponse.from_dict(raw)

    # -- 5 Typed methods (FR-37) -------------------------------------------

    def ping(self, timeout: float | None = None) -> HookResponse:
        """Send Ping request; expect Pong response (AC-12)."""
        return self._request({"type": "Ping"}, timeout=timeout)

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

    def pre_tool_use(
        self,
        session_id: str,
        tool: str,
        input: dict,
        timeout: float | None = None,
    ) -> HookResponse:
        """Send PreToolUse hook event (FR-37)."""
        return self._request(
            {
                "type": "PreToolUse",
                "session_id": session_id,
                "tool": tool,
                "input": input,
            },
            timeout=timeout,
        )

    def post_tool_use(
        self,
        session_id: str,
        tool: str,
        response_size: int,
        response_snippet: str,
        timeout: float | None = None,
    ) -> HookResponse:
        """Send PostToolUse hook event (FR-37)."""
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
