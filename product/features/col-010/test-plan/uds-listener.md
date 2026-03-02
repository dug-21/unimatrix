# Test Plan: uds-listener

Component: UDS Listener Integration (P0)
Covers: AC-02, AC-03, AC-04, AC-05, AC-06
Risks: R-03, R-05, R-10

---

## Unit Tests

### session_id sanitization

```
test_sanitize_session_id_valid
  - Inputs: "abc-123", "session_A", "abc123ABC_-", "a" (single char)
  - Assert: all return Ok(())

test_sanitize_session_id_invalid_char
  - Inputs: "session!1", "sess ion", "sess.id", "sess@id"
  - Assert: all return Err(HookError::InvalidSessionId)

test_sanitize_session_id_too_long
  - Input: "a".repeat(129)
  - Assert: Err(HookError::InvalidSessionId)

test_sanitize_session_id_exactly_128
  - Input: "a".repeat(128)
  - Assert: Ok(())

test_sanitize_session_id_empty
  - Input: ""
  - Assert: Err(HookError::InvalidSessionId) OR allow (clarify in implementation)
```

### agent_role / feature_cycle sanitization (SR-SEC-02)

```
test_sanitize_metadata_field_strips_control_chars
  - Input: "session\x00name\x01"
  - Assert: output == "sessionname" (control chars stripped)

test_sanitize_metadata_field_truncates_at_128
  - Input: "a".repeat(200)
  - Assert: output.len() == 128

test_sanitize_metadata_field_preserves_printable_ascii
  - Input: "feature-col-010 (with spaces and dashes)"
  - Assert: output == input
```

---

## Integration Tests (tmpdir store + UDS handler)

Note: These tests use the existing test infrastructure for UDS listener. Check `crates/unimatrix-server/src/` for test helpers. Use `spawn_blocking` await patterns where needed.

### SessionRegister persistence (AC-02)

```
test_session_register_persists_active_record
  - Arrange: tmpdir store, UDS handler
  - Act: dispatch SessionRegister { session_id: "test-reg-1", feature_cycle: Some("col-010"), agent_role: Some("dev") }
  - Await spawn_blocking completion (give it 100ms)
  - Assert: store.get_session("test-reg-1") == Some(SessionRecord { status: Active, started_at: ~now, ended_at: None, total_injections: 0 })

test_session_register_invalid_session_id_returns_error
  - Arrange: dispatch SessionRegister { session_id: "session!invalid" }
  - Assert: HookResponse.error is set; store.get_session("session!invalid") == None

test_session_register_started_at_within_bounds
  - Register session; retrieve; assert started_at within 2 seconds of test start time
```

### SessionClose persistence (AC-03, AC-04, AC-05)

```
test_session_close_success_updates_record  (AC-03)
  - Register session (3 ContextSearch injections)
  - Close with Success
  - Await spawn_blocking completion
  - Assert: get_session → status=Completed, outcome="success", ended_at set, total_injections=3

test_session_close_rework_outcome  (AC-04)
  - Register session, inject 2
  - Close with Rework
  - Assert: status=Completed, outcome="rework"

test_session_close_abandoned_status  (AC-05)
  - Register session, inject 2
  - Close with Abandoned
  - Assert: status=Abandoned, outcome="abandoned"
  - Assert: context_lookup(category:"outcome", tags:["type:session"]) returns 0 entries for this session

test_session_close_sets_ended_at
  - Register, close with Success
  - Assert: ended_at is Some and >= started_at
```

### ContextSearch INJECTION_LOG write (AC-06)

```
test_context_search_writes_injection_log
  - Register session "search-sess-1"
  - Simulate ContextSearch returning 3 entries for this session
  - Await spawn_blocking
  - Assert: scan_injection_log_by_session("search-sess-1") returns exactly 3 records
  - Assert: each record has correct session_id, entry_id, confidence set
  - Assert: timestamps are within 2 seconds of now

test_context_search_one_transaction_per_response  (R-05)
  - Arrange: observe COUNTERS["next_log_id"] before search
  - Simulate ContextSearch with 3 entries
  - Await; observe next_log_id after
  - Assert: next_log_id incremented by exactly 3 (single batch: one transaction)

test_context_search_no_injection_without_session_id
  - Simulate ContextSearch with no active session_id
  - Assert: scan_injection_log_by_session returns empty (no spurious writes)

test_context_search_no_injection_for_empty_results
  - Register session; simulate ContextSearch returning 0 results
  - Assert: INJECTION_LOG empty for this session
```

### total_injections accuracy (R-03 — documented limitation)

```
test_total_injections_count_matches_injection_log
  - Register session; inject 3 ContextSearch calls (1, 2, 3 results each = 6 total)
  - Close with Success; await all spawn_blocking tasks
  - Assert: SessionRecord.total_injections == 6 (in-memory count)
  - Note: comment that in-flight INJECTION_LOG batch writes may not yet be committed
    at SessionClose time; total_injections is from in-memory count (OQ-01)

test_total_injections_documented_discrepancy
  - Comment in test: fire-and-forget INJECTION_LOG writes may still be in-flight
    when SessionClose reads in-memory count. This is an accepted discrepancy (OQ-01).
    In practice the count will converge once spawned tasks complete.
```

---

## Edge Cases

```
test_session_register_feature_cycle_none
  - Register with feature_cycle = None
  - Assert: SessionRecord.feature_cycle == None (stored as None)

test_session_register_sanitizes_feature_cycle
  - Register with feature_cycle = Some("feat\x00ure")
  - Assert: stored feature_cycle has control chars stripped

test_context_search_session_id_not_sanitized_at_search
  - (If session_id is pre-validated at register time, no re-validation needed at search)
  - Verify this assumption and document
```
