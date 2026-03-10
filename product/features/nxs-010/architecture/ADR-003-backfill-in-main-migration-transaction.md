## ADR-003: Backfill topic_deliveries in Main Migration Transaction

### Context

The v10->v11 migration creates two new tables and backfills `topic_deliveries` from existing attributed sessions. The backfill query aggregates `sessions WHERE feature_cycle IS NOT NULL` into topic-level rows.

Heavy migrations (v5->v6, v8->v9) that drop/recreate tables run in separate transactions after the main migration commits. This is necessary when the migration involves destructive DDL that cannot safely share a transaction boundary with the version update.

The nxs-010 migration is purely additive: CREATE TABLE (idempotent) + INSERT (backfill) + UPDATE schema_version. No tables are dropped or recreated.

The backfill query touches the sessions table. Based on ASS-018 data projections (~500 sessions per 60 days, ~50-100 with feature_cycle attribution), the GROUP BY aggregation is fast (<10ms).

### Decision

Run the entire v10->v11 migration -- table creation, backfill, and version update -- within the existing main transaction in `migrate_if_needed`.

The migration block structure:

```
if current_version < 11 {
    // 1. CREATE TABLE IF NOT EXISTS topic_deliveries (...)
    // 2. CREATE TABLE IF NOT EXISTS query_log (...)
    // 3. CREATE INDEX IF NOT EXISTS idx_query_log_session (...)
    // 4. CREATE INDEX IF NOT EXISTS idx_query_log_ts (...)
    // 5. INSERT OR IGNORE INTO topic_deliveries SELECT ... FROM sessions
    // 6. (schema_version update happens at end of migrate_if_needed)
}
```

No separate transaction. No backup file (no destructive DDL).

The `INSERT OR IGNORE` ensures idempotency: if migration is interrupted after table creation but before the version update, re-running inserts the same rows without error (topic is the PRIMARY KEY).

### Consequences

- **Atomic migration**: Either all three steps complete (tables + backfill + version bump) or none do. No partial state.
- **Brief write lock**: The sessions table is read-locked during the GROUP BY. At ~500 rows, this is sub-millisecond. If future databases grow to 10K+ sessions, this remains acceptable (<50ms).
- **Idempotent**: `CREATE TABLE IF NOT EXISTS` + `INSERT OR IGNORE` + version guard means the migration is safe to re-run.
- **No backup overhead**: Unlike v5->v6 or v8->v9, no file copy is needed before the migration.
- **Backfill sets status='completed'** for all historically attributed topics. This is a conservative default (AC-19). col-020 can update status to 'active' for topics with ongoing sessions.
