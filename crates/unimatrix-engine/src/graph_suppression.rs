//! Contradicts collision suppression for search result sets.
//!
//! Provides `suppress_contradicts` — a pure function that removes the lower-ranked
//! member of any result pair connected by a `Contradicts` edge in a `TypedRelationGraph`.
//!
//! This module is declared as a submodule of `graph.rs` and re-exported from there.
//! It does NOT appear in `lib.rs` (ADR-001, R-09).

use std::collections::HashSet;

use petgraph::Direction;
use petgraph::visit::EdgeRef;

use crate::graph::{RelationType, TypedRelationGraph};

/// Compute a keep/drop bitmask for a ranked result set, suppressing the lower-ranked
/// member of any pair connected by a `Contradicts` edge in `graph`.
///
/// # Arguments
///
/// - `result_ids`: entry IDs in descending rank order (index 0 = highest ranked).
/// - `graph`: read-only typed relation graph containing `Contradicts` edges.
///
/// # Returns
///
/// A tuple `(keep_mask, contradicting_ids)` where both `Vec`s have length
/// `result_ids.len()`:
///
/// - `keep_mask[i] = true`  — entry at index `i` is retained.
/// - `keep_mask[i] = false` — entry at index `i` is suppressed.
/// - `contradicting_ids[i] = Some(id)` — the ID of the highest-ranked entry that
///   contradicts entry `i` (set when `keep_mask[i] = false`).
/// - `contradicting_ids[i] = None` — entry `i` is not suppressed.
///
/// # Properties
///
/// - Pure: no I/O, no async, no mutable state.
/// - Deterministic: same inputs always produce the same output.
/// - Both directions queried per candidate entry (ADR-003): NLI writes `Contradicts`
///   edges unidirectionally; direction is non-deterministic from suppression's perspective.
/// - All graph traversal goes through `edges_of_type` (ADR-002, SR-01 boundary).
/// - Already-suppressed entries still propagate their `Contradicts` edges to lower-ranked
///   entries (Option B — matches chain suppression test case T-GS-04).
pub fn suppress_contradicts(
    result_ids: &[u64],
    graph: &TypedRelationGraph,
) -> (Vec<bool>, Vec<Option<u64>>) {
    let n = result_ids.len();
    let mut keep_mask: Vec<bool> = vec![true; n];
    let mut contradicting_ids: Vec<Option<u64>> = vec![None; n];

    for i in 0..n {
        let entry_id = result_ids[i];

        // Resolve node index; skip if entry not in graph (e.g. stored after last tick rebuild).
        let node_idx = match graph.node_index.get(&entry_id) {
            None => continue,
            Some(&idx) => idx,
        };

        // Query Outgoing Contradicts edges from this node (ADR-002, ADR-003).
        // edges_of_type is the sole traversal boundary (SR-01); no direct .edges_directed().
        let outgoing_neighbors: HashSet<u64> = graph
            .edges_of_type(node_idx, RelationType::Contradicts, Direction::Outgoing)
            .map(|edge_ref| graph.inner[edge_ref.target()])
            .collect();

        // Query Incoming Contradicts edges to this node (ADR-003 — unidirectional NLI writes).
        let incoming_neighbors: HashSet<u64> = graph
            .edges_of_type(node_idx, RelationType::Contradicts, Direction::Incoming)
            .map(|edge_ref| graph.inner[edge_ref.source()])
            .collect();

        // Union of both directions (idempotent if NLI ever adds reverse edges).
        let contradicts_neighbors: HashSet<u64> = outgoing_neighbors
            .union(&incoming_neighbors)
            .copied()
            .collect();

        if contradicts_neighbors.is_empty() {
            continue;
        }

        // Suppress any lower-ranked entry whose ID is in the contradicts neighbors set.
        // NOTE: outer loop processes ALL entries including already-suppressed ones —
        // a suppressed entry still propagates its Contradicts edges (Option B, T-GS-04).
        for j in (i + 1)..n {
            if keep_mask[j] && contradicts_neighbors.contains(&result_ids[j]) {
                keep_mask[j] = false;
                contradicting_ids[j] = Some(entry_id);
                // Do NOT break — one entry may contradict multiple lower-ranked entries.
            }
        }
    }

    (keep_mask, contradicting_ids)
}

#[cfg(test)]
mod tests {
    use unimatrix_core::{EntryRecord, Status};

    use crate::graph::{
        GraphEdgeRow, RelationType, TypedRelationGraph, build_typed_relation_graph,
    };

    use super::*;

    // -- Test helpers --

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

    fn contradicts_edge(source_id: u64, target_id: u64) -> GraphEdgeRow {
        GraphEdgeRow {
            source_id,
            target_id,
            relation_type: RelationType::Contradicts.as_str().to_string(),
            weight: 1.0,
            created_at: 0,
            created_by: "test".to_string(),
            source: "nli".to_string(),
            bootstrap_only: false, // MUST be false — bootstrap_only=true excluded by build_typed_relation_graph
        }
    }

    fn edge_row(source_id: u64, target_id: u64, rel: RelationType) -> GraphEdgeRow {
        GraphEdgeRow {
            source_id,
            target_id,
            relation_type: rel.as_str().to_string(),
            weight: 1.0,
            created_at: 0,
            created_by: "test".to_string(),
            source: "test".to_string(),
            bootstrap_only: false,
        }
    }

    // -- Tests --

    /// T-GS-01: Empty graph — all entries kept, no-panic on empty input.
    #[test]
    fn test_suppress_contradicts_empty_graph_all_kept() {
        let graph = build_typed_relation_graph(&[], &[]).unwrap();
        let result_ids = vec![10u64, 20u64, 30u64];

        let (mask, cids) = suppress_contradicts(&result_ids, &graph);

        assert_eq!(mask.len(), 3);
        assert_eq!(mask, vec![true, true, true]);
        assert_eq!(cids.len(), 3);
        assert_eq!(cids, vec![None, None, None]);

        // Empty input no-panic sub-case (R-06).
        let (mask_empty, cids_empty) = suppress_contradicts(&[], &graph);
        assert_eq!(mask_empty, Vec::<bool>::new());
        assert_eq!(cids_empty, Vec::<Option<u64>>::new());
    }

    /// T-GS-02: Outgoing Contradicts rank-0 → rank-1; rank-1 suppressed.
    #[test]
    fn test_suppress_contradicts_outgoing_rank0_to_rank1_suppressed() {
        let entries = vec![make_entry(1), make_entry(2)];
        let edges = vec![contradicts_edge(1, 2)]; // source=1(rank-0), target=2(rank-1)
        let graph = build_typed_relation_graph(&entries, &edges).unwrap();
        let result_ids = vec![1u64, 2u64];

        let (mask, cids) = suppress_contradicts(&result_ids, &graph);

        assert_eq!(mask.len(), 2);
        assert!(mask[0], "rank-0 must be retained");
        assert!(!mask[1], "rank-1 must be suppressed");
        assert_eq!(cids.len(), 2);
        assert_eq!(cids[0], None);
        assert_eq!(cids[1], Some(1));
    }

    /// T-GS-03: Outgoing Contradicts rank-0 → rank-3 (non-adjacent); rank-1/2 unaffected.
    #[test]
    fn test_suppress_contradicts_outgoing_rank0_to_rank3_nonadjacent() {
        let entries = vec![make_entry(1), make_entry(2), make_entry(3), make_entry(4)];
        let edges = vec![contradicts_edge(1, 4)]; // rank-0→rank-3
        let graph = build_typed_relation_graph(&entries, &edges).unwrap();
        let result_ids = vec![1u64, 2u64, 3u64, 4u64];

        let (mask, cids) = suppress_contradicts(&result_ids, &graph);

        assert_eq!(mask.len(), 4);
        assert_eq!(mask, vec![true, true, true, false]);
        assert_eq!(cids.len(), 4);
        assert_eq!(cids[0], None);
        assert_eq!(cids[1], None);
        assert_eq!(cids[2], None);
        assert_eq!(cids[3], Some(1));
    }

    /// T-GS-04: Chain — rank-0 contradicts rank-2; rank-2 contradicts rank-3.
    /// Both rank-2 and rank-3 suppressed (Option B: suppressed nodes still propagate).
    #[test]
    fn test_suppress_contradicts_chain_suppressed_node_propagates() {
        let entries = vec![make_entry(1), make_entry(2), make_entry(3), make_entry(4)];
        let edges = vec![
            contradicts_edge(1, 3), // rank-0→rank-2
            contradicts_edge(3, 4), // rank-2→rank-3
        ];
        let graph = build_typed_relation_graph(&entries, &edges).unwrap();
        let result_ids = vec![1u64, 2u64, 3u64, 4u64];

        let (mask, cids) = suppress_contradicts(&result_ids, &graph);

        assert_eq!(mask.len(), 4);
        assert_eq!(mask, vec![true, true, false, false]);
        assert_eq!(cids.len(), 4);
        assert_eq!(cids[0], None);
        assert_eq!(cids[1], None);
        assert_eq!(cids[2], Some(1)); // rank-2 suppressed by entry id=1
        assert_eq!(cids[3], Some(3)); // rank-3 suppressed by entry id=3 (which propagates even though suppressed)
    }

    /// T-GS-05: Non-Contradicts edges only — all entries kept.
    #[test]
    fn test_suppress_contradicts_non_contradicts_edges_no_suppression() {
        let entries = vec![make_entry(1), make_entry(2), make_entry(3)];
        let edges = vec![
            edge_row(1, 2, RelationType::CoAccess),
            edge_row(2, 1, RelationType::CoAccess), // CoAccess is bidirectional in practice
            edge_row(1, 3, RelationType::Supports),
        ];
        let graph = build_typed_relation_graph(&entries, &edges).unwrap();
        let result_ids = vec![1u64, 2u64, 3u64];

        let (mask, cids) = suppress_contradicts(&result_ids, &graph);

        assert_eq!(mask.len(), 3);
        assert_eq!(mask, vec![true, true, true]);
        assert_eq!(cids.len(), 3);
        assert_eq!(cids, vec![None, None, None]);
    }

    /// T-GS-06: Incoming direction — edge written rank-1 → rank-0; rank-1 must still be suppressed.
    /// Critical test: catches an Outgoing-only implementation (R-05, ADR-003, AC-03).
    #[test]
    fn test_suppress_contradicts_incoming_direction_rank1_suppressed() {
        let entries = vec![make_entry(1), make_entry(2)];
        let edges = vec![contradicts_edge(2, 1)]; // source=2(rank-1), target=1(rank-0) — INCOMING from rank-0
        let graph = build_typed_relation_graph(&entries, &edges).unwrap();
        let result_ids = vec![1u64, 2u64]; // rank-0=id 1, rank-1=id 2

        let (mask, cids) = suppress_contradicts(&result_ids, &graph);

        assert_eq!(mask.len(), 2);
        assert!(mask[0], "rank-0 must be retained");
        assert!(
            !mask[1],
            "rank-1 must be suppressed via Incoming direction query"
        );
        assert_eq!(cids.len(), 2);
        assert_eq!(cids[0], None);
        assert_eq!(cids[1], Some(1)); // detected via Incoming direction from rank-0
    }

    /// T-GS-07: Contradicts only between rank-2 and rank-3; rank-0 and rank-1 unaffected.
    /// Expected: only rank-3 suppressed; rank-2 is the suppressor and remains kept.
    #[test]
    fn test_suppress_contradicts_edge_only_between_rank2_and_rank3() {
        let entries = vec![make_entry(1), make_entry(2), make_entry(3), make_entry(4)];
        let edges = vec![contradicts_edge(3, 4)]; // rank-2→rank-3
        let graph = build_typed_relation_graph(&entries, &edges).unwrap();
        let result_ids = vec![1u64, 2u64, 3u64, 4u64];

        let (mask, cids) = suppress_contradicts(&result_ids, &graph);

        assert_eq!(mask.len(), 4);
        assert_eq!(mask, vec![true, true, true, false]); // only rank-3 suppressed
        assert_eq!(cids.len(), 4);
        assert_eq!(cids[0], None);
        assert_eq!(cids[1], None);
        assert_eq!(cids[2], None); // rank-2 is the suppressor, not a victim
        assert_eq!(cids[3], Some(3)); // rank-3 suppressed by entry id=3
    }

    /// T-GS-08: Empty TypedRelationGraph (cold-start) — all entries kept without panic.
    /// Confirms function is safe when all IDs are absent from node_index.
    #[test]
    fn test_suppress_contradicts_empty_typed_relation_graph_all_kept() {
        let graph = TypedRelationGraph::empty();
        let result_ids = vec![1u64, 2u64, 3u64];

        let (mask, cids) = suppress_contradicts(&result_ids, &graph);

        assert_eq!(mask.len(), 3);
        assert_eq!(mask, vec![true, true, true]);
        assert_eq!(cids.len(), 3);
        assert_eq!(cids, vec![None, None, None]);
    }
}
