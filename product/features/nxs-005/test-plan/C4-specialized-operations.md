# Test Plan: C4 Specialized Operations

## Signal Queue Tests (from db.rs)

All existing signal tests run unchanged:
- test_insert_signal_returns_monotonic_ids
- test_insert_signal_data_persists
- test_signal_queue_len_counts_all_types
- test_drain_signals_idempotent_on_empty
- test_drain_signals_returns_matching_type
- test_drain_signals_deletes_drained_records
- test_drain_signals_leaves_other_type
- test_signal_queue_cap_at_10001_drops_oldest (R-06 coverage)
- test_insert_drain_full_roundtrip

## Session Tests (from sessions.rs)

All existing session tests run unchanged:
- test_session_record_roundtrip
- test_session_lifecycle_status_roundtrip
- test_insert_and_get_session_roundtrip
- test_get_session_returns_none_for_missing
- test_update_session_changes_status
- test_update_session_not_found_returns_error
- test_scan_sessions_by_feature_returns_matching
- test_scan_sessions_empty_store
- test_scan_sessions_by_feature_with_status_filter
- test_gc_marks_old_active_as_timed_out
- test_gc_does_not_time_out_recent_session
- test_gc_does_not_time_out_completed_session
- test_gc_deletes_old_session_and_cascades_injection_log
- test_gc_does_not_delete_29_day_session
- test_gc_cascade_only_deletes_matching_session_logs
- test_gc_no_sessions_returns_empty_stats
- test_gc_mixed_time_out_and_delete
- test_gc_constants
- test_session_and_injections_survive_store_reopen

## Injection Log Tests (from injection_log.rs)

All existing injection log tests run unchanged:
- test_injection_log_record_roundtrip
- test_injection_log_batch_allocates_ids
- test_injection_log_sequential_batches_no_overlap
- test_injection_log_session_isolation
- test_injection_log_empty_batch_is_noop
- test_injection_log_scan_empty_store
- test_injection_log_confidence_f64_precision
- test_injection_log_one_transaction_per_batch

## Test Adjustments Required

Several tests directly access `store.db`:
- `sessions.rs::open_store()` creates store with `.join("test.redb")` -> needs cfg for `.join("test.db")`
- `injection_log.rs::open_store()` same
- `injection_log.rs::test_injection_log_empty_batch_is_noop` reads COUNTERS via `store.db.begin_read()` -> use `store.read_counter()` under SQLite
- `injection_log.rs::test_injection_log_one_transaction_per_batch` same

Strategy: Use `#[cfg]` in test helper functions to select appropriate file extension, and use Store public API instead of direct db access for SQLite tests.

## New SQLite-Specific Tests

### R-06: Signal Eviction Order
```
test_signal_eviction_preserves_order:
  insert 10001 signals
  verify queue len = 10000
  drain all -> verify signal_id=0 missing, signal_id=10000 present
  -- This is the existing test, confirming SQLite's MIN(signal_id) matches redb's sorted iteration
```

### Session Reopen Persistence
```
test_session_survives_sqlite_reopen:
  open store1 at path
  insert session
  drop store1
  open store2 at same path
  get_session -> still present
  -- Verifies SQLite WAL mode properly persists data across close/reopen
```

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-01 | All parity tests (serialization identical) |
| R-06 | Signal queue cap eviction order |
| R-02 | Session GC atomicity (5-phase transaction) |
