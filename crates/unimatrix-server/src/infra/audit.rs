//! Append-only audit log using SqlxStore async methods.
//!
//! Rewritten for nxs-011: all database access via async SqlxStore methods.
//! Replaces previous rusqlite/SqliteWriteTransaction approach.

use std::sync::Arc;

use unimatrix_store::SqlxStore;

// Re-export types so existing `use crate::infra::audit::*` imports keep working.
pub use unimatrix_store::{AuditEvent, Outcome};

use crate::error::ServerError;

/// Append-only audit log backed by audit_log table.
pub struct AuditLog {
    store: Arc<SqlxStore>,
}

impl AuditLog {
    /// Create a new audit log backed by the given store.
    pub fn new(store: Arc<SqlxStore>) -> Self {
        AuditLog { store }
    }

    /// Append an audit event. Assigns event_id and timestamp.
    ///
    /// The caller provides all fields except `event_id` and `timestamp`,
    /// which are set by this method. The event_id is monotonically increasing
    /// using counters["next_audit_event_id"].
    pub fn log_event(&self, event: AuditEvent) -> Result<(), ServerError> {
        block_sync(self.store.log_audit_event(event))
            .map(|_| ())
            .map_err(|e| ServerError::Audit(e.to_string()))
    }

    /// Count write operations by a specific agent since a given timestamp.
    ///
    /// Uses indexed SQL query instead of full table scan.
    pub fn write_count_since(&self, agent_id: &str, since: u64) -> Result<u64, ServerError> {
        block_sync(self.store.audit_write_count_since(agent_id, since))
            .map_err(|e| ServerError::Audit(e.to_string()))
    }
}

/// Bridge an async future to sync context.
///
/// When called from within a multi-thread tokio runtime, uses `block_in_place`.
/// When called from a sync context (no runtime), creates a temporary runtime.
fn block_sync<F, T, E>(fut: F) -> Result<T, E>
where
    F: std::future::Future<Output = Result<T, E>>,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            rt.block_on(fut)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unimatrix_store::pool_config::PoolConfig;

    async fn make_store() -> Arc<SqlxStore> {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = SqlxStore::open(&path, PoolConfig::default())
            .await
            .expect("open store");
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_first_event_id_is_1() {
        let store = make_store().await;
        let audit = AuditLog::new(store.clone());
        audit.log_event(make_event()).unwrap();

        let event = store.read_audit_event(1).await.unwrap().unwrap();
        assert_eq!(event.event_id, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_monotonic_ids() {
        let store = make_store().await;
        let audit = AuditLog::new(store.clone());

        for _ in 0..10 {
            audit.log_event(make_event()).unwrap();
        }

        let mut prev_id = 0;
        for i in 1..=10u64 {
            let event = store.read_audit_event(i).await.unwrap().unwrap();
            assert_eq!(event.event_id, i);
            assert!(event.event_id > prev_id);
            prev_id = event.event_id;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_cross_session_continuity() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");

        // Session 1: log 5 events
        {
            let store = Arc::new(
                SqlxStore::open(&path, PoolConfig::default())
                    .await
                    .expect("open"),
            );
            let audit = AuditLog::new(store);
            for _ in 0..5 {
                audit.log_event(make_event()).unwrap();
            }
        }

        // Session 2: log 1 event
        let store = Arc::new(
            SqlxStore::open(&path, PoolConfig::default())
                .await
                .expect("open"),
        );
        let audit = AuditLog::new(store.clone());
        audit.log_event(make_event()).unwrap();

        let event = store.read_audit_event(6).await.unwrap().unwrap();
        assert_eq!(event.event_id, 6);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_timestamp_set_by_log_event() {
        let store = make_store().await;
        let audit = AuditLog::new(store.clone());

        let mut event = make_event();
        event.timestamp = 0;
        audit.log_event(event).unwrap();

        let stored = store.read_audit_event(1).await.unwrap().unwrap();
        assert!(stored.timestamp > 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_all_outcome_variants_roundtrip() {
        let store = make_store().await;
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
            let stored = store
                .read_audit_event((i + 1) as u64)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(stored.outcome, *outcome);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_empty_target_ids() {
        let store = make_store().await;
        let audit = AuditLog::new(store.clone());
        audit.log_event(make_event()).unwrap();

        let stored = store.read_audit_event(1).await.unwrap().unwrap();
        assert!(stored.target_ids.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_multiple_target_ids() {
        let store = make_store().await;
        let audit = AuditLog::new(store.clone());

        let mut event = make_event();
        event.target_ids = vec![10, 20, 30];
        audit.log_event(event).unwrap();

        let stored = store.read_audit_event(1).await.unwrap().unwrap();
        assert_eq!(stored.target_ids, vec![10, 20, 30]);
    }

    // -- crt-001: write_count_since tests --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_write_count_since_counts_writes_only() {
        let store = make_store().await;
        let audit = AuditLog::new(store.clone());

        for _ in 0..5 {
            let mut event = make_event();
            event.operation = "context_store".to_string();
            event.agent_id = "agent-a".to_string();
            event.outcome = Outcome::Success;
            audit.log_event(event).unwrap();
        }
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_write_count_since_agent_filtering() {
        let store = make_store().await;
        let audit = AuditLog::new(store.clone());

        for _ in 0..3 {
            let mut event = make_event();
            event.operation = "context_store".to_string();
            event.agent_id = "agent-a".to_string();
            audit.log_event(event).unwrap();
        }
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_write_count_since_timestamp_boundary() {
        let store = make_store().await;
        let audit = AuditLog::new(store.clone());

        let mut event = make_event();
        event.operation = "context_store".to_string();
        event.agent_id = "agent-a".to_string();
        audit.log_event(event).unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        assert_eq!(audit.write_count_since("agent-a", 0).unwrap(), 1);
        assert_eq!(audit.write_count_since("agent-a", now + 10000).unwrap(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_write_count_since_empty_log() {
        let store = make_store().await;
        let audit = AuditLog::new(store);
        assert_eq!(audit.write_count_since("any-agent", 0).unwrap(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_write_count_since_both_write_ops() {
        let store = make_store().await;
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_write_count_since_non_write_ops_excluded() {
        let store = make_store().await;
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_rapid_events_unique_ids() {
        let store = make_store().await;
        let audit = AuditLog::new(store.clone());

        for _ in 0..100 {
            audit.log_event(make_event()).unwrap();
        }

        let mut ids = Vec::new();
        for i in 1..=100u64 {
            let event = store.read_audit_event(i).await.unwrap().unwrap();
            ids.push(event.event_id);
        }

        for window in ids.windows(2) {
            assert!(window[1] > window[0], "IDs not strictly increasing");
        }
        assert_eq!(ids.len(), 100);
    }
}
