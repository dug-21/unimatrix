//! Append-only audit log using direct SQL against the audit_log table.
//!
//! Rewritten for nxs-008: no bincode, no open_table compat layer.
//! Uses SQL columns + JSON for target_ids (ADR-004, ADR-007).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use unimatrix_store::SqliteWriteTransaction;
use unimatrix_store::Store;
use unimatrix_store::rusqlite;

// Re-export types so existing `use crate::infra::audit::*` imports keep working.
pub use unimatrix_store::{AuditEvent, Outcome};

use crate::error::ServerError;

/// Append-only audit log backed by audit_log table.
pub struct AuditLog {
    store: Arc<Store>,
}

impl AuditLog {
    /// Create a new audit log backed by the given store.
    pub fn new(store: Arc<Store>) -> Self {
        AuditLog { store }
    }

    /// Append an audit event. Assigns event_id and timestamp.
    ///
    /// The caller provides all fields except `event_id` and `timestamp`,
    /// which are set by this method. The event_id is monotonically increasing
    /// using counters["next_audit_id"].
    pub fn log_event(&self, event: AuditEvent) -> Result<(), ServerError> {
        let conn = self.store.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| ServerError::Audit(e.to_string()))?;

        let result = (|| -> Result<(), ServerError> {
            // Get and increment the audit ID counter
            let current_id =
                unimatrix_store::counters::read_counter(&conn, "next_audit_id").unwrap_or(1);
            let id = if current_id == 0 { 1 } else { current_id };
            unimatrix_store::counters::set_counter(&conn, "next_audit_id", id + 1)
                .map_err(|e| ServerError::Audit(e.to_string()))?;

            let target_ids_json = serde_json::to_string(&event.target_ids)
                .map_err(|e| ServerError::Audit(e.to_string()))?;
            let now = current_unix_seconds();

            conn.execute(
                "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
                    operation, target_ids, outcome, detail)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    id as i64,
                    now as i64,
                    &event.session_id,
                    &event.agent_id,
                    &event.operation,
                    &target_ids_json,
                    event.outcome as u8 as i64,
                    &event.detail,
                ],
            )
            .map_err(|e| ServerError::Audit(e.to_string()))?;

            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")
                    .map_err(|e| ServerError::Audit(e.to_string()))?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Count write operations by a specific agent since a given timestamp.
    ///
    /// Uses indexed SQL query instead of full table scan.
    pub fn write_count_since(&self, agent_id: &str, since: u64) -> Result<u64, ServerError> {
        let conn = self.store.lock_conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM audit_log
                 WHERE agent_id = ?1 AND timestamp >= ?2
                 AND operation IN ('context_store', 'context_correct')",
                rusqlite::params![agent_id, since as i64],
                |row| row.get(0),
            )
            .map_err(|e| ServerError::Audit(e.to_string()))?;
        Ok(count as u64)
    }

    /// Write an audit event into an existing write transaction without committing.
    ///
    /// The caller owns the transaction and is responsible for committing.
    /// Returns the assigned event_id.
    pub fn write_in_txn(
        &self,
        txn: &SqliteWriteTransaction<'_>,
        event: AuditEvent,
    ) -> Result<u64, ServerError> {
        let conn = &*txn.guard;

        // Read and increment counter within the existing transaction
        let current_id =
            unimatrix_store::counters::read_counter(conn, "next_audit_id").unwrap_or(1);
        let id = if current_id == 0 { 1 } else { current_id };
        unimatrix_store::counters::set_counter(conn, "next_audit_id", id + 1)
            .map_err(|e| ServerError::Audit(e.to_string()))?;

        let target_ids_json = serde_json::to_string(&event.target_ids)
            .map_err(|e| ServerError::Audit(e.to_string()))?;
        let now = current_unix_seconds();

        conn.execute(
            "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
                operation, target_ids, outcome, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                id as i64,
                now as i64,
                &event.session_id,
                &event.agent_id,
                &event.operation,
                &target_ids_json,
                event.outcome as u8 as i64,
                &event.detail,
            ],
        )
        .map_err(|e| ServerError::Audit(e.to_string()))?;

        Ok(id)
    }
}

/// Get the current time as unix seconds.
fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> Arc<Store> {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(&path).unwrap();
        std::mem::forget(dir);
        Arc::new(store)
    }

    fn make_event() -> AuditEvent {
        AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: "test-session".to_string(),
            agent_id: "test-agent".to_string(),
            operation: "context_search".to_string(),
            target_ids: vec![],
            outcome: Outcome::NotImplemented,
            detail: "stub".to_string(),
        }
    }

    /// Helper to read an audit event by event_id directly from SQL.
    fn read_audit_event(store: &Store, event_id: u64) -> Option<AuditEvent> {
        let conn = store.lock_conn();
        conn.query_row(
            "SELECT event_id, timestamp, session_id, agent_id, operation,
                    target_ids, outcome, detail
             FROM audit_log WHERE event_id = ?1",
            rusqlite::params![event_id as i64],
            |row| {
                let target_ids_json: String = row.get("target_ids")?;
                let target_ids: Vec<u64> =
                    serde_json::from_str(&target_ids_json).unwrap_or_default();
                Ok(AuditEvent {
                    event_id: row.get::<_, i64>("event_id")? as u64,
                    timestamp: row.get::<_, i64>("timestamp")? as u64,
                    session_id: row.get("session_id")?,
                    agent_id: row.get("agent_id")?,
                    operation: row.get("operation")?,
                    target_ids,
                    outcome: Outcome::try_from(row.get::<_, i64>("outcome")? as u8)
                        .unwrap_or(Outcome::Error),
                    detail: row.get("detail")?,
                })
            },
        )
        .ok()
    }

    #[test]
    fn test_first_event_id_is_1() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());
        audit.log_event(make_event()).unwrap();

        let event = read_audit_event(&store, 1).unwrap();
        assert_eq!(event.event_id, 1);
    }

    #[test]
    fn test_monotonic_ids() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        for _ in 0..10 {
            audit.log_event(make_event()).unwrap();
        }

        let mut prev_id = 0;
        for i in 1..=10u64 {
            let event = read_audit_event(&store, i).unwrap();
            assert_eq!(event.event_id, i);
            assert!(event.event_id > prev_id);
            prev_id = event.event_id;
        }
    }

    #[test]
    fn test_cross_session_continuity() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");

        // Session 1: log 5 events
        {
            let store = Arc::new(Store::open(&path).unwrap());
            let audit = AuditLog::new(store);
            for _ in 0..5 {
                audit.log_event(make_event()).unwrap();
            }
        }

        // Session 2: log 1 event
        let store = Arc::new(Store::open(&path).unwrap());
        let audit = AuditLog::new(store.clone());
        audit.log_event(make_event()).unwrap();

        let event = read_audit_event(&store, 6).unwrap();
        assert_eq!(event.event_id, 6);
    }

    #[test]
    fn test_timestamp_set_by_log_event() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        let mut event = make_event();
        event.timestamp = 0;
        audit.log_event(event).unwrap();

        let stored = read_audit_event(&store, 1).unwrap();
        assert!(stored.timestamp > 0);
    }

    #[test]
    fn test_all_outcome_variants_roundtrip() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        for (i, outcome) in [
            Outcome::Success,
            Outcome::Denied,
            Outcome::Error,
            Outcome::NotImplemented,
        ]
        .iter()
        .enumerate()
        {
            let event = AuditEvent {
                outcome: *outcome,
                ..make_event()
            };
            audit.log_event(event).unwrap();
            let stored = read_audit_event(&store, (i + 1) as u64).unwrap();
            assert_eq!(stored.outcome, *outcome);
        }
    }

    #[test]
    fn test_empty_target_ids() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());
        audit.log_event(make_event()).unwrap();

        let stored = read_audit_event(&store, 1).unwrap();
        assert!(stored.target_ids.is_empty());
    }

    #[test]
    fn test_multiple_target_ids() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        let mut event = make_event();
        event.target_ids = vec![10, 20, 30];
        audit.log_event(event).unwrap();

        let stored = read_audit_event(&store, 1).unwrap();
        assert_eq!(stored.target_ids, vec![10, 20, 30]);
    }

    #[test]
    fn test_write_in_txn_does_not_commit() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        // Write in a transaction but do NOT commit
        let txn = store.begin_write().unwrap();
        let event_id = audit.write_in_txn(&txn, make_event()).unwrap();
        assert_eq!(event_id, 1);
        drop(txn); // Drop without commit

        // Event should NOT be persisted
        assert!(read_audit_event(&store, 1).is_none());
    }

    #[test]
    fn test_write_in_txn_with_commit() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        let txn = store.begin_write().unwrap();
        let event_id = audit.write_in_txn(&txn, make_event()).unwrap();
        txn.commit().unwrap();

        assert_eq!(event_id, 1);

        let stored = read_audit_event(&store, 1).unwrap();
        assert_eq!(stored.event_id, 1);
        assert!(stored.timestamp > 0);
    }

    #[test]
    fn test_write_in_txn_shares_counter_with_log_event() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        // Use log_event for first 3 events
        for _ in 0..3 {
            audit.log_event(make_event()).unwrap();
        }

        // Use write_in_txn for 4th
        let txn = store.begin_write().unwrap();
        let event_id = audit.write_in_txn(&txn, make_event()).unwrap();
        txn.commit().unwrap();

        assert_eq!(
            event_id, 4,
            "write_in_txn should continue from log_event counter"
        );

        // Use log_event for 5th
        audit.log_event(make_event()).unwrap();

        let stored = read_audit_event(&store, 5).unwrap();
        assert_eq!(stored.event_id, 5);
    }

    #[test]
    fn test_write_in_txn_returns_event_id() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        let txn = store.begin_write().unwrap();
        let id1 = audit.write_in_txn(&txn, make_event()).unwrap();
        let id2 = audit.write_in_txn(&txn, make_event()).unwrap();
        txn.commit().unwrap();

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[test]
    fn test_rapid_events_unique_ids() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        for _ in 0..100 {
            audit.log_event(make_event()).unwrap();
        }

        let mut ids = Vec::new();
        for i in 1..=100u64 {
            let event = read_audit_event(&store, i).unwrap();
            ids.push(event.event_id);
        }

        // Verify unique and strictly increasing
        for window in ids.windows(2) {
            assert!(window[1] > window[0], "IDs not strictly increasing");
        }
        assert_eq!(ids.len(), 100);
    }

    // -- crt-001: write_count_since tests --

    #[test]
    fn test_write_count_since_counts_writes_only() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        // 5 write events (context_store)
        for _ in 0..5 {
            let mut event = make_event();
            event.operation = "context_store".to_string();
            event.agent_id = "agent-a".to_string();
            event.outcome = Outcome::Success;
            audit.log_event(event).unwrap();
        }
        // 5 read events (context_search)
        for _ in 0..5 {
            let mut event = make_event();
            event.operation = "context_search".to_string();
            event.agent_id = "agent-a".to_string();
            event.outcome = Outcome::Success;
            audit.log_event(event).unwrap();
        }

        let count = audit.write_count_since("agent-a", 0).unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn test_write_count_since_agent_filtering() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        // Agent A: 3 writes
        for _ in 0..3 {
            let mut event = make_event();
            event.operation = "context_store".to_string();
            event.agent_id = "agent-a".to_string();
            audit.log_event(event).unwrap();
        }
        // Agent B: 2 writes
        for _ in 0..2 {
            let mut event = make_event();
            event.operation = "context_store".to_string();
            event.agent_id = "agent-b".to_string();
            audit.log_event(event).unwrap();
        }

        assert_eq!(audit.write_count_since("agent-a", 0).unwrap(), 3);
        assert_eq!(audit.write_count_since("agent-b", 0).unwrap(), 2);
        assert_eq!(audit.write_count_since("agent-c", 0).unwrap(), 0);
    }

    #[test]
    fn test_write_count_since_timestamp_boundary() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        // Log events -- they'll get current timestamp
        let mut event = make_event();
        event.operation = "context_store".to_string();
        event.agent_id = "agent-a".to_string();
        audit.log_event(event).unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Events logged before "now" should be counted (since = 0)
        assert_eq!(audit.write_count_since("agent-a", 0).unwrap(), 1);

        // Events with since = far future should return 0
        assert_eq!(audit.write_count_since("agent-a", now + 10000).unwrap(), 0);
    }

    #[test]
    fn test_write_count_since_empty_log() {
        let store = make_store();
        let audit = AuditLog::new(store);
        assert_eq!(audit.write_count_since("any-agent", 0).unwrap(), 0);
    }

    #[test]
    fn test_write_count_since_both_write_ops() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        let mut e1 = make_event();
        e1.operation = "context_store".to_string();
        e1.agent_id = "agent-a".to_string();
        audit.log_event(e1).unwrap();

        let mut e2 = make_event();
        e2.operation = "context_correct".to_string();
        e2.agent_id = "agent-a".to_string();
        audit.log_event(e2).unwrap();

        assert_eq!(audit.write_count_since("agent-a", 0).unwrap(), 2);
    }

    #[test]
    fn test_write_count_since_non_write_ops_excluded() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        for op in [
            "context_search",
            "context_lookup",
            "context_get",
            "context_briefing",
            "context_deprecate",
            "context_status",
        ] {
            let mut event = make_event();
            event.operation = op.to_string();
            event.agent_id = "agent-a".to_string();
            audit.log_event(event).unwrap();
        }

        assert_eq!(audit.write_count_since("agent-a", 0).unwrap(), 0);
    }
}
