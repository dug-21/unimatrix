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
    // AC-15: max_depth=1 — only A→B discovered, not B→C.
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
        max_depth: Some(1),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();
    assert_eq!(resp.edges.len(), 1, "only A→B at depth=1");
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
        max_depth: Some(1),
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
    let result = handle_subgraph(&*store, &handle, &params).await;
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
    let result = handle_subgraph(&*store, &handle, &params).await;
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
