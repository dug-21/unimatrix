//! Cycle-based activity GC methods for crt-036.
//!
//! Implements the retention policy: retain activity data (observations, query_log,
//! sessions, injection_log) for the K most recently reviewed feature cycles;
//! prune all older reviewed cycles. Gate: cycle must have a cycle_review_index row.
//!
//! All write methods use write_pool_server(). list_purgeable_cycles uses read_pool().
//! Per-cycle transaction: pool.begin() / txn.commit() (entry #2159, ADR-001).
//! Connection released between cycles (NFR-02, write_pool_server max_connections=1).

use sqlx::Row;

use crate::db::SqlxStore;
use crate::error::{Result, StoreError};

// ---------------------------------------------------------------------------
// Stats types
// ---------------------------------------------------------------------------

/// Row counts returned after gc_cycle_activity() commits for one feature cycle.
#[derive(Debug, Default)]
pub struct CycleGcStats {
    pub observations_deleted: u64,
    pub query_log_deleted: u64,
    pub injection_log_deleted: u64,
    pub sessions_deleted: u64,
}

/// Row counts returned by gc_unattributed_activity().
#[derive(Debug, Default)]
pub struct UnattributedGcStats {
    pub observations_deleted: u64,
    pub query_log_deleted: u64,
    pub sessions_deleted: u64,
    pub injection_log_deleted: u64,
}

// ---------------------------------------------------------------------------
// Store methods
// ---------------------------------------------------------------------------

impl SqlxStore {
    /// Returns feature_cycle IDs for all reviewed cycles outside the K-window,
    /// ordered oldest-first (lowest computed_at). Result is capped to max_per_tick.
    ///
    /// Uses read_pool() — read-only query (entry #3619).
    ///
    /// If total reviewed cycles <= k, returns an empty Vec (nothing to prune).
    /// The cap (max_per_tick) is applied in the SQL LIMIT clause.
    ///
    /// Also returns the computed_at of the K-th retained cycle (oldest in K-window)
    /// for the PhaseFreqTable alignment check. Returns None when fewer than K cycles
    /// exist in cycle_review_index (no pruning has occurred, no gap is possible).
    pub async fn list_purgeable_cycles(
        &self,
        k: u32,
        max_per_tick: u32,
    ) -> Result<(Vec<String>, Option<i64>)> {
        // Query 1: Fetch the purgeable cycles (all reviewed cycles outside top-K window).
        // The NOT IN subquery with ORDER BY + LIMIT is valid SQLite syntax.
        // Oldest-first ordering (ORDER BY computed_at ASC) ensures the cap processes
        // the oldest cycles before newer ones.
        let purgeable_rows = sqlx::query(
            "SELECT feature_cycle FROM cycle_review_index \
             WHERE feature_cycle NOT IN ( \
                 SELECT feature_cycle FROM cycle_review_index \
                 ORDER BY computed_at DESC \
                 LIMIT ?1 \
             ) \
             ORDER BY computed_at ASC \
             LIMIT ?2",
        )
        .bind(k as i64)
        .bind(max_per_tick as i64)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let purgeable: Vec<String> = purgeable_rows
            .into_iter()
            .map(|row| row.get::<String, _>(0))
            .collect();

        // Query 2: Fetch the computed_at of the K-th oldest retained cycle.
        // This is the oldest cycle in the K-window — used by the alignment guard (ADR-003).
        // Returns None if fewer than K rows exist (OFFSET k-1 returns no row).
        // When k = 0 this would be a programming error — validated at startup.
        let oldest_retained_computed_at: Option<i64> = sqlx::query(
            "SELECT computed_at FROM cycle_review_index \
             ORDER BY computed_at DESC \
             LIMIT 1 OFFSET ?1",
        )
        .bind(k.saturating_sub(1) as i64)
        .fetch_optional(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?
        .map(|row| row.get::<i64, _>(0));

        Ok((purgeable, oldest_retained_computed_at))
    }

    /// Delete all activity data for one feature cycle in a single transaction.
    ///
    /// Transaction acquired via pool.begin() / txn.commit() (ADR-001, entry #2159).
    /// Connection is released (returned to pool) on return from this method.
    ///
    /// Delete order is mandatory (ADR-001):
    ///   1. observations  — joined through sessions (no direct feature_cycle column)
    ///   2. query_log     — joined through sessions (no direct feature_cycle column)
    ///   3. injection_log — joined through sessions; must precede sessions (FK dependency)
    ///   4. sessions      — deleted last; subqueries in steps 1-3 resolve while sessions exist
    ///
    /// Returns CycleGcStats with per-table row counts.
    /// On error: transaction is rolled back by sqlx; the cycle remains purgeable on
    /// the next tick. Caller must NOT call store_cycle_review() when this returns Err.
    pub async fn gc_cycle_activity(&self, feature_cycle: &str) -> Result<CycleGcStats> {
        // Acquire per-cycle transaction via pool.begin().
        // This is the ONLY transaction for this cycle — never opened outside the cycle loop.
        // The connection is held from here until txn.commit() at the end of this method.
        let mut txn = self
            .write_pool_server()
            .begin()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        // Step 1: Delete observations for sessions in this cycle.
        // Two-hop join: observations.session_id -> sessions.session_id -> sessions.feature_cycle
        // Uses idx_observations_session index (NFR-03 performance requirement).
        let obs = sqlx::query(
            "DELETE FROM observations \
             WHERE session_id IN ( \
                 SELECT session_id FROM sessions WHERE feature_cycle = ?1 \
             )",
        )
        .bind(feature_cycle)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 2: Delete query_log rows for sessions in this cycle.
        // Two-hop join: query_log.session_id -> sessions.session_id -> sessions.feature_cycle
        // Uses idx_query_log_session index (NFR-03).
        let qlog = sqlx::query(
            "DELETE FROM query_log \
             WHERE session_id IN ( \
                 SELECT session_id FROM sessions WHERE feature_cycle = ?1 \
             )",
        )
        .bind(feature_cycle)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 3: Delete injection_log rows for sessions in this cycle.
        // Must precede sessions DELETE — injection_log.session_id is a logical FK.
        let ilog = sqlx::query(
            "DELETE FROM injection_log \
             WHERE session_id IN ( \
                 SELECT session_id FROM sessions WHERE feature_cycle = ?1 \
             )",
        )
        .bind(feature_cycle)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 4: Delete the sessions themselves.
        // Sessions deleted last — subqueries in steps 1-3 resolved while sessions existed.
        let sess = sqlx::query("DELETE FROM sessions WHERE feature_cycle = ?1")
            .bind(feature_cycle)
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        // Commit the transaction and release the write pool connection.
        // After this line the connection is available for the next cycle or other writers.
        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        Ok(CycleGcStats {
            observations_deleted: obs.rows_affected(),
            query_log_deleted: qlog.rows_affected(),
            injection_log_deleted: ilog.rows_affected(),
            sessions_deleted: sess.rows_affected(),
        })
    }

    /// Delete orphaned and unattributed activity rows.
    ///
    /// Runs after the per-cycle loop. No transaction required — each DELETE is
    /// independently atomic in SQLite.
    ///
    /// Targets:
    ///   1. Observations whose session_id does not exist in sessions (truly orphaned).
    ///   2. query_log rows whose session_id does not exist in sessions (truly orphaned).
    ///   3. injection_log rows for sessions with feature_cycle IS NULL and status != Active.
    ///   4. Sessions with feature_cycle IS NULL and status != Active (unattributed, closed).
    ///
    /// Active-session guard (SR-06): sessions with status = 0 (Active) are NEVER deleted.
    /// status = 0 is the numeric representation of SessionLifecycleStatus::Active.
    pub async fn gc_unattributed_activity(&self) -> Result<UnattributedGcStats> {
        // Step 1: Delete observations with no matching session.
        // idx_observations_session used for the NOT IN subquery (NFR-03).
        let mut conn = self
            .write_pool_server()
            .acquire()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let obs = sqlx::query(
            "DELETE FROM observations \
             WHERE session_id NOT IN (SELECT session_id FROM sessions)",
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 2: Delete query_log rows with no matching session.
        // idx_query_log_session used for the NOT IN subquery (NFR-03).
        let qlog = sqlx::query(
            "DELETE FROM query_log \
             WHERE session_id NOT IN (SELECT session_id FROM sessions)",
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 3: Delete injection_log for unattributed non-active sessions.
        // Must precede the sessions DELETE in step 4 (FK dependency).
        // status != 0 excludes Active sessions (numeric 0 = Active per SessionLifecycleStatus).
        let ilog = sqlx::query(
            "DELETE FROM injection_log \
             WHERE session_id IN ( \
                 SELECT session_id FROM sessions \
                 WHERE feature_cycle IS NULL AND status != 0 \
             )",
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 4: Delete unattributed non-active sessions.
        // Active sessions (status = 0) are guarded — in-flight retrospectives must not
        // lose their session anchor.
        let sess = sqlx::query("DELETE FROM sessions WHERE feature_cycle IS NULL AND status != 0")
            .execute(&mut *conn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        Ok(UnattributedGcStats {
            observations_deleted: obs.rows_affected(),
            query_log_deleted: qlog.rows_affected(),
            sessions_deleted: sess.rows_affected(),
            injection_log_deleted: ilog.rows_affected(),
        })
    }

    /// Time-based audit log GC — deferred (append-only model).
    ///
    /// The audit_log table is protected by BEFORE DELETE triggers installed in
    /// schema v25 (vnc-014 / ASS-050). Any DELETE statement will be rejected by
    /// the trigger with ABORT. Time-based GC is therefore not possible without a
    /// DROP+recreate strategy.
    ///
    /// Retention policy for audit_log is deferred to a future feature.
    /// This method is retained as a no-op to preserve the call signature used
    /// by callers in services/status.rs.
    ///
    /// Returns Ok(0) — no rows deleted.
    pub async fn gc_audit_log(&self, retention_days: u32) -> Result<u64> {
        tracing::warn!(
            retention_days,
            "gc_audit_log is a no-op: audit_log is append-only (vnc-014). \
             Time-based GC deferred to future retention policy feature."
        );
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cycle_review_index::{CycleReviewRecord, SUMMARY_SCHEMA_VERSION};
    use crate::schema::{AuditEvent, Outcome};
    use crate::sessions::{SessionLifecycleStatus, SessionRecord};
    use crate::test_helpers::open_test_store;

    // Helper: insert a session with optional feature_cycle and specified status.
    async fn insert_session_direct(
        store: &SqlxStore,
        session_id: &str,
        feature_cycle: Option<&str>,
        status: SessionLifecycleStatus,
    ) {
        let record = SessionRecord {
            session_id: session_id.to_string(),
            feature_cycle: feature_cycle.map(str::to_string),
            agent_role: None,
            started_at: 1_700_000_000,
            ended_at: None,
            status,
            compaction_count: 0,
            outcome: None,
            total_injections: 0,
            keywords: None,
        };
        store
            .insert_session(&record)
            .await
            .expect("insert_session must succeed");
    }

    // Helper: insert an observation for a session.
    async fn insert_observation(store: &SqlxStore, session_id: &str) {
        store
            .insert_observation(
                session_id,
                1_700_000_000_000,
                "tool_use",
                None,
                None,
                None,
                None,
            )
            .await
            .expect("insert_observation must succeed");
    }

    // Helper: insert a query_log row directly via write pool.
    async fn insert_query_log_direct(store: &SqlxStore, session_id: &str) {
        let mut conn = store.write_pool_server().acquire().await.expect("acquire");
        sqlx::query(
            "INSERT INTO query_log \
             (session_id, query_text, ts, result_count, \
              result_entry_ids, similarity_scores, retrieval_mode, source) \
             VALUES (?1, 'test query', 1700000000, 0, '[]', '[]', 'vector', 'test')",
        )
        .bind(session_id)
        .execute(&mut *conn)
        .await
        .expect("insert query_log must succeed");
    }

    // Helper: insert an injection_log row directly via write pool.
    async fn insert_injection_log_direct(store: &SqlxStore, session_id: &str) {
        let mut conn = store.write_pool_server().acquire().await.expect("acquire");
        sqlx::query(
            "INSERT INTO injection_log \
             (session_id, entry_id, confidence, timestamp) \
             VALUES (?1, 1, 0.9, 1700000000)",
        )
        .bind(session_id)
        .execute(&mut *conn)
        .await
        .expect("insert injection_log must succeed");
    }

    // Helper: insert a cycle_review_index row.
    async fn insert_cycle_review(store: &SqlxStore, feature_cycle: &str, computed_at: i64) {
        let record = CycleReviewRecord {
            feature_cycle: feature_cycle.to_string(),
            schema_version: SUMMARY_SCHEMA_VERSION,
            computed_at,
            raw_signals_available: 1,
            summary_json: format!(r#"{{"feature_cycle":"{feature_cycle}"}}"#),
            ..Default::default()
        };
        store
            .store_cycle_review(&record)
            .await
            .expect("store_cycle_review must succeed");
    }

    // Helper: count rows in a table for a given session.
    async fn count_for_session(store: &SqlxStore, table: &str, session_id: &str) -> i64 {
        let mut conn = store.write_pool_server().acquire().await.expect("acquire");
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE session_id = ?1");
        sqlx::query_scalar::<_, i64>(&sql)
            .bind(session_id)
            .fetch_one(&mut *conn)
            .await
            .expect("count query must succeed")
    }

    // Helper: count all rows in a table.
    async fn count_table(store: &SqlxStore, table: &str) -> i64 {
        let mut conn = store.write_pool_server().acquire().await.expect("acquire");
        let sql = format!("SELECT COUNT(*) FROM {table}");
        sqlx::query_scalar::<_, i64>(&sql)
            .fetch_one(&mut *conn)
            .await
            .expect("count query must succeed")
    }

    // -----------------------------------------------------------------------
    // test_gc_cycle_based_pruning_correctness (AC-02)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_gc_cycle_based_pruning_correctness() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // Insert 5 reviewed cycles, oldest to newest (computed_at 1000..5000).
        let cycles = ["cycle-1", "cycle-2", "cycle-3", "cycle-4", "cycle-5"];
        for (i, cycle) in cycles.iter().enumerate() {
            insert_cycle_review(&store, cycle, (i as i64 + 1) * 1000).await;
        }

        // For each cycle: 2 sessions, 3 observations/session, 2 query_log/session,
        // 1 injection_log/session.
        for cycle in &cycles {
            for s in 0..2usize {
                let session_id = format!("{cycle}-sess-{s}");
                insert_session_direct(
                    &store,
                    &session_id,
                    Some(cycle),
                    SessionLifecycleStatus::Completed,
                )
                .await;
                for _ in 0..3 {
                    insert_observation(&store, &session_id).await;
                }
                for _ in 0..2 {
                    insert_query_log_direct(&store, &session_id).await;
                }
                insert_injection_log_direct(&store, &session_id).await;
            }
        }

        // K = 3: cycles 3, 4, 5 retained; cycles 1, 2 purgeable.
        let (purgeable, _) = store
            .list_purgeable_cycles(3, 100)
            .await
            .expect("list_purgeable_cycles must not error");

        assert_eq!(
            purgeable.len(),
            2,
            "exactly 2 cycles purgeable with K=3 and 5 reviewed cycles"
        );
        assert!(
            purgeable.contains(&"cycle-1".to_string()),
            "cycle-1 must be purgeable"
        );
        assert!(
            purgeable.contains(&"cycle-2".to_string()),
            "cycle-2 must be purgeable"
        );

        // Run gc_cycle_activity for each purgeable cycle.
        for cycle in &purgeable {
            let stats = store
                .gc_cycle_activity(cycle)
                .await
                .expect("gc_cycle_activity must succeed");
            // Each purgeable cycle has 2 sessions × 3 obs = 6 obs,
            // 2 sessions × 2 qlog = 4 qlog, 2 sessions × 1 ilog = 2 ilog, 2 sessions.
            assert_eq!(stats.observations_deleted, 6, "{cycle}: obs_deleted");
            assert_eq!(stats.query_log_deleted, 4, "{cycle}: qlog_deleted");
            assert_eq!(stats.injection_log_deleted, 2, "{cycle}: ilog_deleted");
            assert_eq!(stats.sessions_deleted, 2, "{cycle}: sess_deleted");
        }

        // Verify pruned cycles are empty.
        for cycle in &["cycle-1", "cycle-2"] {
            for s in 0..2usize {
                let session_id = format!("{cycle}-sess-{s}");
                assert_eq!(
                    count_for_session(&store, "observations", &session_id).await,
                    0,
                    "{cycle} observations not cleared"
                );
                assert_eq!(
                    count_for_session(&store, "query_log", &session_id).await,
                    0,
                    "{cycle} query_log not cleared"
                );
                assert_eq!(
                    count_for_session(&store, "injection_log", &session_id).await,
                    0,
                    "{cycle} injection_log not cleared"
                );
            }
        }

        // Verify retained cycles still have all rows.
        for cycle in &["cycle-3", "cycle-4", "cycle-5"] {
            for s in 0..2usize {
                let session_id = format!("{cycle}-sess-{s}");
                assert_eq!(
                    count_for_session(&store, "observations", &session_id).await,
                    3,
                    "{cycle} observations should be intact"
                );
                assert_eq!(
                    count_for_session(&store, "query_log", &session_id).await,
                    2,
                    "{cycle} query_log should be intact"
                );
                assert_eq!(
                    count_for_session(&store, "injection_log", &session_id).await,
                    1,
                    "{cycle} injection_log should be intact"
                );
            }
        }

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // test_gc_protected_tables_regression (AC-03)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_gc_protected_tables_regression() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // Insert a retained and two purgeable cycles.
        insert_cycle_review(&store, "retained-A", 3000).await;
        insert_cycle_review(&store, "purgeable-1", 1000).await;
        insert_cycle_review(&store, "purgeable-2", 2000).await;

        // Insert an entry (protected table).
        let entry_id = store
            .insert(crate::test_helpers::TestEntry::new("test-topic", "decision").build())
            .await
            .expect("insert entry");

        // Insert a cycle_event (protected table).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        store
            .insert_cycle_event(
                "protected-cycle",
                0,
                "cycle_start",
                None,
                None,
                None,
                now,
                None,
            )
            .await
            .expect("insert cycle_event");

        // Insert an observation_phase_metrics row (protected table).
        // Must insert observation_metrics parent row first (FK constraint).
        {
            let mut conn = store.write_pool_server().acquire().await.expect("acquire");
            sqlx::query(
                "INSERT OR IGNORE INTO observation_metrics \
                 (feature_cycle, computed_at) \
                 VALUES ('protected-cycle', 1700000000)",
            )
            .execute(&mut *conn)
            .await
            .expect("insert observation_metrics parent");
            sqlx::query(
                "INSERT INTO observation_phase_metrics \
                 (feature_cycle, phase_name, duration_secs, tool_call_count) \
                 VALUES ('protected-cycle', 'design', 100, 1)",
            )
            .execute(&mut *conn)
            .await
            .expect("insert observation_phase_metrics");
        }

        // Snapshot protected table counts before GC.
        let entries_before = count_table(&store, "entries").await;
        let cycle_events_before = count_table(&store, "cycle_events").await;
        let cycle_review_before = count_table(&store, "cycle_review_index").await;
        let phase_metrics_before = count_table(&store, "observation_phase_metrics").await;

        // Add sessions + observations for purgeable cycles.
        for cycle in &["purgeable-1", "purgeable-2"] {
            let session_id = format!("{cycle}-sess");
            insert_session_direct(
                &store,
                &session_id,
                Some(cycle),
                SessionLifecycleStatus::Completed,
            )
            .await;
            insert_observation(&store, &session_id).await;
        }

        // Run full GC pass (K=1: only most recent retained).
        let (purgeable, _) = store
            .list_purgeable_cycles(1, 100)
            .await
            .expect("list_purgeable_cycles");
        assert_eq!(purgeable.len(), 2, "2 cycles purgeable with K=1");

        for cycle in &purgeable {
            store
                .gc_cycle_activity(cycle)
                .await
                .expect("gc_cycle_activity");
        }
        store
            .gc_unattributed_activity()
            .await
            .expect("gc_unattributed_activity");
        store.gc_audit_log(180).await.expect("gc_audit_log");

        // Protected tables must be unchanged.
        assert_eq!(
            count_table(&store, "entries").await,
            entries_before,
            "entries count must be unchanged after GC"
        );
        assert_eq!(
            count_table(&store, "cycle_events").await,
            cycle_events_before,
            "cycle_events count must be unchanged after GC"
        );
        assert_eq!(
            count_table(&store, "cycle_review_index").await,
            cycle_review_before,
            "cycle_review_index count must be unchanged after GC"
        );
        assert_eq!(
            count_table(&store, "observation_phase_metrics").await,
            phase_metrics_before,
            "observation_phase_metrics count must be unchanged after GC"
        );

        // Entry must still be retrievable.
        let retrieved = store.get(entry_id).await.expect("entry must still exist");
        assert_eq!(retrieved.id, entry_id, "protected entry survives GC");

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // test_gc_query_log_pruned_with_cycle (AC-07)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_gc_query_log_pruned_with_cycle() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // Cycle A: purgeable (older). Cycle B: retained (newer).
        insert_cycle_review(&store, "qlog-cycle-A", 1000).await;
        insert_cycle_review(&store, "qlog-cycle-B", 2000).await;

        insert_session_direct(
            &store,
            "sess-A",
            Some("qlog-cycle-A"),
            SessionLifecycleStatus::Completed,
        )
        .await;
        insert_session_direct(
            &store,
            "sess-B",
            Some("qlog-cycle-B"),
            SessionLifecycleStatus::Completed,
        )
        .await;

        insert_query_log_direct(&store, "sess-A").await;
        insert_query_log_direct(&store, "sess-A").await;
        insert_query_log_direct(&store, "sess-B").await;
        insert_query_log_direct(&store, "sess-B").await;
        insert_query_log_direct(&store, "sess-B").await;

        // GC cycle A only (K=1: retain only the newest).
        let stats = store
            .gc_cycle_activity("qlog-cycle-A")
            .await
            .expect("gc_cycle_activity");

        assert_eq!(stats.query_log_deleted, 2, "cycle A query_log rows deleted");
        assert_eq!(
            count_for_session(&store, "query_log", "sess-A").await,
            0,
            "sess-A query_log must be empty after GC"
        );
        assert_eq!(
            count_for_session(&store, "query_log", "sess-B").await,
            3,
            "sess-B query_log must be untouched"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // test_gc_cascade_delete_order (AC-08)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_gc_cascade_delete_order() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        insert_cycle_review(&store, "order-cycle", 1000).await;

        // 2 sessions with injection_log, observations, query_log.
        for s in 0..2usize {
            let session_id = format!("order-sess-{s}");
            insert_session_direct(
                &store,
                &session_id,
                Some("order-cycle"),
                SessionLifecycleStatus::Completed,
            )
            .await;
            insert_observation(&store, &session_id).await;
            insert_query_log_direct(&store, &session_id).await;
            insert_injection_log_direct(&store, &session_id).await;
        }

        // Part 1: correct order — gc_cycle_activity.
        let stats = store
            .gc_cycle_activity("order-cycle")
            .await
            .expect("gc_cycle_activity must succeed");

        assert_eq!(stats.sessions_deleted, 2, "2 sessions deleted");
        assert_eq!(
            stats.injection_log_deleted, 2,
            "2 injection_log rows deleted"
        );
        assert_eq!(stats.observations_deleted, 2, "2 observations deleted");
        assert_eq!(stats.query_log_deleted, 2, "2 query_log rows deleted");

        // No orphaned injection_log rows must remain.
        let orphaned: i64 = {
            let mut conn = store.write_pool_server().acquire().await.expect("acquire");
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM injection_log \
                 WHERE session_id NOT IN (SELECT session_id FROM sessions)",
            )
            .fetch_one(&mut *conn)
            .await
            .expect("orphan count")
        };
        assert_eq!(
            orphaned, 0,
            "no orphaned injection_log rows after correct-order GC (R-02 proof)"
        );

        // Part 2: mutation assertion — demonstrate that deleting sessions FIRST
        // would leave injection_log orphans. This uses a separate in-memory scenario.
        //
        // R-02 order enforcement proof: re-insert rows, then delete sessions before
        // injection_log (inverted order) and show injection_log rows survive.
        insert_cycle_review(&store, "inverted-order-cycle", 500).await;
        insert_session_direct(
            &store,
            "inv-sess",
            Some("inverted-order-cycle"),
            SessionLifecycleStatus::Completed,
        )
        .await;
        insert_injection_log_direct(&store, "inv-sess").await;

        // Delete sessions FIRST (wrong order — mutation test).
        {
            let mut conn = store.write_pool_server().acquire().await.expect("acquire");
            sqlx::query("DELETE FROM sessions WHERE feature_cycle = 'inverted-order-cycle'")
                .execute(&mut *conn)
                .await
                .expect("delete sessions");
        }

        // Now attempt injection_log delete via subquery — subquery resolves 0 rows
        // because sessions were already deleted.
        let orphaned_after_inversion: i64 = {
            let mut conn = store.write_pool_server().acquire().await.expect("acquire");
            // The subquery yields no session_ids since sessions for this cycle are gone,
            // so the injection_log row survives.
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM injection_log \
                 WHERE session_id NOT IN (SELECT session_id FROM sessions)",
            )
            .fetch_one(&mut *conn)
            .await
            .expect("orphan count")
        };
        assert!(
            orphaned_after_inversion > 0,
            "R-02 order enforcement proof: deleting sessions before injection_log leaves {} \
             orphaned injection_log rows (correct order prevents this)",
            orphaned_after_inversion
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // test_gc_unattributed_active_guard (AC-06)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_gc_unattributed_active_guard() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // Session A: feature_cycle = NULL, status = Active (0) — must NOT be deleted.
        insert_session_direct(
            &store,
            "unattr-active",
            None,
            SessionLifecycleStatus::Active,
        )
        .await;
        for _ in 0..3 {
            insert_observation(&store, "unattr-active").await;
        }

        // Session B: feature_cycle = NULL, status = Completed (non-zero) — must be deleted.
        insert_session_direct(
            &store,
            "unattr-closed",
            None,
            SessionLifecycleStatus::Completed,
        )
        .await;
        for _ in 0..3 {
            insert_observation(&store, "unattr-closed").await;
        }

        // Session C: feature_cycle = NULL, status = Completed, injection_log only.
        insert_session_direct(
            &store,
            "unattr-ilog-only",
            None,
            SessionLifecycleStatus::Completed,
        )
        .await;
        insert_injection_log_direct(&store, "unattr-ilog-only").await;

        let stats = store
            .gc_unattributed_activity()
            .await
            .expect("gc_unattributed_activity must not error");

        // Session A (Active): observations preserved, session preserved.
        assert_eq!(
            count_for_session(&store, "observations", "unattr-active").await,
            3,
            "Active session observations must not be deleted"
        );
        let active_sess = store
            .get_session("unattr-active")
            .await
            .expect("get_session must not error");
        assert!(
            active_sess.is_some(),
            "Active session must not be deleted by gc_unattributed_activity"
        );

        // Session B (Completed, unattributed): session deleted.
        // NOTE: Session B's observations are NOT deleted by gc_unattributed_activity
        // because step 1 only removes observations for sessions that do NOT exist in
        // the sessions table. Session B was deleted in step 4, so its observations
        // become orphaned — they will be cleaned up on the NEXT gc_unattributed_activity
        // call (step 1). This matches the architecture: the first pass deletes sessions,
        // and a subsequent pass removes their now-orphaned child rows.
        let closed_sess = store
            .get_session("unattr-closed")
            .await
            .expect("get_session must not error");
        assert!(
            closed_sess.is_none(),
            "Closed unattributed session must be deleted"
        );

        // Session C: injection_log deleted, session deleted.
        assert_eq!(
            count_for_session(&store, "injection_log", "unattr-ilog-only").await,
            0,
            "injection_log for unattributed closed session must be deleted"
        );
        let c_sess = store
            .get_session("unattr-ilog-only")
            .await
            .expect("get_session must not error");
        assert!(
            c_sess.is_none(),
            "Unattributed closed session C must be deleted"
        );

        // Stats sanity check: 2 sessions deleted (B + C), 1 injection_log deleted (C).
        // 0 observations deleted (B's observations become orphaned after session delete,
        // cleaned on next call).
        assert_eq!(stats.sessions_deleted, 2, "2 sessions deleted (B + C)");
        assert_eq!(
            stats.injection_log_deleted, 1,
            "1 injection_log deleted (from session C)"
        );

        // Run gc_unattributed_activity a second time — now session B's orphaned
        // observations should be deleted (session no longer in sessions table).
        let stats2 = store
            .gc_unattributed_activity()
            .await
            .expect("second gc_unattributed_activity must not error");

        assert_eq!(
            stats2.observations_deleted, 3,
            "session B's 3 orphaned observations deleted on second gc pass"
        );
        assert_eq!(
            count_for_session(&store, "observations", "unattr-closed").await,
            0,
            "session B observations gone after second gc pass"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // test_gc_audit_log_noop (replaces AC-09 retention_boundary test)
    //
    // gc_audit_log() is a no-op since vnc-014: the audit_log table is
    // append-only (schema v25 BEFORE DELETE trigger). The function returns
    // Ok(0) without deleting any rows regardless of retention_days.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_gc_audit_log_noop() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // Insert audit_log rows with timestamps that the old GC would have deleted.
        let insert_audit = |timestamp: i64| {
            let store_ref = &store;
            async move {
                let mut conn = store_ref
                    .write_pool_server()
                    .acquire()
                    .await
                    .expect("acquire");
                sqlx::query(
                    "INSERT INTO audit_log \
                     (event_id, timestamp, session_id, agent_id, operation, \
                      target_ids, outcome, detail) \
                     VALUES (?1, ?2, 'test-sess', 'test-agent', \
                             'context_store', '[]', 0, '')",
                )
                .bind(timestamp) // use timestamp as event_id for uniqueness
                .bind(timestamp)
                .execute(&mut *conn)
                .await
                .expect("insert audit_log");
            }
        };

        let ts_200d = now - (200 * 86400); // old row — would have been GC'd before vnc-014
        let ts_1d = now - 86400; // recent row

        insert_audit(ts_200d).await;
        insert_audit(ts_1d).await;

        let before = count_table(&store, "audit_log").await;
        assert_eq!(before, 2, "2 audit_log rows before gc_audit_log call");

        // gc_audit_log is a no-op — returns Ok(0), deletes nothing.
        let result = store.gc_audit_log(1).await;
        assert!(result.is_ok(), "gc_audit_log must not error: {result:?}");
        assert_eq!(result.unwrap(), 0, "gc_audit_log must return 0 (no-op)");

        // Both rows must still be present — no rows deleted.
        let after = count_table(&store, "audit_log").await;
        assert_eq!(after, 2, "both audit_log rows must be preserved (no-op)");

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // test_gc_protected_tables_row_level (AC-14)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_gc_protected_tables_row_level() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // Insert one named row in each protected table.
        // entries:
        let entry_id = store
            .insert(
                crate::test_helpers::TestEntry::new("protected-topic", "decision")
                    .with_title("protected-entry")
                    .build(),
            )
            .await
            .expect("insert entry");

        // cycle_events:
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        store
            .insert_cycle_event(
                "protected-cycle-event",
                0,
                "cycle_start",
                None,
                None,
                None,
                now,
                None,
            )
            .await
            .expect("insert cycle_event");

        // cycle_review_index: retained (K=1, only this one).
        insert_cycle_review(&store, "retained-only", 9999).await;

        // observation_phase_metrics (protected table — must not be GC'd).
        // Must insert observation_metrics parent row first (FK constraint).
        {
            let mut conn = store.write_pool_server().acquire().await.expect("acquire");
            sqlx::query(
                "INSERT OR IGNORE INTO observation_metrics \
                 (feature_cycle, computed_at) \
                 VALUES ('protected-metrics', 1700000000)",
            )
            .execute(&mut *conn)
            .await
            .expect("insert observation_metrics parent");
            sqlx::query(
                "INSERT INTO observation_phase_metrics \
                 (feature_cycle, phase_name, duration_secs, tool_call_count) \
                 VALUES ('protected-metrics', 'design', 200, 3)",
            )
            .execute(&mut *conn)
            .await
            .expect("insert observation_phase_metrics");
        }

        // Add 2 purgeable cycles (outside K=1 window, older than retained-only).
        insert_cycle_review(&store, "purgeable-row-A", 1000).await;
        insert_cycle_review(&store, "purgeable-row-B", 2000).await;

        for cycle in &["purgeable-row-A", "purgeable-row-B"] {
            let sess = format!("{cycle}-s");
            insert_session_direct(
                &store,
                &sess,
                Some(cycle),
                SessionLifecycleStatus::Completed,
            )
            .await;
            insert_observation(&store, &sess).await;
        }

        // Run full GC (K=1).
        let (purgeable, _) = store
            .list_purgeable_cycles(1, 100)
            .await
            .expect("list_purgeable_cycles");
        for cycle in &purgeable {
            store
                .gc_cycle_activity(cycle)
                .await
                .expect("gc_cycle_activity");
        }
        store
            .gc_unattributed_activity()
            .await
            .expect("gc_unattributed_activity");
        store.gc_audit_log(180).await.expect("gc_audit_log");

        // Each named protected row must still be retrievable.
        let retrieved_entry = store
            .get(entry_id)
            .await
            .expect("protected entry must still exist");
        assert_eq!(retrieved_entry.id, entry_id, "protected entry survives GC");

        let retained = store
            .get_cycle_review("retained-only")
            .await
            .expect("get_cycle_review must not error")
            .expect("retained cycle review must still exist");
        assert_eq!(
            retained.feature_cycle, "retained-only",
            "retained cycle review survives GC"
        );

        let metrics_count: i64 = {
            let mut conn = store.write_pool_server().acquire().await.expect("acquire");
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM observation_phase_metrics \
                 WHERE feature_cycle = 'protected-metrics'",
            )
            .fetch_one(&mut *conn)
            .await
            .expect("count")
        };
        assert_eq!(
            metrics_count, 1,
            "observation_phase_metrics row survives GC"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // test_gc_query_plan_uses_index (NFR-03 / R-09)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_gc_query_plan_uses_index() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        let pool = store.write_pool_server();

        // EXPLAIN QUERY PLAN for observations DELETE subquery.
        let obs_plan: Vec<String> = sqlx::query(
            "EXPLAIN QUERY PLAN \
             DELETE FROM observations WHERE session_id IN \
               (SELECT session_id FROM sessions WHERE feature_cycle = 'test-cycle')",
        )
        .fetch_all(pool)
        .await
        .expect("EXPLAIN QUERY PLAN must not error")
        .into_iter()
        .map(|row| {
            row.try_get::<String, _>("detail")
                .unwrap_or_else(|_| row.get::<String, _>(3))
        })
        .collect();

        let obs_plan_text = obs_plan.join(" ").to_lowercase();
        assert!(
            obs_plan_text.contains("idx_observations_session"),
            "full-table scan detected on observations; \
             idx_observations_session not used. Query plan: {obs_plan_text}"
        );

        // EXPLAIN QUERY PLAN for query_log DELETE subquery.
        let qlog_plan: Vec<String> = sqlx::query(
            "EXPLAIN QUERY PLAN \
             DELETE FROM query_log WHERE session_id IN \
               (SELECT session_id FROM sessions WHERE feature_cycle = 'test-cycle')",
        )
        .fetch_all(pool)
        .await
        .expect("EXPLAIN QUERY PLAN must not error")
        .into_iter()
        .map(|row| {
            row.try_get::<String, _>("detail")
                .unwrap_or_else(|_| row.get::<String, _>(3))
        })
        .collect();

        let qlog_plan_text = qlog_plan.join(" ").to_lowercase();
        assert!(
            qlog_plan_text.contains("idx_query_log_session"),
            "full-table scan detected on query_log; \
             idx_query_log_session not used. Query plan: {qlog_plan_text}"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // Edge case: list_purgeable_cycles with exactly K reviewed cycles
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_purgeable_cycles_exactly_k_returns_empty() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // Insert exactly 3 cycles, K = 3.
        insert_cycle_review(&store, "ec-1", 1000).await;
        insert_cycle_review(&store, "ec-2", 2000).await;
        insert_cycle_review(&store, "ec-3", 3000).await;

        let (purgeable, oldest_retained) = store
            .list_purgeable_cycles(3, 100)
            .await
            .expect("list_purgeable_cycles must not error");

        assert!(
            purgeable.is_empty(),
            "with exactly K=3 reviewed cycles, purgeable list must be empty"
        );
        // oldest_retained should be the K-th (last in DESC order = ec-1 at computed_at 1000).
        assert_eq!(
            oldest_retained,
            Some(1000),
            "oldest retained computed_at must be 1000 (K-th = oldest)"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // Edge case: list_purgeable_cycles with max_per_tick cap
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_purgeable_cycles_max_per_tick_cap() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // 25 reviewed cycles, K=5, max_per_tick=5.
        for i in 0..25usize {
            insert_cycle_review(&store, &format!("cap-cycle-{i:02}"), (i as i64 + 1) * 100).await;
        }

        let (purgeable, _) = store
            .list_purgeable_cycles(5, 5)
            .await
            .expect("list_purgeable_cycles must not error");

        assert_eq!(
            purgeable.len(),
            5,
            "max_per_tick=5 must cap result to exactly 5 cycles (AC-16)"
        );

        // Oldest 5 (cap-cycle-00 through cap-cycle-04) must be returned first.
        assert_eq!(purgeable[0], "cap-cycle-00", "oldest cycle first");
        assert_eq!(purgeable[4], "cap-cycle-04", "5th oldest last in batch");

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // Edge case: list_purgeable_cycles oldest_retained None when < K cycles
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_purgeable_cycles_oldest_retained_none_when_fewer_than_k() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // Only 2 cycles, K = 5.
        insert_cycle_review(&store, "few-1", 1000).await;
        insert_cycle_review(&store, "few-2", 2000).await;

        let (purgeable, oldest_retained) = store
            .list_purgeable_cycles(5, 100)
            .await
            .expect("list_purgeable_cycles must not error");

        assert!(
            purgeable.is_empty(),
            "fewer than K cycles — nothing purgeable"
        );
        assert_eq!(
            oldest_retained, None,
            "fewer than K cycles — oldest_retained must be None (R-16 boundary)"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // Edge case: gc_cycle_activity idempotency
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_gc_cycle_activity_idempotent() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        insert_cycle_review(&store, "idem-cycle", 1000).await;
        insert_session_direct(
            &store,
            "idem-sess",
            Some("idem-cycle"),
            SessionLifecycleStatus::Completed,
        )
        .await;
        insert_observation(&store, "idem-sess").await;
        insert_injection_log_direct(&store, "idem-sess").await;

        // First run: deletes rows.
        let stats1 = store
            .gc_cycle_activity("idem-cycle")
            .await
            .expect("first gc_cycle_activity must succeed");
        assert!(
            stats1.sessions_deleted > 0,
            "first run must delete sessions"
        );

        // Second run: all counts must be 0, no error (NFR-04, R-06).
        let stats2 = store
            .gc_cycle_activity("idem-cycle")
            .await
            .expect("second gc_cycle_activity must not error");
        assert_eq!(
            stats2.observations_deleted, 0,
            "second run: no observations to delete"
        );
        assert_eq!(
            stats2.query_log_deleted, 0,
            "second run: no query_log to delete"
        );
        assert_eq!(
            stats2.injection_log_deleted, 0,
            "second run: no injection_log to delete"
        );
        assert_eq!(
            stats2.sessions_deleted, 0,
            "second run: no sessions to delete"
        );

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // Edge case: gc_cycle_activity with zero observations is valid
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_gc_cycle_activity_zero_observations_ok() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        insert_cycle_review(&store, "empty-obs-cycle", 1000).await;
        insert_session_direct(
            &store,
            "empty-obs-sess",
            Some("empty-obs-cycle"),
            SessionLifecycleStatus::Completed,
        )
        .await;
        // No observations inserted.

        let stats = store
            .gc_cycle_activity("empty-obs-cycle")
            .await
            .expect("gc_cycle_activity must not fail with zero observations");

        assert_eq!(
            stats.observations_deleted, 0,
            "CycleGcStats.observations_deleted == 0 is valid"
        );
        assert_eq!(stats.sessions_deleted, 1, "session still deleted");

        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // Edge case: gc_audit_log with epoch timestamp row — no-op preserves it
    //
    // Previously this test verified the epoch row was deleted. Since vnc-014
    // gc_audit_log() is a no-op: the row is preserved, Ok(0) is returned.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_gc_audit_log_epoch_row_preserved() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;

        // Insert a row at timestamp 0 (Unix epoch).
        {
            let mut conn = store.write_pool_server().acquire().await.expect("acquire");
            sqlx::query(
                "INSERT INTO audit_log \
                 (event_id, timestamp, session_id, agent_id, operation, \
                  target_ids, outcome, detail) \
                 VALUES (999, 0, 'epoch-sess', 'epoch-agent', \
                         'context_store', '[]', 0, '')",
            )
            .execute(&mut *conn)
            .await
            .expect("insert epoch audit_log");
        }

        // gc_audit_log is a no-op — returns Ok(0), epoch row is NOT deleted.
        let result = store.gc_audit_log(1).await;
        assert!(result.is_ok(), "gc_audit_log must not error: {result:?}");
        assert_eq!(result.unwrap(), 0, "gc_audit_log must return 0 (no-op)");

        // Epoch row must still be present.
        let count = count_table(&store, "audit_log").await;
        assert_eq!(count, 1, "epoch audit_log row must be preserved (no-op)");

        store.close().await.unwrap();
    }
}
