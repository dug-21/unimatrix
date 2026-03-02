# Test Plan: storage-layer

Component: Storage Layer (P0)
Covers: AC-01, AC-07, AC-14
Risks: R-01, R-02 (partial)

---

## Unit Tests

### sessions.rs serialization

```
test_session_record_roundtrip
  - Arrange: SessionRecord with all fields populated
  - Act: serialize_session → deserialize_session
  - Assert: field-by-field equality

test_session_lifecycle_status_roundtrip
  - Arrange: all 4 variants (Active, Completed, TimedOut, Abandoned)
  - Act: serialize → deserialize each
  - Assert: correct variant preserved

test_injection_log_record_roundtrip
  - Arrange: InjectionLogRecord with all fields
  - Act: serialize_injection_log → deserialize_injection_log
  - Assert: field-by-field equality; log_id preserved
```

### schema.rs constants

```
test_sessions_table_name
  - Assert: SESSIONS.name() == "sessions"

test_injection_log_table_name
  - Assert: INJECTION_LOG.name() == "injection_log"
```

---

## Integration Tests (tmpdir store)

### Migration (AC-01)

```
test_schema_v5_migration_from_v4
  - Arrange: open store (creates v4 schema with SIGNAL_QUEUE)
  - Close and reopen store (triggers migrate_v4_to_v5)
  - Assert: schema_version == 5
  - Assert: SESSIONS table exists (insert SessionRecord succeeds)
  - Assert: INJECTION_LOG table exists (insert InjectionLogRecord batch succeeds)
  - Assert: COUNTERS["next_log_id"] == 0
  - Assert: all prior ENTRIES records intact (count unchanged)
  - Assert: all prior SIGNAL_QUEUE records intact

test_schema_v5_migration_idempotency
  - Arrange: open a write transaction on a v4 store
  - Call migrate_v4_to_v5 twice on the same transaction
  - Assert: next_log_id is 0 (not reset by second call)
  - Assert: no error thrown

test_schema_v5_migration_skipped_when_current
  - Arrange: store already at v5
  - Assert: reopening does not run migration (no error, schema_version still 5)
```

### insert_session / get_session (AC-02 prerequisite)

```
test_insert_and_get_session_roundtrip
  - Arrange: SessionRecord { session_id: "test-1", status: Active, started_at: now, ... }
  - Act: insert_session → get_session("test-1")
  - Assert: returned record matches inserted record

test_get_session_returns_none_for_missing
  - Act: get_session("nonexistent")
  - Assert: Ok(None)

test_insert_session_overwrites_on_duplicate_key
  - Insert two SessionRecords with same session_id
  - Assert: get_session returns the second one (redb upsert semantics)
```

### update_session

```
test_update_session_changes_status
  - Insert Active session
  - update_session: set status=Completed, ended_at=Some(now)
  - Assert: get_session returns Completed with ended_at set

test_update_session_not_found_error
  - Act: update_session("nonexistent", |_| {})
  - Assert: Err(StoreError::NotFound)

test_update_session_preserves_unchanged_fields
  - Insert session with all fields
  - update_session: only change outcome
  - Assert: all other fields unchanged
```

### scan_sessions_by_feature

```
test_scan_sessions_by_feature_returns_matching
  - Insert 3 sessions: 2 for "fc-a", 1 for "fc-b"
  - scan_sessions_by_feature("fc-a") → returns exactly 2
  - scan_sessions_by_feature("fc-b") → returns exactly 1
  - scan_sessions_by_feature("fc-c") → returns 0

test_scan_sessions_empty_store
  - Act: scan_sessions_by_feature("anything")
  - Assert: Ok(vec![])
```

### scan_sessions_by_feature_with_status

```
test_scan_with_status_filter
  - Insert 3 sessions for "fc": 2 Completed, 1 Abandoned
  - scan_sessions_by_feature_with_status("fc", Some(Completed)) → 2
  - scan_sessions_by_feature_with_status("fc", Some(Abandoned)) → 1
  - scan_sessions_by_feature_with_status("fc", None) → 3
```

### insert_injection_log_batch / scan_injection_log_by_session (AC-07)

```
test_injection_log_batch_allocates_ids
  - insert_injection_log_batch([r1, r2, r3]) (all log_id=0 initially)
  - Assert: COUNTERS["next_log_id"] == 3
  - scan_injection_log_by_session(r1.session_id) → 3 records with log_ids 0, 1, 2

test_injection_log_sequential_batches_no_overlap
  - insert_injection_log_batch([r1, r2])  → IDs 0, 1
  - insert_injection_log_batch([r3, r4])  → IDs 2, 3
  - Assert: all 4 records exist with distinct IDs

test_injection_log_session_isolation (AC-07)
  - insert_injection_log_batch for session-A: 3 records
  - insert_injection_log_batch for session-B: 2 records
  - scan_injection_log_by_session("session-A") → exactly 3
  - scan_injection_log_by_session("session-B") → exactly 2
  - No cross-contamination

test_injection_log_empty_batch_is_noop
  - insert_injection_log_batch([])
  - Assert: COUNTERS["next_log_id"] unchanged (still 0 or whatever it was)
  - Assert: no error

test_injection_log_scan_empty_store
  - scan_injection_log_by_session("anything")
  - Assert: Ok(vec![])
```

### Server restart persistence (AC-14)

```
test_session_and_injection_survive_store_reopen
  - Insert SessionRecord and 2 InjectionLogRecords into tmpdir store
  - Drop store (close DB)
  - Reopen store from same tmpdir path
  - Assert: get_session returns original SessionRecord
  - Assert: scan_injection_log_by_session returns 2 records
  - Assert: schema_version == 5 (no re-migration)
```

---

## GcStats Struct

```
test_gc_stats_fields
  - Create GcStats { timed_out_count: 1, deleted_session_count: 2, deleted_injection_log_count: 5 }
  - Assert: fields readable
```

---

## Edge Cases

```
test_session_id_with_unicode_stored_and_retrieved
  - session_id = "session-\u{1234}"
  - Assert: insert succeeds; get returns same session_id

test_injection_log_confidence_f64_precision
  - Insert record with confidence=0.123456789012345
  - Scan and retrieve
  - Assert: confidence == 0.123456789012345 (f64 roundtrip)
```
