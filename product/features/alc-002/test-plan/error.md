# Test Plan: error component

## Unit Tests

### test_protected_agent_display
- Assert: `ServerError::ProtectedAgent { agent_id: "system".to_string() }` displays as "agent 'system' is a protected bootstrap agent and cannot be modified via enrollment"
- Assert: Display output does not contain "ServerError"

### test_self_lockout_display
- Assert: `ServerError::SelfLockout` displays as "cannot remove Admin capability from the calling agent"
- Assert: Display output does not contain "ServerError"

### test_protected_agent_error_code
- Assert: `From<ServerError> for ErrorData` maps `ProtectedAgent` to `ERROR_PROTECTED_AGENT` (-32008)

### test_self_lockout_error_code
- Assert: `From<ServerError> for ErrorData` maps `SelfLockout` to `ERROR_SELF_LOCKOUT` (-32009)

### test_protected_agent_error_message_contains_agent_id
- Create `ProtectedAgent { agent_id: "test-agent" }`
- Convert to ErrorData
- Assert: message contains "test-agent"

### test_self_lockout_error_message_actionable
- Convert SelfLockout to ErrorData
- Assert: message contains "Admin" and "lockout"

## Risk Coverage

| Risk | Test |
|------|------|
| R-02 | test_protected_agent_error_code, test_protected_agent_error_message_contains_agent_id |
| R-03 | test_self_lockout_error_code, test_self_lockout_error_message_actionable |
