# Test Plan: C2 Write Operations

## Existing Tests (run unchanged on both backends)

From write.rs:
- test_insert_returns_sequential_ids
- test_insert_populates_all_index_tables
- test_insert_sets_timestamps
- test_insert_computes_content_hash
- test_update_changes_content
- test_update_changes_topic_updates_index
- test_update_changes_tags_updates_index
- test_update_preserves_created_at
- test_update_nonexistent_returns_error
- test_update_status_changes_status
- test_update_status_updates_index
- test_delete_removes_entry_and_indexes
- test_delete_nonexistent_returns_error
- test_record_usage_increments_counts
- test_record_usage_with_confidence
- test_update_confidence
- test_put_vector_mapping_insert_and_overwrite
- test_rewrite_vector_map
- test_record_feature_entries
- test_record_co_access_pairs
- test_cleanup_stale_co_access
- test_store_metrics

## New Risk-Specific Tests

### R-01: Boundary Values
```
test_insert_entry_with_u64_max_fields:
  insert entry, then manually check:
    read_counter("next_entry_id") returns expected value
  -- Note: cannot insert at u64::MAX id directly since next_entry_id is auto-incremented
  -- Test the counter behavior at high values

test_insert_entry_with_empty_strings:
  insert NewEntry with empty topic, category, title, content
  get -> verify all fields preserved as empty (not NULL)
  query_by_topic("") -> returns the entry
  query_by_category("") -> returns the entry

test_insert_entry_with_unicode:
  insert entry with Japanese topic, emoji content
  get -> verify exact roundtrip
  query_by_topic with same Japanese string -> returns entry
```

### R-05: CO_ACCESS CHECK Constraint
```
test_co_access_check_constraint_rejects_wrong_order:
  lock conn directly (test helper)
  INSERT INTO co_access (entry_id_a, entry_id_b, data) VALUES (10, 5, blob)
  -> should fail with constraint violation (CHECK entry_id_a < entry_id_b)

test_co_access_equal_ids_allowed_or_rejected:
  INSERT INTO co_access (entry_id_a, entry_id_b, ...) VALUES (5, 5, ...)
  -> CHECK (entry_id_a < entry_id_b) means equal is REJECTED
  -- Verify: co_access_key(5, 5) returns (5, 5) which violates CHECK
  -- This matches application-level behavior since co_access_key uses <=
```

### R-10: Counter Atomicity
```
test_concurrent_inserts_unique_ids:
  open Store (Arc)
  spawn 10 threads
  each thread inserts 10 entries
  join all -> collect all returned IDs
  verify: 100 unique IDs, no duplicates, no gaps

test_counter_within_transaction:
  insert entry -> id=1
  read_counter("next_entry_id") -> 2
  insert entry -> id=2
  read_counter("next_entry_id") -> 3
  -- Counter is consistent after each transaction
```

### R-02: Mutex Stress
```
test_concurrent_insert_and_query:
  open Store (Arc)
  spawn 5 writer threads: insert 20 entries each
  spawn 5 reader threads: query_by_status(Active) 20 times each
  join all -> no panics, no deadlocks
  verify: 100 total entries inserted
```

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-01 | Boundary values (u64, empty strings, unicode) |
| R-02 | Concurrent insert+query stress test |
| R-05 | CHECK constraint violation test |
| R-10 | Concurrent counter uniqueness, sequential consistency |
