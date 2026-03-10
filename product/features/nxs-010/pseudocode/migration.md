# C2: Migration v10->v11 (migration.rs)

## Purpose

Upgrade existing databases from schema v10 to v11 by creating both new tables and backfilling `topic_deliveries` from attributed session data. Runs within the existing main migration transaction.

## Modified Constant

```
CURRENT_SCHEMA_VERSION: u64 = 11   // was 10
```

## Modified Function: migrate_if_needed()

**File**: `crates/unimatrix-store/src/migration.rs`

### New migration block

Insert a `current_version < 11` block after the existing `current_version < 10` block, before the `schema_version` UPDATE statement. This follows the established pattern.

```
if current_version < 11 {
    // Step 1: Create topic_deliveries table (idempotent)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS topic_deliveries (
            topic TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL,
            completed_at INTEGER,
            status TEXT NOT NULL DEFAULT 'active',
            github_issue INTEGER,
            total_sessions INTEGER NOT NULL DEFAULT 0,
            total_tool_calls INTEGER NOT NULL DEFAULT 0,
            total_duration_secs INTEGER NOT NULL DEFAULT 0,
            phases_completed TEXT
        );"
    )?;

    // Step 2: Create query_log table with AUTOINCREMENT (ADR-001)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS query_log (
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
        CREATE INDEX IF NOT EXISTS idx_query_log_ts ON query_log(ts);"
    )?;

    // Step 3: Backfill topic_deliveries from attributed sessions (ADR-003)
    // INSERT OR IGNORE ensures idempotency on re-run.
    // Groups sessions by feature_cycle, computes:
    //   - created_at = MIN(started_at)
    //   - status = 'completed' (conservative default, AC-19)
    //   - total_sessions = COUNT(*)
    //   - total_tool_calls = 0 (not backfilled, FR-03.7)
    //   - total_duration_secs = COALESCE(SUM(ended_at - started_at), 0)
    //     NULL ended_at rows contribute NULL to (ended_at - started_at),
    //     which SUM excludes. If ALL are NULL, COALESCE returns 0. (R-14)
    conn.execute_batch(
        "INSERT OR IGNORE INTO topic_deliveries
            (topic, created_at, status, total_sessions, total_tool_calls, total_duration_secs)
        SELECT feature_cycle, MIN(started_at), 'completed', COUNT(*), 0,
               COALESCE(SUM(ended_at - started_at), 0)
        FROM sessions
        WHERE feature_cycle IS NOT NULL AND feature_cycle != ''
        GROUP BY feature_cycle;"
    )?;
}
```

### Transaction scope

This block runs inside the existing `BEGIN IMMEDIATE` transaction in `migrate_if_needed`. The schema_version update to 11 happens at the existing `INSERT OR REPLACE INTO counters` statement at the end (which already uses `CURRENT_SCHEMA_VERSION`). Since we changed the constant to 11, that statement writes 11.

No separate transaction. No backup file. (ADR-003)

### Fresh database handling

The existing guard at the top of `migrate_if_needed` checks for the `entries` table. Fresh databases have no `entries` table, so migration is skipped entirely. `create_tables()` handles fresh DB initialization. (R-08 mitigation)

## Error Handling

All SQL calls use `.map_err(StoreError::Sqlite)?` matching the existing pattern. On failure, the outer `match result` block executes `ROLLBACK`, leaving the database at v10. Next `Store::open()` retries.

## Key Test Scenarios

1. **v10 database with attributed sessions**: Seed v10 DB with 3 topics (topic-A: 3 sessions with known durations; topic-B: 1 session; topic-C: sessions with NULL ended_at). Run migration. Verify topic_deliveries has correct rows with expected aggregates. Verify schema_version = 11. (AC-04, R-02)

2. **v10 database with no attributed sessions**: Seed v10 DB with sessions where feature_cycle IS NULL or empty. Run migration. Verify 0 topic_deliveries rows, schema_version = 11. (AC-06, R-02)

3. **Idempotent re-run on v11**: Open a v11 database. Verify migration is a no-op (version guard). No duplicate rows. (AC-05, R-01)

4. **Mixed NULL/non-NULL ended_at**: Seed 3 sessions for same topic: 2 with durations 100s and 200s, 1 with NULL ended_at. Verify total_duration_secs = 300. (R-14)

5. **All NULL ended_at**: Seed all sessions with NULL ended_at. Verify total_duration_secs = 0. (R-14)

6. **Empty feature_cycle excluded**: Seed sessions with feature_cycle = "". Verify excluded from backfill. (R-02)

7. **Fresh database (no entries table)**: Open a brand new database. Verify migration is skipped, create_tables creates the new tables. (R-08)
