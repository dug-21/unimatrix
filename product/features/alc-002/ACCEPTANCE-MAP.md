# alc-002 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | Admin agent can enroll a new agent with Write capability; the enrolled agent can then call context_store successfully | test | Integration test: enroll agent with Write -> agent calls context_store -> success | PENDING |
| AC-02 | Admin agent can promote an existing Restricted agent, upgrading capabilities without re-enrollment | test | Unit test: auto-enroll agent (Restricted) -> enroll with Write -> verify capabilities updated, enrolled_at preserved | PENDING |
| AC-03 | Non-Admin agents are rejected with CapabilityDenied error | test | Unit test: Restricted agent calls context_enroll -> CapabilityDenied; Internal agent (no Admin) calls context_enroll -> CapabilityDenied | PENDING |
| AC-04 | Attempts to modify "system" agent return ProtectedAgent error | test | Unit test: Admin calls context_enroll(target: "system") -> ProtectedAgent error; same for "human" | PENDING |
| AC-05 | Caller cannot remove their own Admin capability (SelfLockout error) | test | Unit test: Admin agent enrolls itself without Admin in capabilities -> SelfLockout error | PENDING |
| AC-06 | Enrollment is audited in AUDIT_LOG with caller and target | test | Unit test: successful enrollment -> audit event exists with operation "context_enroll", detail contains target_agent_id and "created"/"updated" | PENDING |
| AC-07 | Existing auto-enrollment unchanged: unknown agents get Restricted on first contact with other tools | test | Regression test: unknown agent calls context_search -> auto-enrolled as Restricted with Read+Search only | PENDING |
