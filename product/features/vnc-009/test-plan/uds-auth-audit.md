# Test Plan: uds-auth-audit

## Risk Coverage

| Risk | Scenarios | Priority |
|------|----------|----------|
| R-08 | Auth audit write blocking | Low |

## Unit Tests (uds/listener.rs)

### Auth Failure Audit

1. **test_auth_failure_writes_audit_event**
   - Set up an AuditLog backed by tempdir store
   - Simulate auth failure path: call the audit write code directly
   - Verify AUDIT_LOG contains entry with operation="uds_auth_failure"
   - Verify outcome=Failure
   - Verify agent_id="unknown"
   - Covers: AC-36, AC-37, AC-39

2. **test_auth_failure_audit_includes_error_detail**
   - Write audit event with specific error message
   - Read back from AUDIT_LOG
   - Verify detail contains the error message
   - Covers: AC-37

3. **test_auth_failure_preserves_tracing_warn**
   - This is a code inspection check (the tracing::warn! line is preserved)
   - Verify at code review that existing warn! is not removed
   - Covers: FR-05.5

## Integration Notes

Full integration testing of auth failure requires a UDS connection with invalid
credentials, which is complex to set up in unit tests. The test plan focuses on
verifying the audit write mechanics. The actual authentication path is tested by
existing UDS tests.

## Test Setup Pattern

```
fn make_audit_log() -> (Arc<AuditLog>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path().join("test.redb")).unwrap());
    let audit = Arc::new(AuditLog::new(store));
    (audit, dir)
}
```
