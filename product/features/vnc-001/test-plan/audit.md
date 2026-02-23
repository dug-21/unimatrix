# Test Plan: audit.rs

## Risks Covered
- R-07: Audit log ID collision (Medium)
- R-16: Concurrent state corruption (High)

## Unit Tests

### log_event

```
test_first_event_id_is_1
  Arrange: fresh Store with AuditLog
  Act: log one event
  Assert: event_id == 1

test_monotonic_ids
  Arrange: fresh Store with AuditLog
  Act: log 10 events
  Assert: IDs are [1, 2, 3, ..., 10] strictly increasing

test_cross_session_continuity
  Arrange: open Store, log 5 events, drop Store
  Act: reopen Store, log 1 event
  Assert: event_id == 6

test_timestamp_set_by_log_event
  Arrange: create AuditEvent with timestamp=0
  Act: log_event
  Assert: stored event has timestamp > 0 (set by log_event, not caller)

test_event_id_set_by_log_event
  Arrange: create AuditEvent with event_id=999
  Act: log_event
  Assert: stored event has event_id assigned by counter (not 999)
```

### Serialization

```
test_audit_event_roundtrip
  Arrange: AuditEvent with all fields populated
  Act: serialize then deserialize
  Assert: equal to original

test_audit_event_empty_target_ids
  Arrange: AuditEvent with target_ids = vec![]
  Act: serialize then deserialize
  Assert: target_ids is empty vec

test_audit_event_multiple_target_ids
  Arrange: AuditEvent with target_ids = vec![1, 2, 3]
  Act: serialize then deserialize
  Assert: target_ids == [1, 2, 3]

test_all_outcome_variants_roundtrip
  For each Outcome variant (Success, Denied, Error, NotImplemented):
    Arrange: AuditEvent with that outcome
    Act: serialize then deserialize
    Assert: outcome preserved
```

### Concurrency (R-16)

```
test_rapid_events_no_id_collision
  Arrange: fresh Store
  Act: log 100 events in rapid succession (single-threaded but fast)
  Assert: all 100 have unique IDs, strictly increasing
```
