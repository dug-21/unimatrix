"""MCP client library for unimatrix-server integration testing.

Manages a server subprocess, handles MCP JSON-RPC protocol,
and provides typed wrappers for all 10 context_* tools.
"""

import json
import os
import subprocess
import tempfile
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


DEFAULT_TIMEOUT = 10.0
SHUTDOWN_TIMEOUT = 5.0
SIGTERM_TIMEOUT = 3.0


@dataclass
class MCPResponse:
    """Parsed JSON-RPC response from the server."""

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
    """MCP client that manages a unimatrix-server subprocess.

    Spawns the server binary, completes the MCP initialize handshake,
    provides typed methods for all 9 context_* tools, and handles
    graceful shutdown with SIGTERM/SIGKILL fallback.

    Usage:
        with UnimatrixClient("/path/to/binary", project_dir="/tmp/test") as client:
            resp = client.context_store("content", "topic", "convention", agent_id="human")
    """

    def __init__(
        self,
        binary_path: str | Path,
        project_dir: str | Path | None = None,
        timeout: float = DEFAULT_TIMEOUT,
        extra_env: dict[str, str] | None = None,
    ):
        self._binary_path = str(binary_path)
        self._timeout = timeout
        self._request_id = 0
        self._stderr_lines: list[str] = []
        self._stderr_lock = threading.Lock()
        self._initialized = False

        if project_dir is None:
            self._temp_dir = tempfile.mkdtemp(prefix="unimatrix-test-")
            self._project_dir = self._temp_dir
        else:
            self._temp_dir = None
            self._project_dir = str(project_dir)

        env = os.environ.copy()
        env.setdefault("RUST_LOG", "info")
        if extra_env:
            env.update(extra_env)

        # vnc-005: default invocation is now bridge mode; use `serve --stdio` for
        # the stdio MCP path that the test harness exercises.
        self._process = subprocess.Popen(
            [self._binary_path, "--project-dir", self._project_dir, "serve", "--stdio"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        )

        self._stderr_thread = threading.Thread(target=self._drain_stderr, daemon=True)
        self._stderr_thread.start()

    def _drain_stderr(self):
        """Continuously read stderr to prevent buffer deadlock (R-09)."""
        assert self._process.stderr is not None
        for line_bytes in iter(self._process.stderr.readline, b""):
            try:
                line = line_bytes.decode("utf-8", errors="replace").rstrip()
            except Exception:
                line = repr(line_bytes)
            with self._stderr_lock:
                self._stderr_lines.append(line)

    def _next_id(self) -> int:
        self._request_id += 1
        return self._request_id

    def _send(self, message: dict):
        """Send a JSON-RPC message to the server's stdin."""
        assert self._process.stdin is not None
        data = json.dumps(message) + "\n"
        self._process.stdin.write(data.encode("utf-8"))
        self._process.stdin.flush()

    def _read_response(self, timeout: float | None = None) -> dict:
        """Read one JSON-RPC response from stdout.

        Reads line-by-line. Skips non-JSON lines (server tracing).
        Raises TimeoutError if no valid JSON within timeout.
        Raises ServerDied if process exits before responding.
        """
        timeout = timeout or self._timeout
        deadline = time.monotonic() + timeout

        while time.monotonic() < deadline:
            if self._process.poll() is not None:
                raise ServerDied(self._process.returncode, self.get_stderr())

            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break

            line = self._readline_with_timeout(min(remaining, 1.0))
            if line is None:
                continue

            line = line.strip()
            if not line:
                continue

            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue

        raise TimeoutError("read_response", timeout)

    def _readline_with_timeout(self, timeout: float) -> str | None:
        """Read one line from stdout with timeout using a background thread."""
        result: list[bytes | None] = [None]

        def reader():
            try:
                assert self._process.stdout is not None
                result[0] = self._process.stdout.readline()
            except Exception:
                pass

        t = threading.Thread(target=reader, daemon=True)
        t.start()
        t.join(timeout=timeout)

        if result[0] is not None:
            return result[0].decode("utf-8", errors="replace")
        return None

    def _call(
        self,
        method: str,
        params: dict | None = None,
        timeout: float | None = None,
    ) -> MCPResponse:
        """Send JSON-RPC request and wait for matching response."""
        req_id = self._next_id()
        message: dict[str, Any] = {
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
        }
        if params is not None:
            message["params"] = params

        self._send(message)

        timeout = timeout or self._timeout
        deadline = time.monotonic() + timeout

        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(method, timeout)
            raw = self._read_response(timeout=remaining)

            if raw.get("id") == req_id:
                return MCPResponse(
                    id=raw.get("id"),
                    result=raw.get("result"),
                    error=raw.get("error"),
                    raw=raw,
                )

    def _notify(self, method: str, params: dict | None = None):
        """Send JSON-RPC notification (no id, no response expected)."""
        message: dict[str, Any] = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self._send(message)

    # -- MCP Lifecycle -------------------------------------------------

    def initialize(self, timeout: float | None = None) -> MCPResponse:
        """Complete MCP initialize handshake.

        1. Send initialize request with client capabilities
        2. Receive initialize response with server capabilities
        3. Send initialized notification
        """
        response = self._call(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "unimatrix-test-harness",
                    "version": "1.0.0",
                },
            },
            timeout=timeout or self._timeout,
        )

        if response.error:
            raise ClientError(f"Initialize failed: {response.error}")

        self._notify("notifications/initialized")
        self._initialized = True
        return response

    def shutdown(self):
        """Graceful shutdown: MCP shutdown -> SIGTERM -> SIGKILL."""
        if self._process.poll() is not None:
            return

        if self._initialized:
            try:
                self._send(
                    {
                        "jsonrpc": "2.0",
                        "id": self._next_id(),
                        "method": "shutdown",
                        "params": {},
                    }
                )
                try:
                    self._process.wait(timeout=SHUTDOWN_TIMEOUT)
                    return
                except subprocess.TimeoutExpired:
                    pass
            except (BrokenPipeError, OSError):
                pass

        if self._process.poll() is None:
            try:
                self._process.terminate()
                self._process.wait(timeout=SIGTERM_TIMEOUT)
                return
            except subprocess.TimeoutExpired:
                pass

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

    @property
    def is_alive(self) -> bool:
        """Whether the server process is still running."""
        return self._process.poll() is None

    def wait_until_ready(self, timeout: float = 30.0):
        """Wait until the embedding model is loaded and tools are ready.

        Polls stderr for the 'embedding model loaded' message. This avoids
        the -32004 error that occurs when context_store is called before the
        model finishes initializing.
        """
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            with self._stderr_lock:
                for line in self._stderr_lines:
                    if "embedding model loaded" in line:
                        return
            if self._process.poll() is not None:
                raise ServerDied(self._process.returncode, self.get_stderr())
            time.sleep(0.2)
        raise TimeoutError("wait_until_ready", timeout)

    # -- Context Manager -----------------------------------------------

    def __enter__(self):
        self.initialize()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.shutdown()
        return False

    # -- Low-Level Methods ---------------------------------------------

    def call_tool(
        self,
        name: str,
        arguments: dict | None = None,
        timeout: float | None = None,
    ) -> MCPResponse:
        """Low-level tool call. Builds tools/call envelope."""
        return self._call(
            "tools/call",
            {"name": name, "arguments": arguments or {}},
            timeout=timeout,
        )

    def list_tools(self, timeout: float | None = None) -> MCPResponse:
        """List available MCP tools."""
        return self._call("tools/list", {}, timeout=timeout)

    def send_raw(
        self,
        method: str,
        params: dict | None = None,
        timeout: float | None = None,
    ) -> MCPResponse:
        """Send arbitrary JSON-RPC request (for protocol testing)."""
        return self._call(method, params, timeout=timeout)

    def send_raw_bytes(self, data: bytes):
        """Send raw bytes to stdin (for malformed input testing)."""
        assert self._process.stdin is not None
        self._process.stdin.write(data)
        self._process.stdin.flush()

    # -- Typed Tool Methods --------------------------------------------

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
    ) -> MCPResponse:
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
    ) -> MCPResponse:
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
    ) -> MCPResponse:
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
    ) -> MCPResponse:
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
    ) -> MCPResponse:
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
    ) -> MCPResponse:
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
    ) -> MCPResponse:
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
    ) -> MCPResponse:
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
    ) -> MCPResponse:
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
    ) -> MCPResponse:
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

    def context_cycle_review(
        self,
        feature_cycle: str,
        *,
        agent_id: str | None = None,
        format: str | None = None,
        force: bool | None = None,
        timeout: float | None = None,
    ) -> MCPResponse:
        args: dict[str, Any] = {"feature_cycle": feature_cycle}
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        if force is not None:
            args["force"] = force
        return self.call_tool("context_cycle_review", args, timeout=timeout)

    def context_cycle(
        self,
        cycle_type: str,
        topic: str,
        *,
        keywords: list[str] | None = None,
        phase: str | None = None,
        outcome: str | None = None,
        next_phase: str | None = None,
        goal: str | None = None,
        agent_id: str | None = None,
        format: str | None = None,
        timeout: float | None = None,
    ) -> MCPResponse:
        args: dict[str, Any] = {"type": cycle_type, "topic": topic}
        if keywords is not None:
            args["keywords"] = keywords
        if phase is not None:
            args["phase"] = phase
        if outcome is not None:
            args["outcome"] = outcome
        if next_phase is not None:
            args["next_phase"] = next_phase
        if goal is not None:
            args["goal"] = goal
        if agent_id is not None:
            args["agent_id"] = agent_id
        if format is not None:
            args["format"] = format
        return self.call_tool("context_cycle", args, timeout=timeout)
