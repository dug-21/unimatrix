//! Session lifecycle persistence for col-010.
//!
//! Provides CRUD operations on the sessions table and GC logic
//! with injection_log cascade deletion. All operations are synchronous;
//! callers in async contexts must use `tokio::task::spawn_blocking`.

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::db::Store;
use crate::error::{Result, StoreError};

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
    /// In-memory injection count at SessionClose (OQ-01: fire-and-forget discrepancy accepted).
    pub total_injections: u32,
}

/// Session lifecycle phase.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SessionLifecycleStatus {
    /// Session is ongoing.
    Active = 0,
    /// Session closed normally (Success or Rework outcome).
    Completed = 1,
    /// Session was active for > 24h; marked by GC sweep.
    TimedOut = 2,
    /// Session was explicitly abandoned (ADR-001).
    /// Excluded from retrospective metric computation.
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

fn session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        session_id: row.get("session_id")?,
        feature_cycle: row.get("feature_cycle")?,
        agent_role: row.get("agent_role")?,
        started_at: row.get::<_, i64>("started_at")? as u64,
        ended_at: row.get::<_, Option<i64>>("ended_at")?.map(|v| v as u64),
        status: SessionLifecycleStatus::try_from(row.get::<_, i64>("status")? as u8)
            .unwrap_or(SessionLifecycleStatus::Active),
        compaction_count: row.get::<_, i64>("compaction_count")? as u32,
        outcome: row.get("outcome")?,
        total_injections: row.get::<_, i64>("total_injections")? as u32,
    })
}

const SESSION_COLUMNS: &str =
    "session_id, feature_cycle, agent_role, started_at, ended_at, \
     status, compaction_count, outcome, total_injections";

// -- Store methods (SQLite backend) --

impl Store {
    /// Insert a new SessionRecord into sessions.
    ///
    /// If a record with the same session_id already exists, it is overwritten
    /// (INSERT OR REPLACE semantics).
    pub fn insert_session(&self, record: &SessionRecord) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT OR REPLACE INTO sessions (session_id, feature_cycle, agent_role,
                started_at, ended_at, status, compaction_count, outcome, total_injections)
             VALUES (:sid, :fc, :ar, :sa, :ea, :st, :cc, :oc, :ti)",
            rusqlite::named_params! {
                ":sid": &record.session_id,
                ":fc": &record.feature_cycle,
                ":ar": &record.agent_role,
                ":sa": record.started_at as i64,
                ":ea": record.ended_at.map(|v| v as i64),
                ":st": record.status as u8 as i64,
                ":cc": record.compaction_count as i64,
                ":oc": &record.outcome,
                ":ti": record.total_injections as i64,
            },
        )
        .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    /// Read-modify-write a SessionRecord.
    ///
    /// Returns `StoreError::Deserialization` if the record is not found.
    pub fn update_session(
        &self,
        session_id: &str,
        updater: impl FnOnce(&mut SessionRecord),
    ) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<()> {
            let mut record: SessionRecord = conn
                .query_row(
                    &format!("SELECT {} FROM sessions WHERE session_id = ?1", SESSION_COLUMNS),
                    rusqlite::params![session_id],
                    session_from_row,
                )
                .optional()
                .map_err(StoreError::Sqlite)?
                .ok_or_else(|| {
                    StoreError::Deserialization(format!("session not found: {session_id}"))
                })?;

            updater(&mut record);

            conn.execute(
                "UPDATE sessions SET feature_cycle = :fc, agent_role = :ar,
                    started_at = :sa, ended_at = :ea, status = :st,
                    compaction_count = :cc, outcome = :oc, total_injections = :ti
                 WHERE session_id = :sid",
                rusqlite::named_params! {
                    ":sid": &record.session_id,
                    ":fc": &record.feature_cycle,
                    ":ar": &record.agent_role,
                    ":sa": record.started_at as i64,
                    ":ea": record.ended_at.map(|v| v as i64),
                    ":st": record.status as u8 as i64,
                    ":cc": record.compaction_count as i64,
                    ":oc": &record.outcome,
                    ":ti": record.total_injections as i64,
                },
            )
            .map_err(StoreError::Sqlite)?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")
                    .map_err(StoreError::Sqlite)?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Retrieve a single SessionRecord by session_id.
    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionRecord>> {
        let conn = self.lock_conn();
        conn.query_row(
            &format!(
                "SELECT {} FROM sessions WHERE session_id = ?1",
                SESSION_COLUMNS
            ),
            rusqlite::params![session_id],
            session_from_row,
        )
        .optional()
        .map_err(StoreError::Sqlite)
    }

    /// Query all sessions for a given feature_cycle using the indexed column.
    pub fn scan_sessions_by_feature(&self, feature_cycle: &str) -> Result<Vec<SessionRecord>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {} FROM sessions WHERE feature_cycle = ?1",
                SESSION_COLUMNS
            ))
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![feature_cycle], session_from_row)
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }

    /// Query sessions for a feature_cycle, optionally filtering by status.
    pub fn scan_sessions_by_feature_with_status(
        &self,
        feature_cycle: &str,
        status_filter: Option<SessionLifecycleStatus>,
    ) -> Result<Vec<SessionRecord>> {
        let conn = self.lock_conn();
        let mut stmt = match status_filter {
            None => conn
                .prepare(&format!(
                    "SELECT {} FROM sessions WHERE feature_cycle = ?1",
                    SESSION_COLUMNS
                ))
                .map_err(StoreError::Sqlite)?,
            Some(_) => conn
                .prepare(&format!(
                    "SELECT {} FROM sessions WHERE feature_cycle = ?1 AND status = ?2",
                    SESSION_COLUMNS
                ))
                .map_err(StoreError::Sqlite)?,
        };
        let rows = match status_filter {
            None => stmt
                .query_map(rusqlite::params![feature_cycle], session_from_row)
                .map_err(StoreError::Sqlite)?,
            Some(status) => stmt
                .query_map(
                    rusqlite::params![feature_cycle, status as u8 as i64],
                    session_from_row,
                )
                .map_err(StoreError::Sqlite)?,
        };
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }

    /// GC sweep: mark old Active sessions as TimedOut; delete very old sessions
    /// with their injection_log records.
    ///
    /// All phases run in one transaction for atomicity.
    pub fn gc_sessions(
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

        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<GcStats> {
            // Phase 1: Delete injection_log for sessions being deleted
            let deleted_injection_log_count = conn
                .execute(
                    "DELETE FROM injection_log WHERE session_id IN (\
                        SELECT session_id FROM sessions WHERE started_at < ?1\
                    )",
                    rusqlite::params![delete_boundary as i64],
                )
                .map_err(StoreError::Sqlite)? as u32;

            // Phase 2: Delete old sessions
            let deleted_session_count = conn
                .execute(
                    "DELETE FROM sessions WHERE started_at < ?1",
                    rusqlite::params![delete_boundary as i64],
                )
                .map_err(StoreError::Sqlite)? as u32;

            // Phase 3: Mark timed-out Active sessions
            let timed_out_count = conn
                .execute(
                    "UPDATE sessions SET status = ?1 WHERE status = 0 AND started_at < ?2",
                    rusqlite::params![
                        SessionLifecycleStatus::TimedOut as u8 as i64,
                        timed_out_boundary as i64
                    ],
                )
                .map_err(StoreError::Sqlite)? as u32;

            Ok(GcStats {
                deleted_injection_log_count,
                deleted_session_count,
                timed_out_count,
            })
        })();

        match result {
            Ok(stats) => {
                conn.execute_batch("COMMIT")
                    .map_err(StoreError::Sqlite)?;
                Ok(stats)
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
}
