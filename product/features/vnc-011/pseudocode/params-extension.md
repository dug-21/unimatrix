# Component: params-extension

## Purpose

Add `format: Option<String>` to `RetrospectiveParams` so callers can select between markdown (default) and JSON output from `context_retrospective`.

## Location

`crates/unimatrix-server/src/mcp/tools.rs` -- modify existing `RetrospectiveParams` struct.

## Modified Struct: RetrospectiveParams

```
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RetrospectiveParams {
    /// Feature cycle to analyze (e.g., "col-002").
    pub feature_cycle: String,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Maximum evidence items per hotspot (default: 3, JSON path only). (col-010b)
    pub evidence_limit: Option<usize>,
    /// Output format: "markdown" (default) or "json". (vnc-011)
    pub format: Option<String>,
}
```

Changes from current:
- ADD `pub format: Option<String>` field with serde doc comment.
- UPDATE doc comment on `evidence_limit` to clarify it applies to JSON path only with default 3. The `unwrap_or(3)` default is unchanged in handler-dispatch.

## Data Flow

- Input: JSON from MCP request, deserialized by serde.
- Output: `params.format` consumed by handler-dispatch to select formatter path.
- `format` is `Option<String>` not an enum -- matches architecture Integration Surface. Enum-like matching happens in handler-dispatch.

## Error Handling

- Deserialization: serde handles missing `format` as `None` (existing pattern for optional fields).
- Invalid values (e.g., `"xml"`): NOT validated at the struct level. Handled in handler-dispatch.

## Key Test Scenarios

1. **Deserialize with no format**: `{"feature_cycle": "col-002"}` -> `format` is `None`.
2. **Deserialize with format markdown**: `{"feature_cycle": "col-002", "format": "markdown"}` -> `format` is `Some("markdown")`.
3. **Deserialize with format json**: `{"feature_cycle": "col-002", "format": "json"}` -> `format` is `Some("json")`.
4. **Deserialize with unknown format**: `{"feature_cycle": "col-002", "format": "xml"}` -> `format` is `Some("xml")`. No deserialization error; validation deferred to handler.
5. **evidence_limit default semantics**: `params.evidence_limit` is still `None` when omitted. The `unwrap_or(3)` default is unchanged in the handler. A test should confirm `unwrap_or(3)` yields 3 for None.
