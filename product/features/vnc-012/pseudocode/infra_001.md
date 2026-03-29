# Component: infra-001/test_tools.py (modified)

## Purpose

Add two Python smoke tests (IT-01 and IT-02) to the infra-001 integration test suite.
These tests call the MCP server over the real stdio transport with string-encoded integer
IDs, exercising the full rmcp JSON-RPC dispatch path that unit tests cannot reach.

**File**: `product/test/infra-001/suites/test_tools.py`

Per ADR-003: IT-01 and IT-02 are mandatory because the rmcp `Parameters<T>` dispatch path
(`serde_json::from_value` invoked inside rmcp's `FromContextPart`) cannot be exercised by
`serde_json::from_str` tests in `tools.rs`. The Python tests send actual JSON-RPC over
stdio and receive real MCP responses.

---

## Pattern Reference

The existing test pattern is established throughout `test_tools.py`. IT-01 and IT-02
follow the same pattern as `test_store_roundtrip` (T-03):
1. Store an entry, capture its integer ID.
2. Call the target tool using `server.call_tool(name, args)` where args use a
   string-encoded ID instead of the integer ID.
3. Assert success.

Key harness methods used:
- `server.context_store(content, topic, category, agent_id=..., format="json")`
  returns an `MCPResponse`; `extract_entry_id(resp)` returns the integer entry ID.
- `server.call_tool(tool_name, args_dict)` sends a raw `tools/call` JSON-RPC request
  with the given arguments dict.
- `assert_tool_success(resp)` asserts the response is not an error.
- `get_result_text(resp)` extracts the string content of the response.

The `server` fixture provides a fresh server instance per test function (read from
`conftest.py` — standard infra-001 fixture). No special fixture is needed.

---

## New Tests (append to end of file, or in a new section block)

### Section Header

```python
# === vnc-012: String-encoded integer coercion (IT-01, IT-02) ================
```

### IT-01: test_get_with_string_id

```python
@pytest.mark.smoke
def test_get_with_string_id(server):
    """IT-01 (vnc-012): context_get accepts string-encoded id over stdio transport.

    Stores an entry and retrieves it using a JSON string id (e.g., "42" instead of 42).
    This exercises the full rmcp Parameters<T> deserialization path over stdio --
    the exact path where the live bug fires.
    Must return success and non-empty content.
    """
    -- Step 1: store an entry and capture its integer ID
    store_resp = server.context_store(
        "IT-01 string id coercion test content",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    assert_tool_success(store_resp)
    entry_id = extract_entry_id(store_resp)
    -- entry_id is an integer (int type in Python)

    -- Step 2: call context_get with the id as a JSON String, not a JSON Number
    -- Use call_tool (raw) instead of server.context_get so we control the id type
    string_id = str(entry_id)   -- e.g., "42" -- Python str, encodes as JSON String
    get_resp = server.call_tool(
        "context_get",
        {"id": string_id, "agent_id": "human"},
    )

    -- Step 3: assert success (no "invalid type: string" error)
    result = assert_tool_success(get_resp)
    text = get_result_text(get_resp)
    assert len(text) > 0, "IT-01: content must be non-empty"
    assert "IT-01 string id coercion test content" in text, (
        "IT-01: retrieved content must match stored content"
    )
```

### IT-02: test_deprecate_with_string_id

```python
@pytest.mark.smoke
def test_deprecate_with_string_id(server):
    """IT-02 (vnc-012): context_deprecate accepts string-encoded id over stdio transport.

    Stores an entry and deprecates it using a JSON string id.
    This exercises the full rmcp Parameters<T> deserialization path for a write tool.
    Must return success.
    """
    -- Step 1: store an entry
    store_resp = server.context_store(
        "IT-02 string id coercion deprecate test content",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    assert_tool_success(store_resp)
    entry_id = extract_entry_id(store_resp)

    -- Step 2: call context_deprecate with string-encoded id
    string_id = str(entry_id)
    deprecate_resp = server.call_tool(
        "context_deprecate",
        {"id": string_id, "agent_id": "human", "reason": "IT-02 coercion test"},
    )

    -- Step 3: assert success
    assert_tool_success(deprecate_resp)
    -- Optionally verify the entry is now deprecated via context_get
    -- (not required by spec, but useful for debugging)
```

---

## Initialization Sequence

No new fixtures required. Both tests use the standard `server` fixture from
`conftest.py`. The `server` fixture provides a fresh server process for each test
(or may be session-scoped — follow the existing pattern in `test_tools.py`).

The `extract_entry_id` helper is already imported at the top of `test_tools.py`.
The `assert_tool_success` and `get_result_text` helpers are already imported.

No new imports are needed.

---

## Data Flow

```
Python test
  |
  v  server.context_store(..., format="json")
  |
  v  extract_entry_id(resp) -> integer entry_id (e.g., 42)
  |
  v  str(entry_id) -> string_id (e.g., "42")
  |
  v  server.call_tool("context_get", {"id": "42", "agent_id": "human"})
       |
       v  client.py: _call("tools/call", {"name": "context_get", "arguments": {"id": "42", ...}})
            |
            v  JSON-RPC over stdio: {"method": "tools/call", "params": {"name": "context_get", "arguments": {"id": "42"}}}
                 |
                 v  rmcp 0.16.0 receives, dispatches to ToolRouter::call
                      |
                      v  Parameters<GetParams>: FromContextPart
                           serde_json::from_value(Value::Object({"id": String("42"), ...}))
                                |
                                v  GetParams::id with deserialize_with = deserialize_i64_or_string
                                     |
                                     v  visit_str("42") -> str::parse::<i64>() -> Ok(42i64)
                                          |
                                          v  handler proceeds, returns content
  |
  v  assert_tool_success(resp) -- confirms no "invalid type: string" error
  v  get_result_text(resp) -- confirms non-empty content
```

---

## Error Handling

If the coercion is NOT working, `assert_tool_success` will fail with the actual response
text, which will contain the serde error message
`"failed to deserialize parameters: invalid type: string \"42\", expected i64"`.

This failure message is sufficient to diagnose the bug and confirm the fix is needed.

---

## Key Test Scenarios

| Test | Tool | ID type sent | Expected |
|------|------|-------------|---------|
| IT-01 | context_get | JSON String `"<id>"` | success + non-empty content |
| IT-02 | context_deprecate | JSON String `"<id>"` | success |

Both tests are `@pytest.mark.smoke` so they run in the standard CI smoke suite.

ADR-003 identifies these as mandatory: unit tests in `tools.rs` exercise
`serde_json::from_str` directly against the struct, bypassing the rmcp dispatch layer.
Only the infra-001 tests send real JSON-RPC over stdio, exercising the full transport
path where the bug was observed live.

---

## Note on extract_entry_id Return Type

`extract_entry_id(resp)` returns a Python `int`. When converted via `str(entry_id)`,
Python produces a decimal integer string (e.g., `"3770"`), which is a valid base-10
integer literal accepted by `str::parse::<i64>()`. No additional encoding is needed.

Do NOT use `format=None` for the store step — without `format="json"`, `extract_entry_id`
may not find the ID in the response text. The `format="json"` argument is required to
make the response machine-parseable for ID extraction.
