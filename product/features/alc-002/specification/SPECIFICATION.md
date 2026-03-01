# Specification: alc-002 Agent Enrollment Tool

## Objective

Add a `context_enroll` MCP tool that enables Admin-level agents to enroll new agents or update existing agents with specific trust levels and capabilities. This eliminates the blocker where spawned swarm agents cannot write to Unimatrix because auto-enrollment grants only Restricted (read-only) access.

## Functional Requirements

1. **FR-01**: The `context_enroll` tool accepts `target_agent_id` (string, required), `trust_level` (string, required), `capabilities` (string array, required), `agent_id` (string, optional), and `format` (string, optional).
2. **FR-02**: When `target_agent_id` does not exist in AGENT_REGISTRY, create a new AgentRecord with the specified trust level and capabilities.
3. **FR-03**: When `target_agent_id` already exists in AGENT_REGISTRY, update its trust_level and capabilities fields. Preserve `enrolled_at` and `active` fields.
4. **FR-04**: Only agents with Admin capability may call `context_enroll`. Non-Admin callers receive a CapabilityDenied error.
5. **FR-05**: Modification of protected bootstrap agents ("system", "human") is rejected with a ProtectedAgent error.
6. **FR-06**: The caller cannot remove their own Admin capability. If `target_agent_id` equals the caller's resolved agent ID and the capabilities list does not include "admin", reject with a SelfLockout error.
7. **FR-07**: The operation is recorded in the AUDIT_LOG with operation name "context_enroll" and detail indicating "created" or "updated".
8. **FR-08**: Unknown agents contacting the server through any other tool still auto-enroll as Restricted with Read + Search capabilities (existing behavior unchanged).
9. **FR-09**: Trust level parsing accepts exactly four values (case-insensitive): "system", "privileged", "internal", "restricted". Any other value returns InvalidInput.
10. **FR-10**: Capabilities parsing accepts exactly four values (case-insensitive): "read", "write", "search", "admin". Any other value returns InvalidInput. Duplicates in the input are rejected.

## Non-Functional Requirements

1. **NFR-01**: No new crate dependencies.
2. **NFR-02**: No schema changes to AGENT_REGISTRY or any other table.
3. **NFR-03**: Enrollment write transaction duration comparable to existing `resolve_or_enroll()` (~single table read + write).
4. **NFR-04**: All new code must pass `cargo clippy --workspace` with zero warnings.
5. **NFR-05**: Full `cargo test --workspace` must pass with no regressions.

## Acceptance Criteria

| AC-ID | Description | Verification Method |
|-------|-------------|---------------------|
| AC-01 | Admin agent can enroll a new agent with Write capability; the enrolled agent can then call `context_store` successfully | test |
| AC-02 | Admin agent can promote an existing Restricted agent, upgrading capabilities without re-enrollment | test |
| AC-03 | Non-Admin agents are rejected with CapabilityDenied error | test |
| AC-04 | Attempts to modify "system" agent return ProtectedAgent error | test |
| AC-05 | Caller cannot remove their own Admin capability (SelfLockout error) | test |
| AC-06 | Enrollment is audited in AUDIT_LOG with caller and target | test |
| AC-07 | Existing auto-enrollment unchanged: unknown agents get Restricted on first contact with other tools | test |

## Domain Models

### Trust Hierarchy (existing, unchanged)

| Level | Agents | Default Capabilities |
|-------|--------|---------------------|
| System | "system" (server internals) | Read, Write, Search, Admin |
| Privileged | "human" (MCP client user) | Read, Write, Search, Admin |
| Internal | Orchestrator agents | Read, Write, Search (Admin optional) |
| Restricted | Worker agents, unknowns | Read, Search |

### Enrollment Operation

An enrollment operation is either:
- **Create**: target does not exist -> new AgentRecord created
- **Update**: target exists -> trust_level and capabilities replaced, enrolled_at preserved

### Protected Agents

"system" and "human" are bootstrap-only agents. They cannot be modified through `context_enroll`. This is identity-based protection (specific agent IDs), not trust-level-based.

## User Workflows

### Workflow 1: Scrum Master enrolls swarm agents

1. Human (Admin) or orchestrator (pre-enrolled Admin) calls `context_enroll` for each swarm agent ID
2. Each enrolled agent receives Internal trust with Read + Write + Search capabilities
3. Enrolled agents can now call `context_store`, `context_correct` without being blocked

### Workflow 2: Promoting an auto-enrolled agent

1. Agent "uni-researcher" contacts server via `context_search` (auto-enrolled as Restricted)
2. Human calls `context_enroll(target_agent_id: "uni-researcher", trust_level: "internal", capabilities: ["read", "write", "search"])`
3. "uni-researcher" can now store findings via `context_store`

### Workflow 3: Demoting an agent

1. Admin calls `context_enroll(target_agent_id: "rogue-agent", trust_level: "restricted", capabilities: ["read", "search"])`
2. "rogue-agent" loses Write capability immediately

## Constraints

- Tool must follow the existing execution pipeline: identity -> capability -> validation -> business logic -> format -> audit
- No modification to existing tool behavior or tool signatures
- Response format supports summary, markdown, and json (consistent with all other tools)
- Error codes for new error variants must not collide with existing codes (32001-32003 are taken)

## Dependencies

- `AgentRegistry` (registry.rs) — add `enroll_agent()` method
- `AuditLog` (audit.rs) — reuse existing `log_event()`
- `validation.rs` — add validation and parsing functions
- `response.rs` — add response formatter
- `error.rs` — add ProtectedAgent and SelfLockout variants

## NOT in Scope

- Topic/category restrictions on enrolled agents (allowed_topics, allowed_categories fields exist but are not settable via this tool — future enhancement)
- Bulk enrollment (enroll multiple agents in one call)
- Agent deactivation (setting `active: false`) — not exposed through this tool
- List/query enrolled agents (a `context_agents` list tool is a separate feature)
- Automated enrollment rules (prefix-matching, pattern-based auto-promotion)
