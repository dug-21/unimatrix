//! Tests for the `audit_write_count_since` op-list including `context_tag` (vnc-045).
//!
//! Seam: `SqlxStore::open` over a temp DB; rows are inserted with controlled
//! `timestamp`/`operation`/`agent_id` values so op-list membership can be proven to
//! compose with the existing `timestamp >= since` filter. This is a LATENT signal —
//! no throttling/enforcement is asserted here (that is `check_write_rate`, R-07).

use crate::SqlxStore;
use crate::test_helpers::open_test_store;

/// Insert one `audit_log` row directly with a controlled timestamp and operation.
/// Bypasses `log_audit_event` (which stamps `now`) so `since`-boundary and op-list
/// behavior can be exercised deterministically.
async fn insert_audit_row(
    store: &SqlxStore,
    event_id: i64,
    timestamp: i64,
    agent_id: &str,
    operation: &str,
) {
    sqlx::query(
        "INSERT INTO audit_log
             (event_id, timestamp, session_id, agent_id, operation,
              target_ids, outcome, detail,
              credential_type, capability_used, agent_attribution, metadata)
         VALUES (?1, ?2, 'sess', ?3, ?4, '[]', 0, 'seed', 'none', '', '', '{}')",
    )
    .bind(event_id)
    .bind(timestamp)
    .bind(agent_id)
    .bind(operation)
    .execute(store.write_pool_server())
    .await
    .expect("insert audit_log row");
}

/// R-07 / FR-09 / AC-06b — `context_tag` events are counted by `audit_write_count_since`.
#[tokio::test]
async fn test_audit_write_count_includes_context_tag() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let n = 3;
    for i in 1..=n {
        insert_audit_row(&store, i, 100, "agent-a", "context_tag").await;
    }

    let count = store
        .audit_write_count_since("agent-a", 0)
        .await
        .expect("count");
    assert_eq!(
        count, n as u64,
        "all {n} context_tag events must be counted (op-list must include 'context_tag')"
    );
}

/// The op-list addition composes with the `timestamp >= since` filter — events before
/// `since` are excluded, events at/after are counted.
#[tokio::test]
async fn test_audit_write_count_context_tag_since_boundary() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    insert_audit_row(&store, 1, 50, "agent-a", "context_tag").await; // before since
    insert_audit_row(&store, 2, 100, "agent-a", "context_tag").await; // at since
    insert_audit_row(&store, 3, 150, "agent-a", "context_tag").await; // after since

    let count = store
        .audit_write_count_since("agent-a", 100)
        .await
        .expect("count");
    assert_eq!(
        count, 2,
        "only context_tag events at/after `since` count — not an unconditional count"
    );
}

/// The addition is additive: `context_store` + `context_correct` + `context_tag` all
/// count for one agent; unrelated read ops stay excluded.
#[tokio::test]
async fn test_audit_write_count_excludes_non_write_ops() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    insert_audit_row(&store, 1, 100, "agent-b", "context_store").await;
    insert_audit_row(&store, 2, 100, "agent-b", "context_correct").await;
    insert_audit_row(&store, 3, 100, "agent-b", "context_tag").await;
    // Non-write ops — must NOT be pulled in by the op-list widening.
    insert_audit_row(&store, 4, 100, "agent-b", "context_search").await;
    insert_audit_row(&store, 5, 100, "agent-b", "context_lookup").await;

    let count = store
        .audit_write_count_since("agent-b", 0)
        .await
        .expect("count");
    assert_eq!(
        count, 3,
        "context_store + context_correct + context_tag count; read ops excluded"
    );
}
