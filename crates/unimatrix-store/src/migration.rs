//! Schema migration for the SQLite backend (async sqlx, ADR-003).
//!
//! Called from `SqlxStore::open()` on a dedicated non-pooled `SqliteConnection`
//! that is opened and dropped before pool construction (ADR-003).
//!
//! Fresh SQLite databases (no `entries` table) are skipped here and initialized
//! by `create_tables_if_needed()` in `db.rs`. Migration is needed only when
//! opening an existing database created at an older schema version.

use std::path::Path;

use sqlx::Connection;

use crate::error::{Result, StoreError};
use crate::migration_compat;
use crate::schema::{deserialize_entry, serialize_entry};

/// Current schema version. Incremented from 17 to 18 by crt-033 (CYCLE_REVIEW_INDEX).
pub const CURRENT_SCHEMA_VERSION: u64 = 18;

/// Minimum co-access count to bootstrap a CoAccess edge into graph_edges.
/// Pairs below this threshold are too infrequent to represent meaningful relationships.
/// i64 to match sqlx binding conventions for SQLite integer parameters (FR-09, ARCHITECTURE §2b).
const CO_ACCESS_BOOTSTRAP_MIN_COUNT: i64 = 3;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run migration if schema_version is behind CURRENT_SCHEMA_VERSION.
///
/// Called from `SqlxStore::open()` with a dedicated non-pooled connection
/// that has already had all 6 PRAGMAs applied. The connection is dropped
/// by the caller immediately after this function returns.
pub(crate) async fn migrate_if_needed(
    conn: &mut sqlx::SqliteConnection,
    db_path: &Path,
) -> Result<()> {
    // Step 1: Check if this is a fresh database (no entries table).
    // Fresh databases are initialized by create_tables_if_needed(), not migration.
    let has_entries_table: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='entries'",
    )
    .fetch_one(&mut *conn)
    .await
    .map(|count| count > 0)
    .unwrap_or(false);

    if !has_entries_table {
        return Ok(());
    }

    // Step 2: Read current schema version.
    let current_version: u64 =
        sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'schema_version'")
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?
            .map(|v| v as u64)
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
    let mut txn = conn.begin().await.map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    let main_result = run_main_migrations(&mut txn, current_version, db_path).await;

    match main_result {
        Ok(()) => {
            txn.commit().await.map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;
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

// ---------------------------------------------------------------------------
// Main migration transaction
// ---------------------------------------------------------------------------

async fn run_main_migrations(
    txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    current_version: u64,
    _db_path: &Path,
) -> Result<()> {
    // v3 → v4: next_signal_id counter
    if current_version < 4 {
        sqlx::query("INSERT OR IGNORE INTO counters (name, value) VALUES ('next_signal_id', 0)")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;
    }

    // v4 → v5: next_log_id counter
    if current_version < 5 {
        sqlx::query("INSERT OR IGNORE INTO counters (name, value) VALUES ('next_log_id', 0)")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;
    }

    // v6 → v7: observations table (col-012)
    if current_version < 7 {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS observations (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id      TEXT    NOT NULL,
                ts_millis       INTEGER NOT NULL,
                hook            TEXT    NOT NULL,
                tool            TEXT,
                input           TEXT,
                response_size   INTEGER,
                response_snippet TEXT
            )",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_observations_session ON observations(session_id)",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_observations_ts ON observations(ts_millis)")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;
    }

    // v7 → v8: pre_quarantine_status column (vnc-010)
    if current_version < 8 {
        let has_column: bool = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('entries') WHERE name = 'pre_quarantine_status'",
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count| count > 0)
        .unwrap_or(false);

        if !has_column {
            sqlx::query("ALTER TABLE entries ADD COLUMN pre_quarantine_status INTEGER")
                .execute(&mut **txn)
                .await
                .map_err(|e| StoreError::Migration {
                    source: Box::new(e),
                })?;
        }

        // Backfill: quarantined entries were quarantined from Active (status=0)
        sqlx::query(
            "UPDATE entries SET pre_quarantine_status = 0
             WHERE status = 3 AND pre_quarantine_status IS NULL",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;
    }

    // v9 → v10: topic_signal column on observations (col-017)
    if current_version < 10 {
        let has_topic_signal: bool = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'topic_signal'",
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count| count > 0)
        .unwrap_or(false);

        if !has_topic_signal {
            sqlx::query("ALTER TABLE observations ADD COLUMN topic_signal TEXT")
                .execute(&mut **txn)
                .await
                .map_err(|e| StoreError::Migration {
                    source: Box::new(e),
                })?;
        }
    }

    // v10 → v11: topic_deliveries + query_log tables (nxs-010)
    if current_version < 11 {
        sqlx::query(
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
            )",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        sqlx::query(
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
            )",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_query_log_session ON query_log(session_id)")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_query_log_ts ON query_log(ts)")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;

        // Backfill topic_deliveries from attributed sessions (idempotent via INSERT OR IGNORE)
        sqlx::query(
            "INSERT OR IGNORE INTO topic_deliveries
                (topic, created_at, status, total_sessions, total_tool_calls, total_duration_secs)
             SELECT feature_cycle, MIN(started_at), 'completed', COUNT(*), 0,
                    COALESCE(SUM(ended_at - started_at), 0)
             FROM sessions
             WHERE feature_cycle IS NOT NULL AND feature_cycle != ''
             GROUP BY feature_cycle",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;
    }

    // v11 → v12: keywords column on sessions (col-022)
    if current_version < 12 {
        let has_sessions_table: bool = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sessions'",
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count| count > 0)
        .unwrap_or(false);

        if has_sessions_table {
            let has_keywords: bool = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'keywords'",
            )
            .fetch_one(&mut **txn)
            .await
            .map(|count| count > 0)
            .unwrap_or(false);

            if !has_keywords {
                sqlx::query("ALTER TABLE sessions ADD COLUMN keywords TEXT")
                    .execute(&mut **txn)
                    .await
                    .map_err(|e| StoreError::Migration {
                        source: Box::new(e),
                    })?;
            }
        }
        // Fresh DBs: create_tables_if_needed() creates sessions with keywords already; no-op here.
    }

    // v12 → v13: GRAPH_EDGES table + bootstrap inserts (crt-021)
    if current_version < 13 {
        // Step 1: Create graph_edges table (idempotent — CREATE TABLE IF NOT EXISTS).
        // DDL mirrors create_tables_if_needed in db.rs; both must be kept in sync.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS graph_edges (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id      INTEGER NOT NULL,
                target_id      INTEGER NOT NULL,
                relation_type  TEXT    NOT NULL,
                weight         REAL    NOT NULL DEFAULT 1.0,
                created_at     INTEGER NOT NULL,
                created_by     TEXT    NOT NULL DEFAULT '',
                source         TEXT    NOT NULL DEFAULT '',
                bootstrap_only INTEGER NOT NULL DEFAULT 0,
                metadata       TEXT    DEFAULT NULL,
                UNIQUE(source_id, target_id, relation_type)
            )",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_graph_edges_source_id ON graph_edges(source_id)",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_graph_edges_target_id ON graph_edges(target_id)",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_graph_edges_relation_type ON graph_edges(relation_type)",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        // Step 2: Bootstrap Supersedes edges from entries.supersedes.
        //
        // Edge direction: source_id = entry.supersedes (old/replaced),
        //                 target_id = entry.id (new/correcting).
        // This matches ARCHITECTURE §1 and ALIGNMENT-REPORT VARIANCE 1.
        // bootstrap_only = 0: entries.supersedes is authoritative, not heuristic.
        // INSERT OR IGNORE: idempotent via UNIQUE(source_id, target_id, relation_type).
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges
                (source_id, target_id, relation_type, weight, created_at,
                 created_by, source, bootstrap_only)
             SELECT
                 supersedes          AS source_id,
                 id                  AS target_id,
                 'Supersedes'        AS relation_type,
                 1.0                 AS weight,
                 strftime('%s','now') AS created_at,
                 'bootstrap'         AS created_by,
                 'entries.supersedes' AS source,
                 0                   AS bootstrap_only
             FROM entries
             WHERE supersedes IS NOT NULL",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        // Step 3: Bootstrap CoAccess edges from co_access (count >= CO_ACCESS_BOOTSTRAP_MIN_COUNT).
        //
        // Weight formula: COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)
        //   - MAX(count) OVER () computes max over the filtered rows (WHERE count >= 3).
        //   - NULLIF(..., 0) guards against theoretical all-zero counts (division by zero → NULL).
        //   - COALESCE(..., 1.0) handles zero-row result from empty co_access table (R-06).
        //   - On a clean install with no rows matching count >= 3, the INSERT selects zero rows
        //     and succeeds with no data written (window function on zero rows → zero rows, not error).
        //   - bootstrap_only = 0: co_access counts at threshold >= 3 are authoritative signals.
        //   - INSERT OR IGNORE: idempotent on re-run.
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges
                (source_id, target_id, relation_type, weight, created_at,
                 created_by, source, bootstrap_only)
             SELECT
                 entry_id_a          AS source_id,
                 entry_id_b          AS target_id,
                 'CoAccess'          AS relation_type,
                 COALESCE(
                     CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0),
                     1.0
                 )                   AS weight,
                 strftime('%s','now') AS created_at,
                 'bootstrap'         AS created_by,
                 'co_access'         AS source,
                 0                   AS bootstrap_only
             FROM co_access
             WHERE count >= ?1",
        )
        .bind(CO_ACCESS_BOOTSTRAP_MIN_COUNT)
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        // Step 4: No Contradicts bootstrap.
        // shadow_evaluations has no entry ID pairs (entry #2404, AC-08).
        // All Contradicts edges are created at runtime by W1-2 NLI.
        // This comment documents the decision; no SQL is emitted.
    }

    // v13 → v14: domain_metrics_json column on observation_metrics (col-023).
    //
    // Idempotency (FM-05): ALTER TABLE ADD COLUMN fails if the column already exists
    // in SQLite (no IF NOT EXISTS for ADD COLUMN). We pre-check with pragma_table_info
    // to avoid an error on a partially-migrated database.
    if current_version < 14 {
        let has_domain_metrics_json: bool = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('observation_metrics') WHERE name = 'domain_metrics_json'",
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count| count > 0)
        .unwrap_or(false);

        if !has_domain_metrics_json {
            sqlx::query("ALTER TABLE observation_metrics ADD COLUMN domain_metrics_json TEXT NULL")
                .execute(&mut **txn)
                .await
                .map_err(|e| StoreError::Migration {
                    source: Box::new(e),
                })?;
        }
    }

    // v14 → v15: CYCLE_EVENTS table + feature_entries.phase column (crt-025).
    if current_version < 15 {
        // Step 1: Create cycle_events table (idempotent — CREATE TABLE IF NOT EXISTS).
        // DDL mirrors create_tables_if_needed in db.rs; both must be kept in sync.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS cycle_events (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                cycle_id   TEXT    NOT NULL,
                seq        INTEGER NOT NULL,
                event_type TEXT    NOT NULL,
                phase      TEXT,
                outcome    TEXT,
                next_phase TEXT,
                timestamp  INTEGER NOT NULL
            )",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_cycle_events_cycle_id ON cycle_events (cycle_id)",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        // Step 2: Add phase column to feature_entries (idempotent via pragma_table_info pre-check).
        //
        // SQLite does not support ALTER TABLE ADD COLUMN IF NOT EXISTS.
        // Pattern from v7→v8 (pre_quarantine_status) and v13→v14 (domain_metrics_json).
        let has_phase_column: bool = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('feature_entries') WHERE name = 'phase'",
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count| count > 0)
        .unwrap_or(false);

        if !has_phase_column {
            sqlx::query("ALTER TABLE feature_entries ADD COLUMN phase TEXT")
                .execute(&mut **txn)
                .await
                .map_err(|e| StoreError::Migration {
                    source: Box::new(e),
                })?;
        }

        // No backfill: pre-existing feature_entries rows get phase = NULL (C-05, FR-06.4).
    }

    // v15 → v16: cycle_events.goal column (col-025).
    //
    // Idempotency (pattern #1264): ALTER TABLE ADD COLUMN fails if the column already exists
    // in SQLite (no IF NOT EXISTS for ADD COLUMN). We pre-check with pragma_table_info.
    if current_version < 16 {
        let has_goal_column: bool = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal'",
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count| count > 0)
        .unwrap_or(false); // if query fails, treat as absent; ALTER will succeed

        if !has_goal_column {
            sqlx::query("ALTER TABLE cycle_events ADD COLUMN goal TEXT")
                .execute(&mut **txn)
                .await
                .map_err(|e| StoreError::Migration {
                    source: Box::new(e),
                })?;
        }
        // No backfill: pre-existing cycle_events rows get goal = NULL.
        // goal-absent sessions degrade gracefully to topic-ID fallback (ADR-001, col-025).
    }

    // v16 → v17: query_log.phase column (col-028).
    //
    // Idempotency (C-02, pattern #1264): SQLite does not support ALTER TABLE ADD COLUMN
    // IF NOT EXISTS. Pre-check with pragma_table_info before attempting ALTER TABLE.
    if current_version < 17 {
        let has_phase_column: bool = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('query_log') WHERE name = 'phase'",
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count| count > 0)
        .unwrap_or(false);

        if !has_phase_column {
            sqlx::query("ALTER TABLE query_log ADD COLUMN phase TEXT")
                .execute(&mut **txn)
                .await
                .map_err(|e| StoreError::Migration {
                    source: Box::new(e),
                })?;
        }

        // CREATE INDEX IF NOT EXISTS: idempotent even without the pragma pre-check.
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_query_log_phase ON query_log (phase)")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;

        sqlx::query("UPDATE counters SET value = 17 WHERE name = 'schema_version'")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;
    }

    // v17 → v18: cycle_review_index table (crt-033).
    //
    // Stores memoized RetrospectiveReport JSON keyed by feature_cycle.
    // Used as a purge gate by GH #409 (retention pass).
    // CREATE TABLE IF NOT EXISTS: idempotent on re-run (NFR-06).
    if current_version < 18 {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS cycle_review_index (
                feature_cycle         TEXT    PRIMARY KEY,
                schema_version        INTEGER NOT NULL,
                computed_at           INTEGER NOT NULL,
                raw_signals_available INTEGER NOT NULL DEFAULT 1,
                summary_json          TEXT    NOT NULL
            )",
        )
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        sqlx::query("UPDATE counters SET value = 18 WHERE name = 'schema_version'")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;
    }

    // Update schema_version counter to CURRENT_SCHEMA_VERSION (18).
    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', ?1)")
        .bind(CURRENT_SCHEMA_VERSION as i64)
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Check helpers
// ---------------------------------------------------------------------------

async fn check_old_observation_metrics_table(conn: &mut sqlx::SqliteConnection) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('observation_metrics') WHERE name = 'data'",
    )
    .fetch_one(&mut *conn)
    .await
    .map(|count| count > 0)
    .unwrap_or(false)
}

async fn check_old_blob_entries_table(conn: &mut sqlx::SqliteConnection) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('entries') WHERE name = 'data'",
    )
    .fetch_one(&mut *conn)
    .await
    .map(|count| count > 0)
    .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// v5 → v6: full schema normalization (blob→columns)
// ---------------------------------------------------------------------------

async fn migrate_v5_to_v6(conn: &mut sqlx::SqliteConnection, db_path: &Path) -> Result<()> {
    // Step 1: Backup database file (synchronous, outside transaction — migration safety)
    create_backup_file(db_path, "v5-backup")?;

    // Step 2: BEGIN IMMEDIATE transaction
    let mut txn = conn.begin().await.map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    let result = run_v5_to_v6_migration(&mut txn).await;

    match result {
        Ok(()) => {
            txn.commit().await.map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;
            Ok(())
        }
        Err(e) => {
            let _ = txn.rollback().await;
            Err(e)
        }
    }
}

async fn run_v5_to_v6_migration(txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<()> {
    // Step 2: Create new tables with _v6 suffix
    sqlx::query(
        "CREATE TABLE entries_v6 (
            id              INTEGER PRIMARY KEY,
            title           TEXT    NOT NULL,
            content         TEXT    NOT NULL,
            topic           TEXT    NOT NULL,
            category        TEXT    NOT NULL,
            source          TEXT    NOT NULL,
            status          INTEGER NOT NULL DEFAULT 0,
            confidence      REAL    NOT NULL DEFAULT 0.0,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL,
            last_accessed_at INTEGER NOT NULL DEFAULT 0,
            access_count    INTEGER NOT NULL DEFAULT 0,
            supersedes      INTEGER,
            superseded_by   INTEGER,
            correction_count INTEGER NOT NULL DEFAULT 0,
            embedding_dim   INTEGER NOT NULL DEFAULT 0,
            created_by      TEXT    NOT NULL DEFAULT '',
            modified_by     TEXT    NOT NULL DEFAULT '',
            content_hash    TEXT    NOT NULL DEFAULT '',
            previous_hash   TEXT    NOT NULL DEFAULT '',
            version         INTEGER NOT NULL DEFAULT 0,
            feature_cycle   TEXT    NOT NULL DEFAULT '',
            trust_source    TEXT    NOT NULL DEFAULT '',
            helpful_count   INTEGER NOT NULL DEFAULT 0,
            unhelpful_count INTEGER NOT NULL DEFAULT 0,
            pre_quarantine_status INTEGER
        )",
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS entry_tags (
            entry_id INTEGER NOT NULL,
            tag      TEXT    NOT NULL,
            PRIMARY KEY (entry_id, tag),
            FOREIGN KEY (entry_id) REFERENCES entries_v6(id) ON DELETE CASCADE
        )",
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    sqlx::query(
        "CREATE TABLE co_access_v6 (
            entry_id_a   INTEGER NOT NULL,
            entry_id_b   INTEGER NOT NULL,
            count        INTEGER NOT NULL DEFAULT 1,
            last_updated INTEGER NOT NULL,
            PRIMARY KEY (entry_id_a, entry_id_b),
            CHECK (entry_id_a < entry_id_b)
        )",
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    sqlx::query(
        "CREATE TABLE sessions_v6 (
            session_id       TEXT    PRIMARY KEY,
            feature_cycle    TEXT,
            agent_role       TEXT,
            started_at       INTEGER NOT NULL,
            ended_at         INTEGER,
            status           INTEGER NOT NULL DEFAULT 0,
            compaction_count INTEGER NOT NULL DEFAULT 0,
            outcome          TEXT,
            total_injections INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    sqlx::query(
        "CREATE TABLE injection_log_v6 (
            log_id     INTEGER PRIMARY KEY,
            session_id TEXT    NOT NULL,
            entry_id   INTEGER NOT NULL,
            confidence REAL    NOT NULL,
            timestamp  INTEGER NOT NULL
        )",
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    sqlx::query(
        "CREATE TABLE signal_queue_v6 (
            signal_id     INTEGER PRIMARY KEY,
            session_id    TEXT    NOT NULL,
            created_at    INTEGER NOT NULL,
            entry_ids     TEXT    NOT NULL DEFAULT '[]',
            signal_type   INTEGER NOT NULL,
            signal_source INTEGER NOT NULL
        )",
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    sqlx::query(
        "CREATE TABLE agent_registry_v6 (
            agent_id           TEXT    PRIMARY KEY,
            trust_level        INTEGER NOT NULL,
            capabilities       TEXT    NOT NULL DEFAULT '[]',
            allowed_topics     TEXT,
            allowed_categories TEXT,
            enrolled_at        INTEGER NOT NULL,
            last_seen_at       INTEGER NOT NULL,
            active             INTEGER NOT NULL DEFAULT 1
        )",
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    sqlx::query(
        "CREATE TABLE audit_log_v6 (
            event_id   INTEGER PRIMARY KEY,
            timestamp  INTEGER NOT NULL,
            session_id TEXT    NOT NULL,
            agent_id   TEXT    NOT NULL,
            operation  TEXT    NOT NULL,
            target_ids TEXT    NOT NULL DEFAULT '[]',
            outcome    INTEGER NOT NULL,
            detail     TEXT    NOT NULL DEFAULT ''
        )",
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    // Step 3: Migrate entries
    migrate_entries_v5_to_v6(txn).await?;

    // Step 4: Migrate co_access
    migrate_co_access_v5_to_v6(txn).await?;

    // Step 5: Migrate sessions
    migrate_sessions_v5_to_v6(txn).await?;

    // Step 6: Migrate injection_log
    migrate_injection_log_v5_to_v6(txn).await?;

    // Step 7: Migrate signal_queue
    migrate_signal_queue_v5_to_v6(txn).await?;

    // Step 8: Migrate agent_registry
    migrate_agent_registry_v5_to_v6(txn).await?;

    // Step 9: Migrate audit_log
    migrate_audit_log_v5_to_v6(txn).await?;

    // Step 10: Drop old tables (inline literals — no format! interpolation)
    for sql in &[
        "DROP TABLE IF EXISTS entries",
        "DROP TABLE IF EXISTS topic_index",
        "DROP TABLE IF EXISTS category_index",
        "DROP TABLE IF EXISTS tag_index",
        "DROP TABLE IF EXISTS time_index",
        "DROP TABLE IF EXISTS status_index",
        "DROP TABLE IF EXISTS co_access",
        "DROP TABLE IF EXISTS sessions",
        "DROP TABLE IF EXISTS injection_log",
        "DROP TABLE IF EXISTS signal_queue",
        "DROP TABLE IF EXISTS agent_registry",
        "DROP TABLE IF EXISTS audit_log",
    ] {
        sqlx::query(sql)
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;
    }

    // Step 11: Rename new tables (inline literals — no format! interpolation)
    for sql in &[
        "ALTER TABLE entries_v6 RENAME TO entries",
        "ALTER TABLE co_access_v6 RENAME TO co_access",
        "ALTER TABLE sessions_v6 RENAME TO sessions",
        "ALTER TABLE injection_log_v6 RENAME TO injection_log",
        "ALTER TABLE signal_queue_v6 RENAME TO signal_queue",
        "ALTER TABLE agent_registry_v6 RENAME TO agent_registry",
        "ALTER TABLE audit_log_v6 RENAME TO audit_log",
    ] {
        sqlx::query(sql)
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;
    }

    // Step 12: Create indexes
    let indexes = [
        "CREATE INDEX idx_entries_topic ON entries(topic)",
        "CREATE INDEX idx_entries_category ON entries(category)",
        "CREATE INDEX idx_entries_status ON entries(status)",
        "CREATE INDEX idx_entries_created_at ON entries(created_at)",
        "CREATE INDEX idx_entry_tags_tag ON entry_tags(tag)",
        "CREATE INDEX idx_entry_tags_entry_id ON entry_tags(entry_id)",
        "CREATE INDEX idx_co_access_b ON co_access(entry_id_b)",
        "CREATE INDEX idx_sessions_feature_cycle ON sessions(feature_cycle)",
        "CREATE INDEX idx_sessions_started_at ON sessions(started_at)",
        "CREATE INDEX idx_injection_log_session ON injection_log(session_id)",
        "CREATE INDEX idx_injection_log_entry ON injection_log(entry_id)",
        "CREATE INDEX idx_audit_log_agent ON audit_log(agent_id)",
        "CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp)",
    ];
    for idx_sql in &indexes {
        sqlx::query(idx_sql)
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;
    }

    // Step 13: Update schema version
    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', 6)")
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// v5→v6 per-table migration helpers
// ---------------------------------------------------------------------------

async fn migrate_entries_v5_to_v6(txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<()> {
    let rows: Vec<(i64, Vec<u8>)> =
        sqlx::query_as::<_, (i64, Vec<u8>)>("SELECT id, data FROM entries")
            .fetch_all(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;

    for (id, data) in &rows {
        let record = migration_compat::deserialize_entry_v5(data)?;

        sqlx::query(
            "INSERT INTO entries_v6 (id, title, content, topic, category, source,
                status, confidence, created_at, updated_at, last_accessed_at,
                access_count, supersedes, superseded_by, correction_count,
                embedding_dim, created_by, modified_by, content_hash,
                previous_hash, version, feature_cycle, trust_source,
                helpful_count, unhelpful_count)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                    ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
        )
        .bind(*id)
        .bind(&record.title)
        .bind(&record.content)
        .bind(&record.topic)
        .bind(&record.category)
        .bind(&record.source)
        .bind(record.status as u8 as i64)
        .bind(record.confidence)
        .bind(record.created_at as i64)
        .bind(record.updated_at as i64)
        .bind(record.last_accessed_at as i64)
        .bind(record.access_count as i64)
        .bind(record.supersedes.map(|v| v as i64))
        .bind(record.superseded_by.map(|v| v as i64))
        .bind(record.correction_count as i64)
        .bind(record.embedding_dim as i64)
        .bind(&record.created_by)
        .bind(&record.modified_by)
        .bind(&record.content_hash)
        .bind(&record.previous_hash)
        .bind(record.version as i64)
        .bind(&record.feature_cycle)
        .bind(&record.trust_source)
        .bind(record.helpful_count as i64)
        .bind(record.unhelpful_count as i64)
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

        for tag in &record.tags {
            sqlx::query("INSERT OR IGNORE INTO entry_tags (entry_id, tag) VALUES (?1, ?2)")
                .bind(*id)
                .bind(tag)
                .execute(&mut **txn)
                .await
                .map_err(|e| StoreError::Migration {
                    source: Box::new(e),
                })?;
        }
    }

    Ok(())
}

async fn migrate_co_access_v5_to_v6(txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<()> {
    let rows: Vec<(i64, i64, Vec<u8>)> = sqlx::query_as::<_, (i64, i64, Vec<u8>)>(
        "SELECT entry_id_a, entry_id_b, data FROM co_access",
    )
    .fetch_all(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    for (a, b, data) in &rows {
        let record = migration_compat::deserialize_co_access_v5(data)?;
        sqlx::query(
            "INSERT INTO co_access_v6 (entry_id_a, entry_id_b, count, last_updated)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(a)
        .bind(b)
        .bind(record.count as i64)
        .bind(record.last_updated as i64)
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;
    }

    Ok(())
}

async fn migrate_sessions_v5_to_v6(txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<()> {
    let rows: Vec<(String, Vec<u8>)> =
        sqlx::query_as::<_, (String, Vec<u8>)>("SELECT session_id, data FROM sessions")
            .fetch_all(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;

    for (session_id, data) in &rows {
        let record = migration_compat::deserialize_session_v5(data)?;
        let status_int = record.status as u8 as i64;

        sqlx::query(
            "INSERT INTO sessions_v6 (session_id, feature_cycle, agent_role,
                started_at, ended_at, status, compaction_count, outcome, total_injections)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(session_id)
        .bind(&record.feature_cycle)
        .bind(&record.agent_role)
        .bind(record.started_at as i64)
        .bind(record.ended_at.map(|v| v as i64))
        .bind(status_int)
        .bind(record.compaction_count as i64)
        .bind(&record.outcome)
        .bind(record.total_injections as i64)
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;
    }

    Ok(())
}

async fn migrate_injection_log_v5_to_v6(
    txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<()> {
    let rows: Vec<(i64, Vec<u8>)> =
        sqlx::query_as::<_, (i64, Vec<u8>)>("SELECT log_id, data FROM injection_log")
            .fetch_all(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;

    for (log_id, data) in &rows {
        let record = migration_compat::deserialize_injection_log_v5(data)?;
        sqlx::query(
            "INSERT INTO injection_log_v6 (log_id, session_id, entry_id, confidence, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(log_id)
        .bind(&record.session_id)
        .bind(record.entry_id as i64)
        .bind(record.confidence)
        .bind(record.timestamp as i64)
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;
    }

    Ok(())
}

async fn migrate_signal_queue_v5_to_v6(
    txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<()> {
    let rows: Vec<(i64, Vec<u8>)> =
        sqlx::query_as::<_, (i64, Vec<u8>)>("SELECT signal_id, data FROM signal_queue")
            .fetch_all(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;

    for (signal_id, data) in &rows {
        let record = migration_compat::deserialize_signal_v5(data)?;
        let entry_ids_json = serde_json::to_string(&record.entry_ids)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        sqlx::query(
            "INSERT INTO signal_queue_v6 (signal_id, session_id, created_at, entry_ids, signal_type, signal_source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(signal_id)
        .bind(&record.session_id)
        .bind(record.created_at as i64)
        .bind(&entry_ids_json)
        .bind(record.signal_type as u8 as i64)
        .bind(record.signal_source as u8 as i64)
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
    }

    Ok(())
}

async fn migrate_agent_registry_v5_to_v6(
    txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<()> {
    let rows: Vec<(String, Vec<u8>)> =
        sqlx::query_as::<_, (String, Vec<u8>)>("SELECT agent_id, data FROM agent_registry")
            .fetch_all(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;

    for (agent_id, data) in &rows {
        let record = migration_compat::deserialize_agent_v5(data)?;
        let cap_ints: Vec<u8> = record.capabilities.iter().map(|c| *c as u8).collect();
        let caps_json = serde_json::to_string(&cap_ints)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let topics_json: Option<String> = record
            .allowed_topics
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let cats_json: Option<String> = record
            .allowed_categories
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let trust_int = record.trust_level as u8 as i64;

        sqlx::query(
            "INSERT INTO agent_registry_v6 (agent_id, trust_level, capabilities,
                allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(agent_id)
        .bind(trust_int)
        .bind(&caps_json)
        .bind(&topics_json)
        .bind(&cats_json)
        .bind(record.enrolled_at as i64)
        .bind(record.last_seen_at as i64)
        .bind(if record.active { 1_i64 } else { 0_i64 })
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;
    }

    Ok(())
}

async fn migrate_audit_log_v5_to_v6(txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<()> {
    let rows: Vec<(i64, Vec<u8>)> =
        sqlx::query_as::<_, (i64, Vec<u8>)>("SELECT event_id, data FROM audit_log")
            .fetch_all(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;

    for (event_id, data) in &rows {
        let record = migration_compat::deserialize_audit_event_v5(data)?;
        let target_ids_json = serde_json::to_string(&record.target_ids)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        sqlx::query(
            "INSERT INTO audit_log_v6 (event_id, timestamp, session_id, agent_id,
                operation, target_ids, outcome, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(event_id)
        .bind(record.timestamp as i64)
        .bind(&record.session_id)
        .bind(&record.agent_id)
        .bind(&record.operation)
        .bind(&target_ids_json)
        .bind(record.outcome as u8 as i64)
        .bind(&record.detail)
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// v8 → v9: observation metrics normalization (blob→columns)
// ---------------------------------------------------------------------------

async fn migrate_v8_to_v9(conn: &mut sqlx::SqliteConnection, db_path: &Path) -> Result<()> {
    // Step 1: Backup database file
    create_backup_file(db_path, "v8-backup")?;

    // Step 2: Read all existing rows (outside transaction, before DROP)
    let rows: Vec<(String, Vec<u8>)> = sqlx::query_as::<_, (String, Vec<u8>)>(
        "SELECT feature_cycle, data FROM observation_metrics",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    // Step 3: Deserialize blobs
    let mut migrated: Vec<(String, crate::metrics::MetricVector)> = Vec::new();
    for (fc, data) in &rows {
        // Corrupted blob: insert default MetricVector to preserve the key (FR-06)
        let mv = migration_compat::deserialize_metric_vector_v8(data).unwrap_or_default();
        migrated.push((fc.clone(), mv));
    }

    // Step 4: Transaction — drop old, create new, insert data
    let mut txn = conn.begin().await.map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    let result = run_v8_to_v9_migration(&mut txn, &migrated).await;

    match result {
        Ok(()) => {
            txn.commit().await.map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;
            Ok(())
        }
        Err(e) => {
            let _ = txn.rollback().await;
            Err(e)
        }
    }
}

async fn run_v8_to_v9_migration(
    txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    migrated: &[(String, crate::metrics::MetricVector)],
) -> Result<()> {
    sqlx::query("DROP TABLE IF EXISTS observation_phase_metrics")
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

    sqlx::query("DROP TABLE observation_metrics")
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

    sqlx::query(
        "CREATE TABLE observation_metrics (
            feature_cycle                      TEXT    PRIMARY KEY,
            computed_at                        INTEGER NOT NULL DEFAULT 0,
            total_tool_calls                   INTEGER NOT NULL DEFAULT 0,
            total_duration_secs                INTEGER NOT NULL DEFAULT 0,
            session_count                      INTEGER NOT NULL DEFAULT 0,
            search_miss_rate                   REAL    NOT NULL DEFAULT 0.0,
            edit_bloat_total_kb                REAL    NOT NULL DEFAULT 0.0,
            edit_bloat_ratio                   REAL    NOT NULL DEFAULT 0.0,
            permission_friction_events         INTEGER NOT NULL DEFAULT 0,
            bash_for_search_count              INTEGER NOT NULL DEFAULT 0,
            cold_restart_events                INTEGER NOT NULL DEFAULT 0,
            coordinator_respawn_count          INTEGER NOT NULL DEFAULT 0,
            parallel_call_rate                 REAL    NOT NULL DEFAULT 0.0,
            context_load_before_first_write_kb REAL    NOT NULL DEFAULT 0.0,
            total_context_loaded_kb            REAL    NOT NULL DEFAULT 0.0,
            post_completion_work_pct           REAL    NOT NULL DEFAULT 0.0,
            follow_up_issues_created           INTEGER NOT NULL DEFAULT 0,
            knowledge_entries_stored           INTEGER NOT NULL DEFAULT 0,
            sleep_workaround_count             INTEGER NOT NULL DEFAULT 0,
            agent_hotspot_count                INTEGER NOT NULL DEFAULT 0,
            friction_hotspot_count             INTEGER NOT NULL DEFAULT 0,
            session_hotspot_count              INTEGER NOT NULL DEFAULT 0,
            scope_hotspot_count                INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration {
        source: Box::new(e),
    })?;

    sqlx::query(
        "CREATE TABLE observation_phase_metrics (
            feature_cycle   TEXT    NOT NULL,
            phase_name      TEXT    NOT NULL,
            duration_secs   INTEGER NOT NULL DEFAULT 0,
            tool_call_count INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (feature_cycle, phase_name),
            FOREIGN KEY (feature_cycle) REFERENCES observation_metrics(feature_cycle) ON DELETE CASCADE
        )",
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    // Insert migrated data
    for (fc, mv) in migrated {
        let u = &mv.universal;
        sqlx::query(
            "INSERT INTO observation_metrics (
                feature_cycle, computed_at,
                total_tool_calls, total_duration_secs, session_count,
                search_miss_rate, edit_bloat_total_kb, edit_bloat_ratio,
                permission_friction_events, bash_for_search_count,
                cold_restart_events, coordinator_respawn_count,
                parallel_call_rate, context_load_before_first_write_kb,
                total_context_loaded_kb, post_completion_work_pct,
                follow_up_issues_created, knowledge_entries_stored,
                sleep_workaround_count, agent_hotspot_count,
                friction_hotspot_count, session_hotspot_count, scope_hotspot_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
        )
        .bind(fc)
        .bind(mv.computed_at as i64)
        .bind(u.total_tool_calls as i64)
        .bind(u.total_duration_secs as i64)
        .bind(u.session_count as i64)
        .bind(u.search_miss_rate)
        .bind(u.edit_bloat_total_kb)
        .bind(u.edit_bloat_ratio)
        .bind(u.permission_friction_events as i64)
        .bind(u.bash_for_search_count as i64)
        .bind(u.cold_restart_events as i64)
        .bind(u.coordinator_respawn_count as i64)
        .bind(u.parallel_call_rate)
        .bind(u.context_load_before_first_write_kb)
        .bind(u.total_context_loaded_kb)
        .bind(u.post_completion_work_pct)
        .bind(u.follow_up_issues_created as i64)
        .bind(u.knowledge_entries_stored as i64)
        .bind(u.sleep_workaround_count as i64)
        .bind(u.agent_hotspot_count as i64)
        .bind(u.friction_hotspot_count as i64)
        .bind(u.session_hotspot_count as i64)
        .bind(u.scope_hotspot_count as i64)
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

        for (phase_name, phase) in &mv.phases {
            sqlx::query(
                "INSERT INTO observation_phase_metrics (feature_cycle, phase_name, duration_secs, tool_call_count)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(fc)
            .bind(phase_name)
            .bind(phase.duration_secs as i64)
            .bind(phase.tool_call_count as i64)
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
        }
    }

    // Update schema version
    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', 9)")
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration {
            source: Box::new(e),
        })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// v0-v2 entry re-serialization
// ---------------------------------------------------------------------------

/// Re-serialize all entries to current EntryRecord format.
/// Used for v0-v2 migration path.
async fn migrate_entries_to_current_schema(conn: &mut sqlx::SqliteConnection) -> Result<()> {
    let rows: Vec<(i64, Vec<u8>)> =
        sqlx::query_as::<_, (i64, Vec<u8>)>("SELECT id, data FROM entries")
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;

    let mut updates: Vec<(i64, Vec<u8>)> = Vec::new();
    for (id, bytes) in &rows {
        match deserialize_entry(bytes) {
            Ok(record) => {
                let new_bytes = serialize_entry(&record)?;
                if new_bytes != *bytes {
                    updates.push((*id, new_bytes));
                }
            }
            Err(_) => {
                continue;
            }
        }
    }

    for (id, bytes) in updates {
        sqlx::query("UPDATE entries SET data = ?1 WHERE id = ?2")
            .bind(&bytes)
            .bind(id)
            .execute(&mut *conn)
            .await
            .map_err(|e| StoreError::Migration {
                source: Box::new(e),
            })?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Backup helper
// ---------------------------------------------------------------------------

fn create_backup_file(db_path: &Path, suffix: &str) -> Result<()> {
    let path_str = db_path.to_str().unwrap_or("");
    if !path_str.is_empty() && path_str != ":memory:" {
        let backup_path = format!("{}.{}", path_str, suffix);
        std::fs::copy(db_path, &backup_path)
            .map_err(|e| StoreError::Deserialization(format!("backup failed: {e}")))?;
    }
    Ok(())
}
