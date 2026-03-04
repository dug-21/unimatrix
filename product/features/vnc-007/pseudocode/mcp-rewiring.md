# Pseudocode: MCP Rewiring (tools.rs context_briefing)

## Overview

Replace the inline ~230-line briefing assembly in `context_briefing` with a thin delegation to `BriefingService::assemble()`. The tool handler retains all transport-specific concerns: identity resolution, capability check, format parsing, usage recording, response formatting.

Gate the entire `context_briefing` method behind `#[cfg(feature = "mcp-briefing")]`.

## Current code (to be replaced): lines 1385-1610

Steps 6-9 (convention lookup, duties lookup, semantic search, budget allocation) are replaced by a single `services.briefing.assemble()` call.

## Pseudocode

```rust
#[cfg(feature = "mcp-briefing")]
#[tool(
    name = "context_briefing",
    description = "Get an orientation briefing for a role and task. Includes role conventions and task-relevant context from the knowledge base. Use at the start of any task."
)]
async fn context_briefing(
    &self,
    Parameters(params): Parameters<BriefingParams>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    // 1. Identity resolution (TRANSPORT-SPECIFIC, retained)
    let identity = self
        .resolve_agent(&params.agent_id)
        .map_err(rmcp::ErrorData::from)?;

    // 2. Capability check (TRANSPORT-SPECIFIC, retained)
    self.registry
        .require_capability(&identity.agent_id, Capability::Read)
        .map_err(rmcp::ErrorData::from)?;

    // 3. MCP-specific param validation (TRANSPORT-SPECIFIC, retained)
    validate_briefing_params(&params).map_err(rmcp::ErrorData::from)?;
    validate_helpful(&params.helpful).map_err(rmcp::ErrorData::from)?;

    // 4. Parse format (TRANSPORT-SPECIFIC, retained)
    let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

    // 5. Validate max_tokens (TRANSPORT-SPECIFIC validation, reused)
    let max_tokens = validated_max_tokens(params.max_tokens).map_err(rmcp::ErrorData::from)?;

    // 6. Build AuditContext (TRANSPORT-SPECIFIC)
    let audit_ctx = AuditContext {
        source: AuditSource::Mcp {
            agent_id: identity.agent_id.clone(),
            trust_level: identity.trust_level,
        },
        caller_id: identity.agent_id.clone(),
        session_id: None,
        feature_cycle: None,
    };

    // 7. Construct BriefingParams and delegate (NEW — replaces steps 6-9)
    let briefing_params = services::briefing::BriefingParams {
        role: Some(params.role.clone()),
        task: Some(params.task.clone()),
        feature: params.feature.clone(),
        max_tokens,
        include_conventions: true,
        include_semantic: true,
        injection_history: None,
    };

    let result = self.services.briefing
        .assemble(briefing_params, &audit_ctx)
        .await
        .map_err(rmcp::ErrorData::from)?;

    // 8. Convert BriefingResult -> Briefing for format_briefing
    let briefing = Briefing {
        role: params.role.clone(),
        task: params.task.clone(),
        conventions: result.conventions,
        relevant_context: result.relevant_context,
        search_available: result.search_available,
    };

    // 9. Audit (TRANSPORT-SPECIFIC, retained — but now uses result.entry_ids)
    let _ = self.audit.log_event(AuditEvent { ... target_ids: result.entry_ids.clone() ... });

    // 10. Usage recording (TRANSPORT-SPECIFIC, retained)
    self.record_usage_for_entries(
        &identity.agent_id,
        identity.trust_level,
        &result.entry_ids,
        params.helpful,
        params.feature.as_deref(),
    ).await;

    // 11. Format response (TRANSPORT-SPECIFIC, retained)
    Ok(format_briefing(&briefing, format))
}
```

## Changes Summary

| What | Before | After |
|------|--------|-------|
| Convention lookup | Inline QueryFilter + entry_store.query | BriefingService handles |
| Duties lookup | Inline QueryFilter + entry_store.query | REMOVED entirely |
| Semantic search | Inline embed + HNSW + feature/co-access boost (~80 lines) | BriefingService delegates to SearchService |
| Budget allocation | Inline char budget loop for 3 categories | BriefingService handles |
| Entry ID collection | Inline from 3 sources + duties | BriefingResult.entry_ids |
| `#[cfg(feature)]` | None | Added to method |
| Tool description | Mentions "duties" | Updated to remove "duties" |

## Removed Imports

After rewiring, these imports in tools.rs become unused for context_briefing and can be removed if not used by other tools:
- Direct `EmbedService` usage for briefing (already used by other tools, so likely retained)
- `unimatrix_embed::l2_normalized` usage for briefing (may still be used by other code)
- `crate::coaccess::compute_briefing_boost` (only used by briefing, can be removed)
- `crate::coaccess::CO_ACCESS_STALENESS_SECONDS` (may still be used)

Note: Careful dead-code analysis needed during implementation.

## Patterns

- Follows existing tool handler pattern: identity -> capability -> validate -> delegate -> format
- `#[cfg(feature)]` on method is the preferred approach per ADR-001. If rmcp macro does not support this, fallback to wrapper approach.
