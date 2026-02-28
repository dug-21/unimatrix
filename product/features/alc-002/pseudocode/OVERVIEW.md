# Pseudocode Overview: alc-002 Agent Enrollment Tool

## Components

| Component | File | Purpose |
|-----------|------|---------|
| error | crates/unimatrix-server/src/error.rs | Add ProtectedAgent and SelfLockout variants |
| validation | crates/unimatrix-server/src/validation.rs | Add validate_enroll_params, parse_trust_level, parse_capabilities |
| registry | crates/unimatrix-server/src/registry.rs | Add EnrollResult, enroll_agent(), PROTECTED_AGENTS |
| response | crates/unimatrix-server/src/response.rs | Add format_enroll_success() |
| tool | crates/unimatrix-server/src/tools.rs | Add EnrollParams, context_enroll tool handler |

## Data Flow

```
MCP request (context_enroll)
  -> tools.rs: context_enroll()
     1. resolve_agent(params.agent_id) -> identity.rs (existing)
     2. require_capability(Admin) -> registry.rs (existing)
     3. validate_enroll_params(&params) -> validation.rs (NEW)
     4. parse_trust_level(&params.trust_level) -> validation.rs (NEW)
     5. parse_capabilities(&params.capabilities) -> validation.rs (NEW)
     6. registry.enroll_agent(caller, target, trust, caps) -> registry.rs (NEW)
     7. format_enroll_success(&result, format) -> response.rs (NEW)
     8. audit_log.log_event(event) -> audit.rs (existing)
  -> CallToolResult
```

## Shared Types

- `EnrollResult { created: bool, agent: AgentRecord }` -- defined in registry.rs, consumed by response.rs and tools.rs
- `EnrollParams` -- defined in tools.rs, consumed by validation.rs
- `ServerError::ProtectedAgent { agent_id }` -- defined in error.rs, raised by registry.rs
- `ServerError::SelfLockout` -- defined in error.rs, raised by registry.rs

## Error Code Assignment

CRITICAL: The IMPLEMENTATION-BRIEF states codes 32004/32005 but these are already taken:
- -32004 = ERROR_EMBED_NOT_READY
- -32005 = ERROR_NOT_IMPLEMENTED
- -32006 = ERROR_CONTENT_SCAN_REJECTED
- -32007 = ERROR_INVALID_CATEGORY

New codes:
- -32008 = ERROR_PROTECTED_AGENT (new)
- -32009 = ERROR_SELF_LOCKOUT (new)

## Sequencing

All components can be built independently. No ordering constraint -- they connect at well-defined interfaces. However, error.rs should be built first since all other components depend on the new error variants.

## Build Order (Recommended)

1. error.rs (new variants, new error codes)
2. validation.rs (new parsing/validation functions)
3. registry.rs (new EnrollResult, enroll_agent method)
4. response.rs (new format_enroll_success)
5. tools.rs (new EnrollParams, context_enroll handler -- ties everything together)
