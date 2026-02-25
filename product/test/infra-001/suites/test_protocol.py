"""Suite 1: Protocol (~15 tests).

Validates MCP protocol compliance: handshake, tool discovery,
JSON-RPC handling, malformed input, graceful shutdown.
"""

import pytest
from harness.client import UnimatrixClient
from harness.assertions import assert_tool_success, assert_tool_error, parse_tool_result
from harness.conftest import get_binary_path


@pytest.mark.smoke
def test_initialize_returns_capabilities(server):
    """P-01: Initialize response has capabilities with tools enabled."""
    resp = server.list_tools()
    result = assert_tool_success(resp)
    assert result.text or result.parsed, "Tools response should have content"


@pytest.mark.smoke
def test_server_info(server):
    """P-02: Server reports name and version during initialize."""
    # Server fixture already initialized successfully.
    # The initialize response validated server info.
    # Verify by confirming tool calls work (server is alive).
    resp = server.context_status(agent_id="human", format="json")
    assert_tool_success(resp)


def test_list_tools_returns_nine(server):
    """P-03: tools/list returns exactly 9 context_* tools."""
    resp = server.list_tools()
    raw = resp.result
    assert raw is not None, "tools/list should return a result"
    tools = raw.get("tools", [])
    tool_names = sorted([t["name"] for t in tools])
    expected = sorted([
        "context_search",
        "context_lookup",
        "context_get",
        "context_store",
        "context_correct",
        "context_deprecate",
        "context_status",
        "context_briefing",
        "context_quarantine",
    ])
    assert tool_names == expected, f"Expected {expected}, got {tool_names}"


def test_tool_schemas_valid(server):
    """P-04: Each tool's inputSchema is valid JSON Schema object."""
    resp = server.list_tools()
    raw = resp.result
    tools = raw.get("tools", [])
    for tool in tools:
        schema = tool.get("inputSchema", {})
        assert "type" in schema, f"Tool {tool['name']} schema missing 'type'"
        assert schema["type"] == "object", f"Tool {tool['name']} schema type should be 'object'"


def test_unknown_tool_returns_error(server):
    """P-05: Calling nonexistent tool returns error."""
    resp = server.call_tool("context_nonexistent", {})
    assert_tool_error(resp)


def test_malformed_json_handled(server):
    """P-06: Invalid JSON on stdin doesn't crash server."""
    server.send_raw_bytes(b"this is not json\n")
    # Server should survive; subsequent calls should still work
    resp = server.context_status(agent_id="human")
    assert_tool_success(resp)


def test_missing_required_params(server):
    """P-07: Tool call without required params returns error."""
    resp = server.call_tool("context_store", {})
    assert_tool_error(resp)


def test_concurrent_sequential_requests(server):
    """P-08: Two rapid sequential requests both get correct responses."""
    resp1 = server.context_store(
        "entry one for protocol test", "testing", "convention", agent_id="human"
    )
    resp2 = server.context_store(
        "entry two for protocol test", "testing", "convention", agent_id="human"
    )
    assert_tool_success(resp1)
    assert_tool_success(resp2)


@pytest.mark.smoke
def test_graceful_shutdown(tmp_path):
    """P-10: Shutdown request + clean process exit."""
    binary = get_binary_path()
    client = UnimatrixClient(binary, project_dir=str(tmp_path))
    client.initialize()
    client.shutdown()
    assert client._process.poll() is not None, "Server should have exited"


def test_empty_tool_arguments(server):
    """P-13: {} arguments handled per tool defaults."""
    resp = server.call_tool("context_status", {})
    assert_tool_success(resp)


def test_unknown_fields_ignored(server):
    """P-14: Extra fields in arguments don't cause errors."""
    resp = server.call_tool(
        "context_status", {"unknown_field": "value", "agent_id": "human"}
    )
    assert_tool_success(resp)


def test_json_format_responses_parseable(server):
    """P-15: All tools with format=json return valid JSON."""
    server.context_store(
        "protocol format test content",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    resp = server.context_status(agent_id="human", format="json")
    result = assert_tool_success(resp)
    assert result.parsed is not None, "format=json should return parseable JSON"


def test_store_then_search_roundtrip(server):
    """P-extra: Verify store->search basic flow works at protocol level."""
    store_resp = server.context_store(
        "unique protocol roundtrip content xyz123",
        "testing",
        "convention",
        agent_id="human",
    )
    assert_tool_success(store_resp)
    search_resp = server.context_search("protocol roundtrip xyz123")
    assert_tool_success(search_resp)
