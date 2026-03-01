# Test Plan Overview: alc-002 Agent Enrollment Tool

## Test Strategy

Unit tests per component, plus integration tests through the MCP protocol. All tests trace to the Risk Strategy (R-01 through R-11).

## Risk-to-Test Mapping

| Risk ID | Severity | Component | Test Type | Scenario Count |
|---------|----------|-----------|-----------|---------------|
| R-01 | High | tool, registry | Unit + Integration | 3 |
| R-02 | High | registry | Unit + Integration | 4 |
| R-03 | High | registry | Unit | 3 |
| R-04 | High | validation | Unit | 5 |
| R-05 | Med | validation | Unit | 3 |
| R-06 | Med | registry | Unit | 2 |
| R-07 | Med | tool | Unit | 2 |
| R-08 | Med | registry | Unit | 1 |
| R-09 | Med | validation | Unit | 2 |
| R-10 | Low | response | Unit | 4 |
| R-11 | Low | validation | Unit | 3 |

## Cross-Component Test Dependencies

- The tool component tests depend on registry, validation, response, and error all being functional.
- Registry tests need the Store (use tempdir pattern from existing tests).
- Validation and response tests are pure functions with no external dependencies.

## Integration Harness Plan

### Suites to Run (Stage 3c)

Per the suite selection table, this feature touches server tool logic:

| Suite | Reason |
|-------|--------|
| `smoke` | Mandatory gate |
| `tools` | New tool (context_enroll) -- must verify through MCP protocol |
| `security` | Capability enforcement for enrollment |
| `protocol` | Tool discovery must list context_enroll |

### Gaps in Existing Suites

The existing `tools` suite covers 9 tools. It needs new tests for the 10th tool (`context_enroll`).

### New Integration Tests Needed (Stage 3c)

Add to `suites/test_tools.py`:

1. `test_enroll_new_agent` -- Admin enrolls a new agent via MCP, verify success response
2. `test_enroll_update_existing_agent` -- Auto-enroll an agent (via context_search), then enroll with higher capabilities
3. `test_enroll_requires_admin` -- Non-admin agent calls context_enroll, expect capability denied
4. `test_enroll_protected_agent_rejected` -- Attempt to enroll "system", expect error
5. `test_enroll_self_lockout_prevented` -- Admin tries to remove own Admin, expect error

Add to `suites/test_security.py`:

6. `test_enroll_capability_enforcement` -- Verify enrollment respects capability model
7. `test_enrolled_agent_can_write` -- Enroll agent with Write, verify it can context_store

Add to `suites/test_lifecycle.py`:

8. `test_enroll_then_use_capabilities` -- Full lifecycle: enroll agent with Write -> agent stores entry -> search finds it
