//! Append-only audit log using the AUDIT_LOG table.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use unimatrix_store::{AUDIT_LOG, COUNTERS, Store};
use unimatrix_store::SqliteWriteTransaction;

use crate::error::ServerError;

/// An immutable record of a single MCP request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEvent {
    /// Monotonic event ID (assigned by log_event).
    pub event_id: u64,
    /// Unix timestamp in seconds (assigned by log_event).
    pub timestamp: u64,
    /// MCP session identifier.
    pub session_id: String,
    /// Agent that made the request.
    pub agent_id: String,
    /// Tool name (e.g., "context_search").
    pub operation: String,
    /// Entry IDs affected (empty for search/stubs).
    pub target_ids: Vec<u64>,
    /// Result of the operation.
    pub outcome: Outcome,
    /// Human-readable detail.
    pub detail: String,
}

/// Result of an audited operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Outcome {
    /// Operation completed successfully.
    Success,
    /// Operation denied (capability check failed).
    Denied,
    /// Operation failed with an error.
    Error,
    /// Tool not yet implemented (vnc-001 stubs).
    NotImplemented,
}

/// Append-only audit log backed by AUDIT_LOG table.
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
    /// using COUNTERS["next_audit_id"].
    pub fn log_event(&self, event: AuditEvent) -> Result<(), ServerError> {
        let txn = self
            .store
            .begin_write()
            .map_err(|e| ServerError::Audit(e.to_string()))?;
        {
            // Get and increment the audit ID counter
            let mut counters = txn
                .open_table(COUNTERS)
                .map_err(|e| ServerError::Audit(e.to_string()))?;
            let current_id = match counters
                .get("next_audit_id")
                .map_err(|e| ServerError::Audit(e.to_string()))?
            {
                Some(guard) => guard.value(),
                None => 1, // first event ever
            };
            counters
                .insert("next_audit_id", current_id + 1)
                .map_err(|e| ServerError::Audit(e.to_string()))?;

            // Build final event with assigned ID and timestamp
            let final_event = AuditEvent {
                event_id: current_id,
                timestamp: current_unix_seconds(),
                ..event
            };

            // Serialize and insert
            let mut audit_table = txn
                .open_table(AUDIT_LOG)
                .map_err(|e| ServerError::Audit(e.to_string()))?;
            let bytes = serialize_audit_event(&final_event)?;
            audit_table
                .insert(current_id, bytes.as_slice())
                .map_err(|e| ServerError::Audit(e.to_string()))?;
        }
        txn.commit()
            .map_err(|e| ServerError::Audit(e.to_string()))?;
        Ok(())
    }

    /// Count write operations by a specific agent since a given timestamp.
    ///
    /// Scans AUDIT_LOG for entries where `agent_id` matches and `operation`
    /// is a write tool (context_store, context_correct) with `timestamp >= since`.
    /// Returns the count.
    pub fn write_count_since(&self, agent_id: &str, since: u64) -> Result<u64, ServerError> {
        let txn = self
            .store
            .begin_read()
            .map_err(|e| ServerError::Audit(e.to_string()))?;
        let table = txn
            .open_table(AUDIT_LOG)
            .map_err(|e| ServerError::Audit(e.to_string()))?;

        let mut count = 0u64;

        for result in table
            .iter()
            .map_err(|e| ServerError::Audit(e.to_string()))?
        {
            let (_, value) = result.map_err(|e| ServerError::Audit(e.to_string()))?;
            let event = deserialize_audit_event(value.value())?;

            if event.timestamp < since {
                continue;
            }

            if event.agent_id != agent_id {
                continue;
            }

            if is_write_operation(&event.operation) {
                count += 1;
            }
        }

        Ok(count)
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
        let counters = txn
            .open_table(COUNTERS)
            .map_err(|e| ServerError::Audit(e.to_string()))?;
        let current_id = match counters
            .get("next_audit_id")
            .map_err(|e| ServerError::Audit(e.to_string()))?
        {
            Some(guard) => guard.value(),
            None => 1,
        };
        counters
            .insert("next_audit_id", current_id + 1)
            .map_err(|e| ServerError::Audit(e.to_string()))?;
        drop(counters);

        let final_event = AuditEvent {
            event_id: current_id,
            timestamp: current_unix_seconds(),
            ..event
        };

        let audit_table = txn
            .open_table(AUDIT_LOG)
            .map_err(|e| ServerError::Audit(e.to_string()))?;
        let bytes = serialize_audit_event(&final_event)?;
        audit_table
            .insert(current_id, bytes.as_slice())
            .map_err(|e| ServerError::Audit(e.to_string()))?;

        Ok(current_id)
    }
}

/// Get the current time as unix seconds.
fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Serialize an AuditEvent to bincode bytes.
fn serialize_audit_event(event: &AuditEvent) -> Result<Vec<u8>, ServerError> {
    bincode::serde::encode_to_vec(event, bincode::config::standard())
        .map_err(|e| ServerError::Audit(format!("serialization failed: {e}")))
}

/// Check if an operation name is a write operation.
fn is_write_operation(operation: &str) -> bool {
    matches!(operation, "context_store" | "context_correct")
}

/// Deserialize an AuditEvent from bincode bytes.
pub(crate) fn deserialize_audit_event(bytes: &[u8]) -> Result<AuditEvent, ServerError> {
    let (event, _) =
        bincode::serde::decode_from_slice::<AuditEvent, _>(bytes, bincode::config::standard())
            .map_err(|e| ServerError::Audit(format!("deserialization failed: {e}")))?;
    Ok(event)
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

    #[test]
    fn test_first_event_id_is_1() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());
        audit.log_event(make_event()).unwrap();

        // Read the event back
        let read_txn = store.begin_read().unwrap();
        let table = read_txn.open_table(AUDIT_LOG).unwrap();
        let guard = table.get(1u64).unwrap().unwrap();
        let event = deserialize_audit_event(guard.value()).unwrap();
        assert_eq!(event.event_id, 1);
    }

    #[test]
    fn test_monotonic_ids() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        for _ in 0..10 {
            audit.log_event(make_event()).unwrap();
        }

        let read_txn = store.begin_read().unwrap();
        let table = read_txn.open_table(AUDIT_LOG).unwrap();
        let mut prev_id = 0;
        for i in 1..=10u64 {
            let guard = table.get(i).unwrap().unwrap();
            let event = deserialize_audit_event(guard.value()).unwrap();
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

        let read_txn = store.begin_read().unwrap();
        let table = read_txn.open_table(AUDIT_LOG).unwrap();
        let guard = table.get(6u64).unwrap().unwrap();
        let event = deserialize_audit_event(guard.value()).unwrap();
        assert_eq!(event.event_id, 6);
    }

    #[test]
    fn test_timestamp_set_by_log_event() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        let mut event = make_event();
        event.timestamp = 0;
        audit.log_event(event).unwrap();

        let read_txn = store.begin_read().unwrap();
        let table = read_txn.open_table(AUDIT_LOG).unwrap();
        let guard = table.get(1u64).unwrap().unwrap();
        let stored = deserialize_audit_event(guard.value()).unwrap();
        assert!(stored.timestamp > 0);
    }

    #[test]
    fn test_audit_event_roundtrip() {
        let event = AuditEvent {
            event_id: 42,
            timestamp: 1700000000,
            session_id: "sess-1".to_string(),
            agent_id: "uni-architect".to_string(),
            operation: "context_store".to_string(),
            target_ids: vec![1, 2, 3],
            outcome: Outcome::Success,
            detail: "stored 3 entries".to_string(),
        };
        let bytes = serialize_audit_event(&event).unwrap();
        let deserialized = deserialize_audit_event(&bytes).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_all_outcome_variants_roundtrip() {
        for outcome in [
            Outcome::Success,
            Outcome::Denied,
            Outcome::Error,
            Outcome::NotImplemented,
        ] {
            let event = AuditEvent {
                outcome,
                ..make_event()
            };
            let bytes = serialize_audit_event(&event).unwrap();
            let deserialized = deserialize_audit_event(&bytes).unwrap();
            assert_eq!(deserialized.outcome, outcome);
        }
    }

    #[test]
    fn test_empty_target_ids() {
        let event = make_event();
        let bytes = serialize_audit_event(&event).unwrap();
        let deserialized = deserialize_audit_event(&bytes).unwrap();
        assert!(deserialized.target_ids.is_empty());
    }

    #[test]
    fn test_multiple_target_ids() {
        let mut event = make_event();
        event.target_ids = vec![10, 20, 30];
        let bytes = serialize_audit_event(&event).unwrap();
        let deserialized = deserialize_audit_event(&bytes).unwrap();
        assert_eq!(deserialized.target_ids, vec![10, 20, 30]);
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
        let read_txn = store.begin_read().unwrap();
        let table = read_txn.open_table(AUDIT_LOG).unwrap();
        assert!(table.get(1u64).unwrap().is_none());
    }

    #[test]
    fn test_write_in_txn_with_commit() {
        let store = make_store();
        let audit = AuditLog::new(store.clone());

        let txn = store.begin_write().unwrap();
        let event_id = audit.write_in_txn(&txn, make_event()).unwrap();
        txn.commit().unwrap();

        assert_eq!(event_id, 1);

        // Event should be persisted
        let read_txn = store.begin_read().unwrap();
        let table = read_txn.open_table(AUDIT_LOG).unwrap();
        let guard = table.get(1u64).unwrap().unwrap();
        let event = deserialize_audit_event(guard.value()).unwrap();
        assert_eq!(event.event_id, 1);
        assert!(event.timestamp > 0);
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

        assert_eq!(event_id, 4, "write_in_txn should continue from log_event counter");

        // Use log_event for 5th
        audit.log_event(make_event()).unwrap();

        let read_txn = store.begin_read().unwrap();
        let table = read_txn.open_table(AUDIT_LOG).unwrap();
        let guard = table.get(5u64).unwrap().unwrap();
        let event = deserialize_audit_event(guard.value()).unwrap();
        assert_eq!(event.event_id, 5);
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

        let read_txn = store.begin_read().unwrap();
        let table = read_txn.open_table(AUDIT_LOG).unwrap();
        let mut ids = Vec::new();
        for i in 1..=100u64 {
            let guard = table.get(i).unwrap().unwrap();
            let event = deserialize_audit_event(guard.value()).unwrap();
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
        assert_eq!(
            audit.write_count_since("agent-a", now + 10000).unwrap(),
            0
        );
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
