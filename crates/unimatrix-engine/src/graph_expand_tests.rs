//! Unit tests for `graph_expand::graph_expand`.
//!
//! Split from `graph_expand.rs` per NFR-09 (500-line max per file).
//! Declared via `#[path = "graph_expand_tests.rs"] mod tests;` in `graph_expand.rs`.

use std::collections::HashSet;

use unimatrix_core::{EntryRecord, Status};

use crate::graph::{GraphEdgeRow, RelationType, TypedRelationGraph, build_typed_relation_graph};

use super::graph_expand;

// ---- Test helpers ----

/// Returns an empty TypedRelationGraph (no nodes, no edges).
fn make_graph() -> TypedRelationGraph {
    TypedRelationGraph::empty()
}

/// Builds a TypedRelationGraph from a slice of (source_id, target_id, RelationType, weight).
/// All referenced IDs are added as nodes automatically.
fn make_graph_with_edges(edges: &[(u64, u64, RelationType, f32)]) -> TypedRelationGraph {
    let mut ids: Vec<u64> = Vec::new();
    for &(src, tgt, _, _) in edges {
        if !ids.contains(&src) {
            ids.push(src);
        }
        if !ids.contains(&tgt) {
            ids.push(tgt);
        }
    }

    let entries: Vec<EntryRecord> = ids.iter().map(|&id| make_entry(id)).collect();

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

/// Minimal EntryRecord for graph node insertion (matches graph_ppr_tests.rs make_entry).
fn make_entry(id: u64) -> EntryRecord {
    EntryRecord {
        id,
        title: format!("Entry {id}"),
        content: String::new(),
        topic: String::new(),
        category: "decision".to_string(),
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

// ---- AC-03: Positive Edge Types ----

/// AC-03 / CoAccess: outgoing CoAccess edge surfaces neighbor.
#[test]
fn test_graph_expand_coaccess_surfaces_neighbor() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::CoAccess, 1.0)]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert_eq!(
        result,
        HashSet::from([2u64]),
        "CoAccess edge (1→2) must surface entry 2"
    );
}

/// AC-03 / Supports: outgoing Supports edge surfaces neighbor.
#[test]
fn test_graph_expand_supports_surfaces_neighbor() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supports, 1.0)]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert_eq!(
        result,
        HashSet::from([2u64]),
        "Supports edge (1→2) must surface entry 2"
    );
}

/// AC-03 / Informs: outgoing Informs edge surfaces neighbor.
#[test]
fn test_graph_expand_informs_surfaces_neighbor() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Informs, 1.0)]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert_eq!(
        result,
        HashSet::from([2u64]),
        "Informs edge (1→2) must surface entry 2"
    );
}

/// AC-03 / Prerequisite: outgoing Prerequisite edge surfaces neighbor.
#[test]
fn test_graph_expand_prerequisite_surfaces_neighbor() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Prerequisite, 1.0)]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert_eq!(
        result,
        HashSet::from([2u64]),
        "Prerequisite edge (1→2) must surface entry 2"
    );
}

// ---- AC-04: Backward Edge Does NOT Surface ----

/// AC-04: backward edge (3→seed_1) must not surface entry 3 via Outgoing traversal.
///
/// This is the behavioral proof of the direction contract (ADR-006). Given seed {1}
/// and edge 3→1 (Incoming to seed), Outgoing-only traversal from seed 1 sees no
/// outgoing positive edges. Entry 3 must not surface.
#[test]
fn test_graph_expand_backward_edge_does_not_surface() {
    let graph = make_graph_with_edges(&[(3, 1, RelationType::Supports, 1.0)]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert!(
        result.is_empty(),
        "backward edge (3→seed_1) must not surface entry 3 via Outgoing traversal"
    );
}

// ---- AC-05: Two-Hop Chain, Depth=2 ----

/// AC-05: graph 1→2→3, seeds {1}, depth=2 — both hops must be returned.
#[test]
fn test_graph_expand_two_hop_depth2_surfaces_both() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::CoAccess, 1.0),
        (2, 3, RelationType::Supports, 1.0),
    ]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert_eq!(
        result,
        HashSet::from([2u64, 3u64]),
        "depth=2 must surface both hop-1 (2) and hop-2 (3)"
    );
}

// ---- AC-06: Two-Hop Chain, Depth=1 ----

/// AC-06: same graph as AC-05, depth=1 — only first hop returned.
#[test]
fn test_graph_expand_two_hop_depth1_surfaces_only_first() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::CoAccess, 1.0),
        (2, 3, RelationType::Supports, 1.0),
    ]);
    let result = graph_expand(&graph, &[1], 1, 200);
    assert_eq!(
        result,
        HashSet::from([2u64]),
        "depth=1 must surface only the first hop; second hop must be excluded"
    );
    assert!(
        !result.contains(&3),
        "entry 3 is at depth 2 and must be absent when depth=1"
    );
}

// ---- AC-07: Excluded Edge Types (Supersedes, Contradicts) ----

/// AC-07 / Supersedes: Supersedes edges must not be traversed.
#[test]
fn test_graph_expand_supersedes_not_traversed() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supersedes, 1.0)]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert!(
        result.is_empty(),
        "Supersedes edges must not be traversed by graph_expand"
    );
}

/// AC-07 / Contradicts: Contradicts edges must not be traversed.
#[test]
fn test_graph_expand_contradicts_not_traversed() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Contradicts, 1.0)]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert!(
        result.is_empty(),
        "Contradicts edges must not be traversed by graph_expand"
    );
}

// ---- AC-08: Seed Exclusion ----

/// AC-08: seeds must be excluded from result even if reachable via graph edges.
#[test]
fn test_graph_expand_seeds_excluded_from_result() {
    // edge A→B; both A and B are seeds; B is reachable from A but is itself a seed.
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supports, 1.0)]);
    let result = graph_expand(&graph, &[1, 2], 2, 200);
    assert!(
        result.is_empty(),
        "seed IDs must be excluded from result even if reachable via graph edges"
    );
    assert!(!result.contains(&1), "seed 1 must not appear in result");
    assert!(!result.contains(&2), "seed 2 must not appear in result");
}

/// AC-08 / self-loop: self-loop on a seed must not add the seed to result.
#[test]
fn test_graph_expand_self_loop_seed_not_returned() {
    let graph = make_graph_with_edges(&[(1, 1, RelationType::CoAccess, 1.0)]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert!(
        result.is_empty(),
        "self-loop on a seed must not add the seed to the result"
    );
}

// ---- AC-09: Max Candidates Early Exit ----

/// AC-09: max_candidates cap is enforced exactly.
#[test]
fn test_graph_expand_max_candidates_cap() {
    let edges: Vec<(u64, u64, RelationType, f32)> = (2u64..=201)
        .map(|i| (1, i, RelationType::Supports, 1.0))
        .collect();
    let graph = make_graph_with_edges(&edges);
    let result = graph_expand(&graph, &[1], 1, 10);
    assert_eq!(
        result.len(),
        10,
        "result must contain exactly max_candidates=10 entries when cap is hit"
    );
    assert!(
        result.iter().all(|id| (2..=201).contains(id)),
        "all returned IDs must be valid neighbors of seed 1"
    );
}

// ---- AC-10: Empty Seeds ----

/// AC-10: empty seed list must return empty set immediately, no panic.
#[test]
fn test_graph_expand_empty_seeds_returns_empty() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supports, 1.0)]);
    let result = graph_expand(&graph, &[], 2, 200);
    assert!(
        result.is_empty(),
        "empty seed list must return empty set immediately"
    );
}

// ---- AC-11: Empty Graph ----

/// AC-11: graph with no nodes must return empty set immediately, no panic.
#[test]
fn test_graph_expand_empty_graph_returns_empty() {
    let graph = make_graph(); // TypedRelationGraph::empty()
    let result = graph_expand(&graph, &[1, 2], 2, 200);
    assert!(
        result.is_empty(),
        "graph with no nodes must return empty set immediately"
    );
}

// ---- AC-12: Depth Zero ----

/// AC-12: depth=0 must return empty set immediately per FR-03.
#[test]
fn test_graph_expand_depth_zero_returns_empty() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supports, 1.0)]);
    let result = graph_expand(&graph, &[1], 0, 200);
    assert!(
        result.is_empty(),
        "depth=0 must return empty set immediately per FR-03"
    );
}

// ---- R-11: BFS Visited-Set (Cycle Termination) ----

/// R-11 / bidirectional: bidirectional CoAccess must not cause infinite loop.
#[test]
fn test_graph_expand_bidirectional_terminates() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::CoAccess, 1.0),
        (2, 1, RelationType::CoAccess, 1.0),
    ]);
    let result = graph_expand(&graph, &[1], 2, 200);
    // Entry 2 is reachable (1→2). Entry 1 is a seed, excluded.
    // Without visited-set: 1→2→1→2... would loop until max_candidates hit.
    assert_eq!(
        result,
        HashSet::from([2u64]),
        "bidirectional CoAccess must not cause infinite loop; visited-set must prevent revisit"
    );
}

/// R-11 / triangle: triangular cycle (1→2→3→1) must terminate; only non-seed entries returned.
#[test]
fn test_graph_expand_triangular_cycle_terminates() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::Supports, 1.0),
        (2, 3, RelationType::Supports, 1.0),
        (3, 1, RelationType::Supports, 1.0),
    ]);
    let result = graph_expand(&graph, &[1], 3, 200);
    // 1 is seed (excluded); 2 and 3 are reachable.
    assert_eq!(
        result,
        HashSet::from([2u64, 3u64]),
        "triangular cycle must terminate; only non-seed reachable entries returned"
    );
}

// ---- R-13: Determinism ----

/// R-13 / NFR-04: two calls with identical inputs produce identical results (budget-boundary exercised).
///
/// The three lowest IDs (2, 3, 4) should be returned with max=3, since the BFS frontier
/// is processed in sorted node-ID order per hop (ADR-004 crt-030).
#[test]
fn test_graph_expand_deterministic_across_calls() {
    let graph = make_graph_with_edges(&[
        (1, 5, RelationType::Supports, 1.0),
        (1, 2, RelationType::CoAccess, 1.0),
        (1, 3, RelationType::Informs, 1.0),
        (1, 4, RelationType::Prerequisite, 1.0),
        (1, 6, RelationType::Supports, 1.0),
    ]);
    let result_a = graph_expand(&graph, &[1], 1, 3);
    let result_b = graph_expand(&graph, &[1], 1, 3);
    assert_eq!(
        result_a, result_b,
        "graph_expand must be deterministic: same inputs must produce identical HashSets"
    );
    assert_eq!(
        result_a.len(),
        3,
        "budget-boundary: exactly max_candidates=3 results expected"
    );
    // Sorted frontier: 2, 3, 4, 5, 6 — first 3 are {2, 3, 4}.
    assert_eq!(
        result_a,
        HashSet::from([2u64, 3u64, 4u64]),
        "sorted frontier must return the three lowest-ID neighbors when cap is hit"
    );
}

// ---- R-02: S1/S2 Unidirectional Understanding (documentation tests) ----

/// R-02 / S1/S2 failure mode: higher-ID seed cannot reach lower-ID entry via single-direction
/// Informs edge (1→2 only). This is the failure mode the AC-00 back-fill fixes.
#[test]
fn test_graph_expand_unidirectional_informs_from_higher_id_seed_misses() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Informs, 1.0)]);
    let result = graph_expand(&graph, &[2], 2, 200);
    assert!(
        result.is_empty(),
        "before back-fill: higher-ID seed (2) cannot reach lower-ID entry (1) via \
         single-direction Informs edge (1→2 only). This is the failure mode AC-00 back-fill fixes."
    );
}

/// R-02 / S1/S2 post-backfill: bidirectional Informs edges — higher-ID seed reaches lower-ID entry.
#[test]
fn test_graph_expand_bidirectional_informs_after_backfill() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::Informs, 1.0),
        (2, 1, RelationType::Informs, 1.0), // back-fill direction
    ]);
    let result = graph_expand(&graph, &[2], 2, 200);
    assert_eq!(
        result,
        HashSet::from([1u64]),
        "after back-fill: higher-ID seed (2) reaches lower-ID entry (1) via reverse Informs edge"
    );
}

// ---- R-17: S8 CoAccess Unidirectional Gap (documentation test) ----

/// R-17: S8 single-direction CoAccess — higher-ID seed cannot reach lower-ID partner
/// without the crt-035 promotion tick adding the reverse direction.
#[test]
fn test_graph_expand_s8_coaccess_unidirectional_from_higher_id_misses() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::CoAccess, 1.0)]);
    let result = graph_expand(&graph, &[2], 2, 200);
    assert!(
        result.is_empty(),
        "S8 single-direction CoAccess: higher-ID seed cannot reach lower-ID partner \
         without the crt-035 promotion tick adding the reverse direction"
    );
}
