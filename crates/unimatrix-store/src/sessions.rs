//! Session lifecycle persistence for col-010.
//!
//! Provides CRUD operations on the SESSIONS table (schema v5) and GC logic
//! with INJECTION_LOG cascade deletion. All operations are synchronous;
//! callers in async contexts must use `tokio::task::spawn_blocking`.

use redb::{ReadableDatabase, ReadableTable};
use serde::{Deserialize, Serialize};

use crate::db::Store;
use crate::error::{Result, StoreError};
use crate::schema::{INJECTION_LOG, SESSIONS};

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

// -- Store methods --

impl Store {
    /// Insert a new SessionRecord into SESSIONS.
    ///
    /// If a record with the same session_id already exists, it is overwritten
    /// (redb upsert semantics).
    pub fn insert_session(&self, record: &SessionRecord) -> Result<()> {
        let bytes = serialize_session(record)?;
        let txn = self.db.begin_write().map_err(StoreError::Transaction)?;
        {
            let mut table = txn.open_table(SESSIONS).map_err(StoreError::Table)?;
            table
                .insert(record.session_id.as_str(), bytes.as_slice())
                .map_err(StoreError::Storage)?;
        }
        txn.commit().map_err(StoreError::Commit)?;
        Ok(())
    }

    /// Read-modify-write a SessionRecord.
    ///
    /// Returns `StoreError::Deserialization` if the record is not found
    /// (callers typically log warn and continue — the session may have been
    /// registered before col-010 was deployed).
    pub fn update_session(
        &self,
        session_id: &str,
        updater: impl FnOnce(&mut SessionRecord),
    ) -> Result<()> {
        let txn = self.db.begin_write().map_err(StoreError::Transaction)?;

        let updated_bytes = {
            let table = txn.open_table(SESSIONS).map_err(StoreError::Table)?;
            let guard = table.get(session_id).map_err(StoreError::Storage)?;
            match guard {
                None => {
                    // Session was never persisted — log at caller; return a no-op error
                    return Err(StoreError::Deserialization(format!(
                        "session not found: {session_id}"
                    )));
                }
                Some(value_guard) => {
                    let mut record = deserialize_session(value_guard.value())?;
                    drop(value_guard); // release read guard before table borrow ends
                    updater(&mut record);
                    serialize_session(&record)?
                }
            }
        };

        {
            let mut table = txn.open_table(SESSIONS).map_err(StoreError::Table)?;
            table
                .insert(session_id, updated_bytes.as_slice())
                .map_err(StoreError::Storage)?;
        }

        txn.commit().map_err(StoreError::Commit)?;
        Ok(())
    }

    /// Retrieve a single SessionRecord by session_id.
    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionRecord>> {
        let txn = self.db.begin_read().map_err(StoreError::Transaction)?;
        let table = txn.open_table(SESSIONS).map_err(StoreError::Table)?;
        match table.get(session_id).map_err(StoreError::Storage)? {
            None => Ok(None),
            Some(guard) => Ok(Some(deserialize_session(guard.value())?)),
        }
    }

    /// Scan all sessions for a given feature_cycle.
    ///
    /// Full table scan + in-process filter. Acceptable at current volumes.
    pub fn scan_sessions_by_feature(&self, feature_cycle: &str) -> Result<Vec<SessionRecord>> {
        let txn = self.db.begin_read().map_err(StoreError::Transaction)?;
        let table = txn.open_table(SESSIONS).map_err(StoreError::Table)?;
        let mut results = Vec::new();
        for entry in table.iter().map_err(StoreError::Storage)? {
            let (_, value_guard) = entry.map_err(StoreError::Storage)?;
            let record = deserialize_session(value_guard.value())?;
            if record.feature_cycle.as_deref() == Some(feature_cycle) {
                results.push(record);
            }
        }
        Ok(results)
    }

    /// Scan sessions for a feature_cycle, optionally filtering by status.
    ///
    /// If `status_filter` is `None`, all sessions for the feature_cycle are returned.
    pub fn scan_sessions_by_feature_with_status(
        &self,
        feature_cycle: &str,
        status_filter: Option<SessionLifecycleStatus>,
    ) -> Result<Vec<SessionRecord>> {
        let txn = self.db.begin_read().map_err(StoreError::Transaction)?;
        let table = txn.open_table(SESSIONS).map_err(StoreError::Table)?;
        let mut results = Vec::new();
        for entry in table.iter().map_err(StoreError::Storage)? {
            let (_, value_guard) = entry.map_err(StoreError::Storage)?;
            let record = deserialize_session(value_guard.value())?;
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
    /// with their INJECTION_LOG records.
    ///
    /// All 5 phases run in one `WriteTransaction` (ADR-002 atomicity guarantee).
    /// Returns `GcStats` summarizing what was done.
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

        let txn = self.db.begin_write().map_err(StoreError::Transaction)?;
        let mut stats = GcStats::default();

        // Phase 1: collect session_ids to delete (started_at < delete_boundary)
        let sessions_to_delete: Vec<String> = {
            let table = txn.open_table(SESSIONS).map_err(StoreError::Table)?;
            let mut to_delete = Vec::new();
            for entry in table.iter().map_err(StoreError::Storage)? {
                let (_, value_guard) = entry.map_err(StoreError::Storage)?;
                let record = deserialize_session(value_guard.value())?;
                if record.started_at < delete_boundary {
                    to_delete.push(record.session_id);
                }
            }
            to_delete
        };

        // Phase 2: collect log_ids whose session_id is in the deletion set
        let log_ids_to_delete: Vec<u64> = {
            let log_table = txn.open_table(INJECTION_LOG).map_err(StoreError::Table)?;
            let mut to_delete = Vec::new();
            for entry in log_table.iter().map_err(StoreError::Storage)? {
                let (key_guard, value_guard) = entry.map_err(StoreError::Storage)?;
                let log_id = key_guard.value();
                let record = crate::injection_log::deserialize_injection_log(value_guard.value())?;
                if sessions_to_delete.contains(&record.session_id) {
                    to_delete.push(log_id);
                }
            }
            to_delete
        };

        // Phase 3: delete INJECTION_LOG entries
        {
            let mut log_table = txn.open_table(INJECTION_LOG).map_err(StoreError::Table)?;
            for log_id in &log_ids_to_delete {
                log_table.remove(*log_id).map_err(StoreError::Storage)?;
                stats.deleted_injection_log_count += 1;
            }
        }

        // Phase 4: delete SESSIONS entries
        {
            let mut sessions_table = txn.open_table(SESSIONS).map_err(StoreError::Table)?;
            for session_id in &sessions_to_delete {
                sessions_table
                    .remove(session_id.as_str())
                    .map_err(StoreError::Storage)?;
                stats.deleted_session_count += 1;
            }
        }

        // Phase 5: mark Active sessions with started_at < timed_out_boundary as TimedOut
        // (only sessions not already scheduled for deletion)
        let timed_out_updates: Vec<(String, Vec<u8>)> = {
            let table = txn.open_table(SESSIONS).map_err(StoreError::Table)?;
            let mut updates = Vec::new();
            for entry in table.iter().map_err(StoreError::Storage)? {
                let (_, value_guard) = entry.map_err(StoreError::Storage)?;
                let record = deserialize_session(value_guard.value())?;
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

        {
            let mut sessions_table = txn.open_table(SESSIONS).map_err(StoreError::Table)?;
            for (id, bytes) in timed_out_updates {
                sessions_table
                    .insert(id.as_str(), bytes.as_slice())
                    .map_err(StoreError::Storage)?;
            }
        }

        txn.commit().map_err(StoreError::Commit)?;
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::injection_log::InjectionLogRecord;

    fn open_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();
        (dir, store)
    }

    fn make_session(id: &str, status: SessionLifecycleStatus, started_at: u64) -> SessionRecord {
        SessionRecord {
            session_id: id.to_string(),
            feature_cycle: Some("fc-test".to_string()),
            agent_role: Some("dev".to_string()),
            started_at,
            ended_at: None,
            status,
            compaction_count: 0,
            outcome: None,
            total_injections: 0,
        }
    }

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[test]
    fn test_session_record_roundtrip() {
        let record = SessionRecord {
            session_id: "sess-1".to_string(),
            feature_cycle: Some("col-010".to_string()),
            agent_role: Some("rust-dev".to_string()),
            started_at: 1700000000,
            ended_at: Some(1700003600),
            status: SessionLifecycleStatus::Completed,
            compaction_count: 2,
            outcome: Some("success".to_string()),
            total_injections: 5,
        };
        let bytes = serialize_session(&record).unwrap();
        let back = deserialize_session(&bytes).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn test_session_lifecycle_status_roundtrip() {
        for status in [
            SessionLifecycleStatus::Active,
            SessionLifecycleStatus::Completed,
            SessionLifecycleStatus::TimedOut,
            SessionLifecycleStatus::Abandoned,
        ] {
            let record = make_session("s", status.clone(), 1000);
            let bytes = serialize_session(&record).unwrap();
            let back = deserialize_session(&bytes).unwrap();
            assert_eq!(back.status, status);
        }
    }

    #[test]
    fn test_insert_and_get_session_roundtrip() {
        let (_dir, store) = open_store();
        let now = now_secs();
        let record = make_session("test-sess-1", SessionLifecycleStatus::Active, now);
        store.insert_session(&record).unwrap();
        let got = store.get_session("test-sess-1").unwrap().unwrap();
        assert_eq!(got.session_id, "test-sess-1");
        assert_eq!(got.status, SessionLifecycleStatus::Active);
        assert_eq!(got.started_at, now);
        assert_eq!(got.ended_at, None);
        assert_eq!(got.total_injections, 0);
    }

    #[test]
    fn test_get_session_returns_none_for_missing() {
        let (_dir, store) = open_store();
        let result = store.get_session("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update_session_changes_status() {
        let (_dir, store) = open_store();
        let now = now_secs();
        let record = make_session("upd-sess", SessionLifecycleStatus::Active, now);
        store.insert_session(&record).unwrap();

        store
            .update_session("upd-sess", |r| {
                r.status = SessionLifecycleStatus::Completed;
                r.ended_at = Some(now + 100);
                r.outcome = Some("success".to_string());
                r.total_injections = 3;
            })
            .unwrap();

        let got = store.get_session("upd-sess").unwrap().unwrap();
        assert_eq!(got.status, SessionLifecycleStatus::Completed);
        assert_eq!(got.ended_at, Some(now + 100));
        assert_eq!(got.outcome, Some("success".to_string()));
        assert_eq!(got.total_injections, 3);
        // Unchanged fields preserved
        assert_eq!(got.feature_cycle, Some("fc-test".to_string()));
    }

    #[test]
    fn test_update_session_not_found_returns_error() {
        let (_dir, store) = open_store();
        let result = store.update_session("ghost", |r| r.total_injections = 5);
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_sessions_by_feature_returns_matching() {
        let (_dir, store) = open_store();
        let now = now_secs();

        let mut s1 = make_session("fc-a-1", SessionLifecycleStatus::Active, now);
        s1.feature_cycle = Some("fc-a".to_string());
        let mut s2 = make_session("fc-a-2", SessionLifecycleStatus::Completed, now);
        s2.feature_cycle = Some("fc-a".to_string());
        let mut s3 = make_session("fc-b-1", SessionLifecycleStatus::Active, now);
        s3.feature_cycle = Some("fc-b".to_string());

        store.insert_session(&s1).unwrap();
        store.insert_session(&s2).unwrap();
        store.insert_session(&s3).unwrap();

        let fc_a = store.scan_sessions_by_feature("fc-a").unwrap();
        assert_eq!(fc_a.len(), 2);
        let fc_b = store.scan_sessions_by_feature("fc-b").unwrap();
        assert_eq!(fc_b.len(), 1);
        let fc_c = store.scan_sessions_by_feature("fc-c").unwrap();
        assert_eq!(fc_c.len(), 0);
    }

    #[test]
    fn test_scan_sessions_empty_store() {
        let (_dir, store) = open_store();
        let result = store.scan_sessions_by_feature("anything").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_scan_sessions_by_feature_with_status_filter() {
        let (_dir, store) = open_store();
        let now = now_secs();

        let mut s1 = make_session("s1", SessionLifecycleStatus::Completed, now);
        s1.feature_cycle = Some("fc".to_string());
        let mut s2 = make_session("s2", SessionLifecycleStatus::Completed, now);
        s2.feature_cycle = Some("fc".to_string());
        let mut s3 = make_session("s3", SessionLifecycleStatus::Abandoned, now);
        s3.feature_cycle = Some("fc".to_string());

        store.insert_session(&s1).unwrap();
        store.insert_session(&s2).unwrap();
        store.insert_session(&s3).unwrap();

        let completed =
            store.scan_sessions_by_feature_with_status("fc", Some(SessionLifecycleStatus::Completed)).unwrap();
        assert_eq!(completed.len(), 2);

        let abandoned =
            store.scan_sessions_by_feature_with_status("fc", Some(SessionLifecycleStatus::Abandoned)).unwrap();
        assert_eq!(abandoned.len(), 1);

        let all = store.scan_sessions_by_feature_with_status("fc", None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_gc_marks_old_active_as_timed_out() {
        let (_dir, store) = open_store();
        let now = now_secs();
        // 25 hours ago (> TIMED_OUT_THRESHOLD_SECS = 24h)
        let old_started = now.saturating_sub(25 * 3600 + 60);
        let mut session = make_session("gc-timeout-1", SessionLifecycleStatus::Active, old_started);
        session.feature_cycle = Some("gc-test".to_string());
        store.insert_session(&session).unwrap();

        let stats = store.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS).unwrap();
        assert_eq!(stats.timed_out_count, 1);
        assert_eq!(stats.deleted_session_count, 0);

        let got = store.get_session("gc-timeout-1").unwrap().unwrap();
        assert_eq!(got.status, SessionLifecycleStatus::TimedOut);
    }

    #[test]
    fn test_gc_does_not_time_out_recent_session() {
        let (_dir, store) = open_store();
        let now = now_secs();
        let recent = now.saturating_sub(23 * 3600); // 23h ago
        let session = make_session("gc-recent", SessionLifecycleStatus::Active, recent);
        store.insert_session(&session).unwrap();

        let stats = store.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS).unwrap();
        assert_eq!(stats.timed_out_count, 0);

        let got = store.get_session("gc-recent").unwrap().unwrap();
        assert_eq!(got.status, SessionLifecycleStatus::Active);
    }

    #[test]
    fn test_gc_does_not_time_out_completed_session() {
        let (_dir, store) = open_store();
        let now = now_secs();
        let old = now.saturating_sub(25 * 3600 + 60);
        let session = make_session("gc-completed", SessionLifecycleStatus::Completed, old);
        store.insert_session(&session).unwrap();

        let stats = store.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS).unwrap();
        assert_eq!(stats.timed_out_count, 0); // only Active sessions are timed out

        let got = store.get_session("gc-completed").unwrap().unwrap();
        assert_eq!(got.status, SessionLifecycleStatus::Completed);
    }

    #[test]
    fn test_gc_deletes_old_session_and_cascades_injection_log() {
        let (_dir, store) = open_store();
        let now = now_secs();
        // 31 days ago
        let very_old = now.saturating_sub(31 * 24 * 3600 + 60);
        let session = make_session("gc-delete-1", SessionLifecycleStatus::Completed, very_old);
        store.insert_session(&session).unwrap();

        // Insert 3 injection log records for this session
        let records = vec![
            InjectionLogRecord { log_id: 0, session_id: "gc-delete-1".to_string(), entry_id: 1, confidence: 0.8, timestamp: very_old },
            InjectionLogRecord { log_id: 0, session_id: "gc-delete-1".to_string(), entry_id: 2, confidence: 0.7, timestamp: very_old },
            InjectionLogRecord { log_id: 0, session_id: "gc-delete-1".to_string(), entry_id: 3, confidence: 0.9, timestamp: very_old },
        ];
        store.insert_injection_log_batch(&records).unwrap();

        let stats = store.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS).unwrap();
        assert_eq!(stats.deleted_session_count, 1);
        assert_eq!(stats.deleted_injection_log_count, 3);

        assert!(store.get_session("gc-delete-1").unwrap().is_none());
        let log = store.scan_injection_log_by_session("gc-delete-1").unwrap();
        assert!(log.is_empty());
    }

    #[test]
    fn test_gc_does_not_delete_29_day_session() {
        let (_dir, store) = open_store();
        let now = now_secs();
        let recent = now.saturating_sub(29 * 24 * 3600);
        let session = make_session("gc-29d", SessionLifecycleStatus::Completed, recent);
        store.insert_session(&session).unwrap();

        let stats = store.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS).unwrap();
        assert_eq!(stats.deleted_session_count, 0);
        assert!(store.get_session("gc-29d").unwrap().is_some());
    }

    #[test]
    fn test_gc_cascade_only_deletes_matching_session_logs() {
        let (_dir, store) = open_store();
        let now = now_secs();
        let very_old = now.saturating_sub(31 * 24 * 3600 + 60);
        let recent = now.saturating_sub(5 * 24 * 3600);

        let old_session = make_session("old", SessionLifecycleStatus::Completed, very_old);
        let mut new_session = make_session("new", SessionLifecycleStatus::Completed, recent);
        new_session.feature_cycle = Some("fc-test".to_string());
        store.insert_session(&old_session).unwrap();
        store.insert_session(&new_session).unwrap();

        // 2 logs for old, 3 logs for new
        let old_logs = vec![
            InjectionLogRecord { log_id: 0, session_id: "old".to_string(), entry_id: 1, confidence: 0.8, timestamp: very_old },
            InjectionLogRecord { log_id: 0, session_id: "old".to_string(), entry_id: 2, confidence: 0.8, timestamp: very_old },
        ];
        let new_logs = vec![
            InjectionLogRecord { log_id: 0, session_id: "new".to_string(), entry_id: 10, confidence: 0.9, timestamp: recent },
            InjectionLogRecord { log_id: 0, session_id: "new".to_string(), entry_id: 11, confidence: 0.9, timestamp: recent },
            InjectionLogRecord { log_id: 0, session_id: "new".to_string(), entry_id: 12, confidence: 0.9, timestamp: recent },
        ];
        store.insert_injection_log_batch(&old_logs).unwrap();
        store.insert_injection_log_batch(&new_logs).unwrap();

        let stats = store.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS).unwrap();
        assert_eq!(stats.deleted_session_count, 1);
        assert_eq!(stats.deleted_injection_log_count, 2);

        assert!(store.get_session("old").unwrap().is_none());
        assert!(store.get_session("new").unwrap().is_some());

        let new_log = store.scan_injection_log_by_session("new").unwrap();
        assert_eq!(new_log.len(), 3);
        let old_log = store.scan_injection_log_by_session("old").unwrap();
        assert!(old_log.is_empty());
    }

    #[test]
    fn test_gc_no_sessions_returns_empty_stats() {
        let (_dir, store) = open_store();
        let stats = store.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS).unwrap();
        assert_eq!(stats.timed_out_count, 0);
        assert_eq!(stats.deleted_session_count, 0);
        assert_eq!(stats.deleted_injection_log_count, 0);
    }

    #[test]
    fn test_gc_mixed_time_out_and_delete() {
        let (_dir, store) = open_store();
        let now = now_secs();
        let timed_out_age = now.saturating_sub(25 * 3600 + 60);
        let delete_age = now.saturating_sub(31 * 24 * 3600 + 60);

        let mut session_a = make_session("sess-a", SessionLifecycleStatus::Active, timed_out_age);
        session_a.feature_cycle = Some("fc-test".to_string());
        let mut session_b = make_session("sess-b", SessionLifecycleStatus::Completed, delete_age);
        session_b.feature_cycle = Some("fc-test".to_string());

        store.insert_session(&session_a).unwrap();
        store.insert_session(&session_b).unwrap();

        let stats = store.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS).unwrap();
        assert_eq!(stats.timed_out_count, 1);
        assert_eq!(stats.deleted_session_count, 1);

        let got_a = store.get_session("sess-a").unwrap().unwrap();
        assert_eq!(got_a.status, SessionLifecycleStatus::TimedOut);
        assert!(store.get_session("sess-b").unwrap().is_none());
    }

    #[test]
    fn test_gc_constants() {
        assert_eq!(TIMED_OUT_THRESHOLD_SECS, 24 * 3600);
        assert_eq!(DELETE_THRESHOLD_SECS, 30 * 24 * 3600);
    }

    #[test]
    fn test_session_and_injections_survive_store_reopen() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("reopen.redb");
        let now = now_secs();

        {
            let store = Store::open(&path).unwrap();
            let session = make_session("reopen-sess", SessionLifecycleStatus::Active, now);
            store.insert_session(&session).unwrap();

            let records = vec![
                InjectionLogRecord { log_id: 0, session_id: "reopen-sess".to_string(), entry_id: 1, confidence: 0.8, timestamp: now },
                InjectionLogRecord { log_id: 0, session_id: "reopen-sess".to_string(), entry_id: 2, confidence: 0.9, timestamp: now },
            ];
            store.insert_injection_log_batch(&records).unwrap();
        } // store dropped, DB closed

        {
            let store = Store::open(&path).unwrap();
            let got = store.get_session("reopen-sess").unwrap().unwrap();
            assert_eq!(got.session_id, "reopen-sess");
            assert_eq!(got.status, SessionLifecycleStatus::Active);

            let logs = store.scan_injection_log_by_session("reopen-sess").unwrap();
            assert_eq!(logs.len(), 2);
        }
    }
}
