"""Response abstraction layer for test assertions (ADR-001).

All tests assert through this module rather than directly on raw
JSON-RPC responses. This centralizes response format handling so
server response changes require updating one file, not 225 tests.
"""

import json
import re
from dataclasses import dataclass
from typing import Any

from harness.client import MCPResponse


@dataclass
class ToolResult:
    """Parsed MCP tool result."""

    content: list[dict]
    is_error: bool
    text: str
    parsed: dict | list | None


def parse_tool_result(response: MCPResponse) -> ToolResult:
    """Parse an MCPResponse into a ToolResult.

    MCP tool responses have structure:
      result: { content: [{type: "text", text: "..."}], isError: bool }

    For JSON-RPC level errors:
      error: { code: int, message: str }
    """
    if response.error is not None:
        raise AssertionError(
            f"JSON-RPC error {response.error.get('code')}: "
            f"{response.error.get('message')}"
        )

    result = response.result
    if result is None:
        raise AssertionError("Response has neither result nor error")

    content = result.get("content", [])
    is_error = result.get("isError", False)

    text = ""
    if content and isinstance(content, list) and len(content) > 0:
        first = content[0]
        if isinstance(first, dict) and first.get("type") == "text":
            text = first.get("text", "")

    parsed: dict | list | None = None
    try:
        parsed = json.loads(text)
    except (json.JSONDecodeError, TypeError):
        pass

    return ToolResult(
        content=content,
        is_error=is_error,
        text=text,
        parsed=parsed,
    )


def assert_tool_success(response: MCPResponse) -> ToolResult:
    """Assert tool call succeeded and return parsed result."""
    result = parse_tool_result(response)
    assert not result.is_error, f"Tool returned error: {result.text}"
    return result


def assert_tool_error(
    response: MCPResponse, expected_substring: str | None = None
) -> ToolResult:
    """Assert tool call returned an error (tool-level, not JSON-RPC level)."""
    if response.error is not None:
        text = response.error.get("message", "")
        if expected_substring is not None:
            assert expected_substring.lower() in text.lower(), (
                f"Expected error containing '{expected_substring}', got: {text[:200]}"
            )
        return ToolResult(
            content=[], is_error=True, text=text, parsed=None
        )

    result = parse_tool_result(response)
    assert result.is_error, (
        f"Expected tool error but got success: {result.text[:200]}"
    )
    if expected_substring is not None:
        assert expected_substring.lower() in result.text.lower(), (
            f"Expected error containing '{expected_substring}', got: {result.text[:200]}"
        )
    return result


def assert_jsonrpc_error(
    response: MCPResponse, expected_code: int | None = None
) -> dict:
    """Assert JSON-RPC level error (not tool-level error)."""
    assert response.error is not None, "Expected JSON-RPC error but got success"
    if expected_code is not None:
        assert response.error.get("code") == expected_code, (
            f"Expected error code {expected_code}, got {response.error.get('code')}"
        )
    return response.error


def parse_entry(response: MCPResponse) -> dict:
    """Extract a single entry from a tool response.

    Works with context_get, context_store (format=json), context_correct.
    """
    result = assert_tool_success(response)
    if result.parsed is not None and isinstance(result.parsed, dict):
        if "entry" in result.parsed:
            return result.parsed["entry"]
        return result.parsed
    return _parse_entry_from_text(result.text)


def parse_entries(response: MCPResponse) -> list[dict]:
    """Extract a list of entries from search or lookup response.

    Works with context_search and context_lookup (format=json).
    """
    result = assert_tool_success(response)
    if result.parsed is not None:
        if isinstance(result.parsed, dict):
            entries = result.parsed.get("entries", [])
            if isinstance(entries, list):
                return entries
            results_key = result.parsed.get("results", [])
            if isinstance(results_key, list):
                return results_key
        if isinstance(result.parsed, list):
            return result.parsed
    return []


def parse_status_report(response: MCPResponse) -> dict:
    """Extract status report data from context_status (format=json)."""
    result = assert_tool_success(response)
    if result.parsed is not None and isinstance(result.parsed, dict):
        return result.parsed
    return {}


def assert_entry_has(response: MCPResponse, field: str, expected: Any):
    """Assert a specific field value on a parsed entry."""
    entry = parse_entry(response)
    actual = entry.get(field)
    assert actual == expected, (
        f"Expected entry.{field} = {expected!r}, got {actual!r}"
    )


def assert_search_contains(response: MCPResponse, entry_id: int) -> dict:
    """Assert that entry_id appears in search/lookup results."""
    entries = parse_entries(response)
    ids = [_extract_id(e) for e in entries]
    assert entry_id in ids, (
        f"Expected entry {entry_id} in results, got IDs: {ids}"
    )
    return next(e for e in entries if _extract_id(e) == entry_id)


def assert_search_not_contains(response: MCPResponse, entry_id: int):
    """Assert that entry_id does NOT appear in search/lookup results."""
    entries = parse_entries(response)
    ids = [_extract_id(e) for e in entries]
    assert entry_id not in ids, (
        f"Entry {entry_id} should not appear in results, but found in: {ids}"
    )


def extract_entry_id(response: MCPResponse) -> int:
    """Extract the entry ID from a store or correct response.

    Handles multiple response formats:
    - store: {entry: {id: N}, stored: true}
    - correct: {correction: {id: N}, corrected: true}
    - duplicate: {existing_entry: {id: N}, duplicate: true}
    """
    result = assert_tool_success(response)
    if result.parsed and isinstance(result.parsed, dict):
        eid = result.parsed.get("id")
        if eid is None:
            eid = result.parsed.get("entry", {}).get("id")
        if eid is None:
            eid = result.parsed.get("correction", {}).get("id")
        if eid is None:
            eid = result.parsed.get("existing_entry", {}).get("id")
        if eid is not None:
            return int(eid)
    return _extract_id_from_text(result.text)


def get_result_text(response: MCPResponse) -> str:
    """Get raw result text from a successful response."""
    result = assert_tool_success(response)
    return result.text


# -- Private Helpers ---------------------------------------------------


def _extract_id(entry: dict) -> int | None:
    """Extract ID from an entry dict, handling various key names."""
    for key in ("id", "entry_id", "ID"):
        if key in entry:
            try:
                return int(entry[key])
            except (ValueError, TypeError):
                continue
    return None


def _extract_id_from_text(text: str) -> int:
    """Extract entry ID from response text using regex."""
    match = re.search(r"(?:ID|id|entry)[:\s#]+(\d+)", text)
    if match:
        return int(match.group(1))
    match = re.search(r"#(\d+)", text)
    if match:
        return int(match.group(1))
    match = re.search(r"(\d+)", text)
    if match:
        return int(match.group(1))
    raise AssertionError(
        f"Could not extract entry ID from response text: {text[:200]}"
    )


def _parse_entry_from_text(text: str) -> dict:
    """Best-effort parse entry fields from summary/markdown text."""
    entry: dict[str, str] = {}
    for line in text.split("\n"):
        match = re.match(r"\*?\*?(\w+)\*?\*?:\s*(.+)", line.strip())
        if match:
            key = match.group(1).lower()
            value = match.group(2).strip()
            entry[key] = value
    return entry
