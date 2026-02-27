# Pseudocode: C4 — Assertion Helpers

## File: `harness/assertions.py`

## Design (ADR-001)

All test assertions go through this module. Tests never parse raw JSON-RPC responses directly. This centralizes response format handling so server response changes require updating one file.

## Types

```python
from dataclasses import dataclass
from typing import Any

from harness.client import MCPResponse


@dataclass
class ToolResult:
    """Parsed MCP tool result."""
    content: list[dict]        # MCP content array
    is_error: bool             # Whether result indicates error
    text: str                  # Extracted text from first content item
    parsed: dict | None        # JSON-parsed text if applicable
```

## Functions

```python
def parse_tool_result(response: MCPResponse) -> ToolResult:
    """
    Parse an MCPResponse into a ToolResult.

    MCP tool responses have structure:
      result: { content: [{type: "text", text: "..."}], isError: bool }

    For JSON-RPC level errors:
      error: { code: int, message: str }
    """
    if response.error is not None:
        # JSON-RPC level error (protocol error, not tool error)
        raise AssertionError(
            f"JSON-RPC error {response.error.get('code')}: "
            f"{response.error.get('message')}"
        )

    result = response.result
    content = result.get("content", [])
    is_error = result.get("isError", False)

    # Extract text from first content item
    text = ""
    if content and content[0].get("type") == "text":
        text = content[0]["text"]

    # Try to parse text as JSON (for format=json responses)
    parsed = None
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


def assert_tool_error(response: MCPResponse, expected_substring: str | None = None) -> ToolResult:
    """Assert tool call returned an error."""
    result = parse_tool_result(response)
    assert result.is_error, f"Expected tool error but got success: {result.text[:200]}"
    if expected_substring is not None:
        assert expected_substring.lower() in result.text.lower(), (
            f"Expected error containing '{expected_substring}', got: {result.text[:200]}"
        )
    return result


def assert_jsonrpc_error(response: MCPResponse, expected_code: int | None = None) -> dict:
    """Assert JSON-RPC level error (not tool-level error)."""
    assert response.error is not None, "Expected JSON-RPC error but got success"
    if expected_code is not None:
        assert response.error.get("code") == expected_code, (
            f"Expected error code {expected_code}, got {response.error.get('code')}"
        )
    return response.error


def parse_entry(response: MCPResponse) -> dict:
    """
    Extract a single entry from a tool response.

    Works with context_get, context_store (format=json), context_correct.
    Parses the JSON format response to extract entry fields.
    """
    result = assert_tool_success(response)
    if result.parsed is not None:
        # JSON format: look for entry data in parsed structure
        if isinstance(result.parsed, dict):
            # Direct entry dict or wrapped in {entry: {...}}
            return result.parsed.get("entry", result.parsed)
    # Fall back to text parsing for summary/markdown formats
    return _parse_entry_from_text(result.text)


def parse_entries(response: MCPResponse) -> list[dict]:
    """
    Extract a list of entries from search or lookup response.

    Works with context_search and context_lookup (format=json).
    """
    result = assert_tool_success(response)
    if result.parsed is not None:
        if isinstance(result.parsed, dict):
            # Look for entries array
            entries = result.parsed.get("entries", [])
            if isinstance(entries, list):
                return entries
        if isinstance(result.parsed, list):
            return result.parsed
    return []


def parse_status_report(response: MCPResponse) -> dict:
    """Extract status report data from context_status (format=json)."""
    result = assert_tool_success(response)
    if result.parsed is not None:
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
    """Assert that entry_id appears in search/lookup results. Return the entry."""
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
    """Extract the entry ID from a store or correct response."""
    result = assert_tool_success(response)
    if result.parsed and isinstance(result.parsed, dict):
        eid = result.parsed.get("id") or result.parsed.get("entry", {}).get("id")
        if eid is not None:
            return int(eid)
    # Fall back to text parsing: look for "ID: N" or "#N" pattern
    return _extract_id_from_text(result.text)


def get_result_text(response: MCPResponse) -> str:
    """Get raw result text from a successful response."""
    result = assert_tool_success(response)
    return result.text


# ── Private Helpers ──────────────────────────────────────

def _extract_id(entry: dict) -> int | None:
    """Extract ID from an entry dict, handling various key names."""
    for key in ("id", "entry_id", "ID"):
        if key in entry:
            return int(entry[key])
    return None


def _extract_id_from_text(text: str) -> int:
    """Extract entry ID from response text using regex."""
    import re
    # Look for common patterns: "ID: 1", "id: 1", "#1", "Entry 1"
    match = re.search(r'(?:ID|id|entry)[:\s#]+(\d+)', text)
    if match:
        return int(match.group(1))
    # Try bare number at start of line
    match = re.search(r'^(\d+)', text, re.MULTILINE)
    if match:
        return int(match.group(1))
    raise AssertionError(f"Could not extract entry ID from response text: {text[:200]}")


def _parse_entry_from_text(text: str) -> dict:
    """Best-effort parse entry fields from summary/markdown text."""
    # This is a fallback for non-JSON formats.
    # Returns a dict with whatever fields can be extracted.
    entry = {}
    import re
    for line in text.split("\n"):
        # Look for "Field: value" patterns
        match = re.match(r'\*?\*?(\w+)\*?\*?:\s*(.+)', line.strip())
        if match:
            key = match.group(1).lower()
            value = match.group(2).strip()
            entry[key] = value
    return entry
```

## Usage Patterns

```python
# Store an entry and get its ID
resp = client.context_store("content", "topic", "convention", format="json")
entry_id = extract_entry_id(resp)

# Search and verify entry found
resp = client.context_search("query", format="json")
assert_search_contains(resp, entry_id)

# Get entry and check field
resp = client.context_get(entry_id, format="json")
entry = parse_entry(resp)
assert entry["topic"] == "topic"

# Verify error
resp = client.context_store("", "topic", "convention")
assert_tool_error(resp, "content")

# Verify capability enforcement
resp = client.context_quarantine(1, agent_id="unknown-agent")
assert_tool_error(resp, "capability")
```
