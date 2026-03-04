# Pseudocode: tool-context

## Purpose
Extract ToolContext struct to `mcp/context.rs`, add `build_context()` and `require_cap()` methods to `server.rs`, refactor all 12 MCP handlers to use them.

## Files Created
- `src/mcp/context.rs`

## Files Modified
- `src/server.rs`
- `src/mcp/tools.rs`

## Pseudocode

### src/mcp/context.rs

```
use crate::infra::registry::TrustLevel;
use crate::mcp::response::ResponseFormat;
use crate::services::{AuditContext, AuditSource};

/// Pre-validated context available to every MCP tool handler.
/// Constructed via UnimatrixServer::build_context().
pub(crate) struct ToolContext {
    /// Resolved agent identity.
    pub agent_id: String,
    /// Agent trust level.
    pub trust_level: TrustLevel,
    /// Parsed response format.
    pub format: ResponseFormat,
    /// Pre-built audit context for service calls.
    pub audit_ctx: AuditContext,
}
```

### src/server.rs additions

```
use crate::mcp::context::ToolContext;
use crate::mcp::response::parse_format;
use crate::mcp::identity::extract_agent_id;
use crate::infra::registry::Capability;

impl UnimatrixServer {
    /// Resolve identity, parse format, build audit context.
    /// Replaces the 15-25 line ceremony in each MCP handler.
    pub(crate) fn build_context(
        &self,
        agent_id: &Option<String>,
        format: &Option<String>,
    ) -> Result<ToolContext, rmcp::ErrorData> {
        let identity = self.resolve_agent(agent_id)
            .map_err(rmcp::ErrorData::from)?;
        let format = parse_format(format)
            .map_err(rmcp::ErrorData::from)?;
        let audit_ctx = AuditContext {
            source: AuditSource::Mcp {
                agent_id: identity.agent_id.clone(),
                trust_level: identity.trust_level,
            },
            caller_id: identity.agent_id.clone(),
            session_id: None,
            feature_cycle: None,
        };
        Ok(ToolContext {
            agent_id: identity.agent_id,
            trust_level: identity.trust_level,
            format,
            audit_ctx,
        })
    }

    /// Check a capability for the given agent.
    pub(crate) fn require_cap(
        &self,
        agent_id: &str,
        cap: Capability,
    ) -> Result<(), rmcp::ErrorData> {
        self.registry.require_capability(agent_id, cap)
            .map_err(rmcp::ErrorData::from)
    }
}
```

### src/mcp/tools.rs refactoring

For each of the 12 handlers, replace the ceremony block:

BEFORE (context_search example):
```
// 1. Identity
let identity = self.resolve_agent(&params.agent_id).map_err(rmcp::ErrorData::from)?;

// 2. Capability check
self.registry.require_capability(&identity.agent_id, Capability::Search)
    .map_err(rmcp::ErrorData::from)?;

// ... validation ...

// 4. Parse format
let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

// 6. Build AuditContext (MCP transport)
let audit_ctx = AuditContext {
    source: AuditSource::Mcp {
        agent_id: identity.agent_id.clone(),
        trust_level: identity.trust_level,
    },
    caller_id: identity.agent_id.clone(),
    session_id: None,
    feature_cycle: None,
};
```

AFTER:
```
let ctx = self.build_context(&params.agent_id, &params.format)?;
self.require_cap(&ctx.agent_id, Capability::Search)?;
```

Then replace all `identity.agent_id` with `ctx.agent_id`, `identity.trust_level` with `ctx.trust_level`, `format` with `ctx.format`, and `audit_ctx` with `ctx.audit_ctx`.

### Handler-by-handler changes

Each handler gets the same pattern:
1. `let ctx = self.build_context(&params.agent_id, &params.format)?;`
2. `self.require_cap(&ctx.agent_id, Capability::X)?;`
3. Validation calls stay as-is
4. Use `ctx.audit_ctx` instead of constructing AuditContext
5. Use `ctx.format` instead of `format` variable
6. Use `ctx.agent_id` and `ctx.trust_level` for usage recording and audit

Specific handlers:
- `context_search`: Search capability
- `context_lookup`: Read capability
- `context_store`: Write capability
- `context_get`: Read capability
- `context_correct`: Write capability
- `context_deprecate`: Write capability
- `context_status`: Admin capability
- `context_briefing`: Read capability (or no cap check - verify current code)
- `context_quarantine`: Write capability
- `context_enroll`: Admin capability
- `context_retrospective`: Admin capability (verify current code)

Note: `context_briefing` currently behind feature flag. Same pattern applies.

### map_err reduction

The `build_context()` and `require_cap()` calls absorb the identity + format + capability `map_err(rmcp::ErrorData::from)` calls. Remaining `map_err` calls are for validation and service calls. Expected reduction: from ~78 to ~35 (>50%).

## Compilation Gate

After this step: `cargo check --workspace` must succeed. All handler tests pass unchanged.
