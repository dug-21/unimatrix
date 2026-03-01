# Scope Risk Assessment: alc-002

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Bincode schema evolution: adding `enroll_agent()` mutates AgentRecord in AGENT_REGISTRY. If the write path serializes a record with different field expectations than existing records, deserialization of old records could fail. | Med | Low | Architect should confirm AgentRecord schema is unchanged (no new fields). Enrollment updates existing fields only. |
| SR-02 | redb single-writer constraint: `enroll_agent()` takes a write transaction on the same Store used by all tools. Under concurrent MCP requests, enrollment could block or be blocked by active writes. | Low | Med | Architect should use the existing read-first, write-second pattern from `resolve_or_enroll()` to minimize write transaction duration. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Scope says "cannot modify system agent" but does not define behavior for modifying "human" agent. Demotion of the human agent could break the primary MCP client interaction path. | High | Med | Spec writer should clarify whether "human" has the same protection as "system" or whether it is modifiable. Recommend protecting both bootstrap agents. |
| SR-04 | Scope defines trust_level as a string enum ("system", "privileged", "internal", "restricted") but the existing `TrustLevel` Rust enum has specific security semantics. Invalid or adversarial trust_level strings could bypass the hierarchy. | Med | Med | Architect should ensure parsing is exhaustive with explicit rejection of unknown values. No fallback to a default trust level on parse failure. |
| SR-05 | The "self-lockout prevention" rule (caller cannot remove own Admin) is narrowly scoped. An Admin could enroll a second Admin, then that second Admin could remove the first. This is social engineering, not a code bug, but the scope implies single-admin safety. | Low | Low | Spec writer should document that cross-admin demotion is permitted by design. The invariant is: at least one Admin must always exist post-operation. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | The tool adds a 10th tool to the rmcp `#[tool]` macro set. The rmcp SDK auto-generates the tool list from the impl block. Verify that no hard limits exist on tool count in rmcp 0.16. | Low | Low | Architect should confirm rmcp has no tool count limit. Additive tools have been added before (crt-003 added quarantine). |
| SR-07 | Audit logging for enrollment must distinguish create vs. update operations. The existing AuditEvent structure uses a single `operation` string field. If "context_enroll" is used for both, downstream analysis loses granularity. | Med | Med | Architect should decide whether to use "context_enroll:create" / "context_enroll:update" or a single "context_enroll" with detail field disambiguation. |

## Assumptions

- **AGENT_REGISTRY table schema is stable.** SCOPE.md states "No schema changes (uses existing AGENT_REGISTRY table)." This assumes the current AgentRecord fields are sufficient. If enrollment needs metadata (e.g., "enrolled_by", "enrollment_reason"), a schema change would be required.
- **Admin capability is rare.** Only "system" and "human" have Admin today. The enrollment tool assumes the calling agent already has Admin, which in practice means the human or a pre-promoted orchestrator.
- **Auto-enrollment path is unchanged.** SCOPE.md AC-7 explicitly requires that unknown agents still get Restricted on first contact. The enrollment tool is additive, not a replacement.

## Design Recommendations

- **SR-03**: Protect both "system" and "human" bootstrap agents from modification via `context_enroll`. These are infrastructure agents, not swarm agents.
- **SR-04**: Implement strict TrustLevel parsing with no default fallback. Return a clear validation error for unknown trust level strings.
- **SR-07**: Use the existing `detail` field in AuditEvent to capture create/update distinction rather than inventing new operation name conventions.
