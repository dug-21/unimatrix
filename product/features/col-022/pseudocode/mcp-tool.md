# col-022: mcp-tool -- Pseudocode

## Purpose

Register `context_cycle` as the 12th MCP tool on `UnimatrixServer`. Validates parameters via shared `validate_cycle_params()` and returns an acknowledgment response. The tool does not perform attribution directly -- the MCP server has no session identity. Attribution happens on the hook/UDS path.

## File: `crates/unimatrix-server/src/mcp/tools.rs`

### New Type: `CycleParams`

Place after the existing `RetrospectiveParams` struct (around line 253):

```
/// Parameters for the context_cycle tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CycleParams {
    /// Cycle action: "start" or "stop".
    pub r#type: String,
    /// Feature cycle identifier (e.g., "col-022").
    pub topic: String,
    /// Semantic keywords describing the feature work (max 5, each max 64 chars).
    pub keywords: Option<Vec<String>>,
}
```

Note: `r#type` uses raw identifier because `type` is a Rust keyword. The `JsonSchema` derive produces `"type"` in the JSON schema (the `r#` prefix is stripped by schemars).

### New Import

Add to the existing import block from `crate::infra::validation`:
```
use crate::infra::validation::validate_cycle_params;
```

### New Tool Handler: `context_cycle`

Place in the `#[tool_router]` impl block, after `context_retrospective`. Follows the established 6-step handler pipeline (FR-01, #318 convention).

```
#[tool(
    name = "context_cycle",
    description = "Declare the start or end of a feature cycle for this session. \
        Call with type='start' at session beginning to set feature attribution. \
        Call with type='stop' when feature work is complete. \
        Attribution is best-effort via the hook path; confirm via context_retrospective."
)]
async fn context_cycle(
    &self,
    Parameters(params): Parameters<CycleParams>,
) -> Result<CallToolResult, rmcp::model::ErrorData>:

    // Step 1: Identity resolution
    let identity = self.resolve_identity(params_not_available_here)
    // Note: CycleParams does not have agent_id or session_id fields.
    // Use default identity (the MCP connection identity).
    // The tool has no agent_id param because it is called by the SM agent
    // whose identity is established at connection time.

    // Step 2: Capability check -- SessionWrite required
    // Follow the pattern from context_store which also requires Write capability.
    // The #[tool] macro on UnimatrixServer auto-injects capability checking
    // if the server's capability model is set up. Check how existing tools
    // enforce capability.
    //
    // IMPLEMENTATION NOTE: Examine how context_store enforces Write capability.
    // If it's not automatic, add explicit check:
    //   if !self.has_capability(Capability::SessionWrite) { return error }

    // Step 3: Validation
    let keywords_ref = params.keywords.as_deref();
    match validate_cycle_params(&params.r#type, &params.topic, keywords_ref):
        Err(msg) =>
            return Ok(CallToolResult::error(vec![
                rmcp::model::Content::text(format!("Validation error: {msg}"))
            ]))
        Ok(validated) =>
            // proceed

    // Step 4: Build response (no business logic -- MCP server is session-unaware)
    let action = match validated.cycle_type:
        CycleType::Start => "cycle_started"
        CycleType::Stop  => "cycle_stopped"

    let response_text = format!(
        "Acknowledged: {} for topic '{}'. \
         Attribution is applied via the hook path (fire-and-forget). \
         Use context_retrospective to confirm session attribution.",
        action, validated.topic
    )

    // Step 5: Audit log
    let audit = AuditEvent {
        timestamp: 0,  // filled by audit subsystem
        session_id: String::new(),  // MCP server has no session
        agent_id: identity.agent_id,
        operation: "context_cycle".to_string(),
        target_ids: vec![],
        outcome: Outcome::Success,
        detail: format!("{} topic={}", action, validated.topic),
    }
    self.audit_log.record(audit)

    // Step 6: Return
    Ok(CallToolResult::success(vec![
        rmcp::model::Content::text(response_text)
    ]))
```

### Response Format

The response is plain text acknowledgment. It does NOT include `was_set` (Variance 2 in ALIGNMENT-REPORT: MCP server has no session identity to determine attribution outcome).

For `type: "start"`:
```
Acknowledged: cycle_started for topic 'col-022'. Attribution is applied via the hook path (fire-and-forget). Use context_retrospective to confirm session attribution.
```

For `type: "stop"`:
```
Acknowledged: cycle_stopped for topic 'col-022'. Attribution is applied via the hook path (fire-and-forget). Use context_retrospective to confirm session attribution.
```

For validation error:
```
Validation error: invalid type 'pause': must be 'start' or 'stop'
```

## Error Handling

- Validation errors: returned as `CallToolResult::error` with descriptive text (not a protocol-level error, matches existing tool pattern)
- The MCP tool never panics. All paths return `Ok(CallToolResult)`.
- No I/O operations. No database access. No session state mutations.

## Key Test Scenarios

1. **Valid start call**: `context_cycle(type="start", topic="col-022", keywords=["kw1"])` returns success with "cycle_started"
2. **Valid stop call**: `context_cycle(type="stop", topic="col-022")` returns success with "cycle_stopped"
3. **Invalid type**: `context_cycle(type="pause", ...)` returns error with "must be 'start' or 'stop'"
4. **Empty topic**: `context_cycle(type="start", topic="")` returns error with "must not be empty"
5. **Topic without hyphen**: `context_cycle(type="start", topic="foobar")` returns error with "not a valid feature cycle"
6. **Keywords truncation**: 7 keywords passed, response still succeeds (validation truncates to 5)
7. **Omitted keywords**: `keywords` field absent, succeeds
8. **Tool schema introspection**: verify the JSON schema includes `type`, `topic`, `keywords` with correct types
9. **Response does not contain `was_set`**: verify response text is acknowledgment only (R-08 mitigation)
10. **Audit log recorded**: verify `AuditEvent` with operation "context_cycle" is emitted
