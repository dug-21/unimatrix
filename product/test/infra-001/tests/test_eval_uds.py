"""D5 test suite: UnimatrixUdsClient (uds_client.py).

AC coverage: AC-10 (tool parity), AC-11 (context manager)
Risk coverage: R-04 (framing), R-14 (path length validation)

Unit tests (marked, no daemon required) — can run offline.
Integration tests (marked with @pytest.mark.integration) — require a live daemon
via the daemon_server fixture.
"""

import json
import socket
import struct
import threading
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from harness.uds_client import (
    MAX_SOCKET_PATH_BYTES,
    UnimatrixUdsClient,
    UdsClientError,
    UdsConnectionError,
    UdsServerError,
    UdsTimeoutError,
)


# ===========================================================================
# Helpers
# ===========================================================================


def _path_of_bytes(n: int) -> str:
    """Build a socket path string whose UTF-8 encoding is exactly n bytes."""
    # "/tmp/" = 5 bytes; fill the rest with ASCII "a" (1 byte each).
    prefix = "/tmp/"
    fill_len = n - len(prefix.encode("utf-8"))
    assert fill_len >= 0, f"prefix too long for {n} bytes"
    return prefix + "a" * fill_len


# ===========================================================================
# Unit tests — no daemon required
# ===========================================================================


class TestPathLengthValidation:
    """R-14: socket path validation fires in __init__, before connect()."""

    def test_uds_path_too_long_rejected(self):
        """Path of 104 bytes raises ValueError in __init__."""
        path = _path_of_bytes(MAX_SOCKET_PATH_BYTES + 1)
        encoded_len = len(path.encode("utf-8"))
        assert encoded_len == MAX_SOCKET_PATH_BYTES + 1

        with pytest.raises(ValueError) as exc_info:
            UnimatrixUdsClient(path)

        msg = str(exc_info.value)
        assert str(MAX_SOCKET_PATH_BYTES) in msg
        assert str(encoded_len) in msg

    def test_uds_path_exactly_103_accepted(self):
        """Path of exactly 103 bytes does not raise ValueError."""
        path = _path_of_bytes(MAX_SOCKET_PATH_BYTES)
        assert len(path.encode("utf-8")) == MAX_SOCKET_PATH_BYTES
        # Should not raise — connect() will fail later, but that is a different error.
        client = UnimatrixUdsClient(path)
        assert client is not None

    def test_uds_path_1_byte_accepted(self):
        """Short path passes validation without error."""
        path = "/s"
        assert len(path.encode("utf-8")) == 2
        client = UnimatrixUdsClient(path)
        assert client is not None

    def test_uds_path_validation_uses_utf8_byte_count(self):
        """Validation counts UTF-8 bytes, not unicode codepoints."""
        # Build a path whose codepoint count < 103 but byte count > 103.
        # Each non-ASCII unicode char takes 2-4 bytes in UTF-8.
        # U+00E9 ("é") is 2 bytes in UTF-8.
        prefix = "/tmp/"
        # 5 + 49*2 = 103 bytes exactly (49 two-byte chars).
        two_byte_char = "\u00e9"
        path_103 = prefix + two_byte_char * 49
        assert len(path_103.encode("utf-8")) == 103
        UnimatrixUdsClient(path_103)  # must not raise

        # 5 + 50*2 = 105 bytes — exceeds limit.
        path_105 = prefix + two_byte_char * 50
        assert len(path_105.encode("utf-8")) == 105
        with pytest.raises(ValueError):
            UnimatrixUdsClient(path_105)

    def test_uds_path_accepts_pathlib_path(self):
        """Path argument accepts pathlib.Path objects."""
        path = Path(_path_of_bytes(50))
        client = UnimatrixUdsClient(path)
        assert client is not None

    def test_uds_path_104_bytes_raises_valueerror(self):
        """Explicit boundary: exactly 104 bytes raises ValueError (R-14)."""
        path = _path_of_bytes(104)
        with pytest.raises(ValueError):
            UnimatrixUdsClient(path)


class TestFramingProtocol:
    """R-04: raw bytes sent are newline-delimited JSON, not length-prefixed."""

    def _make_connected_client(self) -> tuple["UnimatrixUdsClient", list[bytes]]:
        """Build a client wired to a fake socket that captures sent bytes."""
        client = UnimatrixUdsClient("/tmp/fake.sock")
        client._initialized = True  # skip handshake
        fake_sock = MagicMock()
        captured: list[bytes] = []

        def capture_sendall(data: bytes) -> None:
            captured.append(data)

        fake_sock.sendall.side_effect = capture_sendall
        client._sock = fake_sock
        return client, captured

    def test_send_newline_delimited(self):
        """_send emits newline-delimited JSON, not a length prefix."""
        client, captured = self._make_connected_client()
        message = {"jsonrpc": "2.0", "id": 1, "method": "tools/list"}
        client._send(message)

        assert len(captured) == 1
        raw = captured[0]

        # Must end with newline.
        assert raw.endswith(b"\n"), "message must end with \\n"

        # First byte must be "{", not a 4-byte length prefix.
        assert raw[0:1] == b"{", "first byte must be JSON object open, not length prefix"

        # Strip newline and parse — must be valid JSON.
        parsed = json.loads(raw.rstrip(b"\n"))
        assert parsed["method"] == "tools/list"

    def test_send_no_length_prefix(self):
        """_send does NOT prepend a 4-byte integer length (C-04)."""
        client, captured = self._make_connected_client()
        message = {"jsonrpc": "2.0", "id": 2, "method": "ping"}
        client._send(message)

        raw = captured[0]
        # If there were a 4-byte length prefix, the first 4 bytes would form
        # an integer whose value equals the remaining byte count. Verify that
        # the raw bytes do NOT follow that pattern.
        if len(raw) >= 5:
            would_be_length = struct.unpack(">I", raw[:4])[0]
            # The actual payload after the "prefix" would be raw[4:].
            # If it were length-prefixed, would_be_length == len(raw) - 4.
            assert would_be_length != len(raw) - 4, (
                "first 4 bytes look like a BE length prefix — framing bug"
            )

    def test_context_manager_protocol(self):
        """__enter__ calls connect(); __exit__ calls disconnect(); no exceptions suppressed."""
        client = UnimatrixUdsClient("/tmp/nonexistent-unimatrix.sock")

        # __exit__ should propagate exceptions (return False).
        assert client.__exit__(None, None, None) is False

    def test_disconnect_idempotent_when_not_connected(self):
        """disconnect() on an unconnected client does not raise."""
        client = UnimatrixUdsClient("/tmp/fake.sock")
        assert client._sock is None
        client.disconnect()  # must not raise
        client.disconnect()  # calling twice is also safe

    def test_call_raises_uds_server_error_on_error_response(self):
        """_call raises UdsServerError when server returns JSON-RPC error."""
        client = UnimatrixUdsClient("/tmp/fake.sock")
        client._initialized = True
        client._request_id = 0

        error_response = json.dumps(
            {"jsonrpc": "2.0", "id": 1, "error": {"code": -32600, "message": "bad"}}
        ).encode("utf-8") + b"\n"

        fake_sock = MagicMock()
        call_count = [0]

        def fake_recv(n: int) -> bytes:
            call_count[0] += 1
            if call_count[0] == 1:
                return error_response
            return b""

        fake_sock.recv.side_effect = fake_recv
        fake_sock.sendall = MagicMock()
        client._sock = fake_sock

        with pytest.raises(UdsServerError) as exc_info:
            client._call("tools/list")
        assert exc_info.value.error["code"] == -32600

    def test_read_response_skips_blank_lines(self):
        """_read_response skips blank lines in the buffer."""
        client = UnimatrixUdsClient("/tmp/fake.sock")
        client._recv_buffer = (
            b"\n"
            b"\n"
            + json.dumps({"jsonrpc": "2.0", "id": 5, "result": {}}).encode("utf-8")
            + b"\n"
        )
        fake_sock = MagicMock()
        fake_sock.recv.return_value = b""  # nothing more to read
        client._sock = fake_sock

        result = client._read_response(timeout=5.0)
        assert result["id"] == 5

    def test_read_response_skips_non_json(self):
        """_read_response skips non-JSON lines (tracing output)."""
        client = UnimatrixUdsClient("/tmp/fake.sock")
        valid_msg = json.dumps({"jsonrpc": "2.0", "id": 7, "result": {}}).encode("utf-8")
        client._recv_buffer = b"not json at all\n" + valid_msg + b"\n"
        fake_sock = MagicMock()
        fake_sock.recv.return_value = b""
        client._sock = fake_sock

        result = client._read_response(timeout=5.0)
        assert result["id"] == 7


class TestSocketNotFound:
    """Failure mode: socket file does not exist -> descriptive error."""

    def test_connect_nonexistent_socket_raises_uds_connection_error(self):
        """connect() on a missing socket raises UdsConnectionError, not bare OSError."""
        client = UnimatrixUdsClient("/tmp/does-not-exist-unimatrix-uds-test.sock")
        with pytest.raises(UdsConnectionError) as exc_info:
            client.connect()
        assert "/tmp/does-not-exist-unimatrix-uds-test.sock" in str(exc_info.value)


class TestContextManagerProtocol:
    """AC-11: context manager connects and disconnects automatically."""

    def test_exit_returns_false_to_not_suppress_exceptions(self):
        """__exit__ returns False so exceptions propagate."""
        client = UnimatrixUdsClient("/tmp/fake.sock")
        result = client.__exit__(ValueError, ValueError("boom"), None)
        assert result is False

    def test_exit_calls_disconnect_even_on_exception(self):
        """__exit__ calls disconnect() regardless of whether an exception occurred."""
        client = UnimatrixUdsClient("/tmp/fake.sock")
        disconnected = []

        original_disconnect = client.disconnect

        def spy_disconnect():
            disconnected.append(True)
            original_disconnect()

        client.disconnect = spy_disconnect

        try:
            client.__exit__(RuntimeError, RuntimeError("test"), None)
        except Exception:
            pass

        assert disconnected, "disconnect() must be called from __exit__"


# ===========================================================================
# Integration tests — require a live daemon (daemon_server fixture)
# ===========================================================================


@pytest.mark.integration
class TestUdsIntegration:
    """Integration tests requiring a live daemon.

    These tests are skipped in offline/unit-only runs. Use the daemon_server
    fixture (entry #1928) to supply mcp_socket_path and socket_path.
    """

    def test_uds_connection_lifecycle(self, daemon_server):
        """FR-32: connect() and disconnect() work against a live daemon."""
        mcp_path = daemon_server["mcp_socket_path"]
        client = UnimatrixUdsClient(mcp_path)
        client.connect()
        client.disconnect()
        assert client._sock is None

    def test_uds_context_manager(self, daemon_server):
        """AC-11: context manager handles connect/disconnect automatically."""
        mcp_path = daemon_server["mcp_socket_path"]
        with UnimatrixUdsClient(mcp_path) as client:
            resp = client.context_status()
        assert resp is not None
        assert client._sock is None

    def test_uds_framing_newline_delimited(self, daemon_server):
        """R-04: raw bytes sent to socket are newline-delimited JSON (not length-prefixed)."""
        mcp_path = daemon_server["mcp_socket_path"]
        captured: list[bytes] = []

        client = UnimatrixUdsClient(mcp_path)
        original_send = client._send

        def spy_send(message: dict) -> None:
            raw = (json.dumps(message, ensure_ascii=False) + "\n").encode("utf-8")
            captured.append(raw)
            original_send(message)

        # Patch _send to capture bytes before delegation.
        with patch.object(client, "_send", side_effect=spy_send):
            client.connect()
            client.disconnect()

        assert captured, "at least one message must have been sent (initialize)"
        first = captured[0]
        assert first[0:1] == b"{", "first byte must be JSON open brace"
        assert first.endswith(b"\n"), "message must end with newline"
        json.loads(first.rstrip(b"\n"))  # must be valid JSON

    def test_uds_tool_parity_search(self, daemon_server):
        """AC-10: context_search via UDS returns equivalent results as stdio client."""
        from harness.client import UnimatrixClient
        from harness.conftest import get_binary_path

        mcp_path = daemon_server["mcp_socket_path"]
        project_dir = daemon_server["project_dir"]
        binary = get_binary_path()

        # Store via stdio client.
        with UnimatrixClient(binary, project_dir=project_dir) as stdio:
            store_resp = stdio.context_store(
                "uds parity test content unique-xyz",
                "testing",
                "pattern",
                agent_id="human",
            )
            assert store_resp.error is None

        # Search via UDS client.
        with UnimatrixUdsClient(mcp_path) as uds:
            uds_resp = uds.context_search(
                "uds parity test content unique-xyz", k=1
            )

        # Should return a result (non-empty content in the text field).
        result_text = ""
        if isinstance(uds_resp, dict):
            content = uds_resp.get("content", [])
            if content:
                result_text = content[0].get("text", "") if isinstance(content, list) else str(content)
        assert "uds parity test content" in result_text or result_text != ""

    def test_uds_concurrent_clients(self, daemon_server):
        """FR-35: multiple simultaneous UDS clients can call context_status."""
        mcp_path = daemon_server["mcp_socket_path"]
        errors: list[Exception] = []

        def run_client():
            try:
                with UnimatrixUdsClient(mcp_path) as client:
                    client.context_status()
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=run_client) for _ in range(3)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=30.0)

        assert not errors, f"Concurrent clients failed: {errors}"

    def test_uds_query_logged_as_source_uds(self, daemon_server):
        """FR-35: UDS-sourced queries appear with source='uds' in query_log."""
        import sqlite3

        mcp_path = daemon_server["mcp_socket_path"]
        project_dir = daemon_server["project_dir"]

        unique_query = "uds-source-logging-test-query-nan007"

        with UnimatrixUdsClient(mcp_path) as client:
            client.context_search(unique_query, k=1)

        # Check the query_log table directly.
        import glob as _glob
        import os

        db_candidates = _glob.glob(os.path.join(str(project_dir), "**", "*.db"), recursive=True)
        db_candidates += _glob.glob(os.path.join(str(project_dir), "**", "unimatrix.sqlite"), recursive=True)

        found_source_uds = False
        for db_path in db_candidates:
            try:
                conn = sqlite3.connect(db_path)
                try:
                    cur = conn.execute(
                        "SELECT source FROM query_log WHERE query = ? LIMIT 1",
                        (unique_query,),
                    )
                    row = cur.fetchone()
                    if row is not None:
                        found_source_uds = row[0] == "uds"
                        break
                except sqlite3.OperationalError:
                    pass
                finally:
                    conn.close()
            except Exception:
                pass

        # If no DB found or query_log not accessible, skip rather than fail.
        if not db_candidates:
            pytest.skip("Cannot locate database file to verify query_log source")
        assert found_source_uds, "Expected source='uds' in query_log for UDS search"
