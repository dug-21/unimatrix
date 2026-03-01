# Test Plan: tool component

## Unit Tests

The tool handler ties all components together. These tests verify the full execution pipeline. Since context_enroll is an async tool handler, tests need the full server setup or must test via the component functions directly.

Given the existing codebase pattern, tool-level integration is best tested through the MCP integration test harness (Stage 3c). The unit tests here focus on the EnrollParams struct and the pipeline behavior.

### test_enroll_params_deserialization
- Verify EnrollParams deserializes from JSON with all fields
- Verify EnrollParams deserializes with optional fields missing (agent_id, format)

### test_context_enroll_requires_admin (R-01)
- Via registry: create restricted agent, verify require_capability(Admin) fails
- This tests the capability gate that protects the enrollment tool

### test_context_enroll_default_agent_has_admin (R-01)
- Verify "human" (default when no agent_id) has Admin capability
- This confirms the default path works for human users

### test_audit_event_operation_name (R-07)
- After successful enrollment, verify audit event has operation = "context_enroll"
- Verify detail contains "created" or "updated"

### test_audit_event_on_denied (R-07)
- Verify that when capability is denied, the denial is auditable
- (Denied auditing is handled by the capability check infrastructure, not the tool directly)

### test_enroll_not_write_operation
- Verify "context_enroll" is NOT matched by is_write_operation()
- This prevents enrollment from being counted toward rate limits

## AC Coverage

| AC | Test(s) |
|----|---------|
| AC-01 | Integration test: test_enroll_then_use_capabilities |
| AC-02 | registry: test_enroll_update_existing_agent + test_enroll_update_preserves_enrolled_at |
| AC-03 | validation/registry: test_context_enroll_requires_admin, integration: test_enroll_requires_admin |
| AC-04 | registry: test_enroll_rejects_system, test_enroll_rejects_human |
| AC-05 | registry: test_enroll_self_without_admin_rejected |
| AC-06 | tool: test_audit_event_operation_name |
| AC-07 | Regression: existing test_enroll_unknown_agent in registry.rs |

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-01 | test_context_enroll_requires_admin, test_context_enroll_default_agent_has_admin |
| R-07 | test_audit_event_operation_name, test_audit_event_on_denied |
