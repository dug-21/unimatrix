//! Personalized PageRank over positive edges (Supports, CoAccess, Prerequisite, Informs).
//!
//! This module is declared as a submodule of `graph.rs` via `#[path = "graph_ppr.rs"]`
//! and re-exported from there. It does NOT appear in `lib.rs` (ADR-001, SR-01 boundary).
//!
//! Structural model: mirrors `graph_suppression.rs` (col-030) in every convention:
//! - Pure function, no I/O, no async, no mutable global state.
//! - All traversal via `edges_of_type()` exclusively — no `.edges_directed()` calls (AC-02).
//! - Tests live in `graph_ppr_tests.rs` (overflow split per C-09 / NFR-08).

use std::collections::HashMap;

use petgraph::Direction;
use petgraph::stable_graph::NodeIndex;
use petgraph::visit::EdgeRef;

use crate::graph::{RelationType, TypedRelationGraph};

/// Compute Personalized PageRank over positive edges (Supports, CoAccess, Prerequisite, Informs).
///
/// SR-01 constrains `graph_penalty` and `find_terminal_active` to Supersedes-only
/// traversal; it does not restrict new retrieval functions from using other edge types.
/// PPR uses Supports, CoAccess, Prerequisite, and Informs.
///
/// `seed_scores` must be pre-normalized to sum 1.0 (caller responsibility).
/// The function does NOT re-normalize internally.
///
/// Returns an empty HashMap if `seed_scores` is empty.
/// Runs exactly `iterations` steps — no early-exit convergence check (determinism).
///
/// All graph traversal uses `edges_of_type()` exclusively (AC-02, SR-01 boundary).
/// Direct `.edges_directed()` or `.neighbors_directed()` calls are prohibited here.
///
/// **PPR direction**: Pull from outgoing targets implements the reverse random walk (transpose PPR).
/// For an edge A→B (Supports: A supports B), node A accumulates mass from B's score because A
/// points to B (Direction::Outgoing). When B (a decision) is seeded, mass flows backward to A
/// (the lesson-learned that supports B). Direction::Incoming would implement standard forward PPR:
/// B would pull from A, propagating mass away from seeds toward their successors — the opposite
/// of the desired behavior.
pub fn personalized_pagerank(
    graph: &TypedRelationGraph,
    seed_scores: &HashMap<u64, f64>,
    alpha: f64,
    iterations: usize,
) -> HashMap<u64, f64> {
    // Zero-sum guard (FR-01): empty seed map → return immediately (no iteration, no allocation).
    if seed_scores.is_empty() {
        return HashMap::new();
    }

    // If the graph has no nodes, there is nothing to iterate over.
    if graph.node_index.is_empty() {
        return HashMap::new();
    }

    // ADR-004: sort all node IDs ascending ONCE before the iteration loop.
    // This Vec is constructed HERE, never inside the loop.
    // Covers ALL nodes in the graph (not just seeds) for correct deterministic order.
    let mut all_node_ids: Vec<u64> = graph.node_index.keys().copied().collect();
    all_node_ids.sort_unstable();

    // Initialize current score map: seed nodes get their personalization mass;
    // all other nodes start at 0.0.
    let mut current_scores: HashMap<u64, f64> = HashMap::with_capacity(all_node_ids.len());
    for &node_id in &all_node_ids {
        let score = seed_scores.get(&node_id).copied().unwrap_or(0.0);
        current_scores.insert(node_id, score);
    }

    // Power iteration: exactly `iterations` steps, no early exit (determinism requirement).
    for _ in 0..iterations {
        let mut next_scores: HashMap<u64, f64> = HashMap::with_capacity(current_scores.len());

        // Accumulate contributions in sorted node-ID order (correctness constraint, ADR-004).
        for &node_id in &all_node_ids {
            let node_idx = match graph.node_index.get(&node_id) {
                Some(&idx) => idx,
                None => continue, // defensive; should not occur — both built from the same node_index
            };

            // Teleportation term: (1 - alpha) * personalization[v]
            let teleport = (1.0 - alpha) * seed_scores.get(&node_id).copied().unwrap_or(0.0);

            // Reverse-walk neighbor contribution term:
            //   alpha * Σ_{v: node_id→v ∈ positive_edges}
            //           (current_scores[v] * edge_weight(node_id, v) / positive_out_degree(node_id))
            //
            // Traverse Direction::Outgoing on this node to reach targets v such that node_id → v.
            // Four separate edges_of_type calls (AC-02 — no .edges_directed() allowed).
            // Fourth call: RelationType::Informs (crt-037).
            //
            // This formulation surfaces nodes that point to highly-scored seeds:
            // if node_id→v and v is a seed, node_id gains mass proportional to v's score.
            let out_degree = positive_out_degree_weight(graph, node_idx);

            let mut neighbor_contribution: f64 = 0.0;

            if out_degree > 0.0 {
                for edge_ref in
                    graph.edges_of_type(node_idx, RelationType::Supports, Direction::Outgoing)
                {
                    neighbor_contribution +=
                        outgoing_contribution(&current_scores, &edge_ref, out_degree, graph);
                }
                for edge_ref in
                    graph.edges_of_type(node_idx, RelationType::CoAccess, Direction::Outgoing)
                {
                    neighbor_contribution +=
                        outgoing_contribution(&current_scores, &edge_ref, out_degree, graph);
                }
                for edge_ref in
                    graph.edges_of_type(node_idx, RelationType::Prerequisite, Direction::Outgoing)
                {
                    neighbor_contribution +=
                        outgoing_contribution(&current_scores, &edge_ref, out_degree, graph);
                }
                for edge_ref in
                    graph.edges_of_type(node_idx, RelationType::Informs, Direction::Outgoing)
                {
                    neighbor_contribution +=
                        outgoing_contribution(&current_scores, &edge_ref, out_degree, graph);
                }
            }

            next_scores.insert(node_id, teleport + alpha * neighbor_contribution);
        }

        current_scores = next_scores;
    }

    current_scores
}

/// Compute one outgoing neighbor's contribution to the current node's score.
///
/// When traversing `Direction::Outgoing` from node u to target v (edge u→v),
/// v's current score (weighted by edge weight, normalized by u's out-degree) feeds into u.
/// This implements the reverse random-walk: nodes that point to high-scoring seeds gain mass.
///
/// Private helper used inside the iteration loop.
fn outgoing_contribution(
    current_scores: &HashMap<u64, f64>,
    edge_ref: &petgraph::stable_graph::EdgeReference<'_, crate::graph::RelationEdge>,
    out_degree: f64,
    graph: &TypedRelationGraph,
) -> f64 {
    // Target node (v): the node the edge points TO (Outgoing direction).
    let target_idx = edge_ref.target();
    let target_id = graph.inner[target_idx];

    // Edge weight: f32 → f64 cast (RelationEdge.weight).
    let edge_weight = edge_ref.weight().weight as f64;

    // Current score of target — 0.0 if not yet in the map.
    let target_score = current_scores.get(&target_id).copied().unwrap_or(0.0);
    if target_score == 0.0 {
        return 0.0; // early exit: no mass to propagate
    }

    target_score * edge_weight / out_degree
}

/// Compute the sum of outgoing Supports + CoAccess + Prerequisite + Informs edge weights from a node.
///
/// Used for out-degree normalization in PPR. Returns 0.0 for nodes with no positive out-edges.
/// All traversal uses `edges_of_type()` exclusively (AC-02).
/// `Supersedes` and `Contradicts` edges are excluded by construction.
fn positive_out_degree_weight(graph: &TypedRelationGraph, node_idx: NodeIndex) -> f64 {
    let mut total: f64 = 0.0;

    // Four outgoing edge-type queries (AC-02: edges_of_type only).
    // Fourth call: RelationType::Informs (crt-037).
    for edge_ref in graph.edges_of_type(node_idx, RelationType::Supports, Direction::Outgoing) {
        total += edge_ref.weight().weight as f64;
    }
    for edge_ref in graph.edges_of_type(node_idx, RelationType::CoAccess, Direction::Outgoing) {
        total += edge_ref.weight().weight as f64;
    }
    for edge_ref in graph.edges_of_type(node_idx, RelationType::Prerequisite, Direction::Outgoing) {
        total += edge_ref.weight().weight as f64;
    }
    for edge_ref in graph.edges_of_type(node_idx, RelationType::Informs, Direction::Outgoing) {
        total += edge_ref.weight().weight as f64;
    }

    total
}

/// Test-only re-export of `positive_out_degree_weight` so `graph_ppr_tests.rs` can call it
/// directly for AC-06 assertions without making the function part of the public API.
#[cfg(test)]
pub fn positive_out_degree_weight_pub_for_test(
    graph: &TypedRelationGraph,
    node_idx: NodeIndex,
) -> f64 {
    positive_out_degree_weight(graph, node_idx)
}

// -- Tests --

#[cfg(test)]
#[path = "graph_ppr_tests.rs"]
mod tests;
