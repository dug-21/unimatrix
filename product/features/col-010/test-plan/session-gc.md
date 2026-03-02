# Test Plan: session-gc

Component: Session GC with INJECTION_LOG Cascade (P0)
Covers: AC-08, AC-09
Risks: R-02

---

## Unit Tests

### GC threshold constants

```
test_gc_threshold_constants
  - Assert: TIMED_OUT_THRESHOLD_SECS == 24 * 3600  (86400)
  - Assert: DELETE_THRESHOLD_SECS == 30 * 24 * 3600  (2592000)
```

---

## Integration Tests (tmpdir store)

### TimedOut marking (AC-08)

```
test_gc_marks_old_active_as_timed_out
  - Insert Active session with started_at = now - 25 * 3600  (25 hours ago)
  - gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS)
  - Assert: GcStats.timed_out_count == 1
  - Assert: get_session returns Some with status=TimedOut
  - Assert: session NOT deleted (still present)
  - Assert: INJECTION_LOG records for this session intact

test_gc_does_not_time_out_recent_session
  - Insert Active session with started_at = now - 23 * 3600  (23 hours ago)
  - gc_sessions(...)
  - Assert: GcStats.timed_out_count == 0
  - Assert: status still Active

test_gc_does_not_time_out_completed_session
  - Insert Completed session with started_at = now - 25 * 3600
  - gc_sessions(...)
  - Assert: status still Completed (only Active sessions are marked TimedOut)

test_gc_does_not_time_out_abandoned_session
  - Insert Abandoned session with started_at = now - 25 * 3600
  - gc_sessions(...)
  - Assert: status still Abandoned
```

### Deletion (AC-09)

```
test_gc_deletes_old_session_any_status
  - Insert session (any status) with started_at = now - 31 * 24 * 3600  (31 days ago)
  - Insert 3 InjectionLogRecords for this session
  - gc_sessions(...)
  - Assert: GcStats.deleted_session_count == 1
  - Assert: GcStats.deleted_injection_log_count == 3
  - Assert: get_session returns None
  - Assert: scan_injection_log_by_session returns empty

test_gc_does_not_delete_29_day_session
  - Insert session with started_at = now - 29 * 24 * 3600  (29 days ago)
  - gc_sessions(...)
  - Assert: session still present
  - Assert: GcStats.deleted_session_count == 0

test_gc_cascade_deletes_all_injection_records
  - Insert session with 5 InjectionLogRecords
  - Set started_at = now - 31 days
  - gc_sessions(...)
  - Assert: INJECTION_LOG empty for this session_id
  - Assert: GcStats.deleted_injection_log_count == 5

test_gc_no_sessions_returns_empty_stats
  - gc_sessions on empty store
  - Assert: GcStats { timed_out_count: 0, deleted_session_count: 0, deleted_injection_log_count: 0 }
```

### Atomicity (R-02)

```
test_gc_atomicity_no_orphan_injection_records
  - Insert session (31 days old) with 3 injection records
  - gc_sessions succeeds
  - Assert: SESSIONS entry gone AND all 3 INJECTION_LOG entries gone
  - (Atomicity guaranteed by single write transaction; both must be gone or neither)

test_gc_cascade_only_deletes_matching_session_logs
  - Insert session-A (31 days old) with 2 injection records
  - Insert session-B (5 days old) with 3 injection records
  - gc_sessions(...)
  - Assert: session-A deleted; session-A injection records deleted
  - Assert: session-B intact; session-B injection records intact
  - Assert: GcStats.deleted_injection_log_count == 2 (not 5)
```

### Mixed scenarios

```
test_gc_mixed_time_out_and_delete
  - Insert session-A (25 hours old, Active) → should be timed out
  - Insert session-B (31 days old) → should be deleted
  - gc_sessions(...)
  - Assert: GcStats.timed_out_count == 1, deleted_session_count == 1
  - Assert: session-A status == TimedOut (present)
  - Assert: session-B → None

test_gc_from_tools_maintain_path
  - Call context_status with maintain=true via server handler
  - Assert: GC is triggered (inspect logs or GcStats returned)
  - Assert: context_status response succeeds even if GC has nothing to do
```
