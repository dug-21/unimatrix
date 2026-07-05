//! Behavioral tests for `graph_read_subgraph.rs` — require store + TypedGraphState.
//!
//! Covers parameter validation via handle_subgraph, BFS traversal contracts,
//! and correctness invariants from the vnc-019 component test plan.
//!
//! Declared as a child module inside `graph_read_subgraph_tests.rs`.

use std::sync::Arc;

use unimatrix_core::{EntryRecord, Status};
use unimatrix_engine::graph::{GraphEdgeRow, TypedRelationGraph, build_typed_relation_graph};
use unimatrix_store::{PoolConfig, SqlxStore};

use super::super::{GraphParams, handle_subgraph};
use crate::services::typed_graph::TypedGraphState;

async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open test store");
    (store, dir)
}

fn make_entry(id: u64) -> EntryRecord {
    EntryRecord {
        id,
        title: format!("Entry {id}"),
        content: String::new(),
        topic: String::new(),
        category: "pattern".to_string(),
        tags: vec![],
        source: String::new(),
        status: Status::Active,
        confidence: 0.5,
        created_at: 0,
        updated_at: 0,
        last_accessed_at: 0,
        access_count: 0,
        supersedes: None,
        superseded_by: None,
        correction_count: 0,
        embedding_dim: 0,
        created_by: String::new(),
        modified_by: String::new(),
        content_hash: String::new(),
        previous_hash: String::new(),
        version: 1,
        feature_cycle: String::new(),
        trust_source: "agent".to_string(),
        helpful_count: 0,
        unhelpful_count: 0,
        pre_quarantine_status: None,
    }
}

fn make_supports_edge(source_id: u64, target_id: u64) -> GraphEdgeRow {
    GraphEdgeRow {
        source_id,
        target_id,
        relation_type: "Supports".to_string(),
        weight: 1.0,
        created_at: 0,
        created_by: String::new(),
        source: String::new(),
        bootstrap_only: false,
    }
}

fn make_edge(source_id: u64, target_id: u64, relation_type: &str) -> GraphEdgeRow {
    GraphEdgeRow {
        source_id,
        target_id,
        relation_type: relation_type.to_string(),
        weight: 1.0,
        created_at: 0,
        created_by: String::new(),
        source: String::new(),
        bootstrap_only: false,
    }
}

fn set_test_graph(handle: &Arc<std::sync::RwLock<TypedGraphState>>, graph: TypedRelationGraph) {
    let mut state = handle.write().expect("write lock");
    state.typed_graph = graph;
    state.use_fallback = false;
}

// ---------------------------------------------------------------------------
// Live-DB fixtures (vnc-043) — depth-1 dispatch routes through subgraph_via_db,
// which reads GRAPH_EDGES/entries live, so depth-1 tests must commit to the DB
// (not just the in-memory TypedRelationGraph). Mirrors the GH #623 fixture pattern.
// ---------------------------------------------------------------------------

async fn insert_db_entry(store: &SqlxStore, label: &str, tags: Vec<String>) -> u64 {
    use unimatrix_store::NewEntry;
    store
        .insert(NewEntry {
            title: format!("Entry {label}"),
            content: format!("content-{label}"),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags,
            source: "test".to_string(),
            status: unimatrix_store::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "vnc-043".to_string(),
            trust_source: "agent".to_string(),
        })
        .await
        .expect("insert entry")
}

async fn insert_db_edge(store: &SqlxStore, source_id: u64, target_id: u64, relation_type: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at,
              created_by, source, bootstrap_only)
         VALUES (?1, ?2, ?3, 1.0, ?4, 'test', '', 0)",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(relation_type)
    .bind(now as i64)
    .execute(store.write_pool_server())
    .await
    .expect("insert edge");
}

/// Sorted-ascending predicate on node ids (uniform ordering contract, ADR-003 vnc-043).
fn nodes_sorted_by_id(nodes: &[EntryRecord]) -> bool {
    nodes.windows(2).all(|w| w[0].id <= w[1].id)
}

/// Sorted-ascending predicate on the canonical edge triple (ADR-003 vnc-043).
fn edges_sorted_canonical(edges: &[super::super::EdgeRecord]) -> bool {
    edges.windows(2).all(|w| {
        (w[0].source_id, w[0].target_id, &w[0].relation_type)
            <= (w[1].source_id, w[1].target_id, &w[1].relation_type)
    })
}

// ---------------------------------------------------------------------------
// Section A: Parameter validation via handle_subgraph
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_validate_seed_ids_absent_returns_error() {
    // AC-07: seed_ids absent → validation error with exact message.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: None,
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert_eq!(
        msg, "subgraph mode requires at least one entry ID in seed_ids",
        "exact error message required, got: {msg}"
    );
}

#[tokio::test]
async fn test_validate_seed_ids_empty_returns_error() {
    // AC-07: seed_ids=[] → same exact error.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![]),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert_eq!(
        msg,
        "subgraph mode requires at least one entry ID in seed_ids"
    );
}

#[tokio::test]
async fn test_validate_max_depth_zero_rejected() {
    // AC-06: max_depth=0 is below valid range [1, 10].
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![u64::MAX]),
        max_depth: Some(0),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err(), "max_depth=0 must be rejected");
    let msg = result.unwrap_err().message;
    assert!(
        msg.contains("max_depth") && msg.contains("1..=10"),
        "error must state range, got: {msg}"
    );
}

#[tokio::test]
async fn test_validate_max_depth_eleven_rejected() {
    // AC-06: max_depth=11 is above valid range.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![u64::MAX]),
        max_depth: Some(11),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err(), "max_depth=11 must be rejected");
    let msg = result.unwrap_err().message;
    assert!(
        msg.contains("max_depth") && msg.contains("11"),
        "got: {msg}"
    );
}

#[tokio::test]
async fn test_validate_max_depth_boundary_values_accepted() {
    // AC-06: max_depth=1 and max_depth=10 are valid boundary values.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    for good in [1u8, 10u8] {
        let params = GraphParams {
            mode: "subgraph".to_string(),
            seed_ids: Some(vec![u64::MAX]),
            max_depth: Some(good),
            ..Default::default()
        };
        let result = handle_subgraph(&store, &handle, &params).await;
        assert!(
            result.is_ok(),
            "max_depth={good} must be accepted, got: {result:?}"
        );
    }
}

#[tokio::test]
async fn test_validate_max_nodes_above_200_rejected() {
    // R-07: max_nodes=201 → validation error with range message and echoed value.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        max_nodes: Some(201),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(
        msg.contains("max_nodes") && msg.contains("1..=200"),
        "error must state range 1..=200, got: {msg}"
    );
    assert!(
        msg.contains("201"),
        "error must echo bad value 201, got: {msg}"
    );
}

#[tokio::test]
async fn test_validate_max_nodes_zero_rejected() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        max_nodes: Some(0),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err(), "max_nodes=0 must be rejected");
}

#[tokio::test]
async fn test_validate_unknown_edge_type_rejected() {
    // AC-08: unrecognized edge_type → error naming the bad value.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        edge_types: Some(vec!["BogusEdgeType".to_string()]),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(msg.contains("BogusEdgeType"), "got: {msg}");
    assert!(
        msg.contains("Supports") || msg.contains("Contradicts"),
        "error must list recognized types, got: {msg}"
    );
}

#[tokio::test]
async fn test_validate_direction_forward_rejected() {
    // direction="forward" is chain-mode; invalid for subgraph.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        direction: Some("forward".to_string()),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(msg.contains("direction"), "got: {msg}");
}

// ---------------------------------------------------------------------------
// Section B: BFS algorithm contracts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_bfs_cold_start_empty_result() {
    // AC-17, R-04: empty cold-start graph → Ok with empty nodes/edges.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![u64::MAX]),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok(), "cold-start must return Ok, got: {result:?}");
    let resp = result.unwrap();
    assert!(resp.nodes.is_empty());
    assert!(resp.edges.is_empty());
    assert!(!resp.truncated);
    assert_eq!(resp.depth_reached, 0);
    assert_eq!(resp.seed_ids, vec![u64::MAX]);
}

#[tokio::test]
async fn test_bfs_seed_ids_echoed_in_response() {
    // AC-01: seed_ids in SubgraphResponse echo the input.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![10, 20, 30]),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    assert_eq!(result.unwrap().seed_ids, vec![10, 20, 30]);
}

#[tokio::test]
async fn test_bfs_depth_reached_zero_when_no_edges() {
    // AC-15c: no edges traversed → depth_reached=0.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![u64::MAX]),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    assert_eq!(result.unwrap().depth_reached, 0);
}

#[tokio::test]
async fn test_bfs_traverses_supports_edge() {
    // AC-08, R-14: absent edge_types expands to all non-Supersedes; Supports traversed.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let graph =
        build_typed_relation_graph(&[make_entry(1), make_entry(2)], &[make_supports_edge(1, 2)])
            .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();
    assert_eq!(resp.edges.len(), 1, "one Supports edge must be discovered");
    assert_eq!(resp.edges[0].source_id, 1);
    assert_eq!(resp.edges[0].target_id, 2);
    assert_eq!(resp.edges[0].relation_type, "Supports");
}

#[tokio::test]
async fn test_bfs_edge_direction_always_outgoing() {
    // FR-12, R-02: direction field on all returned EdgeRecords must be "outgoing".
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let graph =
        build_typed_relation_graph(&[make_entry(1), make_entry(2)], &[make_supports_edge(1, 2)])
            .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        direction: Some("both".to_string()),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    for edge in result.unwrap().edges {
        assert_eq!(edge.direction, "outgoing", "got: {}", edge.direction);
    }
}

#[tokio::test]
async fn test_bfs_two_hop_chain_depth_reached_2() {
    // A→B→C chain; max_depth=2. Expect: 2 edges, depth_reached=2.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let graph = build_typed_relation_graph(
        &[make_entry(1), make_entry(2), make_entry(3)],
        &[make_supports_edge(1, 2), make_supports_edge(2, 3)],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        max_depth: Some(2),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();
    assert_eq!(resp.edges.len(), 2, "A→B and B→C must be collected");
    assert_eq!(resp.depth_reached, 2);
    assert!(!resp.truncated);
}

#[tokio::test]
async fn test_bfs_max_depth_one_only_direct_neighbors() {
    // AC-15: max_depth=1 routes to the live DB path (ADR-001 vnc-043). Only A→B
    // (the direct neighbor) is discovered, never B→C (two hops from the seed).
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let a = insert_db_entry(&store, "A", vec![]).await;
    let b = insert_db_entry(&store, "B", vec![]).await;
    let c = insert_db_entry(&store, "C", vec![]).await;
    insert_db_edge(&store, a, b, "Supports").await;
    insert_db_edge(&store, b, c, "Supports").await;

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![a]),
        max_depth: Some(1),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();
    assert_eq!(resp.edges.len(), 1, "only A→B at depth=1");
    assert_eq!(resp.edges[0].source_id, a);
    assert_eq!(resp.edges[0].target_id, b);
    assert_eq!(resp.depth_reached, 1);
}

#[tokio::test]
async fn test_bfs_seed_saturation_sets_truncated() {
    // R-03: 3 seeds with max_nodes=3 → truncated=true, BFS skipped, depth_reached=0.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let entries: Vec<_> = (1u64..=4).map(make_entry).collect();
    let graph = build_typed_relation_graph(&entries, &[]).expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1, 2, 3]),
        max_nodes: Some(3),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();
    assert!(resp.truncated, "truncated must be true");
    assert_eq!(resp.depth_reached, 0, "BFS must not run");
}

#[tokio::test]
async fn test_bfs_direction_both_no_duplicate_edges() {
    // R-02, AC-12: direction="both" with seed=[A,B]; single stored A→B edge.
    // Each canonical (source, target, rel_type) triple appears at most once.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let graph =
        build_typed_relation_graph(&[make_entry(1), make_entry(2)], &[make_supports_edge(1, 2)])
            .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1, 2]),
        direction: Some("both".to_string()),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();

    let keys: Vec<(u64, u64, String)> = resp
        .edges
        .iter()
        .map(|e| (e.source_id, e.target_id, e.relation_type.clone()))
        .collect();
    let unique: std::collections::HashSet<_> = keys.iter().collect();
    assert_eq!(
        keys.len(),
        unique.len(),
        "no duplicate edge triples; got: {keys:?}"
    );
}

#[tokio::test]
async fn test_bfs_not_truncated_under_cap() {
    // Small 2-node graph is well within max_nodes=200 default → truncated=false.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let graph =
        build_typed_relation_graph(&[make_entry(1), make_entry(2)], &[make_supports_edge(1, 2)])
            .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    assert!(
        !result.unwrap().truncated,
        "small graph must not be truncated"
    );
}

#[tokio::test]
async fn test_bfs_star_topology_near_cap_edges_within_bound() {
    // Star topology: 1 center node connected to 199 leaf nodes via 3 relation types
    // (Supports, Informs, RelatedTo) = 199 × 3 = 597 edges.
    // Near ADR-003's documented ~600 typical density. Verifies:
    //   - truncated=false (all 200 nodes fit within max_nodes=200)
    //   - edges.len() <= MAX_EDGES_UPPER (597 << 1000, no OR-chain overflow)
    //   - no panic from debug_assert at fetch_edge_metadata call site (#611)
    // max_depth=2 keeps this on the WARM in-memory BFS path (the near-cap assembly
    // this test exercises); depth-1 now routes live (ADR-001 vnc-043) and leaf nodes
    // have no second hop, so the collected SET is identical at depth 2.
    const CENTER: u64 = 1;
    const LEAF_COUNT: usize = 199;
    const MAX_EDGES_UPPER: usize = 1000;

    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let mut entries: Vec<_> = vec![make_entry(CENTER)];
    for leaf in 2u64..=(LEAF_COUNT as u64 + 1) {
        entries.push(make_entry(leaf));
    }

    let mut edges: Vec<GraphEdgeRow> = Vec::with_capacity(LEAF_COUNT * 3);
    for leaf in 2u64..=(LEAF_COUNT as u64 + 1) {
        edges.push(make_edge(CENTER, leaf, "Supports"));
        edges.push(make_edge(CENTER, leaf, "Informs"));
        edges.push(make_edge(CENTER, leaf, "RelatedTo"));
    }
    assert_eq!(edges.len(), LEAF_COUNT * 3, "597 edges in fixture");

    let graph = build_typed_relation_graph(&entries, &edges).expect("build star graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![CENTER]),
        max_nodes: Some(200),
        max_depth: Some(2),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(
        result.is_ok(),
        "star topology must succeed, got: {result:?}"
    );
    let resp = result.unwrap();

    assert!(
        !resp.truncated,
        "all 200 nodes fit within max_nodes=200; truncated must be false"
    );
    assert!(
        resp.edges.len() <= MAX_EDGES_UPPER,
        "edges.len()={} must be <= MAX_EDGES_UPPER={}",
        resp.edges.len(),
        MAX_EDGES_UPPER
    );
    // 597 unique (center→leaf, rel_type) triples expected.
    assert_eq!(
        resp.edges.len(),
        LEAF_COUNT * 3,
        "expected {} edges, got {}",
        LEAF_COUNT * 3,
        resp.edges.len()
    );
}

// ---------------------------------------------------------------------------
// Section C: DB-fallback (GH #623) — use_fallback=true cold-start regression
// ---------------------------------------------------------------------------

/// GH #623 regression: subgraph returns depth_reached=0 when use_fallback=true.
///
/// Real SqlxStore, entries A and B with A→B Supports edge in GRAPH_EDGES.
/// TypedGraphState::new() directly (use_fallback=true, no rebuild).
/// handle_subgraph(seed_ids=[A], max_depth=1) must return edges.len()==1
/// and depth_reached==1.
#[tokio::test]
async fn test_subgraph_use_fallback_true_with_real_entries_falls_back_to_db() {
    use unimatrix_store::NewEntry;

    let (store, _dir) = open_test_store().await;
    let store = std::sync::Arc::new(store);

    // Step 1: Insert entries A and B into the real store.
    let id_a = store
        .insert(NewEntry {
            title: "Entry A".to_string(),
            content: "content-a".to_string(),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: unimatrix_store::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "bugfix-623".to_string(),
            trust_source: "agent".to_string(),
        })
        .await
        .expect("insert A");

    let id_b = store
        .insert(NewEntry {
            title: "Entry B".to_string(),
            content: "content-b".to_string(),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: unimatrix_store::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "bugfix-623".to_string(),
            trust_source: "agent".to_string(),
        })
        .await
        .expect("insert B");

    // Step 2: Insert A→B Supports edge directly into GRAPH_EDGES via raw SQL.
    // (Same pattern as crt-035 and bugfix-612 tests.)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at,
              created_by, source, bootstrap_only)
         VALUES (?1, ?2, 'Supports', 1.0, ?3, 'test', '', 0)",
    )
    .bind(id_a as i64)
    .bind(id_b as i64)
    .bind(now as i64)
    .execute(store.write_pool_server())
    .await
    .expect("insert Supports edge A→B");

    // Step 3: Construct TypedGraphState::new() directly — use_fallback=true, no rebuild.
    let handle = Arc::new(std::sync::RwLock::new(TypedGraphState::new()));
    {
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(
            guard.use_fallback,
            "TypedGraphState::new() must have use_fallback=true"
        );
    }

    // Step 4: Call handle_subgraph with seed_ids=[A], max_depth=1.
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![id_a]),
        max_depth: Some(1),
        edge_types: Some(vec!["Supports".to_string()]),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(
        result.is_ok(),
        "DB-fallback subgraph must return Ok, got: {result:?}"
    );
    let resp = result.unwrap();

    // Step 5: Assert edges.len()==1 and depth_reached==1.
    assert_eq!(
        resp.edges.len(),
        1,
        "DB-fallback BFS must discover the A→B Supports edge (use_fallback=true); \
         got {} edges. This is the GH #623 regression guard.",
        resp.edges.len()
    );
    assert_eq!(
        resp.depth_reached, 1,
        "depth_reached must be 1 when A→B edge discovered; got {}. \
         GH #623: depth_reached=0 was the bug symptom.",
        resp.depth_reached
    );
    assert_eq!(resp.edges[0].source_id, id_a, "edge source must be A");
    assert_eq!(resp.edges[0].target_id, id_b, "edge target must be B");
    assert_eq!(resp.edges[0].relation_type, "Supports");
    assert!(!resp.truncated, "two-node graph must not be truncated");
}

/// GH #623 regression: direction parameter is forwarded in DB-fallback mode.
///
/// Verifies that direction="incoming" works correctly in cold-start state —
/// hard requirement from the architect's blocking note.
#[tokio::test]
async fn test_subgraph_use_fallback_true_direction_incoming_forwarded() {
    use unimatrix_store::NewEntry;

    let (store, _dir) = open_test_store().await;
    let store = std::sync::Arc::new(store);

    let id_a = store
        .insert(NewEntry {
            title: "Entry A".to_string(),
            content: "content-a".to_string(),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: unimatrix_store::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "bugfix-623".to_string(),
            trust_source: "agent".to_string(),
        })
        .await
        .expect("insert A");

    let id_b = store
        .insert(NewEntry {
            title: "Entry B".to_string(),
            content: "content-b".to_string(),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: unimatrix_store::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "bugfix-623".to_string(),
            trust_source: "agent".to_string(),
        })
        .await
        .expect("insert B");

    // Insert A→B Supports edge.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at,
              created_by, source, bootstrap_only)
         VALUES (?1, ?2, 'Supports', 1.0, ?3, 'test', '', 0)",
    )
    .bind(id_a as i64)
    .bind(id_b as i64)
    .bind(now as i64)
    .execute(store.write_pool_server())
    .await
    .expect("insert Supports edge A→B");

    let handle = Arc::new(std::sync::RwLock::new(TypedGraphState::new()));

    // Seed on B, direction=incoming — should find A→B via the incoming traversal from B.
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![id_b]),
        max_depth: Some(1),
        edge_types: Some(vec!["Supports".to_string()]),
        direction: Some("incoming".to_string()),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(
        result.is_ok(),
        "DB-fallback subgraph (incoming) must return Ok, got: {result:?}"
    );
    let resp = result.unwrap();
    // B seeded with direction=incoming — should find A as a neighbor via A→B incoming edge.
    assert_eq!(
        resp.edges.len(),
        1,
        "incoming direction must find A→B edge from B's perspective; got {} edges",
        resp.edges.len()
    );
    assert_eq!(resp.depth_reached, 1, "depth_reached must be 1");
}

// ---------------------------------------------------------------------------
// Section D: depth-1 live dispatch + uniform ordering (vnc-043)
// ---------------------------------------------------------------------------

/// R-01/AC-01: depth-1 reads LIVE, not the warm cache. The warm graph is set
/// (use_fallback=false) but carries NO edge; the DB carries the edge. A depth-1 read
/// must surface the DB edge — proving the result came from live SQL, not the cache.
#[tokio::test]
async fn test_subgraph_depth1_routes_live() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let a = insert_db_entry(&store, "A", vec![]).await;
    let b = insert_db_entry(&store, "B", vec![]).await;
    insert_db_edge(&store, a, b, "Supports").await;

    // Warm cache holds the nodes but NOT the edge (use_fallback=false).
    let graph = build_typed_relation_graph(&[make_entry(a), make_entry(b)], &[])
        .expect("build edgeless graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![a]),
        max_depth: Some(1),
        ..Default::default()
    };
    let resp = handle_subgraph(&store, &handle, &params)
        .await
        .expect("depth-1 live read");
    assert_eq!(
        resp.edges.len(),
        1,
        "depth-1 must read the DB-committed edge live, not the edgeless cache"
    );
    assert_eq!(resp.edges[0].source_id, a);
    assert_eq!(resp.edges[0].target_id, b);
}

/// R-01/AC-02: depth>1 stays on the cache. Warm cache holds A→B; the DB holds a
/// B→C edge the cache does NOT. A depth-2 (and depth-10) read must NOT surface the
/// DB-only B→C edge — proving depth>1 did not route live.
#[tokio::test]
async fn test_subgraph_depth2_served_from_cache() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let a = insert_db_entry(&store, "A", vec![]).await;
    let b = insert_db_entry(&store, "B", vec![]).await;
    let c = insert_db_entry(&store, "C", vec![]).await;
    // DB-only edge, absent from the warm cache.
    insert_db_edge(&store, b, c, "Supports").await;

    // Warm cache holds only A→B.
    let graph = build_typed_relation_graph(
        &[make_entry(a), make_entry(b), make_entry(c)],
        &[make_supports_edge(a, b)],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    for depth in [2u8, 10u8] {
        let params = GraphParams {
            mode: "subgraph".to_string(),
            seed_ids: Some(vec![a]),
            max_depth: Some(depth),
            ..Default::default()
        };
        let resp = handle_subgraph(&store, &handle, &params)
            .await
            .expect("depth>1 cache read");
        assert!(
            resp.edges
                .iter()
                .any(|e| e.source_id == a && e.target_id == b),
            "cache edge A→B must be present at depth={depth}"
        );
        assert!(
            !resp.edges.iter().any(|e| e.target_id == c),
            "DB-only edge B→C must NOT appear at depth={depth} (served from cache)"
        );
    }
}

/// R-03/AC-05/AC-08: dual-path SET parity. On a warm+fresh graph (cache == DB), the
/// depth-1 live result SET equals the depth-2 warm-cache result SET for the same
/// one-hop star neighborhood — across absent/empty/explicit edge_types.
#[tokio::test]
async fn test_subgraph_depth1_set_parity_vs_warm_cache() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let a = insert_db_entry(&store, "A", vec![]).await;
    let b = insert_db_entry(&store, "B", vec![]).await;
    let c = insert_db_entry(&store, "C", vec![]).await;
    insert_db_edge(&store, a, b, "Supports").await;
    insert_db_edge(&store, a, c, "Informs").await;

    // Warm cache mirrors the DB exactly (fresh).
    let graph = build_typed_relation_graph(
        &[make_entry(a), make_entry(b), make_entry(c)],
        &[make_edge(a, b, "Supports"), make_edge(a, c, "Informs")],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    let node_set = |resp: &super::super::SubgraphResponse| {
        resp.nodes
            .iter()
            .map(|n| n.id)
            .collect::<std::collections::HashSet<_>>()
    };
    let edge_set = |resp: &super::super::SubgraphResponse| {
        resp.edges
            .iter()
            .map(|e| (e.source_id, e.target_id, e.relation_type.clone()))
            .collect::<std::collections::HashSet<_>>()
    };

    // edge_types: absent, empty, and explicit — each identical across both paths.
    for edge_types in [None, Some(vec![]), Some(vec!["Supports".to_string()])] {
        let d1 = handle_subgraph(
            &store,
            &handle,
            &GraphParams {
                mode: "subgraph".to_string(),
                seed_ids: Some(vec![a]),
                max_depth: Some(1),
                edge_types: edge_types.clone(),
                ..Default::default()
            },
        )
        .await
        .expect("depth-1 live");
        // Star: leaves have no second hop, so depth-2 warm covers the same neighborhood.
        let d2 = handle_subgraph(
            &store,
            &handle,
            &GraphParams {
                mode: "subgraph".to_string(),
                seed_ids: Some(vec![a]),
                max_depth: Some(2),
                edge_types: edge_types.clone(),
                ..Default::default()
            },
        )
        .await
        .expect("depth-2 warm");

        assert_eq!(
            node_set(&d1),
            node_set(&d2),
            "node SET parity failed for edge_types={edge_types:?}"
        );
        assert_eq!(
            edge_set(&d1),
            edge_set(&d2),
            "edge SET parity failed for edge_types={edge_types:?}"
        );
    }
}

/// R-04: on the live depth-1 path, when max_nodes caps mid-hop, every returned edge's
/// endpoints are members of the returned node set (post-cap R-05 dangling filter).
#[tokio::test]
async fn test_subgraph_depth1_dangling_edge_filtered_under_cap() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let a = insert_db_entry(&store, "A", vec![]).await;
    let b = insert_db_entry(&store, "B", vec![]).await;
    let c = insert_db_entry(&store, "C", vec![]).await;
    let d = insert_db_entry(&store, "D", vec![]).await;
    insert_db_edge(&store, a, b, "Supports").await;
    insert_db_edge(&store, a, c, "Supports").await;
    insert_db_edge(&store, a, d, "Supports").await;

    // max_nodes=2: seed + at most one neighbor fit; the rest are dropped mid-hop.
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![a]),
        max_depth: Some(1),
        max_nodes: Some(2),
        ..Default::default()
    };
    let resp = handle_subgraph(&store, &handle, &params)
        .await
        .expect("depth-1 capped read");

    assert!(resp.truncated, "cap must set truncated");
    let node_ids: std::collections::HashSet<u64> = resp.nodes.iter().map(|n| n.id).collect();
    for e in &resp.edges {
        assert!(
            node_ids.contains(&e.source_id) && node_ids.contains(&e.target_id),
            "no edge may point at a dropped node: {e:?}"
        );
    }
}

/// R-05/AC-04: depth-1 live hydration carries the full EntryRecord field set incl.
/// tags (via load_tags_for_entries) — field-for-field with the DB entry.
#[tokio::test]
async fn test_subgraph_depth1_entryrecord_field_and_tag_parity() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let a = insert_db_entry(
        &store,
        "Tagged",
        vec!["alpha".to_string(), "beta".to_string()],
    )
    .await;

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![a]),
        max_depth: Some(1),
        ..Default::default()
    };
    let resp = handle_subgraph(&store, &handle, &params)
        .await
        .expect("depth-1 hydration");

    let node = resp
        .nodes
        .iter()
        .find(|n| n.id == a)
        .expect("seed node hydrated");
    assert_eq!(node.title, "Entry Tagged");
    assert_eq!(node.content, "content-Tagged");
    assert_eq!(node.category, "pattern");
    assert_eq!(node.status, Status::Active);
    let mut tags = node.tags.clone();
    tags.sort();
    assert_eq!(tags, vec!["alpha".to_string(), "beta".to_string()]);
}

/// R-06/AC-14: depth-1 output is ordered — nodes ascending by id, edges by the
/// canonical (source_id, target_id, relation_type) triple.
#[tokio::test]
async fn test_subgraph_depth1_node_and_edge_ordering() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let a = insert_db_entry(&store, "A", vec![]).await;
    let b = insert_db_entry(&store, "B", vec![]).await;
    let c = insert_db_entry(&store, "C", vec![]).await;
    let d = insert_db_entry(&store, "D", vec![]).await;
    // Insert edges out of canonical order to prove the sort, not discovery order.
    insert_db_edge(&store, a, d, "Supports").await;
    insert_db_edge(&store, a, c, "Supports").await;
    insert_db_edge(&store, a, b, "Informs").await;
    insert_db_edge(&store, a, b, "Supports").await;

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![a]),
        max_depth: Some(1),
        ..Default::default()
    };
    let resp = handle_subgraph(&store, &handle, &params)
        .await
        .expect("depth-1 read");

    assert!(
        resp.nodes.len() >= 3 && resp.edges.len() >= 3,
        "non-trivial fixture"
    );
    assert!(
        nodes_sorted_by_id(&resp.nodes),
        "nodes must be ascending by id"
    );
    assert!(
        edges_sorted_canonical(&resp.edges),
        "edges must be ascending by (source_id, target_id, relation_type)"
    );
}

/// R-06/AC-14: the depth>1 warm path carries the SAME ordering keys (one contract).
#[tokio::test]
async fn test_subgraph_depth_gt1_same_ordering_keys() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    // Warm graph: A→{D,C,B} then B→E, inserted out of canonical order.
    let graph = build_typed_relation_graph(
        &[
            make_entry(1),
            make_entry(2),
            make_entry(3),
            make_entry(4),
            make_entry(5),
        ],
        &[
            make_edge(1, 4, "Supports"),
            make_edge(1, 3, "Supports"),
            make_edge(1, 2, "Informs"),
            make_edge(2, 5, "Supports"),
        ],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        max_depth: Some(2),
        ..Default::default()
    };
    let resp = handle_subgraph(&store, &handle, &params)
        .await
        .expect("depth-2 warm read");

    assert!(
        nodes_sorted_by_id(&resp.nodes),
        "warm nodes must be ascending by id"
    );
    assert!(
        edges_sorted_canonical(&resp.edges),
        "warm edges must be canonically ordered"
    );
}

/// R-06/NFR-4: the depth-1 one-shot is deterministic — two runs are byte-order-identical.
#[tokio::test]
async fn test_subgraph_dod_oneshot_deterministic() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let a = insert_db_entry(&store, "A", vec![]).await;
    let b = insert_db_entry(&store, "B", vec![]).await;
    let c = insert_db_entry(&store, "C", vec![]).await;
    insert_db_edge(&store, a, c, "Supports").await;
    insert_db_edge(&store, a, b, "Supports").await;

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![a]),
        max_depth: Some(1),
        ..Default::default()
    };
    let r1 = handle_subgraph(&store, &handle, &params)
        .await
        .expect("run 1");
    let r2 = handle_subgraph(&store, &handle, &params)
        .await
        .expect("run 2");

    let ids = |r: &super::super::SubgraphResponse| r.nodes.iter().map(|n| n.id).collect::<Vec<_>>();
    let triples = |r: &super::super::SubgraphResponse| {
        r.edges
            .iter()
            .map(|e| (e.source_id, e.target_id, e.relation_type.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&r1), ids(&r2), "node order must be deterministic");
    assert_eq!(
        triples(&r1),
        triples(&r2),
        "edge order must be deterministic"
    );
}

/// R-11/AC-06: at depth-1, the direction filter changes the inclusion SET, but every
/// EdgeRecord keeps its canonical source→target with direction:"outgoing".
#[tokio::test]
async fn test_subgraph_depth1_direction_label_invariant() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let a = insert_db_entry(&store, "A", vec![]).await;
    let b = insert_db_entry(&store, "B", vec![]).await;
    insert_db_edge(&store, a, b, "Supports").await;

    // Seed B: only "incoming" and "both" should surface A→B; "outgoing" surfaces none.
    for direction in ["incoming", "outgoing", "both"] {
        let params = GraphParams {
            mode: "subgraph".to_string(),
            seed_ids: Some(vec![b]),
            max_depth: Some(1),
            direction: Some(direction.to_string()),
            ..Default::default()
        };
        let resp = handle_subgraph(&store, &handle, &params)
            .await
            .unwrap_or_else(|e| panic!("direction={direction}: {e:?}"));
        for e in &resp.edges {
            assert_eq!(e.direction, "outgoing", "label must stay canonical");
            assert_eq!(e.source_id, a, "canonical source preserved");
            assert_eq!(e.target_id, b, "canonical target preserved");
        }
        if direction == "outgoing" {
            assert!(resp.edges.is_empty(), "B has no outgoing edge");
        } else {
            assert_eq!(resp.edges.len(), 1, "incoming/both surface A→B from B");
        }
    }
}

/// R-09/AC-15: realistic fan-in (>=30 incoming Advances) fits under the default cap —
/// all present, truncated==false, on the live depth-1 path.
#[tokio::test]
async fn test_subgraph_depth1_truncated_false_realistic_fanin() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let seed = insert_db_entry(&store, "Goal", vec![]).await;
    const FAN_IN: usize = 30;
    for i in 0..FAN_IN {
        let cap = insert_db_entry(&store, &format!("cap{i}"), vec![]).await;
        insert_db_edge(&store, cap, seed, "Advances").await;
    }

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![seed]),
        max_depth: Some(1),
        direction: Some("incoming".to_string()),
        edge_types: Some(vec!["Advances".to_string()]),
        ..Default::default()
    };
    let resp = handle_subgraph(&store, &handle, &params)
        .await
        .expect("depth-1 fan-in read");

    assert_eq!(
        resp.edges.len(),
        FAN_IN,
        "all {FAN_IN} Advances edges present"
    );
    assert!(
        !resp.truncated,
        "30 incoming Advances fits under the 200 cap"
    );
}
