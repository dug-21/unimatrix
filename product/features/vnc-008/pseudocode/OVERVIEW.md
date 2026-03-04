# vnc-008 Pseudocode Overview

## Component Interaction

The 7 components execute in a strict sequential order per ADR-004:

```
Step 1: infra-migration    (move 13 modules to infra/, create infra/mod.rs, update lib.rs)
Step 2: mcp-migration      (move tools.rs, identity.rs to mcp/, create mcp/mod.rs)
        response-split     (split response.rs into mcp/response/ sub-module)
Step 3: uds-migration      (move uds_listener.rs, hook.rs to uds/, create uds/mod.rs)
Step 4: tool-context       (create mcp/context.rs, add build_context/require_cap to server.rs)
        status-service     (extract StatusService to services/status.rs)
        session-write      (add SessionWrite capability, UDS enforcement)
Step 5: cleanup            (remove re-exports, verify final state) — done as part of each component
```

## Data Flow

```
MCP Request
  -> mcp/tools.rs handler
     -> self.build_context(agent_id, format)  [mcp/context.rs ToolContext]
     -> self.require_cap(agent_id, cap)       [server.rs -> infra/registry.rs]
     -> validate_*_params()                   [infra/validation.rs]
     -> self.services.{search,store_ops,status,...}.method(audit_ctx)  [services/*.rs]
     -> format_*()                            [mcp/response/*.rs]
  <- MCP Response

UDS Request
  -> uds/listener.rs dispatch
     -> capability check against UDS_CAPABILITIES [uds/mod.rs]
     -> self.services.{search,briefing,...}.method(audit_ctx)  [services/*.rs]
  <- HookResponse
```

## Shared Types

Types used across multiple components:

| Type | Defined In | Used By |
|------|-----------|---------|
| `ToolContext` | `mcp/context.rs` | `mcp/tools.rs` |
| `ResponseFormat` | `mcp/response/mod.rs` | `mcp/tools.rs`, `mcp/context.rs` |
| `AuditContext` | `services/mod.rs` | `mcp/tools.rs`, `uds/listener.rs`, all services |
| `Capability` | `infra/registry.rs` | `server.rs`, `mcp/tools.rs`, `uds/mod.rs` |
| `StatusReport` | `mcp/response/status.rs` | `mcp/tools.rs`, `services/status.rs` |
| `ServiceLayer` | `services/mod.rs` | `server.rs`, `mcp/tools.rs`, `uds/listener.rs` |
| `SecurityGateway` | `services/gateway.rs` | `services/status.rs` |

## Integration Harness Plan

Existing integration test suites that apply:
- `tests/integration_tests.rs` — MCP tool integration tests (all 12 tools)
- These tests import from `unimatrix_server::response::*` and `unimatrix_server::registry::*` — imports must be updated to new paths

New integration tests needed:
- UDS capability enforcement: attempt Admin op via UDS, expect rejection
- StatusService equivalence: snapshot test comparing StatusService output to inline output
- ToolContext behavioral equivalence: verify all 12 handlers produce identical output

## Patterns Used

- **Module re-export pattern**: `infra/mod.rs` re-exports all sub-modules using `pub mod X; pub use X::*;` or selective re-exports
- **Service extraction pattern**: Same as vnc-006 SearchService — new struct with Arc dependencies, async methods wrapping spawn_blocking
- **Thin wrapper pattern**: `format_deprecate_success` becomes a wrapper calling `format_status_change("Deprecated", "deprecated", "deprecated", ...)`
- **Sequential migration with re-exports**: ADR-004 — temporary `pub use infra::X as X;` in lib.rs during migration
