# Pseudocode: response component

## Purpose

Add `format_enroll_success()` to produce format-selectable responses for successful enrollment operations. Follows the same pattern as existing format functions.

## Function: format_enroll_success

```
pub fn format_enroll_success(
    result: &EnrollResult,
    format: ResponseFormat,
) -> CallToolResult:

    let action = if result.created { "Enrolled" } else { "Updated" }
    let agent = &result.agent

    match format:
        Summary =>
            let caps_str = agent.capabilities
                .iter()
                .map(|c| capability_str(c))
                .collect::<Vec<_>>()
                .join(", ")
            let text = format!(
                "{} agent '{}' as {} with capabilities: {}",
                action,
                agent.agent_id,
                trust_level_str(agent.trust_level),
                caps_str
            )
            CallToolResult::success(vec![Content::text(text)])

        Markdown =>
            let caps_str = agent.capabilities
                .iter()
                .map(|c| capability_str(c))
                .collect::<Vec<_>>()
                .join(", ")
            let text = format!(
                "---BEGIN UNIMATRIX RESPONSE---\n\
                 ## Agent {}\n\n\
                 | Field | Value |\n\
                 |-------|-------|\n\
                 | Agent ID | {} |\n\
                 | Action | {} |\n\
                 | Trust Level | {} |\n\
                 | Capabilities | {} |\n\
                 ---END UNIMATRIX RESPONSE---",
                action,
                agent.agent_id,
                action,
                trust_level_str(agent.trust_level),
                caps_str
            )
            CallToolResult::success(vec![Content::text(text)])

        Json =>
            let json = serde_json::json!({
                "action": action.to_lowercase(),
                "agent_id": agent.agent_id,
                "trust_level": trust_level_str(agent.trust_level),
                "capabilities": agent.capabilities
                    .iter()
                    .map(|c| capability_str(c))
                    .collect::<Vec<_>>(),
            })
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&json).unwrap_or_default()
            )])
```

## Helper Functions (new, private)

```
fn trust_level_str(tl: TrustLevel) -> &'static str:
    match tl:
        TrustLevel::System => "system"
        TrustLevel::Privileged => "privileged"
        TrustLevel::Internal => "internal"
        TrustLevel::Restricted => "restricted"

fn capability_str(cap: &Capability) -> &'static str:
    match cap:
        Capability::Read => "read"
        Capability::Write => "write"
        Capability::Search => "search"
        Capability::Admin => "admin"
```

## Imports Required

```
use crate::registry::{EnrollResult, TrustLevel, Capability};
```

## Key Test Scenarios

### Summary format
- Created: "Enrolled agent 'target' as internal with capabilities: read, write, search"
- Updated: "Updated agent 'target' as privileged with capabilities: read, write, search, admin"

### Markdown format
- Contains "---BEGIN UNIMATRIX RESPONSE---" header
- Contains "---END UNIMATRIX RESPONSE---" footer
- Contains table with Agent ID, Action, Trust Level, Capabilities

### JSON format
- Valid JSON output
- Contains "action" field ("enrolled" or "updated")
- Contains "agent_id", "trust_level", "capabilities" fields
- Capabilities are lowercase strings
