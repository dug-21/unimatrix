# Test Plan: outcome-index (store crate)

## Risk Coverage

| Risk | Scenario | Test Name |
|------|----------|-----------|
| R-07 | Fresh database opens with 13 tables | test_open_creates_all_13_tables |
| R-07 | All 13 tables accessible in read txn | test_outcome_index_accessible_after_open |
| AC-01 | OUTCOME_INDEX schema: insert and read (str, u64) pair | test_outcome_index_insert_and_read |

## Tests (in db.rs)

### test_open_creates_all_13_tables
- Open fresh Store
- Open each of the 13 tables in a read transaction (including OUTCOME_INDEX)
- Assert all succeed without error
- **Covers**: R-07, AC-02

### test_outcome_index_accessible_after_open
- Open Store, begin read txn
- Open OUTCOME_INDEX table
- Verify it's empty (iter count == 0)
- **Covers**: R-07, AC-02

### test_outcome_index_insert_and_read
- Open Store, begin write txn
- Open OUTCOME_INDEX, insert ("col-001", 42) -> ()
- Commit
- Begin read txn, open OUTCOME_INDEX
- Read ("col-001", 42), verify it exists
- Range scan ("col-001", 0)..=("col-001", u64::MAX), verify count == 1
- **Covers**: AC-01
