//! Graph expansion via BFS — widen PPR candidate pool (crt-042).
//!
//! Declared as a submodule of `graph.rs` via `#[path = "graph_expand.rs"]`.
//! Re-exported from `graph.rs` as `pub use graph_expand::graph_expand`.
//! Does NOT appear in `lib.rs` (ADR-001, entry #3731).
//!
//! Structural mirrors: `graph_suppression.rs`, `graph_ppr.rs`.
//! - Pure function, no I/O, no async, no mutable global state.
//! - All traversal via `edges_of_type()` exclusively — no `.edges_directed()` calls (AC-16, SR-01).
//! - Tests live in `graph_expand_tests.rs` if inline tests push file over 500 lines (NFR-09).
//!
//! ## Caller Quarantine Obligation (FR-06)
//!
//! `graph_expand` is a pure function and performs NO quarantine checks. Any caller
//! that adds returned IDs to a result set (e.g., Phase 0 in search.rs) MUST
//! independently apply `SecurityGateway::is_quarantined()` before use. Future callers
//! outside search.rs must observe the same obligation — the function contract does not
//! include security enforcement.
//!
//! ## Traversal Contract (behavioral, entry #3754)
//!
//! Given seed B and edge B→A (Outgoing from B), entry A is reachable.
//! Given seed B and edge C→B (Incoming to B), entry C is NOT reachable.
//! Traversal is expressed behaviorally. No Direction:: constant is authoritative
//! in this specification (ADR-006).
//!
//! ## Combined Expansion Ceiling (SR-04)
//!
//! Phase 0 (crt-042) contributes at most `max_candidates` entries to the candidate pool
//! (default 200). Phase 5 (existing) contributes at most `ppr_max_expand` entries (default 50).
//! Combined with HNSW k=20, the maximum pool size before PPR scoring is 270.
//! This ceiling is enforced by the independent caps of each phase, not a combined check.

use std::collections::{HashSet, VecDeque};

use petgraph::Direction;
use petgraph::stable_graph::NodeIndex;
use petgraph::visit::EdgeRef;

use crate::graph::{RelationType, TypedRelationGraph};

/// Expand a seed set via BFS over positive edges in the `TypedRelationGraph`.
///
/// Returns the set of entry IDs reachable from `seed_ids` within `depth` hops via
/// positive edge types (`CoAccess`, `Supports`, `Informs`, `Prerequisite`), excluding
/// the seeds themselves, capped at `max_candidates`.
///
/// ## Behavioral Contract
///
/// - Entry T surfaces when seed S exists and edge S→T of type CoAccess, Supports,
///   Informs, or Prerequisite exists (S points to T via Outgoing traversal).
/// - Entry C does NOT surface when seed B exists and only edge C→B exists (no reverse
///   edge). This is an incoming edge to the seed; Outgoing-only traversal does not follow it.
/// - Returns empty when: `seed_ids` is empty, graph has no nodes, or `depth = 0`.
/// - BFS frontier processed in sorted node-ID order per hop (determinism, ADR-004 crt-030).
/// - Visited-set prevents revisiting nodes (prevents oscillation on bidirectional edges).
/// - All traversal via `edges_of_type()` exclusively — no direct `.edges_directed()` or
///   `.neighbors_directed()` calls (SR-01, entry #3627).
/// - Pure, synchronous, no I/O, no locking, no side effects.
/// - Excluded edge types: `Supersedes` (structural chain), `Contradicts` (negative signal).
///
/// ## Notes
///
/// - Seeds not present in `graph.node_index` are silently skipped (no panic).
/// - Early exit when `max_candidates` reached: entries already in queue are discarded.
/// - `can_expand_further`: a node at `current_depth == depth` is added to result but
///   does NOT enqueue its neighbors, enforcing the depth limit.
pub fn graph_expand(
    graph: &TypedRelationGraph,
    seed_ids: &[u64],
    depth: usize,
    max_candidates: usize,
) -> HashSet<u64> {
    // Degenerate case guards (FR-03 / AC-10, AC-11, AC-12).
    if seed_ids.is_empty() {
        return HashSet::new();
    }
    if graph.node_index.is_empty() {
        return HashSet::new();
    }
    if depth == 0 {
        return HashSet::new();
    }

    // Initialize BFS state.
    // visited: tracks all entry IDs already considered — prevents revisiting (R-11).
    // Seeds are pre-inserted so they can never appear in the result (AC-08).
    // BFS queue carries (entry_id, current_hop_depth).
    let mut visited: HashSet<u64> = seed_ids.iter().copied().collect();
    let mut result: HashSet<u64> = HashSet::new();
    let mut queue: VecDeque<(u64, usize)> = VecDeque::new();

    // Enqueue all seeds at hop 0.
    // Seeds whose entry_id is NOT in graph.node_index are silently skipped (no panic).
    for &seed_id in seed_ids {
        if graph.node_index.contains_key(&seed_id) {
            queue.push_back((seed_id, 0));
        }
    }

    // BFS loop.
    'outer: while let Some((current_id, current_depth)) = queue.pop_front() {
        // Resolve NodeIndex for current entry ID.
        let node_idx: NodeIndex = match graph.node_index.get(&current_id) {
            None => continue, // defensive; should not happen if visited set is consistent
            Some(&idx) => idx,
        };

        // Stop expanding from this node if we have already reached max depth.
        // (We still process the node itself, but don't enqueue its neighbors.)
        let can_expand_further = current_depth < depth;

        // Traverse all positive edge types from this node (Outgoing direction).
        // Four separate edges_of_type calls (SR-01: no .edges_directed() allowed, AC-16).
        // Process neighbors in SORTED NODE-ID ORDER for determinism (NFR-04, ADR-004 crt-030).
        //
        // Positive types: CoAccess, Supports, Informs, Prerequisite.
        // Excluded types: Supersedes (structural chain), Contradicts (negative signal).
        let mut neighbors: Vec<u64> = Vec::new();

        for edge_ref in graph.edges_of_type(node_idx, RelationType::CoAccess, Direction::Outgoing) {
            neighbors.push(graph.inner[edge_ref.target()]);
        }
        for edge_ref in graph.edges_of_type(node_idx, RelationType::Supports, Direction::Outgoing) {
            neighbors.push(graph.inner[edge_ref.target()]);
        }
        for edge_ref in graph.edges_of_type(node_idx, RelationType::Informs, Direction::Outgoing) {
            neighbors.push(graph.inner[edge_ref.target()]);
        }
        for edge_ref in
            graph.edges_of_type(node_idx, RelationType::Prerequisite, Direction::Outgoing)
        {
            neighbors.push(graph.inner[edge_ref.target()]);
        }

        // Sort for deterministic frontier processing (NFR-04, C-09).
        // dedup removes duplicate target IDs from multi-edge pairs (different edge types,
        // same target) preventing double-counting toward max_candidates.
        neighbors.sort_unstable();
        neighbors.dedup();

        // Only add neighbors to result when we haven't yet reached the depth limit.
        // Nodes at current_depth == depth are themselves already in result (added when
        // discovered by their parent at depth current_depth-1), but they do not expand
        // further — their own neighbors are not surfaced.
        if !can_expand_further {
            continue;
        }

        for neighbor_id in neighbors {
            if result.len() >= max_candidates {
                // Early exit: budget reached. Stop processing and return.
                break 'outer;
            }

            if visited.contains(&neighbor_id) {
                continue; // Already queued or added (prevents cycles, R-11).
            }

            visited.insert(neighbor_id);
            result.insert(neighbor_id);

            // Enqueue for further expansion. When current_depth + 1 == depth, the
            // neighbor will be dequeued at the depth limit and won't expand further
            // (the !can_expand_further guard above will fire). This is correct:
            // depth-limit nodes land in result but their own neighbors are not explored.
            queue.push_back((neighbor_id, current_depth + 1));
        }
    }

    result
}

// -- Tests --

#[cfg(test)]
#[path = "graph_expand_tests.rs"]
mod tests;
