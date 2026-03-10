# Risk Coverage Report: nxs-010

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Migration v10->v11 partially applies, leaving DB inconsistent | test_migration_v10_to_v11_basic, test_migration_v10_to_v11_idempotent, test_migration_v10_to_v11_partial_rerun | PASS | Full |
| R-02 | Backfill SQL produces incorrect aggregates from edge-case sessions | test_migration_v10_to_v11_basic, test_migration_v10_to_v11_empty_sessions, test_migration_v10_to_v11_no_attributed_sessions, test_migration_backfill_null_ended_at_mixed, test_migration_backfill_all_null_ended_at | PASS | Full |
| R-03 | AUTOINCREMENT sqlite_sequence not created or corrupted | test_insert_query_log_autoincrement, test_insert_query_log_ignores_provided_query_id | PASS | Full |
| R-04 | Fire-and-forget query_log write panics in spawn_blocking | Code review: UDS uses spawn_blocking_fire_and_forget with Err-match (no unwrap); MCP uses spawn_blocking with Err-match. UDS guard skips if session_id is None/empty. Response returned before spawn. | PASS | Full (structural) |
| R-05 | UDS/MCP QueryLogRecord field divergence | test_query_log_new_constructor_field_parity, code review: both paths use QueryLogRecord::new() shared constructor | PASS | Full |
| R-06 | JSON serialization edge cases for result arrays | test_query_log_json_round_trip_empty_results, test_query_log_json_round_trip_multiple_results, test_query_log_json_round_trip_single_result | PASS | Full |
| R-07 | update_topic_delivery_counters on nonexistent topic silently succeeds | test_update_topic_delivery_counters_nonexistent_topic_returns_error | PASS | Full |
| R-08 | Migration runs on fresh database with no sessions table | test_migration_fresh_database_skips | PASS | Full |
| R-09 | Concurrent Store::open() races on migration | -- | N/A | Accepted risk (SQLite exclusive transaction provides serialization) |
| R-10 | INSERT OR REPLACE destroys concurrent counter updates | test_upsert_replace_overwrites_counters, test_upsert_topic_delivery_replace | PASS | Full |
| R-11 | query_log write holds SQLite write lock blocking concurrent ops | -- | N/A | Accepted risk (sequential UDS processing, no concurrent writes) |
| R-12 | scan_query_log_by_session wrong order or incorrect WHERE | test_scan_query_log_by_session_ordered_by_ts_asc, test_scan_query_log_by_session_filters_correctly, test_scan_query_log_by_session_empty | PASS | Full |
| R-13 | Backfill double-counts whitespace variants of topic names | -- | N/A | Accepted risk (low likelihood, low impact) |
| R-14 | total_duration_secs overflow for sessions with NULL ended_at | test_migration_backfill_null_ended_at_mixed, test_migration_backfill_all_null_ended_at | PASS | Full |

## Test Results

### Unit Tests (cargo test --workspace)

- Total: 1862
- Passed: 1861
- Failed: 1 (pre-existing, not nxs-010)
- Ignored: 18

**Pre-existing failure**: `unimatrix-vector::index::tests::test_compact_search_consistency` -- search results differ after HNSW compaction. Filed as GH#188. Not caused by nxs-010 (no unimatrix-vector code modified).

#### nxs-010 Specific Unit Tests (all PASS)

**query_log (12 tests)**:
- test_insert_query_log_autoincrement
- test_insert_query_log_ignores_provided_query_id
- test_scan_query_log_by_session_ordered_by_ts_asc
- test_scan_query_log_by_session_filters_correctly
- test_scan_query_log_by_session_empty
- test_query_log_json_round_trip_empty_results
- test_query_log_json_round_trip_multiple_results
- test_query_log_json_round_trip_single_result
- test_query_log_all_fields_round_trip
- test_query_log_source_values
- test_query_log_retrieval_mode_values
- test_query_log_new_constructor_field_parity

**topic_deliveries (11 tests)**:
- test_upsert_topic_delivery_insert
- test_upsert_topic_delivery_replace
- test_upsert_replace_overwrites_counters
- test_get_topic_delivery_not_found
- test_get_topic_delivery_all_fields
- test_update_topic_delivery_counters_increment
- test_update_topic_delivery_counters_decrement
- test_update_topic_delivery_counters_nonexistent_topic_returns_error
- test_list_topic_deliveries_empty
- test_list_topic_deliveries_ordered_by_created_at_desc
- test_upsert_topic_delivery_nullable_fields

**migration v10->v11 (8 integration tests)**:
- test_migration_v10_to_v11_basic
- test_migration_v10_to_v11_idempotent
- test_migration_v10_to_v11_empty_sessions
- test_migration_v10_to_v11_no_attributed_sessions
- test_migration_backfill_null_ended_at_mixed
- test_migration_backfill_all_null_ended_at
- test_migration_fresh_database_skips
- test_migration_v10_to_v11_partial_rerun

**nxs-010 total: 31 tests, all PASS**

### Integration Tests (infra-001)

#### Smoke Suite (mandatory gate)
- Total: 19
- Passed: 18
- XFail: 1 (GH#111 -- rate limit blocks volume test, pre-existing)

#### Tools Suite
- Total: 68
- Passed: 67
- XFail: 1 (GH#187 -- file_count field missing from observation section, pre-existing)

#### Lifecycle Suite
- Total: 16
- Passed: 16
- Failed: 0

#### Edge Cases Suite
- Total: 24
- Passed: 23
- XFail: 1 (GH#111 -- rate limit blocks rapid sequential stores, pre-existing)

**Integration totals: 127 tests executed, 124 passed, 3 xfail (all pre-existing)**

### Pre-existing Failures (xfail with GH Issues)

| Test | GH Issue | Root Cause |
|------|----------|------------|
| test_volume.py::TestVolume1K::test_store_1000_entries | GH#111 | Rate limit (60/3600s) blocks volume test storing 200 entries |
| test_edge_cases.py::test_100_rapid_sequential_stores | GH#111 | Same rate limit issue for 100 sequential stores |
| test_tools.py::test_status_includes_observation_fields | GH#187 | Test expects `file_count` field but server returns `record_count` |
| (unit) index::tests::test_compact_search_consistency | GH#188 | HNSW compaction alters nearest-neighbor boundary results |

## Gaps

None. All 14 risks from RISK-TEST-STRATEGY.md are covered:
- 10 risks have explicit test coverage (PASS)
- 3 risks are documented as accepted (R-09, R-11, R-13) with architectural justification
- 1 low-priority risk (R-13) accepted with no test required

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | topic_deliveries DDL verified by test_migration_v10_to_v11_basic (9 columns, correct types) |
| AC-02 | PASS | query_log DDL verified by test_insert_query_log_autoincrement, test_query_log_all_fields_round_trip (9 columns) |
| AC-03 | PASS | query_log indexes verified by migration tests (idx_query_log_session, idx_query_log_ts created in DDL) |
| AC-04 | PASS | test_migration_v10_to_v11_basic: 3 sessions across 2 topics, correct aggregates verified |
| AC-05 | PASS | test_migration_v10_to_v11_idempotent: re-open produces no duplicates, no errors |
| AC-06 | PASS | test_migration_v10_to_v11_empty_sessions, test_migration_v10_to_v11_no_attributed_sessions |
| AC-07 | PASS | test_upsert_topic_delivery_insert, test_upsert_topic_delivery_replace |
| AC-08 | PASS | test_get_topic_delivery_not_found: returns Ok(None) |
| AC-09 | PASS | test_update_topic_delivery_counters_increment, test_update_topic_delivery_counters_nonexistent_topic_returns_error |
| AC-10 | PASS | test_insert_query_log_autoincrement: query_id > 0, monotonically increasing |
| AC-11 | PASS | test_scan_query_log_by_session_ordered_by_ts_asc: ts=300,100,200 returned as 100,200,300 |
| AC-12 | PASS | Code review: UDS listener.rs line 909-937 writes query_log with source="uds", retrieval_mode="strict" after search |
| AC-13 | PASS | Code review: MCP tools.rs line 331-357 writes query_log with source="mcp", retrieval_mode="flexible" after search |
| AC-14 | PASS | test_query_log_json_round_trip_multiple_results: vec![1,2,3,100] round-trips as Vec<u64> |
| AC-15 | PASS | test_query_log_json_round_trip_multiple_results: vec![0.95,0.87,0.0,1.0] round-trips as Vec<f64> |
| AC-16 | PASS | test_query_log_source_values: "uds" and "mcp" preserved |
| AC-17 | PASS | test_query_log_retrieval_mode_values: "strict" and "flexible" preserved |
| AC-18 | PASS | test_migration_v10_to_v11_basic: total_sessions and total_duration_secs match expected aggregates |
| AC-19 | PASS | test_migration_v10_to_v11_basic: all backfilled rows have status='completed' |
| AC-20 | PASS | cargo test --workspace: 1861/1862 pass, 1 pre-existing failure (GH#188, unimatrix-vector, not nxs-010) |

## Constraint Verification

| Constraint | Status | Evidence |
|-----------|--------|----------|
| C-01 | PASS | migration.rs bumps to v11; guard is `current_version < 11` |
| C-02 | PASS | Store::open() calls migrate_if_needed() before create_tables() (verified by test_migration_fresh_database_skips) |
| C-03 | PASS | Both UDS and MCP paths: warn log on error, no retry, no error propagation. Response returned before spawn. |
| C-04 | PASS | query_log uses AUTOINCREMENT; test_insert_query_log_autoincrement confirms |
| C-06 | PASS | Backfill runs inside main migration transaction (single tx in migration block) |
| C-08 | PASS | serde_json::to_string used in QueryLogRecord::new constructor; JSON round-trip tests pass |
| C-09 | PASS | UDS path: `if let Some(ref sid) = session_id { if !sid.is_empty()` guard (listener.rs line 910-911) |
| C-10 | PASS | MCP path: `ctx.audit_ctx.session_id.clone().unwrap_or_default()` (tools.rs line 336) |
