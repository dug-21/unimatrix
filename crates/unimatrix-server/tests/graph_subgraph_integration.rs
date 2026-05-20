//! Integration tests for context_graph subgraph mode (vnc-019).
//!
//! Exercises the full `handle_graph → validate_no_unsupported_params →
//! handle_subgraph → SQL reads` call path with a real store and graph state.
//!
//! FR-23 / AC-14: Requires ≥5 entries with typed edges forming a topology.
//! Asserts returned nodes and edges match expected topology (node IDs, edge
//! triples, depths).
//!
//! These tests do NOT require the ONNX embedding model. They test the BFS
//! graph traversal, not the embedding pipeline.

use unimatrix_server::test_support::{TestHarness, skip_if_no_model};
use unimatrix_store::{NewEntry, Status};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Insert a bare entry (no embedding pipeline needed for graph traversal tests).
async fn insert_entry(harness: &TestHarness, title: &str) -> u64 {
    harness
        .store()
        .insert(NewEntry {
            title: title.to_string(),
            content: format!("Content for {title}"),
            topic: "vnc-019-test".to_string(),
            category: "decision".to_string(),
            tags: vec![],
            source: "integration-test".to_string(),
            status: Status::Active,
            created_by: "test-harness".to_string(),
            feature_cycle: "vnc-019".to_string(),
            trust_source: "agent".to_string(),
        })
        .await
        .expect("insert entry must succeed")
}

// ---------------------------------------------------------------------------
// T-AC14-01: Single-hop BFS with 5 entries, chain A→B→C, satellite D, E
//
// Topology:
//   A --Supports--> B --Supports--> C
//   A --Supports--> D
//   E (isolated)
//
// seed=[A], max_depth=1 → expects A, B, D (C excluded — depth 2)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_subgraph_single_hop_five_entries() {
    if skip_if_no_model() {
        // For graph traversal tests, we don't actually need the model.
        // But TestHarness::new() gates on it. Skip gracefully if not available.
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.db");
    let harness = match TestHarness::new(&path).await {
        Some(h) => h,
        None => return,
    };

    // Insert 5 entries.
    let id_a = insert_entry(&harness, "Entry A").await;
    let id_b = insert_entry(&harness, "Entry B").await;
    let id_c = insert_entry(&harness, "Entry C").await;
    let id_d = insert_entry(&harness, "Entry D").await;
    let _id_e = insert_entry(&harness, "Entry E (isolated)").await;

    // Wire edges: A→B (Supports), B→C (Supports), A→D (Supports).
    harness.insert_graph_edge(id_a, id_b, "Supports").await;
    harness.insert_graph_edge(id_b, id_c, "Supports").await;
    harness.insert_graph_edge(id_a, id_d, "Supports").await;

    // Rebuild the in-memory graph from GRAPH_EDGES.
    harness.rebuild_typed_graph().await;

    // Call context_graph subgraph with seed=[A], max_depth=1, direction=outgoing.
    let resp = harness
        .call_graph(serde_json::json!({
            "mode": "subgraph",
            "seed_ids": [id_a],
            "max_depth": 1,
            "direction": "outgoing",
            "agent_id": "test-harness"
        }))
        .await
        .expect("subgraph call must succeed");

    // Verify node IDs returned.
    let nodes = resp["nodes"].as_array().expect("nodes must be array");
    let node_ids: Vec<u64> = nodes
        .iter()
        .map(|n| n["id"].as_u64().expect("node.id must be u64"))
        .collect();

    // A, B, D must be present. C excluded (depth 2). E excluded (isolated).
    assert!(
        node_ids.contains(&id_a),
        "seed node A (id={id_a}) must be in nodes, got: {node_ids:?}"
    );
    assert!(
        node_ids.contains(&id_b),
        "depth-1 neighbor B (id={id_b}) must be in nodes, got: {node_ids:?}"
    );
    assert!(
        node_ids.contains(&id_d),
        "depth-1 neighbor D (id={id_d}) must be in nodes, got: {node_ids:?}"
    );
    assert!(
        !node_ids.contains(&id_c),
        "depth-2 node C (id={id_c}) must NOT be in nodes at max_depth=1, got: {node_ids:?}"
    );

    // Verify edges.
    let edges = resp["edges"].as_array().expect("edges must be array");
    let edge_triples: Vec<(u64, u64, String)> = edges
        .iter()
        .map(|e| {
            (
                e["source_id"].as_u64().expect("source_id"),
                e["target_id"].as_u64().expect("target_id"),
                e["relation_type"]
                    .as_str()
                    .expect("relation_type")
                    .to_string(),
            )
        })
        .collect();

    // Edge A→B must be present.
    assert!(
        edge_triples.contains(&(id_a, id_b, "Supports".to_string())),
        "edge A→B (Supports) must be present, got: {edge_triples:?}"
    );
    // Edge A→D must be present.
    assert!(
        edge_triples.contains(&(id_a, id_d, "Supports".to_string())),
        "edge A→D (Supports) must be present, got: {edge_triples:?}"
    );
    // Edge B→C must NOT be present (C not in node set at max_depth=1).
    let has_bc = edge_triples
        .iter()
        .any(|(s, t, _)| *s == id_b && *t == id_c);
    assert!(
        !has_bc,
        "edge B→C must not appear (C excluded by dangling-edge filter), got: {edge_triples:?}"
    );

    // Verify depth_reached=1.
    let depth_reached = resp["depth_reached"].as_u64().expect("depth_reached");
    assert_eq!(depth_reached, 1, "depth_reached must be 1");

    // Verify truncated=false.
    let truncated = resp["truncated"].as_bool().expect("truncated");
    assert!(!truncated, "truncated must be false");

    // Verify seed_ids=[A].
    let seed_ids: Vec<u64> = resp["seed_ids"]
        .as_array()
        .expect("seed_ids")
        .iter()
        .map(|v| v.as_u64().expect("seed id"))
        .collect();
    assert_eq!(seed_ids, vec![id_a], "seed_ids must be [id_a]");
}

// ---------------------------------------------------------------------------
// T-AC14-02: Two-hop BFS with 5-entry chain A→B→C→D→E
//
// Topology:
//   A --Supports--> B --Supports--> C --Supports--> D --Supports--> E
//
// seed=[A], max_depth=2 → A, B, C present; D, E excluded (depth 3, 4)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_subgraph_two_hop_linear_chain() {
    if skip_if_no_model() {
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.db");
    let harness = match TestHarness::new(&path).await {
        Some(h) => h,
        None => return,
    };

    let id_a = insert_entry(&harness, "Chain-A").await;
    let id_b = insert_entry(&harness, "Chain-B").await;
    let id_c = insert_entry(&harness, "Chain-C").await;
    let id_d = insert_entry(&harness, "Chain-D").await;
    let id_e = insert_entry(&harness, "Chain-E").await;

    harness.insert_graph_edge(id_a, id_b, "Supports").await;
    harness.insert_graph_edge(id_b, id_c, "Supports").await;
    harness.insert_graph_edge(id_c, id_d, "Supports").await;
    harness.insert_graph_edge(id_d, id_e, "Supports").await;

    harness.rebuild_typed_graph().await;

    let resp = harness
        .call_graph(serde_json::json!({
            "mode": "subgraph",
            "seed_ids": [id_a],
            "max_depth": 2,
            "direction": "outgoing",
            "agent_id": "test-harness"
        }))
        .await
        .expect("subgraph call must succeed");

    let nodes = resp["nodes"].as_array().expect("nodes");
    let node_ids: Vec<u64> = nodes
        .iter()
        .map(|n| n["id"].as_u64().expect("id"))
        .collect();

    assert!(
        node_ids.contains(&id_a),
        "A must be present, got: {node_ids:?}"
    );
    assert!(
        node_ids.contains(&id_b),
        "B (depth 1) must be present, got: {node_ids:?}"
    );
    assert!(
        node_ids.contains(&id_c),
        "C (depth 2) must be present, got: {node_ids:?}"
    );
    assert!(
        !node_ids.contains(&id_d),
        "D (depth 3) must NOT be present at max_depth=2, got: {node_ids:?}"
    );
    assert!(
        !node_ids.contains(&id_e),
        "E (depth 4) must NOT be present at max_depth=2, got: {node_ids:?}"
    );

    let depth_reached = resp["depth_reached"].as_u64().expect("depth_reached");
    assert_eq!(depth_reached, 2, "depth_reached must be 2");
}

// ---------------------------------------------------------------------------
// T-AC14-03: direction default is "both" (FR-05) — bidirectional traversal
//
// Topology:
//   A --Supports--> B --Supports--> C
//   D --Supports--> B
//   E --Supports--> A
//
// seed=[B], max_depth=1, direction omitted (default="both")
// → B, A, C, D must be returned (all direct neighbors in both directions)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_subgraph_default_direction_both() {
    if skip_if_no_model() {
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.db");
    let harness = match TestHarness::new(&path).await {
        Some(h) => h,
        None => return,
    };

    let id_a = insert_entry(&harness, "Dir-A").await;
    let id_b = insert_entry(&harness, "Dir-B").await;
    let id_c = insert_entry(&harness, "Dir-C").await;
    let id_d = insert_entry(&harness, "Dir-D").await;
    let id_e = insert_entry(&harness, "Dir-E").await;

    // B→C (outgoing from B)
    harness.insert_graph_edge(id_b, id_c, "Supports").await;
    // A→B (incoming to B from A)
    harness.insert_graph_edge(id_a, id_b, "Supports").await;
    // D→B (incoming to B from D)
    harness.insert_graph_edge(id_d, id_b, "Supports").await;
    // E→A (not directly connected to B — only depth-2 via A)
    harness.insert_graph_edge(id_e, id_a, "Supports").await;

    harness.rebuild_typed_graph().await;

    // Call without direction (default = "both").
    let resp = harness
        .call_graph(serde_json::json!({
            "mode": "subgraph",
            "seed_ids": [id_b],
            "max_depth": 1,
            "agent_id": "test-harness"
        }))
        .await
        .expect("subgraph call must succeed");

    let nodes = resp["nodes"].as_array().expect("nodes");
    let node_ids: Vec<u64> = nodes
        .iter()
        .map(|n| n["id"].as_u64().expect("id"))
        .collect();

    // B (seed) must be present.
    assert!(
        node_ids.contains(&id_b),
        "B (seed) must be present, got: {node_ids:?}"
    );
    // C (outgoing from B at depth 1) must be present.
    assert!(
        node_ids.contains(&id_c),
        "C (outgoing depth-1) must be present in direction=both, got: {node_ids:?}"
    );
    // A (incoming to B at depth 1 — source of A→B) must be present.
    assert!(
        node_ids.contains(&id_a),
        "A (incoming depth-1) must be present in direction=both, got: {node_ids:?}"
    );
    // D (incoming to B at depth 1 — source of D→B) must be present.
    assert!(
        node_ids.contains(&id_d),
        "D (incoming depth-1) must be present in direction=both, got: {node_ids:?}"
    );
    // E is at depth 2 via A; must NOT be present at max_depth=1.
    assert!(
        !node_ids.contains(&id_e),
        "E (depth-2 via A) must NOT be present at max_depth=1, got: {node_ids:?}"
    );
}
