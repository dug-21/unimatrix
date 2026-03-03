# Test Plan: Store::insert_in_txn

## Unit Tests

### TS-16: insert_in_txn writes all indexes (R-08)
- Setup: Create tempdir Store
- Action: Begin write txn, call `insert_in_txn(txn, entry, embedding_dim, data_id, now)`, commit
- Verify all index tables populated:
  - ENTRIES: entry exists with correct fields
  - TOPIC_INDEX: (topic, id) key exists
  - CATEGORY_INDEX: (category, id) key exists
  - TAG_INDEX: all tags indexed
  - TIME_INDEX: (created_at, id) key exists
  - STATUS_INDEX: (status, id) key exists
  - VECTOR_MAP: id -> data_id mapping exists
- Verify COUNTERS incremented for status

### TS-17: insert_in_txn matches insert behavior (R-08)
- Setup: Two tempdir Stores
- Action: Insert same entry via `store.insert()` in store A, and `insert_in_txn` in store B
- Verify: EntryRecord fields match between both (except id which depends on counter state)
- Verify: Same indexes populated in both stores

### TS-16b: insert_in_txn with outcome entry
- Setup: Create entry with category="outcome" and non-empty feature_cycle
- Action: `insert_in_txn(txn, outcome_entry, ...)`
- Verify: OUTCOME_INDEX populated with (feature_cycle, id) key

## Integration Tests

### TS-16c: insert_in_txn does not commit
- Setup: Begin write txn, call `insert_in_txn`, do NOT commit, drop txn
- Verify: Entry does NOT exist in store (transaction rolled back)

### TS-16d: insert_in_txn with no tags
- Setup: Entry with empty tags vec
- Action: `insert_in_txn(txn, entry, ...)`, commit
- Verify: TAG_INDEX has no entries for this id
- Verify: All other indexes populated correctly
