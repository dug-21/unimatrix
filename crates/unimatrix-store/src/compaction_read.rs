//! Read-only accessors for crt-054's `compaction_events` table (crt-055 Component 5).
//!
//! crt-054 owns the table, its writer ([`crate::SqlxStore::insert_compaction_event`]),
//! and the `idx_compaction_events_session` index. crt-055 only READS — these accessors
//! never alter the schema or rows. All reads go through `read_pool()` with parameterized
//! binds, so `session_id` is bound as data (no SQL-injection surface; the
//! `sqlite_parity` test already proves injection-safety on parameterized binds).
//!
//! ## Unit contract (ADR-006 #5048, binding)
//! `compacted_at` is stored verbatim in **Unix SECONDS** and read verbatim here — there
//! is NO unit conversion in the store. The gate-side `÷1000` normalization of the
//! PostToolUse read `ts` (epoch millis → seconds) belongs to the consumer reckoning
//! (`unimatrix-observe`), NOT to these accessors. `high_water` is reserved (ADR-006) and
//! is never read by crt-055 v1.

use sqlx::Row;

use crate::db::SqlxStore;
use crate::error::{Result, StoreError};

impl SqlxStore {
    /// `MIN(compacted_at)` for a session — the earliest compaction boundary (ADR-006).
    ///
    /// Returns `None` when the session has no `compaction_events` rows (no boundary to
    /// gate against), distinct from a session whose earliest boundary is `0`. Seconds,
    /// read verbatim (no conversion).
    pub async fn min_compacted_at(&self, session_id: &str) -> Result<Option<i64>> {
        // SQLite `MIN()` over zero rows yields one row with a NULL cell; map NULL → None.
        let row: (Option<i64>,) =
            sqlx::query_as("SELECT MIN(compacted_at) FROM compaction_events WHERE session_id = ?1")
                .bind(session_id)
                .fetch_one(self.read_pool())
                .await
                .map_err(|e| StoreError::Database(e.into()))?;
        Ok(row.0)
    }

    /// All `compacted_at` boundaries for a session, ascending (ARCHITECTURE §6 shape).
    ///
    /// Returns an empty `Vec` when the session has no rows. Each value is Unix seconds,
    /// read verbatim. `high_water` is NOT selected (reserved, ADR-006).
    pub async fn compaction_boundaries(&self, session_id: &str) -> Result<Vec<i64>> {
        let rows: Vec<(i64,)> = sqlx::query_as(
            "SELECT compacted_at FROM compaction_events WHERE session_id = ?1 \
             ORDER BY compacted_at ASC",
        )
        .bind(session_id)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;
        Ok(rows.into_iter().map(|(c,)| c).collect())
    }

    /// COUNT of `compaction_events` rows attributed to a set of declared session ids —
    /// the `compaction_count` source (ADR-005).
    ///
    /// `session_ids` is the cycle's DECLARED sessions (resolved via the
    /// session→`feature_cycle` chain at review). Undeclared / evicted sessions (#4140)
    /// are NOT in this list, so their rows do not mis-attribute (R-05). An empty list
    /// returns `0` without touching the DB (no all-rows scan).
    pub async fn compaction_count_for_sessions(&self, session_ids: &[String]) -> Result<i64> {
        if session_ids.is_empty() {
            return Ok(0);
        }
        // Build the IN clause via repeated positional binds (no string interpolation of
        // ids — parameterized, injection-safe), matching `load_observations_for_sessions`.
        let placeholders = session_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql =
            format!("SELECT COUNT(*) FROM compaction_events WHERE session_id IN ({placeholders})");
        let mut q = sqlx::query(&sql);
        for sid in session_ids {
            q = q.bind(sid);
        }
        let row = q
            .fetch_one(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(row.get::<i64, _>(0))
    }
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::open_test_store;

    // ---- Read accessor (R-12) ----

    #[tokio::test]
    async fn test_compaction_events_read_orders_by_compacted_at_asc() {
        let dir = tempfile::tempdir().unwrap();
        let store = open_test_store(&dir).await;
        // Insert out of order; the accessor must return ascending seconds.
        store.insert_compaction_event("s1", 300, 0).await.unwrap();
        store.insert_compaction_event("s1", 100, 0).await.unwrap();
        store.insert_compaction_event("s1", 200, 0).await.unwrap();
        let boundaries = store.compaction_boundaries("s1").await.unwrap();
        assert_eq!(boundaries, vec![100, 200, 300]);
    }

    #[tokio::test]
    async fn test_compaction_boundaries_empty_session_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = open_test_store(&dir).await;
        assert_eq!(
            store.compaction_boundaries("none").await.unwrap(),
            Vec::<i64>::new()
        );
    }

    #[tokio::test]
    async fn test_min_compacted_at_returns_earliest() {
        let dir = tempfile::tempdir().unwrap();
        let store = open_test_store(&dir).await;
        store.insert_compaction_event("s1", 500, 0).await.unwrap();
        store.insert_compaction_event("s1", 120, 0).await.unwrap();
        store.insert_compaction_event("s1", 999, 0).await.unwrap();
        assert_eq!(store.min_compacted_at("s1").await.unwrap(), Some(120));
    }

    #[tokio::test]
    async fn test_min_compacted_at_no_rows_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = open_test_store(&dir).await;
        // No rows → None (distinct from an earliest boundary of 0).
        assert_eq!(store.min_compacted_at("absent").await.unwrap(), None);
    }

    // ---- compaction_count attribution (R-05, AC-11) ----

    #[tokio::test]
    async fn test_compaction_count_counts_attributed_rows() {
        let dir = tempfile::tempdir().unwrap();
        let store = open_test_store(&dir).await;
        store.insert_compaction_event("s1", 100, 0).await.unwrap();
        store.insert_compaction_event("s1", 200, 0).await.unwrap();
        store.insert_compaction_event("s2", 100, 0).await.unwrap();
        let count = store
            .compaction_count_for_sessions(&["s1".to_string(), "s2".to_string()])
            .await
            .unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_compaction_count_undeclared_session_excluded() {
        // #4140: an undeclared/evicted session's rows must NOT count toward the cycle.
        let dir = tempfile::tempdir().unwrap();
        let store = open_test_store(&dir).await;
        store
            .insert_compaction_event("declared", 100, 0)
            .await
            .unwrap();
        store
            .insert_compaction_event("evicted", 100, 0)
            .await
            .unwrap();
        store
            .insert_compaction_event("evicted", 200, 0)
            .await
            .unwrap();
        // Only the declared session is in the list → evicted rows excluded.
        let count = store
            .compaction_count_for_sessions(&["declared".to_string()])
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_compaction_count_empty_list_is_zero() {
        // No declared sessions (honest partial) → 0 without a DB all-rows scan.
        let dir = tempfile::tempdir().unwrap();
        let store = open_test_store(&dir).await;
        store
            .insert_compaction_event("orphan", 100, 0)
            .await
            .unwrap();
        assert_eq!(store.compaction_count_for_sessions(&[]).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_compaction_read_sql_injection_guard() {
        // A malicious session_id is bound as data (parameterized); it matches nothing and
        // does not execute as SQL.
        let dir = tempfile::tempdir().unwrap();
        let store = open_test_store(&dir).await;
        store.insert_compaction_event("s1", 100, 0).await.unwrap();
        let malicious = "s1'; DROP TABLE compaction_events;--";
        assert_eq!(
            store.compaction_boundaries(malicious).await.unwrap(),
            Vec::<i64>::new()
        );
        assert_eq!(store.min_compacted_at(malicious).await.unwrap(), None);
        // The table still exists and the legit row is intact.
        assert_eq!(store.compaction_boundaries("s1").await.unwrap(), vec![100]);
    }
}
