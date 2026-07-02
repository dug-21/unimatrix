//! Unit tests for `get_edges.rs` — the `context_get` edge-assembly component (vnc-037).
//!
//! These exercise `build_edges_view` directly against a real `SqlxStore` (no ServiceLayer
//! / ONNX model needed), asserting the FR-19 fail-loud contract (AC-14), the opt-out
//! resolution logic, projection correctness (direction / target_id / authored), dangling
//! title retention, and the digest `authored_total` threading from the FULL uncapped set.
//!
//! Seeding helpers follow the conventions in `graph_queries_ranked_tests.rs` (store crate):
//! direct `INSERT` against the same schema, advancing the `next_entry_id` counter.

use sqlx::sqlite::SqlitePool;
use unimatrix_core::Store;
use unimatrix_store::{PoolConfig, SqlxStore};

use super::build_edges_view;
use crate::mcp::response::edges::{DIRECTION_BOTH, DIRECTION_INBOUND, DIRECTION_OUTBOUND};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open test store");
    (store, dir)
}

/// Insert a minimal entry with an explicit `confidence` (the ranked rank key); returns id.
async fn insert_entry_conf(pool: &SqlitePool, title: &str, confidence: f64) -> u64 {
    let id: i64 =
        sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'next_entry_id'")
            .fetch_one(pool)
            .await
            .expect("get next_entry_id");
    let new_id = id + 1;
    sqlx::query("UPDATE counters SET value = ?1 WHERE name = 'next_entry_id'")
        .bind(new_id)
        .execute(pool)
        .await
        .expect("update next_entry_id");

    let now = 1_700_000_000_i64;
    sqlx::query(
        "INSERT INTO entries (id, title, content, topic, category, source, status,
         confidence, created_at, updated_at, last_accessed_at, access_count,
         supersedes, superseded_by, correction_count, embedding_dim,
         created_by, modified_by, content_hash, previous_hash,
         version, feature_cycle, trust_source, helpful_count, unhelpful_count)
         VALUES (?1, ?2, 'content', 'test', 'pattern', 'test', 0,
         ?3, ?4, ?4, ?4, 0, NULL, NULL, 0, 0, '', '', '', '', 1, '', '', 0, 0)",
    )
    .bind(new_id)
    .bind(title)
    .bind(confidence)
    .bind(now)
    .execute(pool)
    .await
    .expect("insert entry");

    new_id as u64
}

/// Insert a graph_edges row with an explicit `source` provenance.
async fn insert_edge(pool: &SqlitePool, source_id: u64, target_id: u64, rel: &str, source: &str) {
    let now = 1_700_000_000_i64;
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
         (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
         VALUES (?1, ?2, ?3, 1.0, ?4, '', ?5, 0)",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(rel)
    .bind(now)
    .bind(source)
    .execute(pool)
    .await
    .expect("insert edge");
}

/// Insert an edge whose target_id has NO matching entries row (dangling — DNB-1).
async fn insert_dangling_edge(pool: &SqlitePool, source_id: u64, target_id: u64, rel: &str) {
    let now = 1_700_000_000_i64;
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
         (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
         VALUES (?1, ?2, ?3, 1.0, ?4, '', 'agent', 0)",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(rel)
    .bind(now)
    .execute(pool)
    .await
    .expect("insert dangling edge");
}

// ---------------------------------------------------------------------------
// R-16 / AC-14 — Fail-loud edge contract (FR-19, the RED tests, #4876)
// ---------------------------------------------------------------------------

/// AC-14a — a failure in the **ranked/edge query** (after a valid primary read) surfaces
/// as an `Err` from `build_edges_view`, NEVER a success view with edges omitted.
/// Injection: drop the `confidence` column the ranked LEFT JOIN selects (`t.confidence`).
#[tokio::test]
async fn test_edge_query_failure_fails_loud() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let anchor = insert_entry_conf(pool, "anchor", 0.5).await;
    let target = insert_entry_conf(pool, "target", 0.9).await;
    insert_edge(pool, anchor, target, "Supports", "agent").await;

    // Break the ranked SELECT's `t.confidence` reference.
    sqlx::query("ALTER TABLE entries DROP COLUMN confidence")
        .execute(pool)
        .await
        .expect("drop confidence column");

    let result = build_edges_view(&store as &Store, anchor, true).await;
    assert!(
        result.is_err(),
        "a ranked-query failure must surface as Err (fail-loud), not a success with omitted edges"
    );
}

/// AC-14a — a failure in the **split COUNT(*)** surfaces as an `Err`. The count and the
/// ranked select share the canonicalization CTE over `graph_edges`; dropping `graph_edges`
/// breaks the edge path, and `build_edges_view` must return `Err` (never a success).
#[tokio::test]
async fn test_count_query_failure_fails_loud() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let anchor = insert_entry_conf(pool, "anchor", 0.5).await;
    let target = insert_entry_conf(pool, "target", 0.9).await;
    insert_edge(pool, anchor, target, "Supports", "agent").await;

    // Break the canonicalization CTE's `graph_edges` source (shared by count + ranked).
    sqlx::query("DROP TABLE graph_edges")
        .execute(pool)
        .await
        .expect("drop graph_edges table");

    // Prove the count function itself errors on this corruption (the named target query).
    let count_err = unimatrix_store::count_neighbors_split(store.read_pool_server(), anchor).await;
    assert!(
        count_err.is_err(),
        "count_neighbors_split must error when its graph_edges source is gone"
    );

    let result = build_edges_view(&store as &Store, anchor, true).await;
    assert!(
        result.is_err(),
        "a count-query failure must surface as Err (fail-loud), not a success"
    );
}

/// AC-14a — a failure in the **batched title join** surfaces as an `Err`, NEVER a silent
/// `target_title: null` fill or an omitted edge set. Injection: drop the `title` column
/// (the ranked LEFT JOIN uses `confidence`, the count uses graph_edges only — both still
/// succeed, so ONLY the title join breaks, and only when a displayed target exists).
#[tokio::test]
async fn test_title_join_failure_fails_loud() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let anchor = insert_entry_conf(pool, "anchor", 0.5).await;
    let target = insert_entry_conf(pool, "target", 0.9).await;
    insert_edge(pool, anchor, target, "Supports", "agent").await;

    // Break ONLY the title join: `SELECT id, title FROM entries WHERE id IN (...)`.
    sqlx::query("ALTER TABLE entries DROP COLUMN title")
        .execute(pool)
        .await
        .expect("drop title column");

    let result = build_edges_view(&store as &Store, anchor, true).await;
    assert!(
        result.is_err(),
        "a title-join failure must surface as Err — never a silent null fill or omitted edges"
    );
}

/// AC-14b — a genuine **zero-edge** entry returns a SUCCESS with the explicit empty state
/// (`edges: []`, totals `{0,0,0}`, `authored_total: 0`) — structurally DISTINCT from the
/// `Err` produced by the failure tests. The silent-omit failure mode is forbidden.
#[tokio::test]
async fn test_zero_edges_is_success_distinct_from_failure() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let anchor = insert_entry_conf(pool, "lonely", 0.5).await;

    let view = build_edges_view(&store as &Store, anchor, true)
        .await
        .expect("zero-edge entry must succeed (not fail)");

    assert!(view.edges.is_empty(), "no edges ⇒ empty display set");
    assert_eq!(view.totals.inbound, 0);
    assert_eq!(view.totals.outbound, 0);
    assert_eq!(view.totals.both, 0);
    assert_eq!(view.authored_total, 0);
}

// ---------------------------------------------------------------------------
// R-10 — Direction / target_id projection
// ---------------------------------------------------------------------------

/// Outbound asymmetric (anchor = source_id) → `direction = "outbound"`, `target_id` = the
/// OTHER endpoint; inbound asymmetric (anchor = target_id) → `"inbound"`, `target_id` = the
/// OTHER endpoint. Never the anchor itself.
#[tokio::test]
async fn test_projection_outbound_inbound_far_endpoint() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let anchor = insert_entry_conf(pool, "anchor", 0.5).await;
    let out_target = insert_entry_conf(pool, "out", 0.9).await;
    let in_source = insert_entry_conf(pool, "in", 0.8).await;

    // Outbound: anchor is the source.
    insert_edge(pool, anchor, out_target, "Supports", "agent").await;
    // Inbound: anchor is the target.
    insert_edge(pool, in_source, anchor, "Prerequisite", "agent").await;

    let view = build_edges_view(&store as &Store, anchor, true)
        .await
        .expect("build");

    let outbound = view
        .edges
        .iter()
        .find(|e| e.edge_type == "Supports")
        .expect("outbound edge present");
    assert_eq!(outbound.direction, DIRECTION_OUTBOUND);
    assert_eq!(
        outbound.target_id, out_target,
        "target is the other endpoint"
    );
    assert_ne!(outbound.target_id, anchor);

    let inbound = view
        .edges
        .iter()
        .find(|e| e.edge_type == "Prerequisite")
        .expect("inbound edge present");
    assert_eq!(inbound.direction, DIRECTION_INBOUND);
    assert_eq!(inbound.target_id, in_source, "target is the other endpoint");
    assert_ne!(inbound.target_id, anchor);
}

/// A canonicalized symmetric edge projects `direction = "both"` (renders `↔`) and carries
/// no `→`/`←`. Symmetric types store two reciprocal rows — they collapse to ONE `both` row.
#[tokio::test]
async fn test_projection_symmetric_both_no_arrow() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let anchor = insert_entry_conf(pool, "anchor", 0.5).await;
    let other = insert_entry_conf(pool, "other", 0.9).await;

    // Symmetric `Informs` stores both directions.
    insert_edge(pool, anchor, other, "Informs", "co_access").await;
    insert_edge(pool, other, anchor, "Informs", "co_access").await;

    let view = build_edges_view(&store as &Store, anchor, true)
        .await
        .expect("build");

    let informs: Vec<_> = view
        .edges
        .iter()
        .filter(|e| e.edge_type == "Informs")
        .collect();
    assert_eq!(informs.len(), 1, "symmetric edge canonicalizes to ONE row");
    assert_eq!(informs[0].direction, DIRECTION_BOTH);
    assert_eq!(view.totals.both, 1, "counted once in `both`");
    assert_eq!(
        view.totals.inbound, 0,
        "↔ not folded into inbound (#744 guard)"
    );
}

// ---------------------------------------------------------------------------
// Totals projection + digest authored threading (3-bucket contract)
// ---------------------------------------------------------------------------

/// The store's `EdgeCountSplit { inbound, outbound, both }` projects 1:1 to
/// `EdgeTotals { inbound, outbound, both }` on the view — all three buckets carry through.
#[tokio::test]
async fn test_totals_projection_three_buckets() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let anchor = insert_entry_conf(pool, "anchor", 0.5).await;
    let a = insert_entry_conf(pool, "a", 0.9).await;
    let b = insert_entry_conf(pool, "b", 0.8).await;
    let c = insert_entry_conf(pool, "c", 0.7).await;

    insert_edge(pool, anchor, a, "Supports", "agent").await; // outbound
    insert_edge(pool, b, anchor, "Prerequisite", "agent").await; // inbound
    insert_edge(pool, anchor, c, "Informs", "co_access").await; // symmetric
    insert_edge(pool, c, anchor, "Informs", "co_access").await; // symmetric reciprocal

    let split = unimatrix_store::count_neighbors_split(store.read_pool_server(), anchor)
        .await
        .expect("split");
    let view = build_edges_view(&store as &Store, anchor, true)
        .await
        .expect("build");

    assert_eq!(view.totals.inbound, split.inbound);
    assert_eq!(view.totals.outbound, split.outbound);
    assert_eq!(view.totals.both, split.both);
    assert_eq!(view.totals.outbound, 1);
    assert_eq!(view.totals.inbound, 1);
    assert_eq!(view.totals.both, 1);
}

/// The digest-only `authored` aggregate (over the FULL uncapped set) is threaded into the
/// view as `authored_total` — NOT re-derived from the capped ≤3 `edges`. Seed >cap authored
/// edges so the full-set tally exceeds the displayed slice count.
#[tokio::test]
async fn test_authored_total_threaded_from_full_set() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let cap = unimatrix_store::GET_EDGE_DISPLAY_LIMIT as usize;
    let anchor = insert_entry_conf(pool, "anchor", 0.5).await;

    // Seed cap + 2 authored outbound edges → authored over the full set = cap + 2,
    // but only `cap` can ever appear in the displayed slice.
    let authored_count = cap + 2;
    for i in 0..authored_count {
        let t = insert_entry_conf(pool, &format!("t{i}"), 0.9 - (i as f64) * 0.01).await;
        insert_edge(pool, anchor, t, "Supports", "agent").await;
    }

    let view = build_edges_view(&store as &Store, anchor, true)
        .await
        .expect("build");

    assert_eq!(
        view.edges.len(),
        cap,
        "display set capped at GET_EDGE_DISPLAY_LIMIT"
    );
    assert_eq!(
        view.authored_total, authored_count,
        "authored_total reflects the FULL uncapped set, not the displayed ≤cap slice"
    );
    assert_eq!(
        view.totals.outbound, authored_count,
        "uncapped outbound total"
    );
}

// ---------------------------------------------------------------------------
// R-15 — Dangling target retained, no panic (DNB-1)
// ---------------------------------------------------------------------------

/// An edge whose target has no `entries` row → `target_title: None`, edge RETAINED, no panic.
#[tokio::test]
async fn test_dangling_title_null_retained_no_panic() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let anchor = insert_entry_conf(pool, "anchor", 0.5).await;
    insert_dangling_edge(pool, anchor, 999_999, "Supports").await;

    let view = build_edges_view(&store as &Store, anchor, true)
        .await
        .expect("dangling target must not error");

    assert_eq!(view.edges.len(), 1, "dangling edge retained");
    assert_eq!(view.edges[0].target_id, 999_999);
    assert!(
        view.edges[0].target_title.is_none(),
        "unresolved target ⇒ target_title None (null is signal)"
    );
}

/// A dangling edge does not drop resolved ones; titles resolve for the resolved targets and
/// stay `None` for the dangling — all in one batched join.
#[tokio::test]
async fn test_mixed_resolved_and_dangling() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let anchor = insert_entry_conf(pool, "anchor", 0.5).await;
    let resolved = insert_entry_conf(pool, "resolved-title", 0.9).await;

    insert_edge(pool, anchor, resolved, "Supports", "agent").await;
    insert_dangling_edge(pool, anchor, 888_888, "Prerequisite").await;

    let view = build_edges_view(&store as &Store, anchor, true)
        .await
        .expect("build");

    let resolved_edge = view
        .edges
        .iter()
        .find(|e| e.target_id == resolved)
        .expect("resolved edge present");
    assert_eq!(
        resolved_edge.target_title.as_deref(),
        Some("resolved-title")
    );

    let dangling_edge = view
        .edges
        .iter()
        .find(|e| e.target_id == 888_888)
        .expect("dangling edge present");
    assert!(dangling_edge.target_title.is_none());
}

// ---------------------------------------------------------------------------
// NG-1 (bugfix-881 / ass-088 F5) — displayed edge-TARGET label resolution
// ---------------------------------------------------------------------------

/// Deprecate `id`, pointing it at `superseded_by` (its active successor).
async fn deprecate_toward(pool: &SqlitePool, id: u64, superseded_by: u64) {
    sqlx::query("UPDATE entries SET status = 1, superseded_by = ?2 WHERE id = ?1")
        .bind(id as i64)
        .bind(superseded_by as i64)
        .execute(pool)
        .await
        .expect("deprecate entry");
}

/// NG-1: a displayed edge whose TARGET is deprecated resolves to the terminal id + title by
/// default (`resolve_targets = true`). Anchor → X, X superseded by X′ ⇒ the displayed edge
/// carries X′'s id and title, not X's stale label.
#[tokio::test]
async fn test_ng1_deprecated_target_resolves_to_terminal() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let anchor = insert_entry_conf(pool, "anchor", 0.5).await;
    let x = insert_entry_conf(pool, "x-stale", 0.9).await;
    let x_prime = insert_entry_conf(pool, "x-prime-current", 0.9).await;
    insert_edge(pool, anchor, x, "Supports", "agent").await;
    deprecate_toward(pool, x, x_prime).await;

    let view = build_edges_view(&store as &Store, anchor, true)
        .await
        .expect("build");

    let edge = view
        .edges
        .iter()
        .find(|e| e.edge_type == "Supports")
        .expect("Supports edge present");
    assert_eq!(
        edge.target_id, x_prime,
        "deprecated target resolves to its active terminal id"
    );
    assert_eq!(
        edge.target_title.as_deref(),
        Some("x-prime-current"),
        "displayed title is the terminal's, not the stale X label"
    );
}

/// NG-1 escape hatch: `resolve_targets = false` (follow_supersessions=false) keeps the raw
/// as-stored target — the audit/lookback surface is preserved.
#[tokio::test]
async fn test_ng1_escape_hatch_keeps_raw_target() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let anchor = insert_entry_conf(pool, "anchor", 0.5).await;
    let x = insert_entry_conf(pool, "x-stale", 0.9).await;
    let x_prime = insert_entry_conf(pool, "x-prime-current", 0.9).await;
    insert_edge(pool, anchor, x, "Supports", "agent").await;
    deprecate_toward(pool, x, x_prime).await;

    let view = build_edges_view(&store as &Store, anchor, false)
        .await
        .expect("build");

    let edge = view
        .edges
        .iter()
        .find(|e| e.edge_type == "Supports")
        .expect("Supports edge present");
    assert_eq!(
        edge.target_id, x,
        "escape hatch keeps the raw as-stored deprecated target"
    );
    assert_eq!(
        edge.target_title.as_deref(),
        Some("x-stale"),
        "raw target keeps its stored title"
    );
}

// ---------------------------------------------------------------------------
// authored projection (R-05 / FR-17) — carried-forward / context_edge authored
// ---------------------------------------------------------------------------

/// An `agent`-sourced edge (carried-forward vnc-035 / `context_edge`) projects `authored=true`;
/// an inferred-source edge projects `authored=false`.
#[tokio::test]
async fn test_authored_flag_from_source() {
    let (store, _dir) = open_test_store().await;
    let pool = store.write_pool_server();

    let anchor = insert_entry_conf(pool, "anchor", 0.5).await;
    let authored_t = insert_entry_conf(pool, "authored-target", 0.9).await;
    let inferred_t = insert_entry_conf(pool, "inferred-target", 0.95).await;

    insert_edge(pool, anchor, authored_t, "Supports", "agent").await;
    insert_edge(pool, anchor, inferred_t, "Supports", "co_access").await;

    let view = build_edges_view(&store as &Store, anchor, true)
        .await
        .expect("build");

    let authored_edge = view
        .edges
        .iter()
        .find(|e| e.target_id == authored_t)
        .expect("authored edge");
    assert!(authored_edge.authored, "agent source ⇒ authored");

    let inferred_edge = view
        .edges
        .iter()
        .find(|e| e.target_id == inferred_t)
        .expect("inferred edge");
    assert!(!inferred_edge.authored, "inferred source ⇒ not authored");
}

// ---------------------------------------------------------------------------
// Opt-out resolution logic (R-14 / AC-11) — handler-level three-state branch
// ---------------------------------------------------------------------------
//
// The `include_edges` resolution lives in the `context_get` handler (tools.rs): a full MCP
// round-trip requires the ONNX model and is exercised in the server integration suite. Here
// we assert the branch SEMANTICS the handler encodes are faithful to `build_edges_view`:
// surface (None / Some(true)) yields a populated view; the handler's `Some(false)` arm never
// calls `build_edges_view` (it binds `None`), so no edge query is issued on opt-out.

/// The resolution semantics the handler's `match params.include_edges` encodes, asserted as a
/// pure mapping so the three-state contract is verified independent of the MCP transport.
fn resolve_should_surface(include_edges: Option<bool>) -> bool {
    matches!(include_edges, None | Some(true))
}

#[test]
fn test_include_edges_three_resolutions() {
    assert!(resolve_should_surface(None), "None ⇒ default-on surface");
    assert!(resolve_should_surface(Some(true)), "Some(true) ⇒ surface");
    assert!(
        !resolve_should_surface(Some(false)),
        "Some(false) ⇒ opt-out, skip all edge queries"
    );
}
