# Test Plan: store-observation

## Risk Coverage

| Risk | Scenarios |
|------|-----------|
| R-06 (OBSERVATION_METRICS table addition regression) | Table exists, CRUD, existing tests pass |

## Unit Tests: `crates/unimatrix-store/src/db.rs` and `crates/unimatrix-store/src/write.rs` / `read.rs`

### Table Creation (AC-28, AC-29)

1. **test_open_creates_all_tables** (extend existing) -- Verify 14 tables exist including OBSERVATION_METRICS
2. **test_observation_metrics_accessible_after_open** -- Open table in read txn, verify empty (AC-28)

### CRUD Operations (AC-30)

3. **test_store_metrics_and_get** -- Store bytes, get by key, verify match
4. **test_get_metrics_nonexistent** -- Key not found -> None
5. **test_store_metrics_overwrites** -- Store twice for same key, get returns second value
6. **test_list_all_metrics_empty** -- No entries -> empty vec
7. **test_list_all_metrics_multiple** -- Store 3 entries, list returns all 3
8. **test_list_all_metrics_returns_correct_data** -- Verify both key and value content

### Schema Compatibility (AC-31)

9. **test_schema_version_still_3** -- Open store, verify no migration triggered (schema version remains 3)

### Regression

10. **test_all_existing_table_tests_still_pass** -- Implicit (cargo test -p unimatrix-store should pass)
