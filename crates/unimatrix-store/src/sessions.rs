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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum SessionLifecycleStatus {
    /// Session is ongoing.
    Active,
    /// Session closed normally (Success or Rework outcome).
    Completed,
    /// Session was active for > 24h; marked by GC sweep.
    TimedOut,
    /// Session was explicitly abandoned (ADR-001).
    /// Excluded from retrospective metric computation.
    Abandoned,
}

/// Statistics returned by `gc_sessions`.
#[derive(Debug, Default)]
pub struct GcStats {
    pub timed_out_count: u32,
    pub deleted_session_count: u32,
    pub deleted_injection_log_count: u32,
}

// -- Serialization helpers --

fn serialize_session(record: &SessionRecord) -> Result<Vec<u8>> {
    bincode::serde::encode_to_vec(record, bincode::config::standard())
        .map_err(|e| StoreError::Serialization(e.to_string()))
}

fn deserialize_session(bytes: &[u8]) -> Result<SessionRecord> {
    let (record, _) =
        bincode::serde::decode_from_slice::<SessionRecord, _>(bytes, bincode::config::standard())
            .map_err(|e| StoreError::Deserialization(e.to_string()))?;
    Ok(record)
}

// -- Store methods (SQLite backend) --

impl Store {
    /// Insert a new SessionRecord into sessions.
    ///
    /// If a record with the same session_id already exists, it is overwritten
    /// (INSERT OR REPLACE semantics).
    pub fn insert_session(&self, record: &SessionRecord) -> Result<()> {
        let bytes = serialize_session(record)?;
        let conn = self.lock_conn();
        conn.execute(
            "INSERT OR REPLACE INTO sessions (session_id, data) VALUES (?1, ?2)",
            rusqlite::params![&record.session_id, bytes],
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
            let bytes: Option<Vec<u8>> = conn
                .query_row(
                    "SELECT data FROM sessions WHERE session_id = ?1",
                    rusqlite::params![session_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(StoreError::Sqlite)?;

            match bytes {
                None => {
                    return Err(StoreError::Deserialization(format!(
                        "session not found: {session_id}"
                    )));
                }
                Some(bytes) => {
                    let mut record = deserialize_session(&bytes)?;
                    updater(&mut record);
                    let updated_bytes = serialize_session(&record)?;
                    conn.execute(
                        "UPDATE sessions SET data = ?1 WHERE session_id = ?2",
                        rusqlite::params![updated_bytes, session_id],
                    )
                    .map_err(StoreError::Sqlite)?;
                }
            }
            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?;
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
        let bytes: Option<Vec<u8>> = conn
            .query_row(
                "SELECT data FROM sessions WHERE session_id = ?1",
                rusqlite::params![session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::Sqlite)?;
        match bytes {
            None => Ok(None),
            Some(bytes) => Ok(Some(deserialize_session(&bytes)?)),
        }
    }

    /// Scan all sessions for a given feature_cycle.
    ///
    /// Full table scan + in-process filter. Acceptable at current volumes.
    pub fn scan_sessions_by_feature(&self, feature_cycle: &str) -> Result<Vec<SessionRecord>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare("SELECT data FROM sessions")
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            let bytes = row.map_err(StoreError::Sqlite)?;
            let record = deserialize_session(&bytes)?;
            if record.feature_cycle.as_deref() == Some(feature_cycle) {
                results.push(record);
            }
        }
        Ok(results)
    }

    /// Scan sessions for a feature_cycle, optionally filtering by status.
    pub fn scan_sessions_by_feature_with_status(
        &self,
        feature_cycle: &str,
        status_filter: Option<SessionLifecycleStatus>,
    ) -> Result<Vec<SessionRecord>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare("SELECT data FROM sessions")
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            let bytes = row.map_err(StoreError::Sqlite)?;
            let record = deserialize_session(&bytes)?;
            if record.feature_cycle.as_deref() != Some(feature_cycle) {
                continue;
            }
            match &status_filter {
                None => results.push(record),
                Some(filter) => {
                    if &record.status == filter {
                        results.push(record);
                    }
                }
            }
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
            let mut stats = GcStats::default();

            // Phase 1: collect session_ids to delete (started_at < delete_boundary)
            let sessions_to_delete: Vec<String> = {
                let mut stmt = conn
                    .prepare("SELECT data FROM sessions")
                    .map_err(StoreError::Sqlite)?;
                let rows = stmt
                    .query_map([], |row| row.get::<_, Vec<u8>>(0))
                    .map_err(StoreError::Sqlite)?;
                let mut to_delete = Vec::new();
                for row in rows {
                    let bytes = row.map_err(StoreError::Sqlite)?;
                    let record = deserialize_session(&bytes)?;
                    if record.started_at < delete_boundary {
                        to_delete.push(record.session_id);
                    }
                }
                to_delete
            };

            // Phase 2: collect log_ids whose session_id is in the deletion set
            let log_ids_to_delete: Vec<i64> = {
                let mut stmt = conn
                    .prepare("SELECT log_id, data FROM injection_log")
                    .map_err(StoreError::Sqlite)?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
                    })
                    .map_err(StoreError::Sqlite)?;
                let mut to_delete = Vec::new();
                for row in rows {
                    let (log_id, bytes) = row.map_err(StoreError::Sqlite)?;
                    let record = crate::injection_log::deserialize_injection_log(&bytes)?;
                    if sessions_to_delete.contains(&record.session_id) {
                        to_delete.push(log_id);
                    }
                }
                to_delete
            };

            // Phase 3: delete injection_log entries
            for log_id in &log_ids_to_delete {
                conn.execute(
                    "DELETE FROM injection_log WHERE log_id = ?1",
                    rusqlite::params![log_id],
                )
                .map_err(StoreError::Sqlite)?;
                stats.deleted_injection_log_count += 1;
            }

            // Phase 4: delete sessions
            for session_id in &sessions_to_delete {
                conn.execute(
                    "DELETE FROM sessions WHERE session_id = ?1",
                    rusqlite::params![session_id],
                )
                .map_err(StoreError::Sqlite)?;
                stats.deleted_session_count += 1;
            }

            // Phase 5: mark Active sessions with started_at < timed_out_boundary as TimedOut
            let timed_out_updates: Vec<(String, Vec<u8>)> = {
                let mut stmt = conn
                    .prepare("SELECT data FROM sessions")
                    .map_err(StoreError::Sqlite)?;
                let rows = stmt
                    .query_map([], |row| row.get::<_, Vec<u8>>(0))
                    .map_err(StoreError::Sqlite)?;
                let mut updates = Vec::new();
                for row in rows {
                    let bytes = row.map_err(StoreError::Sqlite)?;
                    let record = deserialize_session(&bytes)?;
                    if record.status == SessionLifecycleStatus::Active
                        && record.started_at < timed_out_boundary
                        && !sessions_to_delete.contains(&record.session_id)
                    {
                        let mut updated = record.clone();
                        updated.status = SessionLifecycleStatus::TimedOut;
                        let bytes = serialize_session(&updated)?;
                        updates.push((updated.session_id, bytes));
                        stats.timed_out_count += 1;
                    }
                }
                updates
            };

            for (id, bytes) in timed_out_updates {
                conn.execute(
                    "UPDATE sessions SET data = ?1 WHERE session_id = ?2",
                    rusqlite::params![bytes, id],
                )
                .map_err(StoreError::Sqlite)?;
            }

            Ok(stats)
        })();

        match result {
            Ok(stats) => {
                conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?;
                Ok(stats)
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
}
