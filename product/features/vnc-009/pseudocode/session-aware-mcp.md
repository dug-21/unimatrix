# Pseudocode: session-aware-mcp

## File: `crates/unimatrix-server/src/mcp/tools.rs` (modifications)

### Add session_id to param structs

```
pub struct SearchParams {
    // ... existing fields ...
    /// Optional session ID (provided by hooks, not agent-reported).
    #[serde(default)]
    pub session_id: Option<String>,
}

pub struct LookupParams {
    // ... existing fields ...
    #[serde(default)]
    pub session_id: Option<String>,
}

pub struct GetParams {
    // ... existing fields ...
    #[serde(default)]
    pub session_id: Option<String>,
}

pub struct BriefingParams {
    // ... existing fields ...
    #[serde(default)]
    pub session_id: Option<String>,
}
```

### Modify build_context call sites

Each tool handler currently calls `self.build_context(&params.agent_id, &params.format)`.
We need to pass session_id through. Two approaches:

**Approach A**: Add session_id parameter to build_context.
**Approach B**: Set session_id on the ToolContext/AuditContext after build_context returns.

**Decision**: Approach A is cleaner -- modify `build_context` to accept `session_id: &Option<String>`.

## File: `crates/unimatrix-server/src/server.rs` (modifications)

### Modify build_context

```
pub(crate) fn build_context(
    &self,
    agent_id: &Option<String>,
    format: &Option<String>,
    session_id: &Option<String>,    // NEW
) -> Result<ToolContext, rmcp::ErrorData> {
    LET identity = self.resolve_agent(agent_id)?
    LET format = parse_format(format)?

    // Session ID validation (S3) and prefixing
    LET prefixed_session = IF let Some(sid) = session_id THEN
        // S3: Validate session_id
        validate_session_id(sid).map_err(rmcp::ErrorData::from)?;
        Some(prefix_session_id("mcp", sid))
    ELSE
        None
    END IF

    LET audit_ctx = AuditContext {
        source: AuditSource::Mcp {
            agent_id: identity.agent_id.clone(),
            trust_level: identity.trust_level,
        },
        caller_id: identity.agent_id.clone(),
        session_id: prefixed_session,         // CHANGED: was None
        feature_cycle: None,
    }

    LET caller_id = CallerId::Agent(identity.agent_id.clone())   // NEW

    Ok(ToolContext {
        agent_id: identity.agent_id,
        trust_level: identity.trust_level,
        format,
        audit_ctx,
        caller_id,   // NEW field
    })
}
```

### Session ID validation

```
/// Validate session_id: max 256 chars, no control characters.
/// Follows existing S3 validation patterns from SecurityGateway.
fn validate_session_id(sid: &str) -> Result<(), ServerError> {
    IF sid.len() > 256 THEN
        RETURN Err(ServerError::InvalidInput {
            field: "session_id".to_string(),
            reason: "session_id exceeds 256 characters".to_string(),
        })
    END IF

    FOR ch in sid.chars() {
        IF ch.is_control() && ch != '\n' && ch != '\t' THEN
            RETURN Err(ServerError::InvalidInput {
                field: "session_id".to_string(),
                reason: "session_id contains control characters".to_string(),
            })
        END IF
    }

    Ok(())
}
```

## File: `crates/unimatrix-server/src/mcp/context.rs` (modifications)

### Add caller_id field to ToolContext

```
pub(crate) struct ToolContext {
    pub agent_id: String,
    pub trust_level: TrustLevel,
    pub format: ResponseFormat,
    pub audit_ctx: AuditContext,
    pub caller_id: CallerId,    // NEW
}
```

## File: `crates/unimatrix-server/src/mcp/tools.rs` (handler updates)

### context_search

```
async fn context_search(&self, params: SearchParams) -> Result<CallToolResult, rmcp::ErrorData> {
    // CHANGED: pass session_id to build_context
    LET ctx = self.build_context(&params.agent_id, &params.format, &params.session_id)?;
    self.require_cap(&ctx.agent_id, Capability::Search)?;

    // ... existing validation ...

    // CHANGED: pass caller_id to search
    LET search_results = self.services.search
        .search(service_params, &ctx.audit_ctx, &ctx.caller_id)
        .await?;

    // ... format response ...

    // CHANGED: use UsageService instead of record_usage_for_entries
    self.services.usage.record_access(
        &entry_ids,
        AccessSource::McpTool,
        UsageContext {
            session_id: ctx.audit_ctx.session_id.clone(),
            agent_id: Some(ctx.agent_id.clone()),
            helpful: params.helpful,
            feature_cycle: params.feature,
            trust_level: Some(ctx.trust_level),
        },
    );

    Ok(result)
}
```

### context_lookup, context_get

Same pattern: pass session_id to build_context, use ctx.caller_id, replace
record_usage_for_entries with usage_service.record_access(McpTool).

### context_briefing

```
async fn context_briefing(&self, params: BriefingParams) -> Result<CallToolResult, rmcp::ErrorData> {
    // CHANGED: pass session_id to build_context
    LET ctx = self.build_context(&params.agent_id, &params.format, &params.session_id)?;

    // ... existing validation ...

    // CHANGED: pass caller_id to assemble
    LET briefing_result = self.services.briefing
        .assemble(briefing_params, &ctx.audit_ctx, Some(&ctx.caller_id))
        .await?;

    // CHANGED: use UsageService with Briefing source
    self.services.usage.record_access(
        &briefing_result.entry_ids,
        AccessSource::Briefing,
        UsageContext {
            session_id: ctx.audit_ctx.session_id.clone(),
            agent_id: Some(ctx.agent_id.clone()),
            helpful: params.helpful,
            feature_cycle: params.feature,
            trust_level: Some(ctx.trust_level),
        },
    );

    Ok(result)
}
```

### context_store

```
// CHANGED: pass caller_id to insert
LET insert_result = self.services.store_ops
    .insert(new_entry, None, &ctx.audit_ctx, &ctx.caller_id)
    .await?;
```

### context_correct

```
// CHANGED: pass caller_id to correct
LET correct_result = self.services.store_ops
    .correct(original_id, corrected_entry, reason, &ctx.audit_ctx, &ctx.caller_id)
    .await?;
```

### Tools that do NOT get session_id

- context_deprecate: no session_id param (write-only, no usage recording)
- context_status: no session_id param (read-only status, no usage recording)
- context_quarantine: no session_id param (admin-only)
- context_enroll: no session_id param (admin-only)
- context_retrospective: no session_id param (analysis tool)

These still call `self.build_context(&params.agent_id, &params.format, &None)` --
passing None for session_id. But they need caller_id for rate limiting on store/correct.

## File: `crates/unimatrix-server/src/uds/listener.rs` (modifications)

### Session ID prefixing

```
// In handle_connection, after authentication:
LET prefixed_session = prefix_session_id("uds", &session_id);

// Use prefixed_session in AuditContext construction:
LET audit_ctx = AuditContext {
    source: AuditSource::Uds { uid, pid, session_id: session_id.clone() },
    caller_id: agent_id.clone(),
    session_id: Some(prefixed_session),
    feature_cycle: None,
};

// Construct CallerId:
LET caller_id = CallerId::UdsSession(session_id.clone());
```

## Open Questions

None. All tools that currently call build_context will pass session_id (Some for 4
retrieval tools, None for admin/write-only tools). The build_context signature change
is the single integration point.
