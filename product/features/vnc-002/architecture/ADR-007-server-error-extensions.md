## ADR-007: New ServerError Variants for Validation and Scanning

### Context

vnc-001's `ServerError` enum has 8 variants covering core errors, registry, audit, project init, embed states, capability denial, not-implemented, and shutdown. vnc-002 introduces three new failure modes:
1. Input validation failures (string too long, control characters, negative IDs)
2. Content scanning rejections (prompt injection, PII)
3. Category allowlist violations

Each needs a distinct MCP error code so agents can programmatically distinguish failure types and take appropriate corrective action.

### Decision

Add three new variants to `ServerError`:

```rust
InvalidInput { field: String, reason: String }
ContentScanRejected { category: String, description: String }
InvalidCategory { category: String, valid_categories: Vec<String> }
```

Map them to MCP error codes:
- `InvalidInput` -> `-32602` (standard JSON-RPC invalid params)
- `ContentScanRejected` -> `-32006` (new custom code)
- `InvalidCategory` -> `-32007` (new custom code)

`InvalidInput` reuses the standard JSON-RPC code because it IS an invalid parameter -- the agent sent a value that doesn't meet constraints. The other two get custom codes because they represent server-side policy enforcement, not JSON-RPC protocol violations.

Error messages are actionable:
- InvalidInput: "Invalid parameter '{field}': {reason}" -- tells the agent which field to fix and why
- ContentScanRejected: "Content rejected: {description} ({category} detected). Remove the flagged content and retry." -- tells the agent what was detected
- InvalidCategory: "Unknown category '{category}'. Valid categories: {list}." -- gives the agent the full list to choose from

### Consequences

**Easier:**
- Agents can programmatically distinguish validation (-32602), content scanning (-32006), and category (-32007) failures
- Error messages are self-documenting -- agents can self-correct without external guidance
- Standard JSON-RPC code for validation is recognized by all MCP clients
- Adding more error variants in vnc-003 follows the same pattern

**Harder:**
- Six custom MCP error codes now exist (-32001 through -32007, with -32602 and -32603 being standard). Documentation must track the full set.
- `Display` impl for ServerError must handle the new variants (straightforward but adds code)
- `From<ServerError> for ErrorData` mapping grows (three new match arms)
