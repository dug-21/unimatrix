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
    /// Uses `INSERT OR REPLACE` semantics — if a record with the same
    /// session_id already exists, it is fully overwritten.
    ///
    /// Writes directly (not via analytics drain) to ensure immediate read
    /// visibility. Session records are read immediately after insert by
    /// callers that need to verify or update them (e.g. `dispatch_cycle_start`).
    pub async fn insert_session(&self, record: &SessionRecord) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO sessions (
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
