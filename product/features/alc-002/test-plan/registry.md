# Test Plan: registry component

## Unit Tests

All tests use the existing `make_store()` tempdir pattern from registry.rs.

### Create path

#### test_enroll_new_agent_created
- Bootstrap defaults, enroll "new-agent" with Internal + [Read, Write, Search]
- Assert: result.created == true
- Assert: result.agent.trust_level == Internal
- Assert: result.agent.capabilities == [Read, Write, Search]
- Assert: result.agent.active == true

#### test_enroll_new_agent_enrolled_at_set
- Enroll new agent, check enrolled_at > 0 and reasonable

### Update path

#### test_enroll_update_existing_agent
- Auto-enroll "worker" via resolve_or_enroll (Restricted, [Read, Search])
- Enroll "worker" with Internal + [Read, Write, Search]
- Assert: result.created == false
- Assert: result.agent.trust_level == Internal
- Assert: result.agent.capabilities == [Read, Write, Search]

#### test_enroll_update_preserves_enrolled_at (R-06)
- Auto-enroll "worker", note enrolled_at
- Enroll "worker" with different trust level
- Assert: result.agent.enrolled_at == original enrolled_at

#### test_enroll_update_preserves_active (R-06)
- Auto-enroll "worker", note active == true
- Update via enroll_agent
- Assert: result.agent.active == true

### Protection (R-02)

#### test_enroll_rejects_system
- Attempt enroll_agent(caller: "human", target: "system", ...) -> Err(ProtectedAgent)
- Assert: error contains "system"

#### test_enroll_rejects_human
- Attempt enroll_agent(caller: "admin-agent", target: "human", ...) -> Err(ProtectedAgent)

#### test_enroll_allows_case_different_system
- Enroll target "SYSTEM" (uppercase) -> Ok (case-sensitive IDs, "SYSTEM" != "system")

#### test_enroll_protected_no_state_change
- Bootstrap, get system record, attempt enroll "system", get system record again
- Assert: records identical (no side effects on rejection)

### Self-lockout (R-03)

#### test_enroll_self_without_admin_rejected
- Pre-enroll "admin-agent" with Admin capability
- Call enroll_agent(caller: "admin-agent", target: "admin-agent", caps: [Read, Write, Search]) -- no Admin
- Assert: Err(SelfLockout)

#### test_enroll_self_with_admin_allowed
- Pre-enroll "admin-agent" with Admin
- Call enroll_agent(caller: "admin-agent", target: "admin-agent", caps: [Read, Write, Admin])
- Assert: Ok with result.created == false

### Sequential updates (R-08)

#### test_enroll_sequential_updates
- Enroll "agent-x" as Internal
- Enroll "agent-x" as Restricted
- Assert: final trust_level == Restricted

### Registry consistency

#### test_enroll_then_resolve
- Enroll "new-agent" with Write capability
- Call resolve_or_enroll("new-agent")
- Assert: returned record has Write capability (not re-enrolled as Restricted)

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-02 | test_enroll_rejects_system, test_enroll_rejects_human, test_enroll_allows_case_different_system, test_enroll_protected_no_state_change |
| R-03 | test_enroll_self_without_admin_rejected, test_enroll_self_with_admin_allowed |
| R-06 | test_enroll_update_preserves_enrolled_at, test_enroll_update_preserves_active |
| R-08 | test_enroll_sequential_updates |
