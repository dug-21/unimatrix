# C6: Write Test Plan

## R1/AC-04: Atomic Multi-Table Insert

### test_insert_populates_all_indexes
- Insert one entry with topic, category, 2 tags, Active status
- Verify ENTRIES contains entry
- Verify TOPIC_INDEX contains (topic, id)
- Verify CATEGORY_INDEX contains (category, id)
- Verify TAG_INDEX contains entry under each tag
- Verify TIME_INDEX contains (created_at, id)
- Verify STATUS_INDEX contains (Active, id)
- Use assert_index_consistent helper

### test_insert_50_entries_all_indexed
- Insert 50 entries with varied topics/categories/tags/statuses
- For each entry, call assert_index_consistent
- Verify every entry reachable via every applicable index

## R2/AC-18: Update Path Stale Index Orphaning

### test_update_topic_migrates_index
- Insert entry with topic="auth"
- Update to topic="security"
- Assert: query_by_topic("auth") does NOT contain entry
- Assert: query_by_topic("security") DOES contain entry

### test_update_category_migrates_index
- Insert with category="convention"
- Update to category="decision"
- Assert old absent, new present

### test_update_tags_add_remove
- Insert with tags=["rust", "error"]
- Update to tags=["rust", "async"]
- Assert: "error" tag no longer has entry, "async" tag has entry, "rust" still has entry

### test_update_multiple_fields_simultaneously
- Insert with topic="auth", category="convention", tags=["rust"]
- Update all: topic="security", category="decision", tags=["go"]
- Assert all old index entries removed, all new index entries present

### test_update_no_change_indexes_unchanged
- Insert entry
- Update with identical field values
- Assert all indexes still correct (no duplicates, no removals)

### test_update_query_old_topic_returns_empty
- Insert with topic="auth", update to topic="security"
- query_by_topic("auth") returns empty or doesn't include this entry

## R8/AC-12: Status Transition Atomicity

### test_status_active_to_deprecated
- Insert Active entry
- update_status(id, Deprecated)
- Assert: STATUS_INDEX no longer has (Active, id)
- Assert: STATUS_INDEX has (Deprecated, id)
- Assert: ENTRIES record shows Deprecated
- Assert: total_active decremented, total_deprecated incremented

### test_status_proposed_to_active
- Insert Proposed entry
- update_status(id, Active)
- Verify counters adjusted

### test_status_deprecated_to_active
- Reactivation test
- Verify STATUS_INDEX migrated and counters adjusted

### test_status_same_noop
- Insert Active, update_status(id, Active)
- No error, no index change

### test_counter_consistency_after_transitions
- Insert 3 Active, 2 Deprecated, 1 Proposed
- Change one Active to Deprecated
- Read all counters, verify total_active=2, total_deprecated=3, total_proposed=1

## R6: Transaction Atomicity

### test_drop_transaction_no_changes
- (This is tested implicitly: if insert fails mid-way and returns Err, no data should persist)
- Verify that after a failed operation, database state is unchanged

## R11/AC-13: VECTOR_MAP

### test_put_vector_mapping_and_read
- put_vector_mapping(42, 7)
- get_vector_mapping(42) -> Some(7)

### test_vector_mapping_overwrite
- put_vector_mapping(42, 7)
- put_vector_mapping(42, 99)
- get_vector_mapping(42) -> Some(99)

### test_vector_mapping_nonexistent
- get_vector_mapping(999) -> None

### test_vector_mapping_u64_max
- put_vector_mapping(1, u64::MAX)
- get_vector_mapping(1) -> Some(u64::MAX)

## Delete

### test_delete_removes_all_indexes
- Insert entry, delete it
- Verify ENTRIES, all indexes, VECTOR_MAP (if present) cleared
- Verify counter decremented

### test_delete_nonexistent_returns_error
- delete(999) -> EntryNotFound
