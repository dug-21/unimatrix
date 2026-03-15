# Component Test Plan: auto-quarantine-audit

**Source**: `crates/unimatrix-server/src/background.rs` (audit event emission from the quarantine path)
**Risk coverage**: R-08 (Medium — tick_skipped event), AC-13, FR-11, FR-13

---

## Unit Test Expectations

All tests in the audit section of `background.rs` test module. The audit infrastructure uses `AuditEvent` structs written through the `AuditLog` path. Tests verify the field values on constructed `AuditEvent` instances before they are written.

### AC-13 / FR-11 — auto_quarantine Audit Event: All 9 Fields Present

The specification (FR-11) requires 9 fields. The test must verify each field independently.

**Test**: `test_auto_quarantine_audit_event_operation_field`
- Construct an auto-quarantine audit event (using the same construction logic as the production code)
- Assert `event.operation == "auto_quarantine"`

**Test**: `test_auto_quarantine_audit_event_agent_id_is_system`
- Assert `event.agent_id == "system"`
- This confirms the identity is hardcoded, not user-controlled (security invariant)

**Test**: `test_auto_quarantine_audit_event_entry_id`
- Assert `event.target_ids` contains the quarantined entry's ID
- Or assert `event.entry_id == quarantined_entry_id` depending on the AuditEvent struct shape

**Test**: `test_auto_quarantine_audit_event_entry_title`
- Construct event for entry with title = "My Test Entry"
- Assert `event.detail` contains "My Test Entry" (title embedded in detail string per FR-11 schema)

**Test**: `test_auto_quarantine_audit_event_entry_category`
- Entry category = "convention"
- Assert `event.detail` contains "convention"

**Test**: `test_auto_quarantine_audit_event_classification`
- Entry was Ineffective
- Assert `event.detail` contains "Ineffective"

**Test**: `test_auto_quarantine_audit_event_consecutive_cycles`
- Entry had `consecutive_bad_cycles = 5` at time of quarantine
- Assert `event.detail` contains "5" or "consecutive_bad_cycles=5" or equivalent
- Assert the count matches the actual counter value (not a hardcoded value)

**Test**: `test_auto_quarantine_audit_event_threshold`
- `AUTO_QUARANTINE_CYCLES = 3`
- Assert `event.detail` contains "threshold=3" or "after 3" or equivalent
- The threshold value at trigger time must be captured, not interpolated later

**Test**: `test_auto_quarantine_audit_event_reason_string`
- Assert `event.detail` (or a dedicated `reason` field) contains a human-readable string
- Expected pattern: `"auto-quarantine: entry '{title}' (id={id}, category={category}, consecutive_bad_cycles={n}, topic={topic}) quarantined after {n} consecutive background maintenance ticks classified as {classification}"`
- Minimum assertion: `event.detail` is non-empty and contains both the entry ID and the classification

**Test**: `test_auto_quarantine_audit_event_outcome_success`
- Assert `event.outcome == Outcome::Success`

### FR-11 — Comprehensive 9-Field Verification

**Test**: `test_auto_quarantine_audit_event_all_nine_fields` (AC-13 primary assertion)
- This is the combined test verifying all 9 required fields in a single scenario
- Input: entry with id=42, title="Test Convention", category="convention", classification=Noisy, consecutive_cycles=4, threshold=3, topic="crt-018b"
- Construct the audit event as the production code would
- Assert all 9 fields:
  1. `operation == "auto_quarantine"`
  2. `agent_id == "system"`
  3. `target_ids` contains 42 (or entry_id == 42)
  4. detail contains "Test Convention"
  5. detail contains "convention"
  6. detail contains "Noisy"
  7. detail contains "4" (consecutive_cycles)
  8. detail contains "3" (threshold)
  9. detail is non-empty and human-readable; outcome == Success

### FR-13 / R-08 — tick_skipped Audit Event

**Test**: `test_tick_skipped_audit_event_operation_field`
- Simulate `compute_report()` error
- Assert emitted event has `operation == "tick_skipped"`

**Test**: `test_tick_skipped_audit_event_agent_id`
- Assert `event.agent_id == "system"` (same identity as auto_quarantine)

**Test**: `test_tick_skipped_audit_event_reason_contains_error`
- Simulate error with message "connection refused"
- Assert `event.detail` contains "connection refused"
- The error reason must flow through to the audit event

**Test**: `test_tick_skipped_audit_event_outcome_failure`
- Assert `event.outcome == Outcome::Failure`

**Test**: `test_tick_skipped_audit_event_comprehensive`
- Combined test: simulate error, capture emitted event, verify all tick_skipped fields:
  1. `operation == "tick_skipped"`
  2. `agent_id == "system"`
  3. `detail` contains the error string
  4. `outcome == Failure`

### Security Invariant — agent_id Not User-Controlled

**Test**: `test_auto_quarantine_audit_agent_id_is_constant`
- Assert that the `agent_id` in the auto_quarantine event is a compile-time constant ("system"), not derived from any request parameter or entry field
- This is a code-structure test: verify the string literal "system" is used directly, not a variable that could be influenced by external input

---

## Integration Test Expectations

The audit event integration test is tied to AC-17 item 3 (auto-quarantine fires after N ticks). If the background tick can be driven at test time:

```python
# test_auto_quarantine_audit_event_appears_in_status(server)
# After triggering auto-quarantine:
# 1. Parse context_status response for effectiveness section
# 2. Assert auto_quarantined_this_cycle contains the quarantined entry's ID
# 3. Note: reading the raw audit log through MCP is not currently a tool —
#    this tests FR-14 (visibility via status report), not direct audit log inspection
```

If the tick cannot be driven externally, this integration test is noted as a gap in the Stage 3c coverage report, with unit tests covering the audit event construction and the existing quarantine flow covering the store write.

---

## Edge Cases

| Scenario | Expected | Test Type |
|----------|----------|-----------|
| Multiple entries quarantined in same tick | One audit event per entry, all emitted | Unit (R-03 bulk test) |
| Entry title is empty string | detail still contains id and classification | Unit |
| Entry topic is None/absent | detail omits topic or uses empty string | Unit |
| tick_skipped emitted for each failed tick | Exactly one event per failed tick, not one per retry | Unit |
| auto_quarantine followed by tick_skipped in same maintenance cycle | Both events emitted independently | Unit (combined error path) |
