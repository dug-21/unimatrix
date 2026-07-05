//! DB-integration tests for `delete_agent_edges_for_entry` (crt-058).
//!
//! Included into `edge_write.rs` via `#[path]` so this file is a submodule of
//! `edge_write` and can reach the helper, `RemovedEdge`, and `EDGE_SOURCE_AGENT`
//! through `super::*`. Seeding goes through the ONE shared
//! `insert_graph_edge_with_source` helper (promoted to `pub(crate)` in
//! `background.rs`) so these tests and the later eager ⊆ tick subset test seed
//! from an identical fixture — a copy would drift and defeat the subset
//! assertion (R-02 fixture identity).

use super::{delete_agent_edges_for_entry, EDGE_SOURCE_AGENT};

use crate::background::insert_graph_edge_with_source;
use sqlx::Row;
use tempfile::TempDir;
use unimatrix_store::test_helpers::open_test_store;
use unimatrix_store::SqlxStore;

// ── local count helpers (test-only) ─────────────────────────────────────────

/// Count every `graph_edges` row touching `id` in either direction (all sources).
async fn count_edges_touching(store: &SqlxStore, id: i64) -> i64 {
    sqlx::query("SELECT COUNT(*) FROM graph_edges WHERE source_id = ?1 OR target_id = ?1")
        .bind(id)
        .fetch_one(store.write_pool_server())
        .await
        .expect("count query must succeed")
        .get::<i64, _>(0)
}

/// Count `source = 'agent'` rows touching `id` in either direction.
async fn count_agent_edges_touching(store: &SqlxStore, id: i64) -> i64 {
    sqlx::query(
        "SELECT COUNT(*) FROM graph_edges \
         WHERE (source_id = ?1 OR target_id = ?1) AND source = 'agent'",
    )
    .bind(id)
    .fetch_one(store.write_pool_server())
    .await
    .expect("count query must succeed")
    .get::<i64, _>(0)
}

/// True if a row with the given `source` value touches `id`.
async fn source_present_touching(store: &SqlxStore, id: i64, source: &str) -> bool {
    let n: i64 = sqlx::query(
        "SELECT COUNT(*) FROM graph_edges \
         WHERE (source_id = ?1 OR target_id = ?1) AND source = ?2",
    )
    .bind(id)
    .bind(source)
    .fetch_one(store.write_pool_server())
    .await
    .expect("count query must succeed")
    .get::<i64, _>(0);
    n > 0
}

// ── AC-01 / FR-01 — both directions removed ─────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_delete_agent_edges_for_entry_removes_inbound_and_outbound_returns_ok() {
    let tmp = TempDir::new().expect("tempdir");
    let store = open_test_store(&tmp).await;
    let e: i64 = 100;

    // Outbound: source_id = E.
    insert_graph_edge_with_source(&store, e, 200, "Supersedes", EDGE_SOURCE_AGENT).await;
    // Inbound: target_id = E.
    insert_graph_edge_with_source(&store, 300, e, "RelatesTo", EDGE_SOURCE_AGENT).await;

    let removed = delete_agent_edges_for_entry(&store, e as u64)
        .await
        .expect("eager delete must succeed");

    assert_eq!(removed.len(), 2, "both inbound and outbound agent edges removed");
    assert_eq!(
        count_agent_edges_touching(&store, e).await,
        0,
        "no agent edges touching E remain after delete"
    );

    // Tuple capture correctness: both directions present in the returned set.
    let has_outbound = removed.iter().any(|r| r.source_id == 100 && r.target_id == 200);
    let has_inbound = removed.iter().any(|r| r.source_id == 300 && r.target_id == 100);
    assert!(has_outbound, "outbound tuple (100->200) captured");
    assert!(has_inbound, "inbound tuple (300->100) captured");
}

// ── AC-04(a) / FR-02 / R-09 — per-source removal matrix ─────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_delete_agent_edges_only_removes_agent_source() {
    let tmp = TempDir::new().expect("tempdir");
    let store = open_test_store(&tmp).await;
    let e: i64 = 100;

    // Exactly one edge of EACH source touching E, mixing inbound / outbound.
    insert_graph_edge_with_source(&store, e, 1, "Supersedes", "agent").await; // outbound, THE one
    insert_graph_edge_with_source(&store, 2, e, "RelatesTo", "nli").await; // inbound
    insert_graph_edge_with_source(&store, e, 3, "CoAccess", "co_access").await;
    insert_graph_edge_with_source(&store, 4, e, "Supports", "cosine_supports").await;
    insert_graph_edge_with_source(&store, e, 5, "RelatesTo", "S1").await;
    insert_graph_edge_with_source(&store, 6, e, "RelatesTo", "S2").await;
    insert_graph_edge_with_source(&store, e, 7, "RelatesTo", "S8").await;

    let removed = delete_agent_edges_for_entry(&store, e as u64)
        .await
        .expect("eager delete must succeed");

    // Only the single agent edge is returned.
    assert_eq!(removed.len(), 1, "exactly one agent edge removed");
    assert_eq!(removed[0].source_id, 100);
    assert_eq!(removed[0].target_id, 1);
    assert_eq!(removed[0].relation_type, "Supersedes");

    // Every machine source remains — a newly-added source surfaces here as
    // "not removed" (enumeration-bound + subset-safe completeness).
    for src in ["nli", "co_access", "cosine_supports", "S1", "S2", "S8"] {
        assert!(
            source_present_touching(&store, e, src).await,
            "machine source '{src}' must remain after agent-only delete"
        );
    }
    assert_eq!(
        count_edges_touching(&store, e).await,
        6,
        "6 machine edges remain; only the agent edge is gone"
    );
}

// ── R-03 — atomic single-statement DELETE … RETURNING ───────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_delete_returning_is_single_statement_capture() {
    let tmp = TempDir::new().expect("tempdir");
    let store = open_test_store(&tmp).await;
    let e: i64 = 42;

    insert_graph_edge_with_source(&store, e, 1, "Supersedes", "agent").await;
    insert_graph_edge_with_source(&store, 2, e, "RelatesTo", "agent").await;
    insert_graph_edge_with_source(&store, e, 3, "CoAccess", "agent").await;

    // Pre-count of rows the LOCKED predicate matches.
    let pre = count_agent_edges_touching(&store, e).await;
    assert_eq!(pre, 3);

    let removed = delete_agent_edges_for_entry(&store, e as u64)
        .await
        .expect("eager delete must succeed");

    // ONE call both deleted and captured the SAME set: returned count equals the
    // pre-count of matching rows, and none remain. No delete-then-separate-SELECT
    // window could produce this equality.
    assert_eq!(removed.len() as i64, pre, "captured tuples == rows that matched");
    assert_eq!(
        count_agent_edges_touching(&store, e).await,
        0,
        "all matched rows are gone after the single statement"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_count_source_of_truth_is_tuples_len_not_rows_affected() {
    let tmp = TempDir::new().expect("tempdir");
    let store = open_test_store(&tmp).await;
    let e: i64 = 77;

    for t in 0..5 {
        insert_graph_edge_with_source(&store, e, 1000 + t, "RelatesTo", "agent").await;
    }
    let expected = count_agent_edges_touching(&store, e).await;

    let removed = delete_agent_edges_for_entry(&store, e as u64)
        .await
        .expect("eager delete must succeed");

    // Count is derived from the tuples (needed for audit), and equals the rows
    // removed — pinning `tuples.len()` as the single source of truth.
    assert_eq!(removed.len() as i64, expected, "count == tuples.len()");
    assert_eq!(removed.len(), 5);
}

// ── R-07 — zero-row tolerance ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_delete_agent_edges_empty_match_returns_ok_empty() {
    let tmp = TempDir::new().expect("tempdir");
    let store = open_test_store(&tmp).await;
    let e: i64 = 500;

    // Only machine edges touch E — nothing for the agent-only predicate to match.
    insert_graph_edge_with_source(&store, e, 1, "RelatesTo", "nli").await;
    insert_graph_edge_with_source(&store, 2, e, "CoAccess", "co_access").await;

    let removed = delete_agent_edges_for_entry(&store, e as u64)
        .await
        .expect("empty match is Ok, not an error");

    assert!(removed.is_empty(), "no agent edges → Ok(vec![])");
    assert_eq!(
        count_edges_touching(&store, e).await,
        2,
        "machine edges untouched"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_delete_agent_edges_no_edges_at_all_returns_ok_empty() {
    let tmp = TempDir::new().expect("tempdir");
    let store = open_test_store(&tmp).await;

    let removed = delete_agent_edges_for_entry(&store, 9999)
        .await
        .expect("no edges at all is Ok, not an error");

    assert!(removed.is_empty());
}

// ── R-10 — self-loop counted once & high-degree ─────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_self_loop_agent_edge_removed_and_counted_once() {
    let tmp = TempDir::new().expect("tempdir");
    let store = open_test_store(&tmp).await;
    let e: i64 = 314;

    // source_id == target_id == E. The OR predicate matches this row ONCE.
    insert_graph_edge_with_source(&store, e, e, "RelatesTo", "agent").await;

    let removed = delete_agent_edges_for_entry(&store, e as u64)
        .await
        .expect("eager delete must succeed");

    assert_eq!(
        removed.len(),
        1,
        "self-loop matched by (source OR target) is returned exactly once, not doubled"
    );
    assert_eq!(removed[0].source_id, 314);
    assert_eq!(removed[0].target_id, 314);
    assert_eq!(
        count_agent_edges_touching(&store, e).await,
        0,
        "self-loop gone"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_high_degree_entry_all_agent_edges_removed() {
    let tmp = TempDir::new().expect("tempdir");
    let store = open_test_store(&tmp).await;
    let e: i64 = 1000;

    // 50 agent edges, mixed directions, all distinct neighbors (never == E).
    for i in 1..=50i64 {
        if i % 2 == 0 {
            insert_graph_edge_with_source(&store, e, i, "RelatesTo", "agent").await;
        } else {
            insert_graph_edge_with_source(&store, i, e, "RelatesTo", "agent").await;
        }
    }
    // A few machine edges that must survive.
    insert_graph_edge_with_source(&store, e, 2000, "RelatesTo", "nli").await;
    insert_graph_edge_with_source(&store, 2001, e, "CoAccess", "co_access").await;

    let removed = delete_agent_edges_for_entry(&store, e as u64)
        .await
        .expect("eager delete must succeed");

    assert_eq!(removed.len(), 50, "all 50 agent edges removed in one statement");
    assert_eq!(
        count_agent_edges_touching(&store, e).await,
        0,
        "no agent edges remain"
    );
    assert!(source_present_touching(&store, e, "nli").await);
    assert!(source_present_touching(&store, e, "co_access").await);
}

// ── Edge case — shared agent edge, sequential deprecation ───────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_shared_edge_removed_by_first_deprecation() {
    let tmp = TempDir::new().expect("tempdir");
    let store = open_test_store(&tmp).await;
    let a: i64 = 10;
    let b: i64 = 20;

    // One agent edge shared by A and B.
    insert_graph_edge_with_source(&store, a, b, "RelatesTo", "agent").await;

    // Deprecate A first: it removes the shared edge, count attributed here.
    let first = delete_agent_edges_for_entry(&store, a as u64)
        .await
        .expect("first delete must succeed");
    assert_eq!(first.len(), 1, "first deprecation removes the shared edge");

    // Deprecate B next: the edge is already gone → RETURNING yields nothing.
    let second = delete_agent_edges_for_entry(&store, b as u64)
        .await
        .expect("second delete must succeed");
    assert!(second.is_empty(), "second deprecation's RETURNING omits the already-gone edge");
}

// ── NFR-01 / NFR-02 / R-02 — predicate & pool pin (structural) ──────────────

#[test]
fn test_helper_predicate_and_pool_are_locked() {
    // Pin the exact LOCKED predicate + pool against source drift (R-02): a
    // WHERE/RETURNING edit (e.g. a relation_type clause) or a switch off the
    // write pool trips this.
    const SRC: &str = include_str!("edge_write.rs");

    assert!(
        SRC.contains("WHERE (source_id = ?1 OR target_id = ?1) AND source = ?2"),
        "LOCKED WHERE predicate must be present verbatim"
    );
    assert!(
        SRC.contains("RETURNING source_id, target_id, relation_type"),
        "LOCKED RETURNING columns must be present verbatim"
    );
    // The WHERE line must END at `?2` (line-continuation backslash immediately
    // after), so nothing — no relation_type filter, no runtime superseded_by
    // clause — can be appended to the predicate without tripping this pin.
    assert!(
        SRC.contains("AND source = ?2 \\\n"),
        "WHERE predicate must terminate at `AND source = ?2` with no appended clause"
    );
    assert!(
        SRC.contains("let pool = store.write_pool_server();"),
        "helper must use write_pool_server(), not the read pool"
    );
    assert!(
        SRC.contains("run_orphaned_edge_compaction"),
        "code-adjacency comment linking the helper to the tick backstop (C-11/SR-05) must be present"
    );
}
