//! Session lifecycle persistence for col-010.
//!
//! Provides CRUD operations on the sessions table and GC logic
//! with injection_log cascade deletion. All operations are async.

use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::db::{SqlxStore, map_pool_timeout};
use crate::error::{PoolKind, Result, StoreError};

// -- Constants --

/// Active sessions older than this are marked TimedOut during GC.
pub const TIMED_OUT_THRESHOLD_SECS: u64 = 24 * 3600;

/// Sessions older than this (any status) are deleted during GC.
pub const DELETE_THRESHOLD_SECS: u64 = 30 * 24 * 3600;

// -- Types --

/// Persistent lifecycle record for one agent session.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SessionRecord {
    pub session_id: String,
    pub feature_cycle: Option<String>,
    pub agent_role: Option<String>,
    /// Unix epoch seconds.
    pub started_at: u64,
    /// Set on SessionClose.
    pub ended_at: Option<u64>,
    pub status: SessionLifecycleStatus,
    /// Compaction events observed during this session.
    pub compaction_count: u32,
    /// "success" | "rework" | "abandoned"
    pub outcome: Option<String>,
    /// In-memory injection count at SessionClose.
    pub total_injections: u32,
    /// JSON array string of semantic keywords (col-022, ADR-003).
    #[serde(default)]
    pub keywords: Option<String>,
}

/// Session lifecycle phase.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SessionLifecycleStatus {
    Active = 0,
    Completed = 1,
    TimedOut = 2,
    Abandoned = 3,
}

impl TryFrom<u8> for SessionLifecycleStatus {
    type Error = StoreError;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Active),
            1 => Ok(Self::Completed),
            2 => Ok(Self::TimedOut),
            3 => Ok(Self::Abandoned),
            other => Err(StoreError::InvalidStatus(other)),
        }
    }
}

/// Statistics returned by `gc_sessions`.
#[derive(Debug, Default)]
pub struct GcStats {
    pub timed_out_count: u32,
    pub deleted_session_count: u32,
    pub deleted_injection_log_count: u32,
}

// -- Row helper --

fn session_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<SessionRecord> {
    Ok(SessionRecord {
        session_id: row
            .try_get("session_id")
            .map_err(|e| StoreError::Database(e.into()))?,
        feature_cycle: row
            .try_get("feature_cycle")
            .map_err(|e| StoreError::Database(e.into()))?,
        agent_role: row
            .try_get("agent_role")
            .map_err(|e| StoreError::Database(e.into()))?,
        started_at: row
            .try_get::<i64, _>("started_at")
            .map_err(|e| StoreError::Database(e.into()))? as u64,
        ended_at: row
            .try_get::<Option<i64>, _>("ended_at")
            .map_err(|e| StoreError::Database(e.into()))?
            .map(|v| v as u64),
        status: SessionLifecycleStatus::try_from(
            row.try_get::<i64, _>("status")
                .map_err(|e| StoreError::Database(e.into()))? as u8,
        )
        .unwrap_or(SessionLifecycleStatus::Active),
        compaction_count: row
            .try_get::<i64, _>("compaction_count")
            .map_err(|e| StoreError::Database(e.into()))? as u32,
        outcome: row
            .try_get("outcome")
            .map_err(|e| StoreError::Database(e.into()))?,
        total_injections: row
            .try_get::<i64, _>("total_injections")
            .map_err(|e| StoreError::Database(e.into()))? as u32,
        keywords: row
            .try_get("keywords")
            .map_err(|e| StoreError::Database(e.into()))?,
    })
}

// -- Store methods (sqlx backend) --

impl SqlxStore {
    /// Insert a new SessionRecord directly into the write pool.
    ///
    /// Uses `INSERT OR IGNORE` semantics — if a record with the same
    /// session_id already exists, the existing row is left fully intact and
    /// this call is a no-op. This makes SessionStart idempotent: a
    /// context-compaction resume re-fires the hook with the same stable
    /// session_id, and the re-fire must NOT overwrite accumulated state (#300).
    /// In particular `started_at` is write-once (no path ever rewrites it), so
    /// the original session-age/attribution window is preserved.
    ///
    /// Columns that legitimately change on a live session are written via the
    /// separate `update_session` UPDATE path, never here: `status`/`ended_at`/
    /// `outcome`/`total_injections`/`compaction_count` at SessionClose, and
    /// `feature_cycle` by the cycle path (`update_session_feature_cycle`).
    /// `feature_cycle` is therefore owned-by-the-cycle-path (mutable-by-owner),
    /// NOT immutable — IGNORE only prevents SessionStart from clobbering it.
    ///
    /// Writes directly (not via analytics drain) to ensure immediate read
    /// visibility. Session records are read immediately after insert by
    /// callers that need to verify or update them (e.g. `dispatch_cycle_start`).
    pub async fn insert_session(&self, record: &SessionRecord) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO sessions (
                session_id, feature_cycle, agent_role, started_at, ended_at,
                status, compaction_count, outcome, total_injections, keywords
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(&record.session_id)
        .bind(&record.feature_cycle)
        .bind(&record.agent_role)
        .bind(record.started_at as i64)
        .bind(record.ended_at.map(|v| v as i64))
        .bind(record.status as u8 as i64)
        .bind(record.compaction_count as i64)
        .bind(&record.outcome)
        .bind(record.total_injections as i64)
        .bind(&record.keywords)
        .execute(&self.write_pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Read-modify-write a SessionRecord (write via write_pool, then enqueue update).
    ///
    /// Returns `StoreError::Deserialization` if the record is not found.
    pub async fn update_session(
        &self,
        session_id: &str,
        updater: impl FnOnce(&mut SessionRecord),
    ) -> Result<()> {
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        let row = sqlx::query(
            "SELECT session_id, feature_cycle, agent_role, started_at, ended_at, \
                    status, compaction_count, outcome, total_injections, keywords \
             FROM sessions WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_optional(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?
        .ok_or_else(|| StoreError::Deserialization(format!("session not found: {session_id}")))?;

        let mut record = session_from_row(&row)?;
        updater(&mut record);

        sqlx::query(
            "UPDATE sessions SET feature_cycle = ?1, agent_role = ?2,
                started_at = ?3, ended_at = ?4, status = ?5,
                compaction_count = ?6, outcome = ?7, total_injections = ?8,
                keywords = ?9
             WHERE session_id = ?10",
        )
        .bind(&record.feature_cycle)
        .bind(&record.agent_role)
        .bind(record.started_at as i64)
        .bind(record.ended_at.map(|v| v as i64))
        .bind(record.status as u8 as i64)
        .bind(record.compaction_count as i64)
        .bind(&record.outcome)
        .bind(record.total_injections as i64)
        .bind(&record.keywords)
        .bind(&record.session_id)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Retrieve a single SessionRecord by session_id.
    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionRecord>> {
        let row = sqlx::query(
            "SELECT session_id, feature_cycle, agent_role, started_at, ended_at, \
                    status, compaction_count, outcome, total_injections, keywords \
             FROM sessions WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_optional(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        match row {
            Some(r) => Ok(Some(session_from_row(&r)?)),
            None => Ok(None),
        }
    }

    /// Query all sessions for a given feature_cycle using the indexed column.
    pub async fn scan_sessions_by_feature(
        &self,
        feature_cycle: &str,
    ) -> Result<Vec<SessionRecord>> {
        let rows = sqlx::query(
            "SELECT session_id, feature_cycle, agent_role, started_at, ended_at, \
                    status, compaction_count, outcome, total_injections, keywords \
             FROM sessions WHERE feature_cycle = ?1",
        )
        .bind(feature_cycle)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        rows.iter().map(session_from_row).collect()
    }

    /// Query sessions for a feature_cycle, optionally filtering by status.
    pub async fn scan_sessions_by_feature_with_status(
        &self,
        feature_cycle: &str,
        status_filter: Option<SessionLifecycleStatus>,
    ) -> Result<Vec<SessionRecord>> {
        let rows = match status_filter {
            None => sqlx::query(
                "SELECT session_id, feature_cycle, agent_role, started_at, ended_at, \
                        status, compaction_count, outcome, total_injections, keywords \
                 FROM sessions WHERE feature_cycle = ?1",
            )
            .bind(feature_cycle)
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?,
            Some(status) => sqlx::query(
                "SELECT session_id, feature_cycle, agent_role, started_at, ended_at, \
                        status, compaction_count, outcome, total_injections, keywords \
                 FROM sessions WHERE feature_cycle = ?1 AND status = ?2",
            )
            .bind(feature_cycle)
            .bind(status as u8 as i64)
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?,
        };

        rows.iter().map(session_from_row).collect()
    }

    /// Update only the `keywords` column for a given session (analytics write).
    ///
    /// Used by the UDS listener to persist keywords without read-modify-write overhead.
    /// No-op if the session does not exist.
    pub async fn update_session_keywords(
        &self,
        session_id: &str,
        keywords_json: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE sessions SET keywords = ?1 WHERE session_id = ?2")
            .bind(keywords_json)
            .bind(session_id)
            .execute(&self.write_pool)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// GC sweep: mark old Active sessions as TimedOut; delete very old sessions.
    ///
    /// All phases run in one transaction for atomicity. Uses write_pool directly
    /// because GC modifies persistent state (not analytics).
    pub async fn gc_sessions(
        &self,
        timed_out_threshold_secs: u64,
        delete_threshold_secs: u64,
    ) -> Result<GcStats> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let timed_out_boundary = now.saturating_sub(timed_out_threshold_secs);
        let delete_boundary = now.saturating_sub(delete_threshold_secs);

        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        // Phase 1: Delete injection_log for sessions being deleted
        let deleted_injection_log = sqlx::query(
            "DELETE FROM injection_log WHERE session_id IN (\
                SELECT session_id FROM sessions WHERE started_at < ?1\
            )",
        )
        .bind(delete_boundary as i64)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Phase 2: Delete old sessions
        let deleted_sessions = sqlx::query("DELETE FROM sessions WHERE started_at < ?1")
            .bind(delete_boundary as i64)
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        // Phase 3: Mark timed-out Active sessions
        let timed_out =
            sqlx::query("UPDATE sessions SET status = ?1 WHERE status = 0 AND started_at < ?2")
                .bind(SessionLifecycleStatus::TimedOut as u8 as i64)
                .bind(timed_out_boundary as i64)
                .execute(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        Ok(GcStats {
            deleted_injection_log_count: deleted_injection_log.rows_affected() as u32,
            deleted_session_count: deleted_sessions.rows_affected() as u32,
            timed_out_count: timed_out.rows_affected() as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::open_test_store;

    /// Inception record as built by the SessionStart hook arm: all
    /// non-identity columns are default/inception values.
    fn inception_record(session_id: &str, started_at: u64) -> SessionRecord {
        SessionRecord {
            session_id: session_id.to_string(),
            feature_cycle: None,
            agent_role: Some("delivery".to_string()),
            started_at,
            ended_at: None,
            status: SessionLifecycleStatus::Active,
            compaction_count: 0,
            outcome: None,
            total_injections: 0,
            keywords: None,
        }
    }

    /// First SessionStart creates the row exactly as supplied.
    #[tokio::test]
    async fn test_insert_session_first_insert_creates_row() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = open_test_store(&dir).await;

        let rec = inception_record("sess-first", 1_700_000_000);
        store.insert_session(&rec).await.expect("first insert");

        let got = store
            .get_session("sess-first")
            .await
            .expect("get_session")
            .expect("row exists");
        assert_eq!(got, rec, "first insert stores the record verbatim");
    }

    /// #300: a second `insert_session` for an EXISTING session_id (compaction
    /// resume re-fire) must be a no-op — `started_at` is write-once and must
    /// NOT be reset to the resume time.
    #[tokio::test]
    async fn test_insert_session_resume_preserves_started_at() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = open_test_store(&dir).await;

        let t0 = 1_700_000_000;
        store
            .insert_session(&inception_record("sess-resume", t0))
            .await
            .expect("first insert");

        // Compaction resume: same session_id, later started_at candidate.
        let t1 = t0 + 9_999;
        store
            .insert_session(&inception_record("sess-resume", t1))
            .await
            .expect("resume re-fire (IGNORE no-op)");

        let got = store
            .get_session("sess-resume")
            .await
            .expect("get_session")
            .expect("row exists");
        assert_eq!(
            got.started_at, t0,
            "started_at is write-once: resume must not reset it to t1"
        );
    }

    /// #300: a `Declared` feature_cycle set by the cycle path (an UPDATE,
    /// modeled here via `update_session`) AFTER the first insert must SURVIVE
    /// a subsequent SessionStart re-fire that carries feature_cycle = None.
    #[tokio::test]
    async fn test_insert_session_resume_preserves_declared_feature_cycle() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = open_test_store(&dir).await;

        let t0 = 1_700_000_000;
        store
            .insert_session(&inception_record("sess-fc", t0))
            .await
            .expect("first insert");

        // Cycle path declares the feature via the UPDATE path (owner-written).
        store
            .update_session("sess-fc", |r| {
                r.feature_cycle = Some("col-300".to_string());
            })
            .await
            .expect("cycle-path UPDATE");

        // Compaction resume re-fire with no feature (inception default None).
        store
            .insert_session(&inception_record("sess-fc", t0 + 50))
            .await
            .expect("resume re-fire (IGNORE no-op)");

        let got = store
            .get_session("sess-fc")
            .await
            .expect("get_session")
            .expect("row exists");
        assert_eq!(
            got.feature_cycle.as_deref(),
            Some("col-300"),
            "Declared feature_cycle must survive the SessionStart re-fire"
        );
        assert_eq!(got.started_at, t0, "started_at still write-once");
    }

    /// #300 NB-1: a Completed/closed row (status + ended_at set via the close
    /// UPDATE path) must NOT be revived to Active by a SessionStart re-fire.
    #[tokio::test]
    async fn test_insert_session_resume_does_not_revive_closed_row() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = open_test_store(&dir).await;

        let t0 = 1_700_000_000;
        store
            .insert_session(&inception_record("sess-closed", t0))
            .await
            .expect("first insert");

        // SessionClose: mark Completed with an ended_at and an outcome.
        let closed_at = t0 + 100;
        store
            .update_session("sess-closed", |r| {
                r.status = SessionLifecycleStatus::Completed;
                r.ended_at = Some(closed_at);
                r.outcome = Some("success".to_string());
            })
            .await
            .expect("close UPDATE");

        // Resume re-fire carries status = Active, ended_at = None.
        store
            .insert_session(&inception_record("sess-closed", t0 + 200))
            .await
            .expect("resume re-fire (IGNORE no-op)");

        let got = store
            .get_session("sess-closed")
            .await
            .expect("get_session")
            .expect("row exists");
        assert_eq!(
            got.status,
            SessionLifecycleStatus::Completed,
            "closed row must NOT be revived to Active"
        );
        assert_eq!(
            got.ended_at,
            Some(closed_at),
            "ended_at must stay intact (no zombie revival)"
        );
        assert_eq!(got.outcome.as_deref(), Some("success"));
    }

    /// #300 NB-3: a persisted compaction_count set on the row must not be
    /// zeroed by a SessionStart re-fire (which always carries 0).
    #[tokio::test]
    async fn test_insert_session_resume_does_not_zero_compaction_count() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = open_test_store(&dir).await;

        let t0 = 1_700_000_000;
        store
            .insert_session(&inception_record("sess-cc", t0))
            .await
            .expect("first insert");

        store
            .update_session("sess-cc", |r| {
                r.compaction_count = 7;
            })
            .await
            .expect("set compaction_count");

        store
            .insert_session(&inception_record("sess-cc", t0 + 5))
            .await
            .expect("resume re-fire (IGNORE no-op)");

        let got = store
            .get_session("sess-cc")
            .await
            .expect("get_session")
            .expect("row exists");
        assert_eq!(
            got.compaction_count, 7,
            "compaction_count must not be zeroed by the re-fire"
        );
    }
}
