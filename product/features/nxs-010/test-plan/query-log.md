# Test Plan: query-log (C4)

## Component

`crates/unimatrix-store/src/query_log.rs` -- `QueryLogRecord` struct and Store API: `insert_query_log`, `scan_query_log_by_session`.

## Risks Covered

| Risk ID | Priority | Scenarios |
|---------|----------|-----------|
| R-03 | Med | AUTOINCREMENT ID allocation |
| R-06 | High | JSON serialization edge cases |
| R-12 | Med | scan ordering and session filtering |

## Unit Tests

### test_insert_query_log_autoincrement

**Arrange**: Open fresh `TestDb`. Build a `QueryLogRecord` with query_id=0.
**Act**: Call `store.insert_query_log(&record)`. Insert a second record.
**Assert**:
- Both inserts return `Ok(())`.
- `scan_query_log_by_session` returns 2 rows with query_id > 0 and monotonically increasing.

**AC**: AC-10

### test_insert_query_log_ignores_provided_query_id

**Arrange**: Build a `QueryLogRecord` with query_id=999.
**Act**: Insert it. Scan results.
**Assert**: The returned row has a query_id allocated by AUTOINCREMENT (not 999, unless AUTOINCREMENT happened to pick 999, which it would not on a fresh DB -- it starts at 1).

**AC**: AC-10

### test_scan_query_log_by_session_ordered_by_ts_asc

**Arrange**: Insert 3 `QueryLogRecord` rows for session "sess-1" with ts values 300, 100, 200.
**Act**: Call `store.scan_query_log_by_session("sess-1")`.
**Assert**: Returns 3 rows in order ts=100, 200, 300.

**AC**: AC-11

### test_scan_query_log_by_session_filters_correctly

**Arrange**: Insert 2 rows for "sess-a" and 3 rows for "sess-b".
**Act**: Call `scan_query_log_by_session("sess-a")`.
**Assert**: Returns exactly 2 rows, all with session_id="sess-a".

**AC**: AC-11 (R-12 -- correct WHERE clause)

### test_scan_query_log_by_session_empty

**Arrange**: Open fresh `TestDb`.
**Act**: Call `scan_query_log_by_session("nonexistent")`.
**Assert**: Returns `Ok(vec![])`.

**AC**: R-12

### test_query_log_json_round_trip_empty_results

**Arrange**: Build record with:
- result_entry_ids = `serde_json::to_string(&Vec::<u64>::new())`  (= `"[]"`)
- similarity_scores = `serde_json::to_string(&Vec::<f64>::new())`  (= `"[]"`)
- result_count = 0

**Act**: Insert and scan back.
**Assert**:
- `result_entry_ids` deserializes as `Vec::<u64>` with length 0.
- `similarity_scores` deserializes as `Vec::<f64>` with length 0.
- `result_count = 0`.

**AC**: AC-14, AC-15 (R-06 edge case)

### test_query_log_json_round_trip_multiple_results

**Arrange**: Build record with:
- entry_ids = `vec![1u64, 2, 3, 100]`
- scores = `vec![0.95, 0.87, 0.0, 1.0]`
- result_count = 4

**Act**: Insert and scan back.
**Assert**:
- `result_entry_ids` deserializes as `vec![1, 2, 3, 100]`.
- `similarity_scores` deserializes as `vec![0.95, 0.87, 0.0, 1.0]`.
- Lengths match. Boundary values (0.0, 1.0) preserved.

**AC**: AC-14, AC-15 (R-06)

### test_query_log_json_round_trip_single_result

**Arrange**: Build record with entry_ids = `vec![42u64]`, scores = `vec![0.5]`, result_count = 1.
**Act**: Insert and scan.
**Assert**: Both arrays have length 1 with correct values.

**AC**: AC-14, AC-15

### test_query_log_all_fields_round_trip

**Arrange**: Build a `QueryLogRecord` with all fields populated:
- session_id = "test-session"
- query_text = "how to handle errors in Rust"
- ts = 1700000000
- result_count = 2
- result_entry_ids = "[1,2]"
- similarity_scores = "[0.9,0.8]"
- retrieval_mode = "strict"
- source = "uds"

**Act**: Insert and scan.
**Assert**: All fields match on read-back.

### test_query_log_source_values

**Arrange**: Insert one row with source="uds", another with source="mcp".
**Act**: Scan both rows.
**Assert**: source field preserved correctly for both values.

**AC**: AC-16

### test_query_log_retrieval_mode_values

**Arrange**: Insert one row with retrieval_mode="strict", another with retrieval_mode="flexible".
**Act**: Scan both rows.
**Assert**: retrieval_mode field preserved correctly for both values.

**AC**: AC-17

## Edge Cases

| Edge Case | Test |
|-----------|------|
| Empty result arrays | test_query_log_json_round_trip_empty_results |
| Boundary similarity scores (0.0, 1.0) | test_query_log_json_round_trip_multiple_results |
| Single-element arrays | test_query_log_json_round_trip_single_result |
| No rows for session | test_scan_query_log_by_session_empty |
| Multiple sessions filtered correctly | test_scan_query_log_by_session_filters_correctly |
| AUTOINCREMENT ID allocation | test_insert_query_log_autoincrement |
