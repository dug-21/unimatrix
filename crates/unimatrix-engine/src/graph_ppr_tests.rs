//! Unit tests for `graph_ppr::personalized_pagerank`.
//!
//! Split from `graph_ppr.rs` per C-09 / NFR-08 (500-line max per file).
//! Declared via `#[path = "graph_ppr_tests.rs"] mod tests;` in `graph_ppr.rs`.

use std::collections::HashMap;

use crate::graph::{GraphEdgeRow, RelationType, TypedRelationGraph, build_typed_relation_graph};

use super::personalized_pagerank;

// ---- Test helpers ----

/// Returns an empty TypedRelationGraph (no nodes, no edges).
fn make_graph() -> TypedRelationGraph {
    TypedRelationGraph::empty()
}

/// Builds a TypedRelationGraph by inserting nodes for all referenced IDs
/// and adding directed edges per the input slice.
fn make_graph_with_edges(edges: &[(u64, u64, RelationType, f32)]) -> TypedRelationGraph {
    // Collect all unique node IDs referenced in the edges.
    let mut ids: Vec<u64> = Vec::new();
    for &(src, tgt, _, _) in edges {
        if !ids.contains(&src) {
            ids.push(src);
        }
        if !ids.contains(&tgt) {
            ids.push(tgt);
        }
    }

    let entries: Vec<unimatrix_core::EntryRecord> = ids.iter().map(|&id| make_entry(id)).collect();

    let edge_rows: Vec<GraphEdgeRow> = edges
        .iter()
        .map(|&(src, tgt, rel, weight)| GraphEdgeRow {
            source_id: src,
            target_id: tgt,
            relation_type: rel.as_str().to_string(),
            weight,
            created_at: 0,
            created_by: "test".to_string(),
            source: "test".to_string(),
            bootstrap_only: false,
        })
        .collect();

    build_typed_relation_graph(&entries, &edge_rows).expect("test graph build must succeed")
}

/// Returns a normalized seed map where each ID has weight 1.0 / ids.len().
fn uniform_seeds(ids: &[u64]) -> HashMap<u64, f64> {
    let weight = 1.0 / ids.len() as f64;
    ids.iter().map(|&id| (id, weight)).collect()
}

fn make_entry(id: u64) -> unimatrix_core::EntryRecord {
    unimatrix_core::EntryRecord {
        id,
        title: format!("Entry {id}"),
        content: String::new(),
        topic: String::new(),
        category: "decision".to_string(),
        tags: vec![],
        source: String::new(),
        status: unimatrix_core::Status::Active,
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

// ---- Edge-case / FR-01 tests ----

/// E-01 / FR-01: Empty seed scores → empty return (no panic, no allocation).
#[test]
fn test_ppr_empty_seed_map_returns_empty() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::Supports, 1.0),
        (2, 3, RelationType::CoAccess, 0.8),
    ]);
    let result = personalized_pagerank(&graph, &HashMap::new(), 0.85, 20);
    assert!(
        result.is_empty(),
        "empty seed_scores must return empty HashMap"
    );
}

/// E-01: Empty graph — both seeds and graph empty → empty return.
#[test]
fn test_ppr_empty_graph_returns_empty() {
    let graph = make_graph();
    let result = personalized_pagerank(&graph, &HashMap::new(), 0.85, 20);
    assert!(result.is_empty());
}

/// E-01: Non-empty seeds but empty graph — graph has no nodes, return empty.
#[test]
fn test_ppr_empty_graph_nonempty_seeds_returns_empty() {
    let graph = make_graph();
    let seeds: HashMap<u64, f64> = [(42u64, 1.0)].into_iter().collect();
    let result = personalized_pagerank(&graph, &seeds, 0.85, 20);
    assert!(result.is_empty(), "no nodes in graph → nothing to score");
}

/// E-03: Single seed node, no edges — teleportation only, no divide-by-zero.
#[test]
fn test_ppr_single_node_no_edges() {
    let entries = vec![make_entry(1)];
    let g = build_typed_relation_graph(&entries, &[]).expect("single-node graph must succeed");

    let seeds: HashMap<u64, f64> = [(1u64, 1.0)].into_iter().collect();
    let result = personalized_pagerank(&g, &seeds, 0.85, 20);

    assert!(
        result.get(&1).copied().unwrap_or(0.0) > 0.0,
        "seed gets teleportation mass"
    );
    assert_eq!(result.len(), 1, "only one node in the graph");
}

/// E-02: No positive edges at all — teleportation only.
#[test]
fn test_ppr_no_positive_edges_only_teleportation() {
    // All edges via Supersedes (not a positive type).
    // Supersedes must be added via entries.supersedes field.
    let mut e2 = make_entry(2);
    let mut e3 = make_entry(3);
    e2.supersedes = Some(1);
    e3.supersedes = Some(2);
    let entries = vec![make_entry(1), e2, e3];
    let g = build_typed_relation_graph(&entries, &[]).expect("build ok");

    let seeds: HashMap<u64, f64> = [(1u64, 1.0)].into_iter().collect();
    let result = personalized_pagerank(&g, &seeds, 0.85, 20);

    assert!(
        result.get(&1).copied().unwrap_or(0.0) > 0.0,
        "seed has teleportation mass"
    );
    // Nodes 2 and 3 have no positive out-edges reachable from the seed's positive-edge paths.
    assert_eq!(
        result.get(&2).copied().unwrap_or(0.0),
        0.0,
        "node 2 has no positive path from seed"
    );
    assert_eq!(
        result.get(&3).copied().unwrap_or(0.0),
        0.0,
        "node 3 has no positive path from seed"
    );
}

/// E-07: Disconnected subgraph — isolated nodes get zero score.
#[test]
fn test_ppr_disconnected_subgraph_zero_expansion() {
    // Nodes A=1, B=2, C=3 connected via Supports A→B→C.
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::Supports, 1.0),
        (2, 3, RelationType::Supports, 1.0),
    ]);
    // Seeds: A=1 and B=2.
    let seeds: HashMap<u64, f64> = [(1u64, 0.5), (2u64, 0.5)].into_iter().collect();
    let result = personalized_pagerank(&graph, &seeds, 0.85, 20);

    // A and B are seeds — they have teleportation mass.
    assert!(result.get(&1).copied().unwrap_or(0.0) > 0.0);
    assert!(result.get(&2).copied().unwrap_or(0.0) > 0.0);
    // A points to B (a seed) so A gains neighbor contribution on top of teleportation.
    // C has no out-edges to seeds, so C gets zero (no reverse-walk path to seeds).
    assert!(
        result.get(&1).copied().unwrap_or(0.0) > result.get(&3).copied().unwrap_or(0.0),
        "A (seed, points to seed B) scores higher than C (no out-edges to seeds)"
    );
}

// ---- AC-02 / R-09: Supersedes and Contradicts edges excluded ----

/// T-PPR-08 / AC-03 / R-09: Supersedes edge excluded — seed does not propagate via Supersedes.
#[test]
fn test_supersedes_edge_excluded_from_ppr() {
    // n_with_supers supersedes s: edge 1→2 Supersedes (pred→succ).
    let mut n_with_supers = make_entry(2);
    n_with_supers.supersedes = Some(1);
    let entries = vec![make_entry(1), n_with_supers];
    let g = build_typed_relation_graph(&entries, &[]).expect("build ok");

    // Seed: node 1. Node 2 is reachable only via Supersedes from 1.
    let seeds: HashMap<u64, f64> = [(1u64, 1.0)].into_iter().collect();
    let result = personalized_pagerank(&g, &seeds, 0.85, 20);

    // Node 2 must receive zero PPR mass (Supersedes excluded from positive-edge set).
    assert_eq!(
        result.get(&2).copied().unwrap_or(0.0),
        0.0,
        "N must receive zero PPR mass via Supersedes edge"
    );
}

/// T-PPR-09 / AC-03 / R-09: Contradicts edge excluded.
#[test]
fn test_contradicts_edge_excluded_from_ppr() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Contradicts, 1.0)]);
    let seeds: HashMap<u64, f64> = [(1u64, 1.0)].into_iter().collect();
    let result = personalized_pagerank(&graph, &seeds, 0.85, 20);

    assert_eq!(
        result.get(&2).copied().unwrap_or(0.0),
        0.0,
        "N must receive zero PPR mass via Contradicts edge"
    );
}

// ---- AC-07 / R-07: Zero positive out-degree ----

/// Zero positive out-degree: node with only Supersedes out-edge does not propagate.
#[test]
fn test_zero_positive_out_degree_no_forward_propagation() {
    // A=1 has only a Supersedes out-edge (not positive). Seed: A=1.
    let mut b = make_entry(2);
    b.supersedes = Some(1); // 1→2 Supersedes; A (id=1) has zero positive out-edges
    let entries = vec![make_entry(1), b, make_entry(3)];
    let g = build_typed_relation_graph(&entries, &[]).expect("build ok");

    let seeds: HashMap<u64, f64> = [(1u64, 1.0)].into_iter().collect();
    let result = personalized_pagerank(&g, &seeds, 0.85, 20);

    // B gets zero (Supersedes edge from A is not positive; A has no positive out-edges).
    assert_eq!(
        result.get(&2).copied().unwrap_or(0.0),
        0.0,
        "B receives zero — A has no positive out-edges to propagate from"
    );
    // A itself has teleportation mass (it is a seed).
    assert!(result.get(&1).copied().unwrap_or(0.0) > 0.0);
}

/// Node with mixed edges only propagates via positive types.
#[test]
fn test_node_with_mixed_edges_only_propagates_via_positive() {
    // A=1→B=2 Supersedes; A=1→C=3 Supports. Seed: C=3.
    let mut b = make_entry(2);
    b.supersedes = Some(1); // 1→2 Supersedes
    let entries = vec![make_entry(1), b, make_entry(3)];
    let edge_rows = vec![GraphEdgeRow {
        source_id: 1,
        target_id: 3,
        relation_type: RelationType::Supports.as_str().to_string(),
        weight: 1.0,
        created_at: 0,
        created_by: "test".to_string(),
        source: "test".to_string(),
        bootstrap_only: false,
    }];
    let g = build_typed_relation_graph(&entries, &edge_rows).expect("build ok");

    // Seed: C=3.
    let seeds: HashMap<u64, f64> = [(3u64, 1.0)].into_iter().collect();
    let result = personalized_pagerank(&g, &seeds, 0.85, 5);

    // A gains mass from C (A→C Supports, C is seed): A surfaces as supporter of C.
    assert!(
        result.get(&1).copied().unwrap_or(0.0) > 0.0,
        "A must surface because A→C Supports and C is a seed"
    );
    // B receives zero (Supersedes excluded, B has no out-edges to seeds).
    assert_eq!(
        result.get(&2).copied().unwrap_or(0.0),
        0.0,
        "B must receive zero — Supersedes excluded, B has no positive path to C"
    );
}

// ---- AC-08 / R-12: Edge direction semantics ----

/// T-PPR-04 / test_plan: Supports direction — seed B surfaces A (A→B edge, seed on B).
#[test]
fn test_supports_incoming_direction() {
    // A=1→B=2 via Supports. B is the seed (a decision that A supports).
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supports, 1.0)]);
    let seeds: HashMap<u64, f64> = [(2u64, 1.0)].into_iter().collect();
    let result = personalized_pagerank(&graph, &seeds, 0.85, 20);

    // A is surfaced as supporter of B: A→B and B is high-scoring → A gains mass.
    assert!(
        result.get(&1).copied().unwrap_or(0.0) > 0.0,
        "A must surface as supporter of B (A→B Supports, B is seed)"
    );
}

/// Direction sanity: when A is the seed (not B), B must NOT receive edge-propagated mass.
#[test]
fn test_supports_seed_does_not_propagate_to_target() {
    // A=1→B=2 via Supports; A is the seed.
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supports, 1.0)]);
    let seeds: HashMap<u64, f64> = [(1u64, 1.0)].into_iter().collect();
    let result = personalized_pagerank(&graph, &seeds, 0.85, 20);

    // A→B: A accumulates from B's score. B is not a seed (score=0) → B stays 0.
    assert_eq!(
        result.get(&2).copied().unwrap_or(0.0),
        0.0,
        "B must not receive edge-propagated mass when A is seed (B has no out-edges to seeds)"
    );
    assert!(
        result.get(&1).copied().unwrap_or(0.0) > 0.0,
        "A (seed) retains teleportation mass"
    );
}

/// T-PPR-06 / AC-18: CoAccess direction — seed S, edge N→S, N surfaces.
#[test]
fn test_coaccess_incoming_direction() {
    // Edge: N=2 → S=1 via CoAccess, weight=0.8. Seed: S=1.
    // N→S and S is seed → N gains mass from S's high score.
    let graph = make_graph_with_edges(&[(2, 1, RelationType::CoAccess, 0.8)]);
    let seeds: HashMap<u64, f64> = [(1u64, 1.0)].into_iter().collect();
    let result = personalized_pagerank(&graph, &seeds, 0.85, 20);

    // N surfaces: N→S CoAccess, S is a high-scoring seed → N accumulates from S.
    assert!(
        result.get(&2).copied().unwrap_or(0.0) > 0.0,
        "N must surface via CoAccess N→S (N gains mass from seed S)"
    );
}

/// T-PPR-07 / R-12: Prerequisite direction — seed B surfaces A (A→B edge).
#[test]
fn test_prerequisite_incoming_direction() {
    // A=1 is prerequisite of B=2: edge A→B via Prerequisite. Seed: B.
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Prerequisite, 1.0)]);
    let seeds: HashMap<u64, f64> = [(2u64, 1.0)].into_iter().collect();
    let result = personalized_pagerank(&graph, &seeds, 0.85, 20);

    // A surfaces as prerequisite of B: A→B and B is seed → A gains mass from B.
    assert!(
        result.get(&1).copied().unwrap_or(0.0) > 0.0,
        "A must surface as prerequisite of B (A→B Prerequisite, B is seed)"
    );
}

/// R-12 regression guard: when A is seed, B does not receive edge-based mass.
#[test]
fn test_prerequisite_wrong_direction_does_not_propagate() {
    // A=1→B=2 via Prerequisite; A is the seed.
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Prerequisite, 1.0)]);
    let seeds: HashMap<u64, f64> = [(1u64, 1.0)].into_iter().collect();
    let result = personalized_pagerank(&graph, &seeds, 0.85, 20);

    // A→B and A is seed: A accumulates from B's score. B is not a seed, so B=0.
    assert_eq!(
        result.get(&2).copied().unwrap_or(0.0),
        0.0,
        "B must not receive edge-based mass from A when A is seed"
    );
    assert!(
        result.get(&1).copied().unwrap_or(0.0) > result.get(&2).copied().unwrap_or(0.0),
        "A score must exceed B score"
    );
}

/// Proof that Direction::Outgoing is the deliberate correct choice for reverse-walk PPR.
///
/// Graph: A=1 → B=2 via Supports (A supports B).
/// Seed: B only.
///
/// WHY Direction::Outgoing achieves reverse-walk behavior:
///   A points to B (Outgoing edge from A), so when iterating A's outgoing neighbors we
///   find B. A therefore accumulates from B's seed score. Mass flows *backward*: seeding
///   B surfaces A — exactly the goal (surface lesson-learneds that support a seeded decision).
///
///   If Direction::Incoming were used instead, B would pull from A (B's incoming neighbor).
///   A starts at 0.0 and B is already the seed, so no additional mass would reach A via
///   the edge — A would score only 0.0 from the edge term, staying at zero (no teleportation
///   because A is not in seed_scores).
///
/// This test FAILS if Direction::Incoming is substituted — that is the proof that Outgoing
/// is the deliberate correct choice.
#[test]
fn test_ppr_outgoing_not_incoming_is_correct_direction() {
    // Minimal graph: single Supports edge A=1 → B=2 (A supports B).
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supports, 1.0)]);

    // Seed exclusively on B (the decision that A supports).
    let seed_scores: HashMap<u64, f64> = [(2u64, 1.0)].into_iter().collect();

    let result = personalized_pagerank(&graph, &seed_scores, 0.85, 20);

    // A must receive non-trivial PPR mass: Direction::Outgoing lets A accumulate from B's seed.
    // This assertion fails if Direction::Incoming is used (A would score 0.0 via the edge).
    assert!(
        result.get(&1).copied().unwrap_or(0.0) > 0.0,
        "A must receive mass from B's seed via Direction::Outgoing (reverse walk). \
         A score = {}. If this is 0.0, Direction::Incoming was used instead — wrong direction.",
        result.get(&1).copied().unwrap_or(0.0)
    );
}

// ---- AC-05 / R-04: Determinism ----

/// T-PPR-10: Same inputs, two calls, identical output (exact HashMap equality).
#[test]
fn test_ppr_deterministic_same_inputs() {
    let graph = make_graph_with_edges(&[
        (1, 3, RelationType::Supports, 1.0),
        (2, 3, RelationType::CoAccess, 0.7),
        (3, 4, RelationType::Supports, 1.0),
        (4, 5, RelationType::CoAccess, 0.5),
        (5, 1, RelationType::CoAccess, 0.9),
    ]);
    let seeds: HashMap<u64, f64> = [(1u64, 0.5), (2u64, 0.3), (3u64, 0.2)]
        .into_iter()
        .collect();

    let result1 = personalized_pagerank(&graph, &seeds, 0.85, 20);
    let result2 = personalized_pagerank(&graph, &seeds, 0.85, 20);

    assert_eq!(
        result1, result2,
        "PPR must be deterministic: two calls with identical inputs must produce identical outputs"
    );
}

/// Determinism on larger graph (100 nodes, ~300 edges).
#[test]
fn test_ppr_deterministic_large_graph() {
    let mut edges: Vec<(u64, u64, RelationType, f32)> = Vec::new();
    for i in 0u64..100 {
        let target = (i * 7 + 3) % 100;
        if target != i {
            let rel = if i % 3 == 0 {
                RelationType::Supports
            } else {
                RelationType::CoAccess
            };
            edges.push((i + 1, target + 1, rel, 1.0));
        }
        let target2 = (i * 13 + 7) % 100;
        if target2 != i && target2 != target {
            edges.push((i + 1, target2 + 1, RelationType::CoAccess, 0.5));
        }
        let target3 = (i * 17 + 11) % 100;
        if target3 != i && target3 != target && target3 != target2 {
            edges.push((i + 1, target3 + 1, RelationType::Supports, 0.8));
        }
    }
    let graph = make_graph_with_edges(&edges);
    let seeds: HashMap<u64, f64> = [(1u64, 0.4), (50u64, 0.35), (99u64, 0.25)]
        .into_iter()
        .collect();

    let result1 = personalized_pagerank(&graph, &seeds, 0.85, 20);
    let result2 = personalized_pagerank(&graph, &seeds, 0.85, 20);

    assert_eq!(
        result1, result2,
        "PPR must be deterministic on large graphs"
    );
}

/// R-04 sort-length check: result cannot have more keys than the graph has nodes.
#[test]
fn test_ppr_sort_covers_all_nodes() {
    let mut edges: Vec<(u64, u64, RelationType, f32)> = Vec::new();
    // 50 nodes with varied IDs, not sequential.
    for i in 0u64..50 {
        let id = i * 3 + 7; // non-sequential IDs: 7, 10, 13, ...
        let target_id = ((i + 1) % 50) * 3 + 7;
        edges.push((id, target_id, RelationType::CoAccess, 0.5));
    }
    let graph = make_graph_with_edges(&edges);
    let node_count = graph.node_index.len();

    let seeds = uniform_seeds(&graph.node_index.keys().copied().take(5).collect::<Vec<_>>());
    let result = personalized_pagerank(&graph, &seeds, 0.85, 10);

    assert!(
        result.len() <= node_count,
        "result cannot have more keys ({}) than graph has nodes ({})",
        result.len(),
        node_count
    );
}

// ---- R-07: NaN / Infinity guards ----

/// All PPR scores must be finite and in [0.0, 1.0] for realistic input.
#[test]
fn test_ppr_scores_all_finite() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::Supports, 1.0),
        (2, 3, RelationType::CoAccess, 0.8),
        (3, 4, RelationType::Supports, 0.6),
        (4, 1, RelationType::CoAccess, 0.4),
        (1, 5, RelationType::Prerequisite, 1.0),
        (5, 3, RelationType::CoAccess, 0.7),
        (2, 4, RelationType::Supports, 0.9),
        (4, 5, RelationType::CoAccess, 0.3),
        (3, 5, RelationType::Supports, 0.5),
        (5, 2, RelationType::CoAccess, 0.6),
    ]);
    let total = 0.5_f64 + 0.3 + 0.2;
    let seeds: HashMap<u64, f64> = [
        (1u64, 0.5 / total),
        (2u64, 0.3 / total),
        (3u64, 0.2 / total),
    ]
    .into_iter()
    .collect();

    let result = personalized_pagerank(&graph, &seeds, 0.85, 20);

    for (&id, &score) in &result {
        assert!(
            score.is_finite(),
            "score for id={id} must be finite, got {score}"
        );
        assert!(
            score >= 0.0,
            "score for id={id} must be non-negative, got {score}"
        );
        assert!(
            score <= 1.0 + f64::EPSILON,
            "score for id={id} must be <= 1.0, got {score}"
        );
    }
}

/// MIN_POSITIVE seed — must produce finite output (no NaN from near-zero arithmetic).
#[test]
fn test_ppr_single_min_positive_seed_no_nan() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supports, 1.0)]);
    let seeds: HashMap<u64, f64> = [(1u64, f64::MIN_POSITIVE)].into_iter().collect();
    let result = personalized_pagerank(&graph, &seeds, 0.85, 20);

    for (&id, &score) in &result {
        assert!(
            score.is_finite(),
            "score for id={id} must be finite with MIN_POSITIVE seed"
        );
    }
}

// ---- Timing tests ----

/// Dense 50-node CoAccess graph must complete within 5ms (5x NFR budget as test gate).
/// T-PPR-12 / R-13.
#[test]
#[cfg(not(debug_assertions))]
fn test_ppr_dense_50_node_coaccess_completes_under_5ms() {
    // 50 nodes each CoAccess-connected to every other node (50 x 49 = 2450 edges).
    let mut edges: Vec<(u64, u64, RelationType, f32)> = Vec::new();
    for i in 1u64..=50 {
        for j in 1u64..=50 {
            if i != j {
                edges.push((i, j, RelationType::CoAccess, 0.5));
            }
        }
    }
    let graph = make_graph_with_edges(&edges);
    let seeds = uniform_seeds(&(1u64..=50).collect::<Vec<_>>());

    let t = std::time::Instant::now();
    let _result = personalized_pagerank(&graph, &seeds, 0.85, 20);
    let elapsed = t.elapsed();

    assert!(
        elapsed.as_millis() < 5,
        "PPR on 50-node dense CoAccess graph took {}ms (must be < 5ms)",
        elapsed.as_millis()
    );
}

/// 10K-node scale gate — marked #[ignore] to exclude from normal cargo test runs.
/// Run with: cargo test -- --ignored test_ppr_10k_node_completes_within_budget
/// T-PPR-11 / R-04.
#[test]
#[ignore]
fn test_ppr_10k_node_completes_within_budget() {
    let mut edges: Vec<(u64, u64, RelationType, f32)> = Vec::new();
    for i in 1u64..=10_000 {
        for k in 1u64..=5 {
            let target = (i * (k * 7 + 3)) % 10_000 + 1;
            if target != i {
                edges.push((i, target, RelationType::CoAccess, 0.5));
            }
        }
    }
    let graph = make_graph_with_edges(&edges);
    let seeds = uniform_seeds(&[1, 100, 500, 1000, 2500, 5000, 7500, 9000, 9500, 10000]);

    let t = std::time::Instant::now();
    let _result = personalized_pagerank(&graph, &seeds, 0.85, 20);
    let elapsed = t.elapsed();

    assert!(
        elapsed.as_millis() < 10,
        "PPR on 10K-node graph took {}ms (must be < 10ms)",
        elapsed.as_millis()
    );
}

// ---- crt-037: Informs edge PPR traversal (AC-05, AC-06, R-02) ----

/// AC-05 / R-02: Informs edge propagates PPR mass to lesson node when decision node is seeded.
///
/// Two-node graph: lesson A (id=1) --Informs--> decision B (id=2).
/// Both A→B (forward, the Informs edge) and B→A (reverse) are inserted per entry #3896:
/// without the B→A reverse edge the test would pass vacuously — A would score 0.0 regardless
/// of direction because A has no path back to B via PPR.
///
/// Seed: B (the decision node). After PPR, A must have a non-zero score.
/// The assertion is on the SPECIFIC node index for A (id=1), not an aggregate check.
///
/// If Direction::Incoming were used in the fourth edges_of_type call, A would receive
/// zero mass because PPR would pull FROM A's incoming neighbors (none), not push from
/// B's score. See entries #3744 and #3896.
#[test]
fn test_personalized_pagerank_informs_edge_propagates_mass_to_lesson_node() {
    // lesson_A --Informs--> decision_B, plus reverse edge so both directions exist.
    // Entry #3896 trap: without B→A, A scores 0.0 regardless of Direction choice.
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::Informs, 0.5), // A→B: the Informs edge
        (2, 1, RelationType::Informs, 0.5), // B→A: reverse edge (required per #3896)
    ]);

    // Seed exclusively on B (the decision node, id=2).
    let seed_scores: HashMap<u64, f64> = [(2u64, 1.0)].into_iter().collect();

    let scores = personalized_pagerank(&graph, &seed_scores, 0.85, 20);

    // AC-05: assert by the specific lesson node index (id=1), not scores.values().any(...)
    assert!(
        scores.get(&1).copied().unwrap_or(0.0) > 0.0,
        "lesson node A (id=1) must receive non-zero PPR mass when decision node B (id=2) is seeded. \
         score[A]={:.6}. If 0.0, Direction::Incoming was used instead of Direction::Outgoing. \
         See entry #3744 (direction trap) and entry #3896 (both-edges required).",
        scores.get(&1).copied().unwrap_or(0.0)
    );
}

/// AC-05 extension / test-plan: three-node graph — unrelated node C must receive zero mass.
///
/// Three nodes: lesson A (id=1), decision B (id=2), unrelated C (id=3).
/// Informs edge A→B and reverse B→A. No edges to/from C.
/// Seed: B. After PPR: A > 0.0, C == 0.0.
#[test]
fn test_personalized_pagerank_decision_seed_reaches_only_lesson_via_informs() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::Informs, 0.5), // A→B
        (2, 1, RelationType::Informs, 0.5), // B→A (reverse, required per #3896)
                                            // C (id=3) has no edges
    ]);
    // C must appear in the graph even with no edges — add it via an isolated entry.
    // make_graph_with_edges only adds nodes referenced in edges, so we build manually.
    use crate::graph::{GraphEdgeRow, build_typed_relation_graph};
    let entries = vec![make_entry(1), make_entry(2), make_entry(3)];
    let edge_rows = vec![
        GraphEdgeRow {
            source_id: 1,
            target_id: 2,
            relation_type: RelationType::Informs.as_str().to_string(),
            weight: 0.5,
            created_at: 0,
            created_by: "test".to_string(),
            source: "test".to_string(),
            bootstrap_only: false,
        },
        GraphEdgeRow {
            source_id: 2,
            target_id: 1,
            relation_type: RelationType::Informs.as_str().to_string(),
            weight: 0.5,
            created_at: 0,
            created_by: "test".to_string(),
            source: "test".to_string(),
            bootstrap_only: false,
        },
    ];
    let graph =
        build_typed_relation_graph(&entries, &edge_rows).expect("test graph build must succeed");

    let seed_scores: HashMap<u64, f64> = [(2u64, 1.0)].into_iter().collect();
    let scores = personalized_pagerank(&graph, &seed_scores, 0.85, 20);

    // A surfaces (lesson informs decision B, B is seeded).
    assert!(
        scores.get(&1).copied().unwrap_or(0.0) > 0.0,
        "lesson A must receive non-zero PPR mass when decision B is seeded"
    );
    // C is unreachable — no edges to or from C.
    assert_eq!(
        scores.get(&3).copied().unwrap_or(0.0),
        0.0,
        "unrelated node C must receive zero PPR mass (no edges)"
    );
}

/// test-plan: Informs and Supports produce comparable PPR mass with equal edge weights.
///
/// Verifies that Informs participates in the PPR mass pool with the same mechanics as
/// Supports. Two independent two-node graphs, each with matching weights.
#[test]
fn test_personalized_pagerank_informs_weight_influences_mass() {
    // Informs pair: A=1 → B=2 (+ reverse), seed at B.
    let graph_informs = make_graph_with_edges(&[
        (1, 2, RelationType::Informs, 0.8),
        (2, 1, RelationType::Informs, 0.8),
    ]);
    // Supports pair: C=3 → D=4 (+ reverse), seed at D.
    let graph_supports = make_graph_with_edges(&[
        (3, 4, RelationType::Supports, 0.8),
        (4, 3, RelationType::Supports, 0.8),
    ]);

    let seed_b: HashMap<u64, f64> = [(2u64, 1.0)].into_iter().collect();
    let seed_d: HashMap<u64, f64> = [(4u64, 1.0)].into_iter().collect();

    let scores_informs = personalized_pagerank(&graph_informs, &seed_b, 0.85, 20);
    let scores_supports = personalized_pagerank(&graph_supports, &seed_d, 0.85, 20);

    let score_a = scores_informs.get(&1).copied().unwrap_or(0.0);
    let score_c = scores_supports.get(&3).copied().unwrap_or(0.0);

    assert!(
        score_a > 0.0,
        "Informs: lesson A must receive non-zero PPR mass"
    );
    assert!(
        score_c > 0.0,
        "Supports: node C must receive non-zero PPR mass"
    );

    // Both should be comparable in magnitude (same edge weight, same graph topology).
    let ratio = if score_c > 0.0 {
        score_a / score_c
    } else {
        0.0
    };
    assert!(
        (0.9..=1.1).contains(&ratio),
        "Informs and Supports with equal weights must produce comparable PPR mass. \
         score_a={score_a:.6}, score_c={score_c:.6}, ratio={ratio:.4}"
    );
}

/// AC-06: positive_out_degree_weight includes Informs edge weight.
///
/// Node X with exactly one outgoing Informs edge to Y at weight 0.6, no other edges.
/// positive_out_degree_weight(X) must return 0.6, not 0.0.
#[test]
fn test_positive_out_degree_weight_includes_informs_edge() {
    use super::positive_out_degree_weight_pub_for_test as positive_out_degree_weight;

    // Build graph: X (id=10) → Y (id=11) via Informs, weight 0.6.
    let graph = make_graph_with_edges(&[(10, 11, RelationType::Informs, 0.6)]);

    let x_idx = *graph.node_index.get(&10).expect("node 10 must exist");
    let result = positive_out_degree_weight(&graph, x_idx);

    assert!(
        (result - 0.6_f64).abs() < 1e-6,
        "positive_out_degree_weight for node with only Informs edge must return 0.6, got {result}"
    );
}

/// AC-06 additive: Informs weight adds to existing positive edges, does not replace.
///
/// Node X with one Supports edge (weight 0.8) and one Informs edge (weight 0.6).
/// positive_out_degree_weight(X) must return 1.4.
#[test]
fn test_positive_out_degree_weight_informs_adds_to_existing_positive_edges() {
    use super::positive_out_degree_weight_pub_for_test as positive_out_degree_weight;

    // X (id=20) → Y (id=21) Supports 0.8, X (id=20) → Z (id=22) Informs 0.6.
    let graph = make_graph_with_edges(&[
        (20, 21, RelationType::Supports, 0.8),
        (20, 22, RelationType::Informs, 0.6),
    ]);

    let x_idx = *graph.node_index.get(&20).expect("node 20 must exist");
    let result = positive_out_degree_weight(&graph, x_idx);

    assert!(
        (result - 1.4_f64).abs() < 1e-6,
        "positive_out_degree_weight must sum Supports + Informs: expected 1.4, got {result}"
    );
}

/// Supersedes edge is not included in positive_out_degree_weight (penalty edge).
#[test]
fn test_positive_out_degree_weight_supersedes_not_included() {
    use super::positive_out_degree_weight_pub_for_test as positive_out_degree_weight;

    // Node 30 supersedes node 31 — Supersedes is a penalty edge, not positive.
    let mut entry31 = make_entry(31);
    entry31.supersedes = Some(30);
    let entries = vec![make_entry(30), entry31];
    let graph = crate::graph::build_typed_relation_graph(&entries, &[]).expect("build ok");

    let idx30 = *graph.node_index.get(&30).expect("node 30 must exist");
    let result = positive_out_degree_weight(&graph, idx30);

    assert_eq!(
        result, 0.0,
        "Supersedes is a penalty edge — positive_out_degree_weight must return 0.0, got {result}"
    );
}

/// Direction regression guard (R-02, C-14): Informs mass flows correctly with Direction::Outgoing.
///
/// Documents the wrong-direction failure mode. With Direction::Outgoing (correct),
/// lesson node A receives mass when decision node B is seeded.
///
/// If Direction::Incoming were substituted in the fourth edges_of_type call, this test
/// would fail: A would score 0.0 because PPR would look at A's incoming neighbors
/// (none, since only A→B and B→A exist) rather than A's outgoing target B.
/// See entry #3744 (direction semantics trap documented from crt-030).
#[test]
fn test_direction_outgoing_required_for_informs_mass_flow() {
    // lesson A (id=100) --Informs--> decision B (id=101).
    // Both edges present per entry #3896.
    // Comment: if Direction::Incoming were used instead of Direction::Outgoing in the
    // fourth edges_of_type call, scores[100] would be 0.0. See entry #3744.
    let graph = make_graph_with_edges(&[
        (100, 101, RelationType::Informs, 0.7),
        (101, 100, RelationType::Informs, 0.7),
    ]);

    let seed_scores: HashMap<u64, f64> = [(101u64, 1.0)].into_iter().collect();
    let scores = personalized_pagerank(&graph, &seed_scores, 0.85, 20);

    // With Direction::Outgoing, A accumulates from B's seed score.
    assert!(
        scores.get(&100).copied().unwrap_or(0.0) > 0.0,
        "Direction::Outgoing must allow lesson A (id=100) to accumulate mass from seeded \
         decision B (id=101). score[100]={:.6}. \
         A score of 0.0 indicates Direction::Incoming was used — wrong direction. \
         See entry #3744.",
        scores.get(&100).copied().unwrap_or(0.0)
    );
}
