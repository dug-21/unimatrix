# C1: Schema DDL (db.rs)

## Purpose

Add `CREATE TABLE IF NOT EXISTS` statements for `topic_deliveries` and `query_log` to the existing `create_tables()` function in `db.rs`. These ensure fresh databases get both tables. On migrated databases (v10->v11), these are no-ops.

## Modified Function: create_tables()

**File**: `crates/unimatrix-store/src/db.rs`
**Location**: Append to the existing `execute_batch` string in `create_tables()`, before the closing `";` of the batch.

### DDL to append

```sql
CREATE TABLE IF NOT EXISTS topic_deliveries (
    topic TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    completed_at INTEGER,
    status TEXT NOT NULL DEFAULT 'active',
    github_issue INTEGER,
    total_sessions INTEGER NOT NULL DEFAULT 0,
    total_tool_calls INTEGER NOT NULL DEFAULT 0,
    total_duration_secs INTEGER NOT NULL DEFAULT 0,
    phases_completed TEXT
);
CREATE TABLE IF NOT EXISTS query_log (
    query_id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    query_text TEXT NOT NULL,
    ts INTEGER NOT NULL,
    result_count INTEGER NOT NULL,
    result_entry_ids TEXT,
    similarity_scores TEXT,
    retrieval_mode TEXT,
    source TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_query_log_session ON query_log(session_id);
CREATE INDEX IF NOT EXISTS idx_query_log_ts ON query_log(ts);
```

### Placement

Insert after the `shadow_evaluations` table and its index (the last DDL block currently in `create_tables()`), still within the same `execute_batch` call. No new `execute_batch` call needed.

### No counter initialization needed

`query_log` uses AUTOINCREMENT (ADR-001), not a named counter. `topic_deliveries` uses a natural TEXT primary key. Neither requires a counter row in the `counters` table.

## Error Handling

Same as existing: `execute_batch(...).map_err(StoreError::Sqlite)?`. The entire `create_tables()` is already within a single error propagation path.

## Key Test Scenarios

1. **Fresh database**: Open a new Store. Verify `pragma_table_info('topic_deliveries')` returns 9 columns with correct names and types. Verify `pragma_table_info('query_log')` returns 9 columns. Verify `pragma_index_list('query_log')` returns 2 indexes (idx_query_log_session, idx_query_log_ts). (AC-01, AC-02, AC-03)

2. **Idempotent re-run**: Call `create_tables()` twice on the same connection. No errors. (NFR-04)

3. **Post-migration**: Open a v10 database (migration creates the tables). Verify `create_tables()` is a no-op for these tables -- no errors, no schema changes. (C-02)
