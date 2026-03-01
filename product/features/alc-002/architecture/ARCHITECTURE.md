# Architecture: alc-002 Agent Enrollment Tool

## System Overview

alc-002 adds a `context_enroll` MCP tool (10th tool) to the Unimatrix server that enables Admin-level agents to enroll new agents or update existing agents with specific trust levels and capabilities. This fills the gap between auto-enrollment (Restricted, read-only) and the required capability for swarm agents to write ADRs, outcomes, and patterns.

The feature is additive: it introduces one new tool, one new registry method, one new params struct, one new validation function, and one new response formatter. No existing behavior changes. No schema changes.

## Component Breakdown

### 1. Registry: `enroll_agent()` method

**File:** `crates/unimatrix-server/src/registry.rs`
**Responsibility:** Create or update an AgentRecord in AGENT_REGISTRY with caller-specified trust level and capabilities.

Behavior:
- If `target_agent_id` does not exist: create new AgentRecord
- If `target_agent_id` exists: update trust_level and capabilities, preserve enrolled_at
- Protected agents ("system", "human") cannot be modified
- Self-lockout prevention: caller cannot remove their own Admin capability

Returns an `EnrollResult` indicating whether the operation was a create or update, plus the final AgentRecord.

### 2. Tool: `context_enroll` tool function

**File:** `crates/unimatrix-server/src/tools.rs`
**Responsibility:** MCP tool handler following the standard execution pipeline.

Execution order (consistent with all 9 existing tools):
1. Identity resolution (`resolve_agent`)
2. Capability check (Admin required)
3. Input validation (`validate_enroll_params`)
4. Trust level + capabilities parsing
5. Business logic (`registry.enroll_agent()`)
6. Response formatting (`format_enroll_success`)
7. Audit logging

### 3. Validation: `validate_enroll_params()`

**File:** `crates/unimatrix-server/src/validation.rs`
**Responsibility:** Pure validation of EnrollParams fields.

Validates:
- `target_agent_id`: required, non-empty, max length, no control chars
- `trust_level`: required, must be one of "system", "privileged", "internal", "restricted" (case-insensitive)
- `capabilities`: required, non-empty, each must be one of "read", "write", "search", "admin" (case-insensitive), no duplicates

### 4. Response: `format_enroll_success()`

**File:** `crates/unimatrix-server/src/response.rs`
**Responsibility:** Format-selectable (summary/markdown/json) enrollment success response.

## Component Interactions

```
MCP Client
    |
    v
context_enroll(target_agent_id, trust_level, capabilities, agent_id, format)
    |
    v
[1] resolve_agent(agent_id) --> AgentRegistry.resolve_or_enroll()
    |                             (existing method, unchanged)
    v
[2] require_capability(Admin) --> AgentRegistry.require_capability()
    |                               (existing method, unchanged)
    v
[3] validate_enroll_params()  --> validation.rs
    |                               (new function)
    v
[4] parse_trust_level()       --> validation.rs
    parse_capabilities()            (new functions)
    |
    v
[5] registry.enroll_agent()   --> AgentRegistry
    |                               (new method, writes AGENT_REGISTRY)
    v
[6] format_enroll_success()   --> response.rs
    |                               (new function)
    v
[7] audit_log.log_event()    --> AuditLog
    |                              (existing method, unchanged)
    v
CallToolResult
```

**Data flow:** All data stays within the unimatrix-server crate. No cross-crate boundaries. The enrollment tool reads and writes the same AGENT_REGISTRY redb table that `resolve_or_enroll()` and `has_capability()` already use.

## Technology Decisions

- **No new crates or dependencies.** Uses existing redb, bincode, rmcp, schemars.
- **No schema changes.** AgentRecord struct is unchanged. The enrollment tool writes the same record format as `resolve_or_enroll()`.
- **Strict trust level parsing.** See ADR-001.
- **Bootstrap agent protection.** See ADR-002.

## Integration Points

| Integration | Type | Notes |
|------------|------|-------|
| AgentRegistry | Direct method call | New `enroll_agent()` added alongside existing methods |
| AuditLog | Direct method call | Reuse existing `log_event()` |
| validation.rs | New function | Follows existing validation patterns |
| response.rs | New function | Follows existing response format patterns |
| rmcp `#[tool]` macro | Additive | 10th tool in the impl block |

No external dependencies. No cross-crate boundaries. No new tables.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `AgentRegistry::enroll_agent()` | `fn enroll_agent(&self, caller_id: &str, target_id: &str, trust_level: TrustLevel, capabilities: Vec<Capability>) -> Result<EnrollResult, ServerError>` | registry.rs (new) |
| `EnrollResult` | `struct { created: bool, agent: AgentRecord }` | registry.rs (new) |
| `EnrollParams` | `struct { target_agent_id: String, trust_level: String, capabilities: Vec<String>, agent_id: Option<String>, format: Option<String> }` | tools.rs (new) |
| `validate_enroll_params()` | `fn validate_enroll_params(params: &EnrollParams) -> Result<(), ServerError>` | validation.rs (new) |
| `parse_trust_level()` | `fn parse_trust_level(s: &str) -> Result<TrustLevel, ServerError>` | validation.rs (new) |
| `parse_capabilities()` | `fn parse_capabilities(caps: &[String]) -> Result<Vec<Capability>, ServerError>` | validation.rs (new) |
| `format_enroll_success()` | `fn format_enroll_success(result: &EnrollResult, format: ResponseFormat) -> CallToolResult` | response.rs (new) |
| `ServerError::ProtectedAgent` | New variant: `ProtectedAgent { agent_id: String }` | error.rs (new) |
| `ServerError::SelfLockout` | New variant: `SelfLockout` | error.rs (new) |

## Error Handling

Two new error variants added to `ServerError`:

| Variant | When | Error Code | Message |
|---------|------|-----------|---------|
| `ProtectedAgent { agent_id }` | Attempt to modify "system" or "human" | 32004 (new) | "Agent '{agent_id}' is a protected bootstrap agent and cannot be modified via enrollment." |
| `SelfLockout` | Caller tries to remove own Admin | 32005 (new) | "Cannot remove Admin capability from the calling agent. This would cause lockout." |

Existing error variants reused:
- `CapabilityDenied` — non-Admin caller
- `InvalidInput` — validation failures (bad trust level string, empty capabilities, etc.)
