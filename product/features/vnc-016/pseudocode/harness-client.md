# Component 3: Harness Client Extension — `client.py`

## Purpose

Expose the `feature_cycle` parameter on the `context_store` MCP tool call from the Python
test harness. This parameter already exists in `StoreParams` in `tools.rs` (line 143) but
was not forwarded by the Python client. Integration tests require it to tag entries to a
feature cycle, enabling `feature_entries` writes that the SQL query can then find.

## File

`product/test/infra-001/harness/client.py`, method `context_store` starting at line 383.

## Current Signature

```python
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
    edges: list[dict] | None = None,
    timeout: float | None = None,
) -> MCPResponse:
```

## Modified Signature

```python
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
    edges: list[dict] | None = None,
    feature_cycle: str | None = None,   # NEW — keyword-only, after edges, before timeout
    timeout: float | None = None,
) -> MCPResponse:
```

## Body Change

Current body (lines 397-414):

```python
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
if edges is not None:
    args["edges"] = edges
return self.call_tool("context_store", args, timeout=timeout)
```

Modified body — add the `feature_cycle` guard immediately after the `edges` guard:

```python
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
if edges is not None:
    args["edges"] = edges
if feature_cycle is not None:
    args["feature_cycle"] = feature_cycle   # NEW — key matches StoreParams field name
return self.call_tool("context_store", args, timeout=timeout)
```

## Data Flow

1. Caller passes `feature_cycle="vnc016-abc12345"` as a keyword argument.
2. Guard `if feature_cycle is not None` evaluates `True`.
3. Key `"feature_cycle"` is added to `args` dict with the string value.
4. `call_tool("context_store", args, timeout=timeout)` serializes `args` to JSON.
5. MCP server deserializes JSON into `StoreParams`; `feature_cycle: Option<String>` receives
   `Some("vnc016-abc12345")` via standard serde deserialization (absent key = `None`;
   present string key = `Some(...)`).
6. Handler sets `usage_feature_cycle = Some("vnc016-abc12345")`.
7. `UsageContext { feature_cycle: Some("vnc016-abc12345"), write_capable: true, ... }` is
   constructed and passed to `usage.record_access`.
8. Gate: `if ctx.write_capable` → `feature_recording = Some(("vnc016-abc12345", [id]))`.
9. `record_feature_entries("vnc016-abc12345", &[id], phase)` writes the `feature_entries` row.

## Backward Compatibility

- `feature_cycle` defaults to `None`. All existing `context_store(...)` call sites that do
  not pass `feature_cycle` are unaffected: the guard is `False`, the key is absent from `args`,
  and `StoreParams.feature_cycle` deserializes to `None` (same behavior as before).
- The new parameter is keyword-only (after `*` separator). It cannot be passed positionally.
  No existing positional call site can accidentally bind it.
- `timeout` remains the last keyword parameter, unchanged.
- The `args` dict structure for callers not passing `feature_cycle` is byte-for-byte identical
  to the current implementation.

## Serde Contract

`StoreParams.feature_cycle: Option<String>` in `tools.rs:143` has no `#[serde(default)]`
annotation. The serde behavior for `Option<T>` without `#[serde(default)]` is:
- Absent key in JSON → `None` (serde default for `Option<T>` fields without `#[serde(deny_unknown_fields)]`)
- Explicit `null` in JSON → `None`
- String value → `Some(value)`

The `if feature_cycle is not None: args["feature_cycle"] = feature_cycle` guard ensures the
key is absent (not null) when not provided. This is the safe path per SR-05/R-09: the key is
simply not in the JSON payload, which is unambiguously `None` for serde.

## Error Handling

No new error cases. `call_tool` handles all transport errors. If `feature_cycle` is a
non-string type at the Python call site, Python's own type system surfaces the error before
the MCP call is made (the type annotation `str | None` is a development-time hint, not a
runtime check, but the `args["feature_cycle"] = feature_cycle` assignment would serialize
correctly for any JSON-serializable type anyway).

## Key Test Scenarios

1. `context_store(..., feature_cycle="vnc016-abc")` — guard fires, key in args, MCP call
   includes `"feature_cycle": "vnc016-abc"` in JSON.
2. `context_store(...)` (no feature_cycle) — guard skipped, key absent from args, backward
   compat preserved.
3. `context_store(..., feature_cycle=None)` (explicit None) — guard skips, key absent, safe.
4. All existing `context_store` call sites in `test_tools.py` and `test_lifecycle.py` pass
   without modification — regression check for backward compat (R-10).

## Constraints

- C-07: No new test files. This method change is in `client.py` only.
- C-09: `uds_client.py` is NOT modified.
- The dict key must be exactly `"feature_cycle"` — matching the `StoreParams` field name
  in `tools.rs:143`. Any other key name causes serde to silently ignore the value.
- The new parameter must be placed before `timeout` so that `timeout` remains the final
  keyword argument (callers that use `timeout` as a keyword are unaffected by the insertion).
