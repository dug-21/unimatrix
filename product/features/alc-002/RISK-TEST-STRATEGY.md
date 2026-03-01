# Risk-Based Test Strategy: alc-002

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Non-Admin agent bypasses capability check and enrolls agents | High | Low | High |
| R-02 | Protected bootstrap agent ("system"/"human") modified via enrollment | High | Low | High |
| R-03 | Self-lockout: Admin removes own Admin capability | High | Med | High |
| R-04 | Invalid trust level string accepted silently, mapping to wrong TrustLevel | High | Med | High |
| R-05 | Duplicate capabilities in input cause unexpected behavior in capability checks | Med | Med | Med |
| R-06 | Enrollment of existing agent overwrites enrolled_at timestamp (audit trail loss) | Med | Med | Med |
| R-07 | Audit log fails to record enrollment operation | Med | Low | Med |
| R-08 | Concurrent enrollment of same target_agent_id causes data race | Med | Low | Med |
| R-09 | Empty capabilities array accepted, creating agent with no permissions | Med | Med | Med |
| R-10 | Enrollment response format inconsistent with other tools | Low | Med | Low |
| R-11 | Control characters in target_agent_id stored in registry | Med | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Non-Admin agent bypasses capability check
**Severity**: High
**Likelihood**: Low
**Impact**: Unprivileged agent can promote itself or others, undermining the entire trust hierarchy.

**Test Scenarios**:
1. Restricted agent calls `context_enroll` -> CapabilityDenied error
2. Internal agent (with Read+Write+Search, no Admin) calls `context_enroll` -> CapabilityDenied error
3. Agent with no agent_id (defaults to "human") calls `context_enroll` -> succeeds (human has Admin)

**Coverage Requirement**: Every trust level below Admin must be tested for rejection. The capability check must occur before any business logic.

### R-02: Protected bootstrap agent modified
**Severity**: High
**Likelihood**: Low
**Impact**: "system" or "human" agent loses capabilities, potentially breaking server operation or human access.

**Test Scenarios**:
1. Admin calls `context_enroll(target_agent_id: "system", ...)` -> ProtectedAgent error
2. Admin calls `context_enroll(target_agent_id: "human", ...)` -> ProtectedAgent error
3. Admin calls `context_enroll(target_agent_id: "SYSTEM", ...)` -> test case-sensitivity of protection (IDs are case-sensitive in registry)
4. Verify "system" and "human" records unchanged after rejected enrollment attempt

**Coverage Requirement**: Both protected agents tested. Verify no state change on rejection.

### R-03: Self-lockout
**Severity**: High
**Likelihood**: Med
**Impact**: Calling Admin loses Admin capability, leaving no Admin to re-promote. Requires database-level intervention.

**Test Scenarios**:
1. Admin "human" calls `context_enroll(target_agent_id: "human", ...)` -> ProtectedAgent error (caught by R-02 protection)
2. Admin "admin-agent" calls `context_enroll(target_agent_id: "admin-agent", trust_level: "internal", capabilities: ["read", "write", "search"])` -> SelfLockout error
3. Admin "admin-agent" calls `context_enroll(target_agent_id: "admin-agent", ..., capabilities: ["read", "write", "admin"])` -> succeeds (Admin still present)

**Coverage Requirement**: Self-modification that removes Admin is blocked. Self-modification that retains Admin is permitted.

### R-04: Invalid trust level string accepted
**Severity**: High
**Likelihood**: Med
**Impact**: Agent enrolled at wrong trust level, bypassing or unexpectedly restricting access.

**Test Scenarios**:
1. `trust_level: "admin"` (not a valid trust level) -> InvalidInput error
2. `trust_level: "SYSTEM"` -> succeeds (case-insensitive)
3. `trust_level: ""` -> InvalidInput error
4. `trust_level: "system "` (trailing space) -> InvalidInput error (strict matching)
5. `trust_level: "superadmin"` -> InvalidInput error

**Coverage Requirement**: Only the four exact values (case-insensitive) accepted. All other strings rejected.

### R-05: Duplicate capabilities
**Severity**: Med
**Likelihood**: Med
**Impact**: Vec<Capability> contains duplicates, potentially causing double-counting in capability checks (unlikely with `contains()` but violates invariant).

**Test Scenarios**:
1. `capabilities: ["read", "read"]` -> InvalidInput error (duplicates rejected)
2. `capabilities: ["read", "READ"]` -> InvalidInput error (case-insensitive dedup)
3. `capabilities: ["read", "write", "search", "admin"]` -> succeeds

**Coverage Requirement**: Duplicate detection works regardless of case.

### R-06: Enrollment overwrites enrolled_at
**Severity**: Med
**Likelihood**: Med
**Impact**: Loss of original enrollment timestamp, degrading audit trail.

**Test Scenarios**:
1. Create agent via auto-enrollment, note enrolled_at, then update via `context_enroll` -> enrolled_at preserved
2. Create agent via `context_enroll`, note enrolled_at, then update via `context_enroll` -> enrolled_at preserved

**Coverage Requirement**: `enrolled_at` field never changes on update.

### R-07: Audit log failure
**Severity**: Med
**Likelihood**: Low
**Impact**: Enrollment operation succeeds but is not audited, creating an invisible privilege change.

**Test Scenarios**:
1. Successful enrollment -> audit event exists with operation "context_enroll", agent_id = caller, detail contains "created" or "updated"
2. Failed enrollment (CapabilityDenied) -> audit event exists with outcome Denied

**Coverage Requirement**: Both success and denial paths produce audit events.

### R-08: Concurrent enrollment of same target
**Severity**: Med
**Likelihood**: Low
**Impact**: Race condition where two callers enroll the same target simultaneously, one overwriting the other.

**Test Scenarios**:
1. Sequential: enroll same target twice with different trust levels -> second call updates to latest values

**Coverage Requirement**: The read-first-then-write pattern with redb's single-writer constraint serializes concurrent writes. No explicit concurrency test needed beyond sequential update verification.

### R-09: Empty capabilities array
**Severity**: Med
**Likelihood**: Med
**Impact**: Agent exists in registry but has no capabilities, unable to do anything.

**Test Scenarios**:
1. `capabilities: []` -> InvalidInput error
2. `capabilities: [""]` -> InvalidInput error (empty string capability)

**Coverage Requirement**: At least one valid capability required.

### R-10: Response format inconsistency
**Severity**: Low
**Likelihood**: Med
**Impact**: Consumers (agents, scripts) parsing enrollment responses fail due to unexpected format.

**Test Scenarios**:
1. `format: "summary"` -> single-line response with action and target
2. `format: "markdown"` -> structured response with headers
3. `format: "json"` -> valid JSON with action, target, trust_level, capabilities
4. No format -> defaults to summary

**Coverage Requirement**: All three formats tested. JSON output must be valid JSON.

### R-11: Control characters in target_agent_id
**Severity**: Med
**Likelihood**: Low
**Impact**: Malformed agent ID stored in registry, potentially corrupting serialization or audit display.

**Test Scenarios**:
1. `target_agent_id: "agent\x00bad"` -> InvalidInput error
2. `target_agent_id: ""` -> InvalidInput error
3. `target_agent_id` exceeding max length -> InvalidInput error

**Coverage Requirement**: Standard string field validation applied to target_agent_id.

## Integration Risks

### Registry consistency
The `enroll_agent()` method and `resolve_or_enroll()` both write to AGENT_REGISTRY. After enrollment, `resolve_or_enroll()` must return the enrolled record (not re-enroll as Restricted). Test: enroll agent -> call `resolve_or_enroll()` -> verify enrolled capabilities returned.

### Capability check cascade
After enrollment with Write capability, the enrolled agent must pass `require_capability(Write)` in `context_store`. Test: enroll agent with Write -> simulate `context_store` call -> verify no CapabilityDenied.

### Audit log operation tracking
The `is_write_operation()` function in audit.rs currently checks for "context_store" and "context_correct". `context_enroll` is an administrative operation, not a knowledge write. Verify it is not counted in `write_count_since()` (used for rate limiting). If it should be counted, update `is_write_operation()`.

## Edge Cases

- Maximum length agent ID (at validation boundary)
- Agent ID with Unicode characters (valid, should work)
- Enrolling an agent that was previously auto-enrolled then never used
- Self-enrollment: Admin enrolls themselves with same capabilities (no-op should succeed)
- All four capabilities granted at once
- Single capability granted (e.g., only "read")

## Security Risks

### Untrusted input surface
`context_enroll` accepts all parameters from MCP clients. The `target_agent_id`, `trust_level`, and `capabilities` fields are attacker-controlled strings.

**Mitigations:**
- String validation (length, control chars) on `target_agent_id`
- Exhaustive enum parsing on `trust_level` and `capabilities` (no default fallback)
- Admin capability gate prevents unauthorized access to the enrollment function

### Blast radius
A compromised Admin agent could enroll arbitrary agents with Admin capabilities. The blast radius is the entire trust hierarchy.

**Mitigations:**
- Bootstrap agent protection prevents modifying "system" and "human"
- Self-lockout prevention preserves the calling Admin's access
- Audit trail records all enrollment operations for forensic analysis
- The number of Admin agents should be minimal (ideally just "human")

### Privilege escalation
An agent could enroll itself with higher privileges if it already has Admin. This is by design (Admin is the ceiling). The risk is Admin acquisition by non-Admin agents, which is blocked by the capability check.

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| Non-Admin calls enroll | CapabilityDenied error returned, no state change, audit logged with Denied outcome |
| Protected agent target | ProtectedAgent error returned, no state change |
| Self-lockout attempt | SelfLockout error returned, no state change |
| Invalid trust level | InvalidInput error returned before registry write |
| Invalid capabilities | InvalidInput error returned before registry write |
| Registry write fails (redb error) | ServerError::Registry propagated, audit not written (operation did not complete) |
| Audit write fails | Enrollment succeeded but audit failed. Log error but do not roll back enrollment (audit is fire-and-forget in existing tools) |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (bincode schema) | -- | No schema change. AgentRecord struct unchanged. No new fields. |
| SR-02 (redb single-writer) | R-08 | Same read-first-then-write pattern as resolve_or_enroll(). redb single-writer serializes. |
| SR-03 (human agent protection) | R-02 | ADR-002: Both "system" and "human" protected. ProtectedAgent error variant. |
| SR-04 (trust level parsing) | R-04 | ADR-001: Strict exhaustive parsing. No fallback. InvalidInput on unknown values. |
| SR-05 (cross-admin demotion) | R-03 | Self-lockout prevented. Cross-admin demotion is permitted by design. |
| SR-06 (rmcp tool limit) | -- | Confirmed: rmcp has no tool count limit. crt-003 added quarantine (9th tool) without issue. |
| SR-07 (audit create/update) | R-07 | Detail field in AuditEvent captures "created" or "updated" distinction. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 4 | 16 scenarios |
| Medium | 5 | 12 scenarios |
| Low | 2 | 5 scenarios |
