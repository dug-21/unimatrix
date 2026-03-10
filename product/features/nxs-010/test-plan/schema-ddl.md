# Test Plan: schema-ddl (C1)

## Component

`crates/unimatrix-store/src/db.rs` -- `create_tables()` function. Adds `CREATE TABLE IF NOT EXISTS` for `topic_deliveries` and `query_log` plus two indexes on `query_log`.

## Risks Covered

| Risk ID | Relevance |
|---------|-----------|
| R-01 | DDL idempotency -- IF NOT EXISTS prevents duplicate table errors |
| R-03 | AUTOINCREMENT on query_log creates sqlite_sequence table |

## Unit Tests

### test_create_tables_topic_deliveries_schema

**Arrange**: Open a fresh database via `TestDb::new()`.
**Act**: Query `pragma_table_info('topic_deliveries')`.
**Assert**:
- Returns 9 columns.
- Column names: `topic`, `created_at`, `completed_at`, `status`, `github_issue`, `total_sessions`, `total_tool_calls`, `total_duration_secs`, `phases_completed`.
- `topic` column has `pk = 1` (primary key).
- `created_at` is `INTEGER` with `notnull = 1`.
- `status` has default value `'active'`.
- `completed_at`, `github_issue`, `phases_completed` are nullable (`notnull = 0`).
- `total_sessions`, `total_tool_calls`, `total_duration_secs` default to `0`.

**AC**: AC-01

### test_create_tables_query_log_schema

**Arrange**: Open a fresh database via `TestDb::new()`.
**Act**: Query `pragma_table_info('query_log')`.
**Assert**:
- Returns 9 columns.
- Column names: `query_id`, `session_id`, `query_text`, `ts`, `result_count`, `result_entry_ids`, `similarity_scores`, `retrieval_mode`, `source`.
- `query_id` is `INTEGER` with `pk = 1`.
- `session_id` and `query_text` are `TEXT` with `notnull = 1`.
- `source` is `TEXT` with `notnull = 1`.
- `result_entry_ids`, `similarity_scores`, `retrieval_mode` are nullable.

**AC**: AC-02

### test_create_tables_query_log_indexes

**Arrange**: Open a fresh database via `TestDb::new()`.
**Act**: Query `pragma_index_list('query_log')`.
**Assert**:
- At least 2 indexes exist (excluding autoindex).
- Index names include `idx_query_log_session` and `idx_query_log_ts`.

**AC**: AC-03

### test_create_tables_query_log_autoincrement

**Arrange**: Open a fresh database via `TestDb::new()`.
**Act**: Query `SELECT name FROM sqlite_master WHERE type='table' AND name='sqlite_sequence'`.
**Assert**: `sqlite_sequence` table exists (AUTOINCREMENT creates it).

**AC**: AC-02 (supports R-03)

### test_create_tables_idempotent

**Arrange**: Open a fresh database via `TestDb::new()`.
**Act**: Call `Store::open()` a second time on the same path.
**Assert**: No error. Tables unchanged. Row counts unchanged.

**AC**: AC-05 (DDL portion)

## Edge Cases

- Fresh database with no prior data: all CREATE TABLE IF NOT EXISTS succeed on first open.
- Re-open after v11 migration: `create_tables()` is a no-op for new tables (tested in migration.md).
