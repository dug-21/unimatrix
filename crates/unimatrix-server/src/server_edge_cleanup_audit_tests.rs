//! crt-058 audit-emit tests — `UnimatrixServer::emit_edge_cleanup_audit` and the
//! `context_deprecate.edge_cleanup` `AuditEvent` it produces (ADR-002 / AC-03 /
//! AC-11 / R-08 / R-10).
//!
//! Driven at the reachable seam: `make_server` + the REAL
//! `delete_agent_edges_for_entry` helper + the REAL `emit_edge_cleanup_audit`,
//! then an audit_log read-back. The full `context_deprecate` #[tool] handler is
//! not constructible in unit scope (no `RequestContext`); its end-to-end route
//! proof lives in the Stage-3c Python integration suite.

use std::collections::HashSet;

use crate::background::insert_graph_edge_with_source;
use crate::infra::audit::{AuditEvent, Outcome};
use crate::mcp::edge_write::delete_agent_edges_for_entry;
use crate::server::UnimatrixServer;
use crate::server::tests::make_server;

/// Let the fire-and-forget audit write (`tokio::spawn`) land before read-back
/// (existing 50ms convention, server.rs).
async fn audit_settle() {
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

/// Read back `(target_ids_json, detail, metadata)` for every `edge_cleanup` row.
async fn read_cleanup_rows(server: &UnimatrixServer) -> Vec<(String, String, String)> {
    sqlx::query_as::<_, (String, String, String)>(
        "SELECT target_ids, detail, metadata FROM audit_log \
         WHERE operation = 'context_deprecate.edge_cleanup'",
    )
    .fetch_all(server.store.read_pool_test())
    .await
    .expect("audit read must succeed")
}

/// Parse an `edge_cleanup` metadata JSON array into a tuple set.
fn parse_metadata_tuples(metadata: &str) -> HashSet<(u64, u64, String)> {
    let val: serde_json::Value =
        serde_json::from_str(metadata).expect("metadata must be well-formed JSON");
    val.as_array()
        .expect("metadata must be a JSON array")
        .iter()
        .map(|o| {
            (
                o["source_id"].as_u64().expect("source_id"),
                o["target_id"].as_u64().expect("target_id"),
                o["relation_type"]
                    .as_str()
                    .expect("relation_type")
                    .to_string(),
            )
        })
        .collect()
}

/// AC-03 / FR-04 / R-08: record content — `target_ids == [E]`, count + `#E` in
/// `detail`, exactly one record.
#[tokio::test(flavor = "multi_thread")]
async fn test_edge_cleanup_audit_record_content() {
    let server = make_server().await;
    let e: u64 = 500;
    insert_graph_edge_with_source(&server.store, 600, e as i64, "DependsOn", "agent").await;
    insert_graph_edge_with_source(&server.store, 601, e as i64, "Supports", "agent").await;
    insert_graph_edge_with_source(&server.store, e as i64, 700, "DependsOn", "agent").await;

    let removed = delete_agent_edges_for_entry(&server.store, e)
        .await
        .expect("delete ok");
    assert_eq!(removed.len(), 3);
    server.emit_edge_cleanup_audit(e, &removed, "sess".into(), "agent-x".into(), "attr".into());
    audit_settle().await;

    let rows = read_cleanup_rows(&server).await;
    assert_eq!(rows.len(), 1, "exactly one edge_cleanup record");
    let (target_ids_json, detail, _metadata) = &rows[0];
    let target_ids: Vec<u64> = serde_json::from_str(target_ids_json).unwrap();
    assert_eq!(target_ids, vec![e], "target_ids must be [entry_id]");
    assert!(detail.contains('3'), "detail carries the count 3");
    assert!(
        detail.contains(&format!("#{e}")),
        "detail carries #entry_id"
    );
}

/// AC-11 / R-03 / SR-01: metadata is a well-formed JSON array whose tuple SET
/// equals EXACTLY the pre-delete agent-edge set (order-independent; not a
/// count-only check).
#[tokio::test(flavor = "multi_thread")]
async fn test_edge_cleanup_audit_metadata_tuple_set_equality() {
    let server = make_server().await;
    let e: u64 = 500;
    let expected: HashSet<(u64, u64, String)> = [
        (600, e, "DependsOn".to_string()),
        (601, e, "Supports".to_string()),
        (e, 700, "DependsOn".to_string()),
    ]
    .into_iter()
    .collect();
    insert_graph_edge_with_source(&server.store, 600, e as i64, "DependsOn", "agent").await;
    insert_graph_edge_with_source(&server.store, 601, e as i64, "Supports", "agent").await;
    insert_graph_edge_with_source(&server.store, e as i64, 700, "DependsOn", "agent").await;

    let removed = delete_agent_edges_for_entry(&server.store, e)
        .await
        .expect("delete ok");
    server.emit_edge_cleanup_audit(e, &removed, "s".into(), "a".into(), "c".into());
    audit_settle().await;

    let rows = read_cleanup_rows(&server).await;
    assert_eq!(rows.len(), 1);
    let tuples = parse_metadata_tuples(&rows[0].2);
    assert_eq!(
        tuples, expected,
        "metadata tuple set must equal the pre-delete agent-edge set exactly"
    );
}

/// AC-11: on a non-empty removal the metadata must NOT fall through to the
/// audit-layer `"{}"` empty sentinel (`audit.rs`) and must be valid JSON.
#[tokio::test(flavor = "multi_thread")]
async fn test_edge_cleanup_audit_metadata_not_sentinel_on_nonempty() {
    let server = make_server().await;
    let e: u64 = 500;
    insert_graph_edge_with_source(&server.store, 600, e as i64, "DependsOn", "agent").await;
    let removed = delete_agent_edges_for_entry(&server.store, e)
        .await
        .expect("delete ok");
    server.emit_edge_cleanup_audit(e, &removed, "s".into(), "a".into(), "c".into());
    audit_settle().await;

    let rows = read_cleanup_rows(&server).await;
    assert_eq!(rows.len(), 1);
    assert_ne!(
        rows[0].2, "{}",
        "non-empty removal must not use the empty sentinel"
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&rows[0].2).is_ok(),
        "metadata must be valid JSON"
    );
}

/// Security: an unusual `relation_type` string is encoder-escaped (well-formed
/// JSON), never interpolated — the tuple round-trips intact.
#[tokio::test(flavor = "multi_thread")]
async fn test_edge_cleanup_audit_metadata_wellformed_with_unusual_relation_type() {
    let server = make_server().await;
    let e: u64 = 500;
    let weird = "weird\"rel\\type\nwith,\"quotes";
    insert_graph_edge_with_source(&server.store, 600, e as i64, weird, "agent").await;

    let removed = delete_agent_edges_for_entry(&server.store, e)
        .await
        .expect("delete ok");
    server.emit_edge_cleanup_audit(e, &removed, "s".into(), "a".into(), "c".into());
    audit_settle().await;

    let rows = read_cleanup_rows(&server).await;
    assert_eq!(rows.len(), 1);
    let tuples = parse_metadata_tuples(&rows[0].2);
    assert!(
        tuples.contains(&(600, e, weird.to_string())),
        "unusual relation_type must survive encoder-escaping intact"
    );
}

/// R-08: the flip's `"context_deprecate"` audit and the cleanup's
/// `"context_deprecate.edge_cleanup"` audit are TWO DISTINCT records — a test
/// keying on the wrong operation must not match the cleanup event.
#[tokio::test(flavor = "multi_thread")]
async fn test_flip_and_cleanup_are_two_distinct_records() {
    let server = make_server().await;
    let e: u64 = 500;
    insert_graph_edge_with_source(&server.store, 600, e as i64, "DependsOn", "agent").await;

    // Emit a flip-style record (mirrors the handler's step-6 audit).
    server.audit_fire_and_forget(AuditEvent {
        operation: "context_deprecate".to_string(),
        target_ids: vec![e],
        outcome: Outcome::Success,
        ..AuditEvent::default()
    });

    let removed = delete_agent_edges_for_entry(&server.store, e)
        .await
        .expect("delete ok");
    server.emit_edge_cleanup_audit(e, &removed, "s".into(), "a".into(), "c".into());
    audit_settle().await;

    let flip: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE operation = 'context_deprecate'")
            .fetch_one(server.store.read_pool_test())
            .await
            .unwrap();
    let cleanup: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_log WHERE operation = 'context_deprecate.edge_cleanup'",
    )
    .fetch_one(server.store.read_pool_test())
    .await
    .unwrap();
    assert_eq!(flip.0, 1, "one distinct flip record");
    assert_eq!(cleanup.0, 1, "one distinct cleanup record");
}

/// No cleanup event on an empty removal (zero agent edges): `emit` is guarded on
/// `!is_empty()`, so a `Some(0)` deprecation writes NO cleanup record even though
/// the response advisory would render `0`.
#[tokio::test(flavor = "multi_thread")]
async fn test_no_cleanup_audit_when_zero_agent_edges() {
    let server = make_server().await;
    let e: u64 = 500;

    // Nothing to remove; the delete returns empty and emit is a no-op on empty.
    let removed = delete_agent_edges_for_entry(&server.store, e)
        .await
        .expect("delete ok");
    assert!(removed.is_empty());
    server.emit_edge_cleanup_audit(e, &removed, "s".into(), "a".into(), "c".into());
    audit_settle().await;

    let rows = read_cleanup_rows(&server).await;
    assert!(
        rows.is_empty(),
        "no cleanup record when zero agent edges removed"
    );
}

/// R-10 / NFR-03: high-degree entry — every removed tuple is carried in the
/// metadata array, and `detail` reports the full count.
#[tokio::test(flavor = "multi_thread")]
async fn test_high_degree_audit_metadata_carries_all_tuples() {
    let server = make_server().await;
    let e: u64 = 500;
    let n: i64 = 50;
    for i in 0..n {
        // Distinct inbound neighbors keep every edge unique.
        insert_graph_edge_with_source(&server.store, 1000 + i, e as i64, "DependsOn", "agent")
            .await;
    }

    let removed = delete_agent_edges_for_entry(&server.store, e)
        .await
        .expect("delete ok");
    assert_eq!(removed.len(), n as usize);
    server.emit_edge_cleanup_audit(e, &removed, "s".into(), "a".into(), "c".into());
    audit_settle().await;

    let rows = read_cleanup_rows(&server).await;
    assert_eq!(rows.len(), 1);
    let tuples = parse_metadata_tuples(&rows[0].2);
    assert_eq!(
        tuples.len(),
        n as usize,
        "all 50 tuples present in metadata"
    );
    assert!(rows[0].1.contains("50"), "detail count == 50");
}
