# Test Plan: session-aware-mcp

## Risk Coverage

| Risk | Scenarios | Priority |
|------|----------|----------|
| R-04 | Session ID prefix stripping | High |
| R-05 | Backward compatibility | Low |

## Unit Tests

### R-05: Backward Compatibility (Deserialization)

1. **test_search_params_without_session_id**
   - Deserialize SearchParams JSON without session_id field
   - Verify session_id == None
   - Covers: R-05 scenario 1

2. **test_search_params_with_session_id_null**
   - Deserialize SearchParams JSON with session_id: null
   - Verify session_id == None
   - Covers: R-05 scenario 2

3. **test_search_params_with_session_id_value**
   - Deserialize SearchParams JSON with session_id: "abc"
   - Verify session_id == Some("abc")
   - Covers: R-05 scenario 3

4. **test_lookup_params_session_id_default**
   - Same test for LookupParams without session_id
   - Verify session_id == None

5. **test_get_params_session_id_default**
   - Same test for GetParams without session_id
   - Verify session_id == None

6. **test_briefing_params_session_id_default**
   - Same test for BriefingParams without session_id
   - Verify session_id == None

### R-04: Session ID Prefix/Strip in Context

7. **test_build_context_with_session_id_prefixes_mcp**
   - Call build_context with session_id: Some("abc")
   - Verify ctx.audit_ctx.session_id == Some("mcp::abc")
   - Covers: AC-16

8. **test_build_context_without_session_id_stays_none**
   - Call build_context with session_id: None
   - Verify ctx.audit_ctx.session_id == None
   - Covers: AC-40 (backward compat)

9. **test_build_context_sets_caller_id_agent**
   - Call build_context
   - Verify ctx.caller_id == CallerId::Agent(agent_id)
   - Covers: AC-29

### Session ID Validation (S3)

10. **test_session_id_too_long_rejected**
    - Call build_context with session_id of 257 chars
    - Verify error returned

11. **test_session_id_control_chars_rejected**
    - Call build_context with session_id containing \x01
    - Verify error returned

12. **test_session_id_256_chars_accepted**
    - Call build_context with session_id of exactly 256 chars
    - Verify Ok

## Test Setup Pattern

Deserialization tests use `serde_json::from_str::<SearchParams>(json)`.
build_context tests require a UnimatrixServer instance with registry.
Can use existing `make_server()` pattern from server.rs tests.
