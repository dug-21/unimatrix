# Test Plan: audit-optimization (C6)

## Unit Tests (database required)

### write_in_txn (R-14)

1. `test_write_in_txn_visible_after_commit` -- open txn, write_in_txn, commit -> audit event visible
2. `test_write_in_txn_invisible_without_commit` -- open txn, write_in_txn, drop txn -> audit event NOT visible
3. `test_write_in_txn_increments_counter` -- after write_in_txn + commit, COUNTERS["next_audit_id"] incremented

### Monotonic IDs Across Paths (R-12)

4. `test_combined_then_standalone_sequential` -- write_in_txn (commit) then log_event -> IDs sequential (1, 2)
5. `test_standalone_then_combined_sequential` -- log_event then write_in_txn (commit) -> IDs sequential (1, 2)
6. `test_interleaved_paths_no_gaps` -- mixed 10 operations -> all IDs sequential 1..=10

### insert_with_audit Integration (R-03)

7. `test_insert_with_audit_entry_exists` -- after call, entry exists in store
8. `test_insert_with_audit_audit_exists` -- after call, audit event exists with correct target_ids
9. `test_insert_with_audit_vector_mapping` -- after call, vector index contains the entry
10. `test_insert_with_audit_atomicity` -- entry and audit in same transaction (both visible after commit)
11. `test_insert_with_audit_then_standalone_sequential` -- insert_with_audit then log_event -> sequential audit IDs
