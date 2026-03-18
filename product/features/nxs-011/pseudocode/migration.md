# Component: migration.rs Adaptation
## File: `crates/unimatrix-store/src/migration.rs` (rewrite)

---

## Purpose

Adapts `migrate_if_needed()` from a synchronous rusqlite-based function to an async function
accepting a dedicated non-pooled `&mut sqlx::SqliteConnection` (ADR-003). The SQL logic for
all 12 schema version transitions is preserved verbatim — only the connection API and
execution mechanism change. `txn.rs` is deleted in this wave.

The migration connection is opened by `SqlxStore::open()` before pool construction, ensuring
no pool connection ever observes a pre-migration schema (FR-08, C-03).

---

## Key API Change (ADR-003)

```rust
// Before (rusqlite, called with Store wrapper):
pub(crate) fn migrate_if_needed(store: &Store, db_path: &Path) -> Result<()>

// After (sqlx, non-pooled connection passed directly):
pub(crate) async fn migrate_if_needed(
    conn: &mut sqlx::SqliteConnection,
    db_path: &Path,
) -> Result<()>
```

`conn` has already had all 6 PRAGMAs applied via `apply_pragmas_to_connection()` before
this function is called (see pool-config.md).

---

## Function: `migrate_if_needed`

```rust
pub(crate) async fn migrate_if_needed(
    conn: &mut sqlx::SqliteConnection,
    db_path: &Path,
) -> Result<()> {
    // Step 1: Check if this is a fresh database (no entries table).
    // Fresh databases are initialized by create_tables(), not migration.
    let has_entries_table: bool = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='entries'"
    )
    .fetch_one(&mut *conn)
    .await
    .map(|count| count > 0)
    .unwrap_or(false);

    if !has_entries_table {
        return Ok(());
    }

    // Step 2: Read current schema version.
    let current_version: u64 = sqlx::query_scalar!(
        "SELECT value FROM counters WHERE name = 'schema_version'"
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?
    .map(|v: i64| v as u64)
    .unwrap_or(0);

    if current_version >= CURRENT_SCHEMA_VERSION {
        return Ok(()); // Already up to date; idempotent.
    }

    // Step 3: Entry-rewriting migrations for very old schemas (v0, v1, v2).
    // These must run before table-creation migrations.
    if current_version <= 2 {
        migrate_entries_to_current_schema(conn).await?;
    }

    // Step 4: Main migration transaction (all transitions except v8→v9 and v5→v6).
    // BEGIN IMMEDIATE to serialize against concurrent writers (none expected, but safe).
    let mut txn = conn.begin().await.map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    let main_result = run_main_migrations(&mut txn, current_version, db_path).await;

    match main_result {
        Ok(()) => {
            txn.commit().await.map_err(|e| StoreError::Migration { source: Box::new(e) })?;
        }
        Err(e) => {
            let _ = txn.rollback().await;
            return Err(e);
        }
    }

    // Step 5: Out-of-transaction migrations (v5→v6 table rename, v8→v9 metrics normalization).
    // These must run outside the main transaction (they DROP/RENAME tables).
    if (6..9).contains(&current_version) {
        let needs_v8_v9 = check_old_observation_metrics_table(conn).await;
        if needs_v8_v9 {
            migrate_v8_to_v9(conn, db_path).await?;
        }
    }

    if current_version > 0 && current_version <= 5 {
        let has_blob_entries = check_old_blob_entries_table(conn).await;
        if has_blob_entries {
            migrate_v5_to_v6(conn, db_path).await?;
        }
    }

    Ok(())
}
```

### `run_main_migrations` (called within the open transaction)

```rust
async fn run_main_migrations(
    txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    current_version: u64,
    db_path: &Path,
) -> Result<()> {
    // v3 → v4: next_signal_id counter
    if current_version < 4 {
        sqlx::query!(
            "INSERT OR IGNORE INTO counters (name, value) VALUES ('next_signal_id', 0)"
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
    }

    // v4 → v5: next_log_id counter
    if current_version < 5 {
        sqlx::query!(
            "INSERT OR IGNORE INTO counters (name, value) VALUES ('next_log_id', 0)"
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
    }

    // v6 → v7: observations table (col-012)
    if current_version < 7 {
        sqlx::query!(
            "CREATE TABLE IF NOT EXISTS observations (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id      TEXT    NOT NULL,
                ts_millis       INTEGER NOT NULL,
                hook            TEXT    NOT NULL,
                tool            TEXT,
                input           TEXT,
                response_size   INTEGER,
                response_snippet TEXT
            )"
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

        sqlx::query!(
            "CREATE INDEX IF NOT EXISTS idx_observations_session ON observations(session_id)"
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

        sqlx::query!(
            "CREATE INDEX IF NOT EXISTS idx_observations_ts ON observations(ts_millis)"
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
    }

    // v7 → v8: pre_quarantine_status column (vnc-010)
    if current_version < 8 {
        let has_column: bool = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM pragma_table_info('entries') WHERE name = 'pre_quarantine_status'"
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count: i64| count > 0)
        .unwrap_or(false);

        if !has_column {
            sqlx::query!("ALTER TABLE entries ADD COLUMN pre_quarantine_status INTEGER")
                .execute(&mut **txn)
                .await
                .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
        }

        // Backfill: quarantined entries were quarantined from Active (status=0)
        sqlx::query!(
            "UPDATE entries SET pre_quarantine_status = 0
             WHERE status = 3 AND pre_quarantine_status IS NULL"
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
    }

    // v9 → v10: topic_signal column on observations (col-017)
    if current_version < 10 {
        let has_topic_signal: bool = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'topic_signal'"
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count: i64| count > 0)
        .unwrap_or(false);

        if !has_topic_signal {
            sqlx::query!("ALTER TABLE observations ADD COLUMN topic_signal TEXT")
                .execute(&mut **txn)
                .await
                .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
        }
    }

    // v10 → v11: topic_deliveries + query_log tables (nxs-010)
    if current_version < 11 {
        sqlx::query!(
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
            )"
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

        sqlx::query!(
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
            )"
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

        sqlx::query!(
            "CREATE INDEX IF NOT EXISTS idx_query_log_session ON query_log(session_id)"
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

        sqlx::query!(
            "CREATE INDEX IF NOT EXISTS idx_query_log_ts ON query_log(ts)"
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

        // Backfill topic_deliveries from attributed sessions (idempotent via INSERT OR IGNORE)
        sqlx::query!(
            "INSERT OR IGNORE INTO topic_deliveries
                (topic, created_at, status, total_sessions, total_tool_calls, total_duration_secs)
             SELECT feature_cycle, MIN(started_at), 'completed', COUNT(*), 0,
                    COALESCE(SUM(ended_at - started_at), 0)
             FROM sessions
             WHERE feature_cycle IS NOT NULL AND feature_cycle != ''
             GROUP BY feature_cycle"
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
    }

    // v11 → v12: keywords column on sessions (col-022)
    if current_version < 12 {
        let has_sessions_table: bool = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sessions'"
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count: i64| count > 0)
        .unwrap_or(false);

        if has_sessions_table {
            let has_keywords: bool = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'keywords'"
            )
            .fetch_one(&mut **txn)
            .await
            .map(|count: i64| count > 0)
            .unwrap_or(false);

            if !has_keywords {
                sqlx::query!("ALTER TABLE sessions ADD COLUMN keywords TEXT")
                    .execute(&mut **txn)
                    .await
                    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
            }
        }
        // Fresh DBs: create_tables() creates sessions with keywords already; no-op here.
    }

    // Update schema_version counter to CURRENT_SCHEMA_VERSION (12)
    sqlx::query!(
        "INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', ?1)",
        CURRENT_SCHEMA_VERSION as i64
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    Ok(())
}
```

---

## Functions: `migrate_v5_to_v6` and `migrate_v8_to_v9`

These perform DROP/RENAME/CREATE operations and cannot run inside the main transaction.
They open their own transactions on the provided `&mut SqliteConnection`.

The existing logic in these functions is preserved verbatim — only the connection API
changes from rusqlite (`conn.execute_batch(...)`) to sqlx (`sqlx::query!(...).execute(&mut *conn)`).

The file backup step in `migrate_v5_to_v6` (copying the database file before destructive
migration) remains unchanged — it uses `std::fs::copy`, which is synchronous and acceptable
in this context (called only during startup, not on the hot path). If the file copy blocks
the tokio runtime during startup for large databases, this is a known acceptable tradeoff
for migration safety.

```rust
async fn migrate_v5_to_v6(conn: &mut sqlx::SqliteConnection, db_path: &Path) -> Result<()> {
    // Step 1: File backup (synchronous, outside any transaction)
    create_backup_file(db_path)?; // existing logic, returns StoreError::Deserialization on failure

    // Step 2: BEGIN IMMEDIATE transaction
    let mut txn = conn.begin().await.map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    // Steps 3-11: All CREATE, INSERT, DROP, RENAME operations (preserved verbatim as SQL)
    // Each conn.execute_batch("...") call becomes one or more sqlx::query!("...").execute(&mut **txn)
    // Each rusqlite helper call (migrate_entries_v5_to_v6, etc.) becomes an async fn
    // accepting &mut sqlx::Transaction<'_, sqlx::Sqlite>

    txn.commit().await.map_err(|e| StoreError::Migration { source: Box::new(e) })
}

async fn migrate_v8_to_v9(conn: &mut sqlx::SqliteConnection, db_path: &Path) -> Result<()> {
    // Similar pattern: backup, own transaction, DROP/CREATE observation_metrics table
    // Preserved verbatim as SQL; only connection API changes
    todo!("implement by adapting existing migrate_v8_to_v9 logic")
}
```

The `todo!` above is a delivery-level implementation note, not a pseudocode placeholder —
the implementation agent must port the existing function bodies from rusqlite to sqlx.
The SQL within both functions is already correct and only the API calls change.

---

## `migrate_entries_to_current_schema`

Called for v0/v1/v2 databases before the main transaction. It iterates all entries
and re-serializes them. In the rusqlite version it uses `conn.prepare(...)` and
`rows.map(...)`. In the sqlx version:

```rust
async fn migrate_entries_to_current_schema(conn: &mut sqlx::SqliteConnection) -> Result<()> {
    // Read all entries using sqlx::query_as or manual fetch_all
    // Re-serialize each entry using the existing schema::serialize_entry / deserialize_entry logic
    // Write back using sqlx::query!("UPDATE entries SET ... WHERE id = ?", ...)
    // Error mapping: all sqlx::Error → StoreError::Migration { source: Box::new(e) }
    todo!("port from existing rusqlite logic; SQL preserved, only connection API changes")
}
```

---

## Error Handling

All errors in `migrate_if_needed` and its sub-functions are mapped to
`StoreError::Migration { source: Box::new(e) }`. This allows the caller in
`SqlxStore::open()` to distinguish migration failures from pool construction failures.

On any error in the main transaction: `txn.rollback()` is called, returning `Err`. The
migration connection is then dropped by `SqlxStore::open()`, which returns the error to
the server startup caller. Pool construction does not proceed.

---

## Key Test Scenarios

1. **`test_migrate_fresh_database`** (AC-17): Open empty database; call `migrate_if_needed`;
   assert `schema_version == 12` and all expected tables exist.

2. **`test_migrate_v0_to_v12`** (AC-17): Construct a v0-schema database (entries table
   with blob data format); run `migrate_if_needed`; assert final schema version is 12 and
   entries are readable.

3. **Per-version transition tests** (AC-17): For each of v0→v1, v1→v2, ..., v11→v12:
   create a database at the starting version; run migration; assert final schema version
   is CURRENT_SCHEMA_VERSION and the expected table/column changes are present.

4. **`test_migrate_idempotent`** (R-03): Run `migrate_if_needed` twice on a v12 database;
   assert no error and version remains 12.

5. **`test_migrate_failure_blocks_pool_construction`** (R-03): Inject a sqlx error at a
   specific migration step (e.g., simulate a constraint violation during v11→v12); assert
   `StoreError::Migration` is returned and no pool connections are opened.

6. **`test_migrate_partial_v7_column_exists_idempotent`** (R-03): Database that was
   partially migrated (v7 column exists but schema_version is still 7); assert migration
   completes without error (guard checks prevent duplicate ALTER TABLE).

---

## OQ-DURING Items Affecting This Component

None. ADR-003 fully resolves the migration connection sequencing question.
