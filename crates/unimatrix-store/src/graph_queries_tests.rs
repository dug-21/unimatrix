//! Unit tests for `graph_queries.rs` — SQL traversal functions for context_graph (vnc-018).
//!
//! Covers:
//!   - `query_supersession_chain` (chain mode, both directions, depth cap)
//!   - `query_current_terminal` (current mode, R-20 Active filter)
//!   - `query_direct_neighbors` (neighbors mode depth=1, all directions)
//!
//! Extracted to a separate file to keep `graph_queries.rs` under the 500-line limit.

use super::*;
use crate::db::SqlxStore;
use crate::pool_config::PoolConfig;
use crate::schema::Status;

// -----------------------------------------------------------------------
// Test helpers
// -----------------------------------------------------------------------

async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open test store");
    (store, dir)
}

/// Insert a minimal entry into the `entries` table. Returns the assigned id.
async fn insert_entry(
    pool: &SqlitePool,
    title: &str,
    status: Status,
    supersedes: Option<u64>,
    superseded_by: Option<u64>,
) -> u64 {
    // Get next id from counters
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

    let status_i = match status {
        Status::Active => 0i64,
        Status::Deprecated => 1,
        Status::Proposed => 2,
        Status::Quarantined => 3,
    };
    let now = 1_700_000_000_i64;
    sqlx::query(
        "INSERT INTO entries (id, title, content, topic, category, source, status,
         confidence, created_at, updated_at, last_accessed_at, access_count,
         supersedes, superseded_by, correction_count, embedding_dim,
         created_by, modified_by, content_hash, previous_hash,
         version, feature_cycle, trust_source, helpful_count, unhelpful_count)
         VALUES (?1, ?2, 'content', 'test', 'pattern', 'test', ?3,
         0.5, ?4, ?4, ?4, 0, ?5, ?6, 0, 0, '', '', '', '', 1, '', '', 0, 0)",
    )
    .bind(new_id)
    .bind(title)
    .bind(status_i)
    .bind(now)
    .bind(supersedes.map(|v| v as i64))
    .bind(superseded_by.map(|v| v as i64))
    .execute(pool)
    .await
    .expect("insert entry");

    new_id as u64
}

/// Insert a graph_edges row with an empty `source` (default provenance).
async fn insert_edge(pool: &SqlitePool, source: u64, target: u64, rel: &str) {
    insert_edge_with_source(pool, source, target, rel, "").await;
}

/// Insert a graph_edges row with an explicit `source` provenance string (vnc-037).
async fn insert_edge_with_source(
    pool: &SqlitePool,
    source_id: u64,
    target_id: u64,
    rel: &str,
    source: &str,
) {
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

// -----------------------------------------------------------------------
// query_supersession_chain tests
// -----------------------------------------------------------------------

/// R-01, R-05: Cold-start test — no ticks, empty DB, queries the SQL path directly.
#[tokio::test]
async fn test_query_supersession_chain_empty_db_returns_empty() {
    let (store, _dir) = open_test_store().await;
    let result = query_supersession_chain(store.read_pool(), 999_999, ChainDirection::Both, 50)
        .await
        .expect("query");

    assert!(
        result.entries.is_empty(),
        "non-existent id must return empty"
    );
    assert!(!result.forward_capped);
    assert!(!result.backward_capped);
    store.close().await.unwrap();
}

/// Single isolated entry — no ancestors or descendants.
#[tokio::test]
async fn test_query_supersession_chain_single_entry() {
    let (store, _dir) = open_test_store().await;
    let pool = store.read_pool();

    let id = insert_entry(&&store.write_pool, "Entry A", Status::Active, None, None).await;

    let result = query_supersession_chain(pool, id, ChainDirection::Both, 50)
        .await
        .expect("query");

    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].id, id);
    assert!(!result.forward_capped);
    assert!(!result.backward_capped);
    store.close().await.unwrap();
}

/// AC-01: Five-entry chain A→B→C→D→E queried from C — should return all 5, oldest first.
#[tokio::test]
async fn test_query_supersession_chain_five_entry_chain_both() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    // Insert A, B, C, D, E as a chain where A is oldest, E is newest.
    // superseded_by points toward newer; supersedes points toward older.
    // A.superseded_by = B, B.superseded_by = C, C.superseded_by = D, D.superseded_by = E
    // B.supersedes = A, C.supersedes = B, D.supersedes = C, E.supersedes = D
    let a = insert_entry(wp, "A", Status::Deprecated, None, None).await;
    let b = insert_entry(wp, "B", Status::Deprecated, Some(a), None).await;
    let c = insert_entry(wp, "C", Status::Deprecated, Some(b), None).await;
    let d = insert_entry(wp, "D", Status::Deprecated, Some(c), None).await;
    let e = insert_entry(wp, "E", Status::Active, Some(d), None).await;

    // Set superseded_by links: A → B, B → C, C → D, D → E
    sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
        .bind(b as i64)
        .bind(a as i64)
        .execute(wp)
        .await
        .unwrap();
    sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
        .bind(c as i64)
        .bind(b as i64)
        .execute(wp)
        .await
        .unwrap();
    sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
        .bind(d as i64)
        .bind(c as i64)
        .execute(wp)
        .await
        .unwrap();
    sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
        .bind(e as i64)
        .bind(d as i64)
        .execute(wp)
        .await
        .unwrap();

    let result = query_supersession_chain(store.read_pool(), c, ChainDirection::Both, 50)
        .await
        .expect("query");

    assert_eq!(result.entries.len(), 5, "all 5 entries must be returned");
    let ids: Vec<u64> = result.entries.iter().map(|e| e.id).collect();
    // Oldest (A) must come before seed (C); seed before newest (E).
    let pos_a = ids.iter().position(|&x| x == a).unwrap();
    let pos_c = ids.iter().position(|&x| x == c).unwrap();
    let pos_e = ids.iter().position(|&x| x == e).unwrap();
    assert!(pos_a < pos_c, "A must come before C");
    assert!(pos_c < pos_e, "C must come before E");
    assert!(!result.forward_capped);
    assert!(!result.backward_capped);
    store.close().await.unwrap();
}

/// AC-02: Forward direction only returns seed + descendants.
#[tokio::test]
async fn test_query_supersession_chain_direction_forward_only() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry(wp, "A", Status::Deprecated, None, None).await;
    let b = insert_entry(wp, "B", Status::Deprecated, Some(a), None).await;
    let c = insert_entry(wp, "C", Status::Active, Some(b), None).await;

    sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
        .bind(b as i64)
        .bind(a as i64)
        .execute(wp)
        .await
        .unwrap();
    sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
        .bind(c as i64)
        .bind(b as i64)
        .execute(wp)
        .await
        .unwrap();

    // Query forward from A — should get A (seed) and B, C (descendants that supersede A).
    let result = query_supersession_chain(store.read_pool(), a, ChainDirection::Forward, 50)
        .await
        .expect("query");

    let ids: Vec<u64> = result.entries.iter().map(|e| e.id).collect();
    assert!(ids.contains(&a), "seed must be in forward result");
    assert!(ids.contains(&b), "B must be in forward result");
    assert!(ids.contains(&c), "C must be in forward result");
    assert!(!result.forward_capped);
    store.close().await.unwrap();
}

/// AC-02: Backward direction only returns seed + ancestors.
#[tokio::test]
async fn test_query_supersession_chain_direction_backward_only() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry(wp, "A", Status::Deprecated, None, None).await;
    let b = insert_entry(wp, "B", Status::Deprecated, Some(a), None).await;
    let c = insert_entry(wp, "C", Status::Active, Some(b), None).await;

    sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
        .bind(b as i64)
        .bind(a as i64)
        .execute(wp)
        .await
        .unwrap();
    sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
        .bind(c as i64)
        .bind(b as i64)
        .execute(wp)
        .await
        .unwrap();

    // Query backward from C — should get C (seed), B, and A (ancestors).
    let result = query_supersession_chain(store.read_pool(), c, ChainDirection::Backward, 50)
        .await
        .expect("query");

    let ids: Vec<u64> = result.entries.iter().map(|e| e.id).collect();
    assert!(ids.contains(&a), "A must be in backward result");
    assert!(ids.contains(&b), "B must be in backward result");
    assert!(ids.contains(&c), "seed must be in backward result");
    assert!(!result.backward_capped);
    store.close().await.unwrap();
}

/// AC-04: Non-existent ID returns empty result, not an error.
#[tokio::test]
async fn test_query_supersession_chain_nonexistent_id() {
    let (store, _dir) = open_test_store().await;
    let result = query_supersession_chain(store.read_pool(), 999_999, ChainDirection::Both, 50)
        .await
        .expect("no error for non-existent id");

    assert!(result.entries.is_empty());
    assert!(!result.forward_capped);
    assert!(!result.backward_capped);
    store.close().await.unwrap();
}

// -----------------------------------------------------------------------
// query_current_terminal tests
// -----------------------------------------------------------------------

/// Existing active entry with no superseded_by returns Some(that entry).
#[tokio::test]
async fn test_query_current_terminal_active_entry_returns_some() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let id = insert_entry(wp, "Active Entry", Status::Active, None, None).await;

    let result = query_current_terminal(store.read_pool(), id)
        .await
        .expect("query");

    assert!(result.is_some(), "active entry must return Some");
    assert_eq!(result.unwrap().id, id);
    store.close().await.unwrap();
}

/// Orphaned deprecated entry (superseded_by IS NULL, status=Deprecated) returns None.
/// This is the R-20 Critical risk test — validates AND e.status='Active' filter.
#[tokio::test]
async fn test_query_current_terminal_orphaned_deprecated_returns_none() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    // Insert a deprecated entry with no superseded_by — orphaned deprecated terminal.
    let id = insert_entry(wp, "Orphaned Deprecated", Status::Deprecated, None, None).await;

    let result = query_current_terminal(store.read_pool(), id)
        .await
        .expect("query");

    assert!(
        result.is_none(),
        "orphaned deprecated entry (superseded_by IS NULL, status=Deprecated) must return None \
         — AND e.status='Active' filter is mandatory (R-20 Critical)"
    );
    store.close().await.unwrap();
}

/// Non-existent ID returns None.
#[tokio::test]
async fn test_query_current_terminal_nonexistent_id_returns_none() {
    let (store, _dir) = open_test_store().await;

    let result = query_current_terminal(store.read_pool(), 999_999)
        .await
        .expect("query");

    assert!(result.is_none(), "non-existent id must return None");
    store.close().await.unwrap();
}

/// Deprecated entry with active successor: returns the active successor.
#[tokio::test]
async fn test_query_current_terminal_deprecated_with_active_successor() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let old = insert_entry(wp, "Old Entry", Status::Deprecated, None, None).await;
    let new = insert_entry(wp, "New Entry", Status::Active, Some(old), None).await;

    // Set superseded_by on old → new
    sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
        .bind(new as i64)
        .bind(old as i64)
        .execute(wp)
        .await
        .unwrap();

    let result = query_current_terminal(store.read_pool(), old)
        .await
        .expect("query");

    assert!(result.is_some(), "must resolve to active successor");
    assert_eq!(result.unwrap().id, new);
    store.close().await.unwrap();
}

// -----------------------------------------------------------------------
// query_direct_neighbors tests
// -----------------------------------------------------------------------

/// AC-08: Outgoing edges with specific type.
#[tokio::test]
async fn test_query_direct_neighbors_outgoing_specific_type() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let x = insert_entry(wp, "X", Status::Active, None, None).await;
    let y = insert_entry(wp, "Y", Status::Active, None, None).await;
    let z = insert_entry(wp, "Z", Status::Active, None, None).await;

    insert_edge(wp, x, y, "Prerequisite").await;
    insert_edge(wp, x, z, "Prerequisite").await;

    let rows = query_direct_neighbors(
        store.read_pool(),
        x,
        &["Prerequisite"],
        NeighborDirection::Outgoing,
    )
    .await
    .expect("query");

    assert_eq!(rows.len(), 2);
    let targets: Vec<u64> = rows.iter().map(|r| r.target_id).collect();
    assert!(targets.contains(&y));
    assert!(targets.contains(&z));
    assert!(rows.iter().all(|r| r.source_id == x));
    assert!(rows.iter().all(|r| r.relation_type == "Prerequisite"));
    store.close().await.unwrap();
}

/// AC-09: Incoming edges with specific type.
#[tokio::test]
async fn test_query_direct_neighbors_incoming_specific_type() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let x = insert_entry(wp, "X", Status::Active, None, None).await;
    let y = insert_entry(wp, "Y", Status::Active, None, None).await;
    let z_entry = insert_entry(wp, "Z", Status::Active, None, None).await;

    insert_edge(wp, y, x, "Supports").await;
    insert_edge(wp, z_entry, x, "Supports").await;

    let rows = query_direct_neighbors(
        store.read_pool(),
        x,
        &["Supports"],
        NeighborDirection::Incoming,
    )
    .await
    .expect("query");

    assert_eq!(rows.len(), 2);
    let sources: Vec<u64> = rows.iter().map(|r| r.source_id).collect();
    assert!(sources.contains(&y));
    assert!(sources.contains(&z_entry));
    assert!(rows.iter().all(|r| r.target_id == x));
    store.close().await.unwrap();
}

/// Both directions union.
#[tokio::test]
async fn test_query_direct_neighbors_both_directions() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let x = insert_entry(wp, "X", Status::Active, None, None).await;
    let y = insert_entry(wp, "Y", Status::Active, None, None).await;
    let z_entry = insert_entry(wp, "Z", Status::Active, None, None).await;

    insert_edge(wp, x, y, "Supports").await; // outgoing from X
    insert_edge(wp, z_entry, x, "Informs").await; // incoming to X

    let rows = query_direct_neighbors(
        store.read_pool(),
        x,
        &["Supports", "Informs"],
        NeighborDirection::Both,
    )
    .await
    .expect("query");

    assert_eq!(rows.len(), 2);
    let has_outgoing = rows.iter().any(|r| r.source_id == x && r.target_id == y);
    let has_incoming = rows
        .iter()
        .any(|r| r.source_id == z_entry && r.target_id == x);
    assert!(has_outgoing, "outgoing edge X→Y must be included");
    assert!(has_incoming, "incoming edge Z→X must be included");
    store.close().await.unwrap();
}

/// AC-10, R-06: Empty type list excludes Supersedes silently.
#[tokio::test]
async fn test_query_direct_neighbors_empty_type_list_excludes_supersedes() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let x = insert_entry(wp, "X", Status::Active, None, None).await;
    let y = insert_entry(wp, "Y", Status::Active, None, None).await;
    let z_entry = insert_entry(wp, "Z", Status::Active, None, None).await;

    insert_edge(wp, x, y, "Supports").await;
    insert_edge(wp, x, z_entry, "Supersedes").await;

    let rows = query_direct_neighbors(store.read_pool(), x, &[], NeighborDirection::Both)
        .await
        .expect("query");

    let targets: Vec<u64> = rows.iter().map(|r| r.target_id).collect();
    assert!(targets.contains(&y), "Supports edge to Y must be returned");
    assert!(
        !targets.contains(&z_entry),
        "Supersedes edge must be excluded from empty-type-list query"
    );
    assert!(!rows.is_empty(), "result must not be empty");
    store.close().await.unwrap();
}

/// R-12, OQ-01: Non-existent anchor returns empty Vec, no error.
#[tokio::test]
async fn test_query_direct_neighbors_nonexistent_anchor_returns_empty() {
    let (store, _dir) = open_test_store().await;

    let rows = query_direct_neighbors(store.read_pool(), 999_999, &[], NeighborDirection::Both)
        .await
        .expect("no error for non-existent anchor — OQ-01 resolved");

    assert!(rows.is_empty());
    store.close().await.unwrap();
}

/// Entry exists but has no graph_edges rows.
#[tokio::test]
async fn test_query_direct_neighbors_zero_edges_from_anchor() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let id = insert_entry(wp, "Isolated", Status::Active, None, None).await;

    let rows = query_direct_neighbors(store.read_pool(), id, &[], NeighborDirection::Both)
        .await
        .expect("query");

    assert!(rows.is_empty());
    store.close().await.unwrap();
}

// -----------------------------------------------------------------------
// vnc-037: additive `source` on the plain neighbor path (ADR-004, store-neighbor-source)
// -----------------------------------------------------------------------

/// R-08 / #4166: `source` populates correctly across ALL 4 plain SELECT branches —
/// `run_outgoing_query` and `run_incoming_query`, each in the empty-`edge_types`
/// branch and the `IN (…)` branch. A wrong column index in `map_edge_row` fails here.
#[tokio::test]
async fn test_map_edge_row_populates_source_all_4_branches() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let x = insert_entry(wp, "X", Status::Active, None, None).await;
    let out = insert_entry(wp, "Out", Status::Active, None, None).await;
    let inc = insert_entry(wp, "Inc", Status::Active, None, None).await;

    // Distinct source values so a branch confusion is detectable.
    insert_edge_with_source(wp, x, out, "Supports", "agent").await; // outgoing from X
    insert_edge_with_source(wp, inc, x, "Supports", "co_access").await; // incoming to X

    // 1. run_outgoing_query, empty-type branch.
    let out_empty = query_direct_neighbors(store.read_pool(), x, &[], NeighborDirection::Outgoing)
        .await
        .expect("outgoing empty-type");
    let row = out_empty
        .iter()
        .find(|r| r.target_id == out)
        .expect("outgoing edge present");
    assert_eq!(row.source, "agent", "outgoing empty-type source");

    // 2. run_outgoing_query, IN(…) branch.
    let out_in = query_direct_neighbors(
        store.read_pool(),
        x,
        &["Supports"],
        NeighborDirection::Outgoing,
    )
    .await
    .expect("outgoing IN-type");
    let row = out_in
        .iter()
        .find(|r| r.target_id == out)
        .expect("outgoing edge present");
    assert_eq!(row.source, "agent", "outgoing IN-type source");

    // 3. run_incoming_query, empty-type branch.
    let in_empty = query_direct_neighbors(store.read_pool(), x, &[], NeighborDirection::Incoming)
        .await
        .expect("incoming empty-type");
    let row = in_empty
        .iter()
        .find(|r| r.source_id == inc)
        .expect("incoming edge present");
    assert_eq!(row.source, "co_access", "incoming empty-type source");

    // 4. run_incoming_query, IN(…) branch.
    let in_in = query_direct_neighbors(
        store.read_pool(),
        x,
        &["Supports"],
        NeighborDirection::Incoming,
    )
    .await
    .expect("incoming IN-type");
    let row = in_in
        .iter()
        .find(|r| r.source_id == inc)
        .expect("incoming edge present");
    assert_eq!(row.source, "co_access", "incoming IN-type source");

    store.close().await.unwrap();
}

/// R-09: every live `source` value carries through verbatim — the get-path
/// `authored = (source == "agent")` projection depends on this exact string.
#[tokio::test]
async fn test_source_values_present_for_all_live_sources() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let x = insert_entry(wp, "X", Status::Active, None, None).await;

    let live_sources = ["agent", "co_access", "cosine", "behavioral", "S8"];
    let mut targets = Vec::new();
    for (i, src) in live_sources.iter().enumerate() {
        let t = insert_entry(wp, &format!("T{i}"), Status::Active, None, None).await;
        insert_edge_with_source(wp, x, t, "Supports", src).await;
        targets.push((t, *src));
    }

    let rows = query_direct_neighbors(store.read_pool(), x, &[], NeighborDirection::Outgoing)
        .await
        .expect("query");

    for (target, expected_src) in targets {
        let row = rows
            .iter()
            .find(|r| r.target_id == target)
            .expect("edge present");
        assert_eq!(
            row.source, expected_src,
            "source string must be preserved verbatim for {expected_src}"
        );
    }

    store.close().await.unwrap();
}

/// R-20: the raw `source` string is preserved underneath the (derived) `authored`
/// boolean — no information loss. Near-miss strings are retained verbatim, NOT
/// normalized; the exact-match `authored` projection lives in get-edge-vocabulary.
#[tokio::test]
async fn test_source_string_retained_beneath_boolean() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let x = insert_entry(wp, "X", Status::Active, None, None).await;
    let a = insert_entry(wp, "A", Status::Active, None, None).await;
    let b = insert_entry(wp, "B", Status::Active, None, None).await;

    // Near-miss strings: must be retained EXACTLY (not coerced to "agent").
    insert_edge_with_source(wp, x, a, "Supports", "Agent").await;
    insert_edge_with_source(wp, x, b, "Supports", " agent").await;

    let rows = query_direct_neighbors(store.read_pool(), x, &[], NeighborDirection::Outgoing)
        .await
        .expect("query");

    let row_a = rows.iter().find(|r| r.target_id == a).expect("edge A");
    let row_b = rows.iter().find(|r| r.target_id == b).expect("edge B");
    assert_eq!(row_a.source, "Agent", "near-miss 'Agent' retained verbatim");
    assert_eq!(
        row_b.source, " agent",
        "near-miss ' agent' retained verbatim"
    );
    assert_ne!(row_a.source, "agent");
    assert_ne!(row_b.source, "agent");

    store.close().await.unwrap();
}

/// R-08 surface 3 / SR-06: the plain path does NOT canonicalize symmetric edges
/// (a reciprocal pair returns TWO rows) and never carries a `target_confidence`
/// — the `↔` collapse and confidence JOIN are get-only (ranked variant). This is
/// the SR-02 firewall: get-only logic must not leak into the shared neighbors path.
#[tokio::test]
async fn test_no_canon_or_confidence_leak_into_plain_query() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let x = insert_entry(wp, "X", Status::Active, None, None).await;
    let y = insert_entry(wp, "Y", Status::Active, None, None).await;

    // A symmetric type stored as two reciprocal rows (A→B and B→A).
    insert_edge_with_source(wp, x, y, "CoAccess", "co_access").await;
    insert_edge_with_source(wp, y, x, "CoAccess", "co_access").await;

    let rows = query_direct_neighbors(store.read_pool(), x, &[], NeighborDirection::Both)
        .await
        .expect("query");

    // Plain path: BOTH reciprocal rows present — no get-only canonicalization.
    assert_eq!(
        rows.len(),
        2,
        "plain path returns both reciprocal rows (no ↔ canonicalization)"
    );
    // No confidence leaks into the shared path.
    assert!(
        rows.iter().all(|r| r.target_confidence.is_none()),
        "plain path must never populate target_confidence"
    );

    store.close().await.unwrap();
}
