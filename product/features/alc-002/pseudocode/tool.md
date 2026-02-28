# Pseudocode: tool component

## Purpose

Add the `context_enroll` MCP tool (10th tool) to the `UnimatrixServer` impl block. Add `EnrollParams` struct. Follow the standard execution pipeline.

## New Struct: EnrollParams

```
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EnrollParams {
    /// Agent ID to enroll or update.
    pub target_agent_id: String,
    /// Trust level: "system", "privileged", "internal", "restricted".
    pub trust_level: String,
    /// Capabilities: ["read", "write", "search", "admin"].
    pub capabilities: Vec<String>,
    /// Calling agent (must have Admin).
    pub agent_id: Option<String>,
    /// Response format: "summary", "markdown", "json".
    pub format: Option<String>,
}
```

## Tool Handler: context_enroll

```
#[tool(
    name = "context_enroll",
    description = "Enroll a new agent or update an existing agent's trust level and capabilities. Requires Admin capability."
)]
async fn context_enroll(
    &self,
    Parameters(params): Parameters<EnrollParams>,
) -> Result<CallToolResult, rmcp::ErrorData>:

    // 1. Identity resolution
    let identity = self.resolve_agent(&params.agent_id)
        .map_err(rmcp::ErrorData::from)?

    // 2. Capability check (Admin required)
    self.registry.require_capability(&identity.agent_id, Capability::Admin)
        .map_err(rmcp::ErrorData::from)?

    // 3. Input validation
    validate_enroll_params(&params)
        .map_err(rmcp::ErrorData::from)?

    // 4. Parse format
    let format = parse_format(&params.format)
        .map_err(rmcp::ErrorData::from)?

    // 5. Parse trust level and capabilities (strict per ADR-001)
    let trust_level = parse_trust_level(&params.trust_level)
        .map_err(rmcp::ErrorData::from)?
    let capabilities = parse_capabilities(&params.capabilities)
        .map_err(rmcp::ErrorData::from)?

    // 6. Business logic: enroll or update agent
    let result = self.registry.enroll_agent(
        &identity.agent_id,
        &params.target_agent_id,
        trust_level,
        capabilities,
    ).map_err(rmcp::ErrorData::from)?

    // 7. Format response
    let response = format_enroll_success(&result, format)

    // 8. Audit logging
    let detail = if result.created {
        format!("created agent '{}' as {:?}", result.agent.agent_id, result.agent.trust_level)
    } else {
        format!("updated agent '{}' to {:?}", result.agent.agent_id, result.agent.trust_level)
    }

    let event = AuditEvent {
        event_id: 0,          // assigned by log_event
        timestamp: 0,         // assigned by log_event
        session_id: String::new(),
        agent_id: identity.agent_id.clone(),
        operation: "context_enroll".to_string(),
        target_ids: vec![],   // enrollment operates on agents, not entries
        outcome: Outcome::Success,
        detail,
    }
    self.audit_log.log_event(event)
        .map_err(rmcp::ErrorData::from)?

    Ok(response)
```

## Imports Required

```
// New imports needed in tools.rs
use crate::validation::{validate_enroll_params, parse_trust_level, parse_capabilities};
use crate::response::format_enroll_success;
```

## Integration with Existing Code

- `EnrollParams` struct goes with other *Params structs at the top of tools.rs
- `context_enroll` handler goes inside the `#[rmcp::tool_router]` impl block (after context_briefing or context_quarantine)
- The tool is NOT added to `is_write_operation()` (it's administrative, not a knowledge write)
- The rmcp `#[tool]` macro generates tool metadata automatically

## Error Handling

All errors are mapped to rmcp::ErrorData via `.map_err(rmcp::ErrorData::from)?`. The pipeline is fail-fast: any step failure returns the error and prevents subsequent steps.

Error sequence:
1. Identity resolution failure -> Registry error
2. Non-Admin caller -> CapabilityDenied
3. Invalid target_agent_id -> InvalidInput
4. Invalid format -> InvalidInput
5. Invalid trust level -> InvalidInput
6. Invalid capabilities -> InvalidInput
7. Protected agent -> ProtectedAgent
8. Self-lockout -> SelfLockout
9. Registry write failure -> Registry error
10. Audit write failure -> Audit error

## Key Test Scenarios

- Full happy path: Admin enrolls new agent -> success response with "Enrolled"
- Update path: Admin updates existing agent -> success response with "Updated"
- Non-Admin caller -> CapabilityDenied error
- Protected agent target -> ProtectedAgent error
- Self-lockout attempt -> SelfLockout error
- Invalid trust level -> InvalidInput error
- Invalid capabilities -> InvalidInput error
- Audit event recorded with correct operation name and detail
