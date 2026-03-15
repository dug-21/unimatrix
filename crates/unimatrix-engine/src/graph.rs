//! Supersession DAG — topology-aware penalty computation and multi-hop traversal.
//!
//! Builds a directed acyclic graph (DAG) from entry supersession relationships
//! at query time. Provides:
//! - `build_supersession_graph` — constructs the DAG and detects cycles
//! - `graph_penalty` — derives a topology-informed penalty multiplier per entry
//! - `find_terminal_active` — traverses directed edges to the terminal active node
//!
//! All functions are synchronous and pure (no I/O). Callers in `search.rs`
//! wrap in `spawn_blocking` (ADR-002).
//!
//! Edge direction: `pred_id → entry.id` when `entry.supersedes == Some(pred_id)`.
//! Outgoing edges point toward newer knowledge.

use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::Direction;
use petgraph::algo::is_cyclic_directed;
use petgraph::stable_graph::{NodeIndex, StableGraph};
use unimatrix_core::{EntryRecord, Status};

// -- Penalty constants (ADR-006: named, fixed for v1) --

/// Deprecated entry with no successors — softest penalty (orphan, not replaceable).
pub const ORPHAN_PENALTY: f64 = 0.75;

/// Superseded entry with exactly one active terminal at depth 1 — cleanly replaced.
pub const CLEAN_REPLACEMENT_PENALTY: f64 = 0.40;

/// Multiplier applied per additional hop beyond depth 1.
pub const HOP_DECAY_FACTOR: f64 = 0.60;

/// Superseded entry with more than one direct successor — ambiguous replacement.
pub const PARTIAL_SUPERSESSION_PENALTY: f64 = 0.60;

/// Entry with successors but no active terminal reachable — chain leads nowhere.
pub const DEAD_END_PENALTY: f64 = 0.65;

/// Flat fallback used by search.rs when CycleDetected prevents graph construction.
pub const FALLBACK_PENALTY: f64 = 0.70;

/// Maximum DFS depth for find_terminal_active. Chains beyond this return None.
pub const MAX_TRAVERSAL_DEPTH: usize = 10;

// -- Error type --

/// Error returned when the supersession graph contains a cycle.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("supersession cycle detected")]
    CycleDetected,
}

// -- Graph type --

/// Directed supersession DAG. Node weight = entry id (u64). Edge weight = () (unit).
///
/// Edge direction: `pred_id → entry.id` when `entry.supersedes == Some(pred_id)`.
/// Outgoing edges point toward more-recent knowledge.
///
/// `pub(crate)` fields allow unit tests to inspect graph structure directly (R-04).
pub struct SupersessionGraph {
    /// Directed petgraph StableGraph. StableGraph used for crt-017 forward
    /// compatibility — node indices remain stable when nodes are removed in
    /// future phases.
    pub(crate) inner: StableGraph<u64, ()>,
    /// Maps entry id → NodeIndex for O(1) lookup.
    pub(crate) node_index: HashMap<u64, NodeIndex>,
}

// -- Public API --

/// Build directed supersession DAG from a slice of all entries.
///
/// Pass 1: add one node per entry.
/// Pass 2: for each `entry.supersedes == Some(pred_id)`, add edge `pred_id → entry.id`.
///   Dangling references (pred_id not in entries) are skipped with `tracing::warn!`.
/// Pass 3: cycle detection via `petgraph::algo::is_cyclic_directed`.
///
/// Returns `Err(GraphError::CycleDetected)` if a cycle is found.
/// Returns `Ok` with zero nodes for an empty slice.
pub fn build_supersession_graph(entries: &[EntryRecord]) -> Result<SupersessionGraph, GraphError> {
    let mut graph = SupersessionGraph {
        inner: StableGraph::new(),
        node_index: HashMap::with_capacity(entries.len()),
    };

    // Pass 1: add one node per entry
    for entry in entries {
        let idx = graph.inner.add_node(entry.id);
        graph.node_index.insert(entry.id, idx);
    }

    // Pass 2: add directed edges for supersession relationships
    for entry in entries {
        if let Some(pred_id) = entry.supersedes {
            match graph.node_index.get(&pred_id) {
                None => {
                    tracing::warn!(
                        entry_id = entry.id,
                        missing_pred_id = pred_id,
                        "build_supersession_graph: dangling supersedes reference, skipping edge"
                    );
                }
                Some(&pred_idx) => {
                    let succ_idx = graph.node_index[&entry.id];
                    graph.inner.add_edge(pred_idx, succ_idx, ());
                }
            }
        }
    }

    // Pass 3: cycle detection
    if is_cyclic_directed(&graph.inner) {
        return Err(GraphError::CycleDetected);
    }

    Ok(graph)
}

/// Topology-derived penalty multiplier for a node.
///
/// Returns `1.0` (no penalty) for node IDs absent from the graph.
///
/// Priority order:
/// 1. `is_orphan` (Deprecated + zero outgoing edges) → `ORPHAN_PENALTY`
/// 2. `!active_reachable` → `DEAD_END_PENALTY`
/// 3. `successor_count > 1` → `PARTIAL_SUPERSESSION_PENALTY`
/// 4. `chain_depth == Some(1)` → `CLEAN_REPLACEMENT_PENALTY`
/// 5. `chain_depth == Some(d >= 2)` → `CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR^(d-1)`,
///    clamped to `[0.10, CLEAN_REPLACEMENT_PENALTY]`
/// 6. Defensive fallback → `DEAD_END_PENALTY`
///
/// Pure function: no I/O, deterministic, no side effects (NFR-02).
pub fn graph_penalty(node_id: u64, graph: &SupersessionGraph, entries: &[EntryRecord]) -> f64 {
    // Guard: node not in graph → no penalty
    let node_idx = match graph.node_index.get(&node_id) {
        Some(&idx) => idx,
        None => return 1.0,
    };

    // Lookup entry record
    let entry = match entry_by_id(node_id, entries) {
        Some(e) => e,
        None => return 1.0,
    };

    // Signal 1 & 2: outgoing edge count
    let outgoing_count = graph
        .inner
        .edges_directed(node_idx, Direction::Outgoing)
        .count();
    let successor_count = outgoing_count;

    // Signal: is_orphan — Deprecated with no outgoing edges
    let is_orphan = entry.status == Status::Deprecated && outgoing_count == 0;

    // Priority 1: orphan
    if is_orphan {
        return ORPHAN_PENALTY;
    }

    // Signal 3: active_reachable — DFS following outgoing edges
    let active_reachable = dfs_active_reachable(node_idx, graph, entries);

    // Priority 2: no active terminal reachable
    if !active_reachable {
        return DEAD_END_PENALTY;
    }

    // Priority 3: partial supersession — multiple direct successors
    if successor_count > 1 {
        return PARTIAL_SUPERSESSION_PENALTY;
    }

    // Signal 4: chain_depth — BFS distance to nearest active terminal
    let chain_depth = bfs_chain_depth(node_idx, graph, entries);

    // Priority 4: clean replacement at depth 1
    if chain_depth == Some(1) {
        return CLEAN_REPLACEMENT_PENALTY;
    }

    // Priority 5: clean replacement at depth >= 2 with hop decay
    if let Some(d) = chain_depth
        && d >= 2
    {
        let raw = CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR.powi((d - 1) as i32);
        return raw.clamp(0.10, CLEAN_REPLACEMENT_PENALTY);
    }

    // Priority 6: defensive fallback — should not be reached in valid data
    DEAD_END_PENALTY
}

/// DFS from `node_id`; returns the id of the first node where
/// `status == Active && superseded_by.is_none()`.
///
/// Depth-capped at `MAX_TRAVERSAL_DEPTH`. Returns `None` if no active terminal
/// is reachable or if `node_id` is not in the graph.
///
/// The starting node itself is checked (depth 0), allowing callers to pass an
/// already-terminal node.
pub fn find_terminal_active(
    node_id: u64,
    graph: &SupersessionGraph,
    entries: &[EntryRecord],
) -> Option<u64> {
    let start_idx = match graph.node_index.get(&node_id) {
        Some(&idx) => idx,
        None => return None,
    };

    // Iterative DFS — no recursion, no stack overflow risk on pathological chains (R-07).
    // Stack entries: (NodeIndex, depth_from_start)
    let mut stack: Vec<(NodeIndex, usize)> = vec![(start_idx, 0)];
    let mut visited: HashSet<NodeIndex> = HashSet::new();
    visited.insert(start_idx);

    while let Some((current_idx, depth)) = stack.pop() {
        let current_id = graph.inner[current_idx];
        if let Some(e) = entry_by_id(current_id, entries)
            && e.status == Status::Active
            && e.superseded_by.is_none()
        {
            return Some(current_id);
        }

        // Do not push neighbors if they would exceed MAX_TRAVERSAL_DEPTH.
        // Nodes at depth MAX_TRAVERSAL_DEPTH are visited+checked; their
        // neighbors are not pushed (AC-11: chain of 11 → None).
        if depth + 1 > MAX_TRAVERSAL_DEPTH {
            continue;
        }

        for neighbor_idx in graph
            .inner
            .neighbors_directed(current_idx, Direction::Outgoing)
        {
            if !visited.contains(&neighbor_idx) {
                visited.insert(neighbor_idx);
                stack.push((neighbor_idx, depth + 1));
            }
        }
    }

    None
}

// -- Private helpers --

/// DFS following outgoing edges from `start_idx`.
/// Returns `true` if any reachable successor is `Active && superseded_by.is_none()`.
/// Does NOT check `start_idx` itself — checks successors only.
fn dfs_active_reachable(
    start_idx: NodeIndex,
    graph: &SupersessionGraph,
    entries: &[EntryRecord],
) -> bool {
    let mut stack: Vec<NodeIndex> = vec![start_idx];
    let mut visited: HashSet<NodeIndex> = HashSet::new();

    while let Some(current_idx) = stack.pop() {
        if !visited.insert(current_idx) {
            continue;
        }

        for neighbor_idx in graph
            .inner
            .neighbors_directed(current_idx, Direction::Outgoing)
        {
            let neighbor_id = graph.inner[neighbor_idx];
            if let Some(e) = entry_by_id(neighbor_id, entries)
                && e.status == Status::Active
                && e.superseded_by.is_none()
            {
                return true;
            }
            stack.push(neighbor_idx);
        }
    }

    false
}

/// BFS from `start_idx` to find the shortest hop distance to the nearest
/// `Active && superseded_by.is_none()` node.
///
/// Returns `Some(depth)` where depth >= 1 (start node itself not counted as terminal
/// since `graph_penalty` is called on entries needing penalizing).
/// Returns `None` if no active terminal reachable or depth exceeds `MAX_TRAVERSAL_DEPTH`.
fn bfs_chain_depth(
    start_idx: NodeIndex,
    graph: &SupersessionGraph,
    entries: &[EntryRecord],
) -> Option<usize> {
    let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::new();
    let mut visited: HashSet<NodeIndex> = HashSet::new();

    queue.push_back((start_idx, 0));
    visited.insert(start_idx);

    while let Some((current_idx, depth)) = queue.pop_front() {
        if depth > MAX_TRAVERSAL_DEPTH {
            continue;
        }

        for neighbor_idx in graph
            .inner
            .neighbors_directed(current_idx, Direction::Outgoing)
        {
            if visited.contains(&neighbor_idx) {
                continue;
            }
            visited.insert(neighbor_idx);
            let next_depth = depth + 1;

            let neighbor_id = graph.inner[neighbor_idx];
            if let Some(e) = entry_by_id(neighbor_id, entries)
                && e.status == Status::Active
                && e.superseded_by.is_none()
            {
                return Some(next_depth);
            }
            queue.push_back((neighbor_idx, next_depth));
        }
    }

    None
}

/// Linear scan for an entry by id.
///
/// O(n) per call — acceptable for expected slice sizes (≤1,000 entries, NFR-01).
fn entry_by_id(id: u64, entries: &[EntryRecord]) -> Option<&EntryRecord> {
    entries.iter().find(|e| e.id == id)
}

// -- Tests --

#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::visit::EdgeRef;
    use unimatrix_core::Status;

    /// Build a minimal EntryRecord with specified topology fields.
    /// All other fields use sensible defaults.
    fn make_entry(
        id: u64,
        status: Status,
        supersedes: Option<u64>,
        superseded_by: Option<u64>,
    ) -> EntryRecord {
        EntryRecord {
            id,
            title: format!("Entry {id}"),
            content: String::new(),
            topic: String::new(),
            category: "decision".to_string(),
            tags: vec![],
            source: String::new(),
            status,
            confidence: 0.5,
            created_at: 0,
            updated_at: 0,
            last_accessed_at: 0,
            access_count: 0,
            supersedes,
            superseded_by,
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

    // -- AC-03: Cycle detection --

    #[test]
    fn cycle_two_node_detected() {
        let entries = vec![
            make_entry(1, Status::Active, Some(2), None),
            make_entry(2, Status::Active, Some(1), None),
        ];
        let result = build_supersession_graph(&entries);
        assert!(
            matches!(result, Err(GraphError::CycleDetected)),
            "two-node cycle must be detected"
        );
    }

    #[test]
    fn cycle_three_node_detected() {
        // A(id=1) supersedes C(id=3), B(id=2) supersedes A, C supersedes B → triangle
        let entries = vec![
            make_entry(1, Status::Active, Some(3), None),
            make_entry(2, Status::Active, Some(1), None),
            make_entry(3, Status::Active, Some(2), None),
        ];
        assert!(
            matches!(
                build_supersession_graph(&entries),
                Err(GraphError::CycleDetected)
            ),
            "three-node triangle cycle must be detected"
        );
    }

    #[test]
    fn cycle_self_referential_detected() {
        let entries = vec![make_entry(1, Status::Active, Some(1), None)];
        assert!(
            matches!(
                build_supersession_graph(&entries),
                Err(GraphError::CycleDetected)
            ),
            "self-loop must be detected as cycle"
        );
    }

    // -- AC-04: Valid DAGs --

    #[test]
    fn valid_dag_depth_1() {
        let entries = vec![
            make_entry(1, Status::Active, None, None),
            make_entry(2, Status::Active, Some(1), None),
        ];
        assert!(
            build_supersession_graph(&entries).is_ok(),
            "depth-1 chain must be a valid DAG"
        );
    }

    #[test]
    fn valid_dag_depth_2() {
        let entries = vec![
            make_entry(1, Status::Active, None, None),
            make_entry(2, Status::Active, Some(1), None),
            make_entry(3, Status::Active, Some(2), None),
        ];
        assert!(
            build_supersession_graph(&entries).is_ok(),
            "depth-2 chain must be a valid DAG"
        );
    }

    #[test]
    fn valid_dag_depth_3() {
        let entries = vec![
            make_entry(1, Status::Active, None, None),
            make_entry(2, Status::Active, Some(1), None),
            make_entry(3, Status::Active, Some(2), None),
            make_entry(4, Status::Active, Some(3), None),
        ];
        assert!(
            build_supersession_graph(&entries).is_ok(),
            "depth-3 chain must be a valid DAG"
        );
    }

    #[test]
    fn empty_entry_slice_is_valid_dag() {
        let result = build_supersession_graph(&[]);
        assert!(result.is_ok());
        let graph = result.unwrap();
        assert_eq!(graph.node_index.len(), 0);
    }

    #[test]
    fn single_entry_no_supersedes() {
        let entries = vec![make_entry(1, Status::Active, None, None)];
        assert!(build_supersession_graph(&entries).is_ok());
    }

    // -- AC-04: Edge direction verification (R-04) --

    #[test]
    fn edge_direction_pred_to_successor() {
        // B.supersedes = Some(A.id) → edge must be A → B
        let entries = vec![
            make_entry(1, Status::Active, None, None),    // A
            make_entry(2, Status::Active, Some(1), None), // B supersedes A
        ];
        let graph = build_supersession_graph(&entries).unwrap();
        let a_index = graph.node_index[&1];
        let b_index = graph.node_index[&2];
        let outgoing: Vec<_> = graph
            .inner
            .edges_directed(a_index, Direction::Outgoing)
            .collect();
        assert_eq!(outgoing.len(), 1, "A must have exactly one outgoing edge");
        assert!(
            outgoing.iter().any(|e| e.target() == b_index),
            "outgoing edge from A must point to B"
        );
    }

    // -- AC-05: graph_penalty range --

    #[test]
    fn penalty_range_all_scenarios() {
        // Orphan: Deprecated, no outgoing
        {
            let entries = vec![make_entry(1, Status::Deprecated, None, None)];
            let g = build_supersession_graph(&entries).unwrap();
            let p = graph_penalty(1, &g, &entries);
            assert_eq!(p, ORPHAN_PENALTY, "orphan must return ORPHAN_PENALTY");
            assert!(p > 0.0 && p < 1.0);
        }

        // Dead-end: outgoing edge but successor is Deprecated (no active reachable)
        {
            let entries = vec![
                make_entry(1, Status::Active, None, Some(2)),
                make_entry(2, Status::Deprecated, Some(1), None),
            ];
            let g = build_supersession_graph(&entries).unwrap();
            let p = graph_penalty(1, &g, &entries);
            assert_eq!(p, DEAD_END_PENALTY, "dead-end must return DEAD_END_PENALTY");
            assert!(p > 0.0 && p < 1.0);
        }

        // Partial supersession: two active successors
        {
            let entries = vec![
                make_entry(1, Status::Active, None, None),
                make_entry(2, Status::Active, Some(1), None),
                make_entry(3, Status::Active, Some(1), None),
            ];
            let g = build_supersession_graph(&entries).unwrap();
            let p = graph_penalty(1, &g, &entries);
            assert_eq!(
                p, PARTIAL_SUPERSESSION_PENALTY,
                "partial supersession must return PARTIAL_SUPERSESSION_PENALTY"
            );
            assert!(p > 0.0 && p < 1.0);
        }

        // Depth-1 clean replacement
        {
            let entries = vec![
                make_entry(1, Status::Active, None, Some(2)),
                make_entry(2, Status::Active, Some(1), None),
            ];
            let g = build_supersession_graph(&entries).unwrap();
            let p = graph_penalty(1, &g, &entries);
            assert_eq!(
                p, CLEAN_REPLACEMENT_PENALTY,
                "depth-1 must return CLEAN_REPLACEMENT_PENALTY"
            );
            assert!(p > 0.0 && p < 1.0);
        }

        // Depth-2 decay: ~0.24
        {
            let entries = vec![
                make_entry(1, Status::Active, None, Some(2)),
                make_entry(2, Status::Active, Some(1), Some(3)),
                make_entry(3, Status::Active, Some(2), None),
            ];
            let g = build_supersession_graph(&entries).unwrap();
            let p = graph_penalty(1, &g, &entries);
            let expected = CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR;
            assert!(
                (p - expected).abs() < 1e-10,
                "depth-2 must be ~0.24, got {p}"
            );
            assert!(p >= 0.10 && p <= CLEAN_REPLACEMENT_PENALTY);
        }

        // Depth-5 decay: clamped to 0.10
        {
            let entries = vec![
                make_entry(1, Status::Active, None, Some(2)),
                make_entry(2, Status::Active, Some(1), Some(3)),
                make_entry(3, Status::Active, Some(2), Some(4)),
                make_entry(4, Status::Active, Some(3), Some(5)),
                make_entry(5, Status::Active, Some(4), Some(6)),
                make_entry(6, Status::Active, Some(5), None),
            ];
            let g = build_supersession_graph(&entries).unwrap();
            let p = graph_penalty(1, &g, &entries);
            assert!(
                (p - 0.10).abs() < 1e-10,
                "depth-5 must clamp to 0.10, got {p}"
            );
        }
    }

    #[test]
    fn penalty_absent_node_returns_one() {
        let graph = build_supersession_graph(&[]).unwrap();
        let result = graph_penalty(9999, &graph, &[]);
        assert_eq!(result, 1.0);
    }

    // -- AC-06: Orphan softer than clean replacement (R-05) --

    #[test]
    fn orphan_softer_than_clean_replacement() {
        assert!(
            ORPHAN_PENALTY > CLEAN_REPLACEMENT_PENALTY,
            "orphan ({ORPHAN_PENALTY}) must be softer (higher multiplier) than clean replacement ({CLEAN_REPLACEMENT_PENALTY})"
        );

        // Also verify via graph_penalty on constructed entries
        let orphan_entries = vec![make_entry(1, Status::Deprecated, None, None)];
        let orphan_graph = build_supersession_graph(&orphan_entries).unwrap();
        let orphan_p = graph_penalty(1, &orphan_graph, &orphan_entries);

        let chain_entries = vec![
            make_entry(2, Status::Active, None, Some(3)),
            make_entry(3, Status::Active, Some(2), None),
        ];
        let chain_graph = build_supersession_graph(&chain_entries).unwrap();
        let clean_p = graph_penalty(2, &chain_graph, &chain_entries);

        assert!(
            orphan_p > clean_p,
            "orphan ({orphan_p}) must be softer than clean replacement ({clean_p})"
        );
    }

    // -- AC-07: 2-hop harsher than 1-hop (R-05) --

    #[test]
    fn two_hop_harsher_than_one_hop() {
        // Chain: A(1) → B(2) → C(3 Active, terminal)
        let entries = vec![
            make_entry(1, Status::Active, None, Some(2)),
            make_entry(2, Status::Active, Some(1), Some(3)),
            make_entry(3, Status::Active, Some(2), None),
        ];
        let graph = build_supersession_graph(&entries).unwrap();
        let penalty_a = graph_penalty(1, &graph, &entries); // depth-2 → harsher
        let penalty_b = graph_penalty(2, &graph, &entries); // depth-1 → softer

        assert!(
            penalty_a < penalty_b,
            "2-hop entry ({penalty_a}) must receive harsher (lower) penalty than 1-hop entry ({penalty_b})"
        );

        assert!(
            (penalty_b - CLEAN_REPLACEMENT_PENALTY).abs() < 1e-10,
            "depth-1 must equal CLEAN_REPLACEMENT_PENALTY (0.40), got {penalty_b}"
        );
        assert!(
            (penalty_a - CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR).abs() < 1e-10,
            "depth-2 must equal 0.40 * 0.60 = 0.24, got {penalty_a}"
        );
    }

    // -- AC-08: Partial supersession softer than clean (R-05) --

    #[test]
    fn partial_supersession_softer_than_clean() {
        assert!(
            PARTIAL_SUPERSESSION_PENALTY > CLEAN_REPLACEMENT_PENALTY,
            "partial ({PARTIAL_SUPERSESSION_PENALTY}) must be softer than clean replacement ({CLEAN_REPLACEMENT_PENALTY})"
        );

        // Partial: A(1) has two active successors B(2) and C(3)
        let partial_entries = vec![
            make_entry(1, Status::Active, None, None),
            make_entry(2, Status::Active, Some(1), None),
            make_entry(3, Status::Active, Some(1), None),
        ];
        let partial_graph = build_supersession_graph(&partial_entries).unwrap();
        let partial_p = graph_penalty(1, &partial_graph, &partial_entries);

        // Clean: X(10) has one active successor Y(11)
        let clean_entries = vec![
            make_entry(10, Status::Active, None, Some(11)),
            make_entry(11, Status::Active, Some(10), None),
        ];
        let clean_graph = build_supersession_graph(&clean_entries).unwrap();
        let clean_p = graph_penalty(10, &clean_graph, &clean_entries);

        assert!(
            partial_p > clean_p,
            "partial ({partial_p}) must be softer than clean replacement ({clean_p})"
        );
    }

    // -- AC-09: find_terminal_active three-hop chain --

    #[test]
    fn terminal_active_three_hop_chain() {
        // A(1, superseded by B) → B(2, superseded by C) → C(3, Active, terminal)
        let entries = vec![
            make_entry(1, Status::Active, None, Some(2)),
            make_entry(2, Status::Active, Some(1), Some(3)),
            make_entry(3, Status::Active, Some(2), None),
        ];
        let graph = build_supersession_graph(&entries).unwrap();
        let result = find_terminal_active(1, &graph, &entries);
        assert_eq!(result, Some(3), "terminal must be C (id=3)");
    }

    #[test]
    fn terminal_active_depth_one_chain() {
        let entries = vec![
            make_entry(1, Status::Active, None, Some(2)),
            make_entry(2, Status::Active, Some(1), None),
        ];
        let graph = build_supersession_graph(&entries).unwrap();
        let result = find_terminal_active(1, &graph, &entries);
        assert_eq!(result, Some(2));
    }

    #[test]
    fn terminal_active_superseded_intermediate_skipped() {
        // A(1) → B(2) → C(3, Active but superseded_by=Some(4)) → D(4, Active, terminal)
        let entries = vec![
            make_entry(1, Status::Active, None, Some(2)),
            make_entry(2, Status::Active, Some(1), Some(3)),
            make_entry(3, Status::Active, Some(2), Some(4)),
            make_entry(4, Status::Active, Some(3), None),
        ];
        let graph = build_supersession_graph(&entries).unwrap();
        let result = find_terminal_active(1, &graph, &entries);
        assert_eq!(result, Some(4), "must skip C (superseded) and reach D");
    }

    // -- AC-10: find_terminal_active returns None --

    #[test]
    fn terminal_active_no_reachable() {
        // A(1) → B(2, Deprecated)
        let entries = vec![
            make_entry(1, Status::Active, None, Some(2)),
            make_entry(2, Status::Deprecated, Some(1), None),
        ];
        let graph = build_supersession_graph(&entries).unwrap();
        let result = find_terminal_active(1, &graph, &entries);
        assert_eq!(result, None);
    }

    #[test]
    fn terminal_active_absent_node() {
        let graph = build_supersession_graph(&[]).unwrap();
        let result = find_terminal_active(9999, &graph, &[]);
        assert_eq!(result, None);
    }

    // -- AC-11: find_terminal_active depth cap --

    #[test]
    fn terminal_active_depth_cap() {
        // Chain of 11 entries: 0→1→2→...→10 (Active, terminal)
        // From node 0, reaching node 10 requires 10 hops = MAX_TRAVERSAL_DEPTH.
        // But the pseudocode correction: chain of 11 → None.
        // Nodes 0..=9 are the first 10; node 10 (11th entry) is the terminal.
        // depth of node 10 from start = 10 = MAX_TRAVERSAL_DEPTH.
        // push check: depth + 1 > MAX_TRAVERSAL_DEPTH → 10 + 1 = 11 > 10 → don't push.
        // So node 10 itself gets pushed at depth 10 from node 9 (depth 9).
        // Wait: node 9 is at depth 9; 9 + 1 = 10, 10 > 10 is false → we DO push node 10.
        // Node 10 is visited at depth 10, checked → returns Some(10).
        // For a chain of 11 to return None, we need 11 entries where the terminal is at depth 11.
        // Build: 0→1→...→10→11(terminal Active). From 0, depth to 11 = 11. Don't push 11.
        let mut entries: Vec<EntryRecord> = (0u64..=10)
            .map(|i| {
                make_entry(
                    i,
                    Status::Active,
                    if i > 0 { Some(i - 1) } else { None },
                    Some(i + 1),
                )
            })
            .collect();
        // node 11: Active terminal
        entries.push(make_entry(11, Status::Active, Some(10), None));

        let graph = build_supersession_graph(&entries).unwrap();
        let result = find_terminal_active(0, &graph, &entries);
        assert_eq!(
            result, None,
            "chain of 12 entries (terminal at depth 11) must return None"
        );
    }

    #[test]
    fn terminal_active_depth_boundary() {
        // Chain of 10 entries: 0→1→...→9 (Active terminal at depth 9 from start 0).
        // depth 9 + 1 = 10, 10 > 10 is false → neighbors of 9 can be pushed.
        // But node 9 itself is the terminal — found at depth 9, returned immediately.
        // Actually need the terminal at depth 10. Build chain 0→...→10 where
        // nodes 0..9 have superseded_by set and node 10 is the Active terminal.
        // From 0: node 10 is at depth 10.
        let mut entries: Vec<EntryRecord> = (0u64..=9)
            .map(|i| {
                make_entry(
                    i,
                    Status::Active,
                    if i > 0 { Some(i - 1) } else { None },
                    Some(i + 1),
                )
            })
            .collect();
        entries.push(make_entry(10, Status::Active, Some(9), None));

        let graph = build_supersession_graph(&entries).unwrap();
        let result = find_terminal_active(0, &graph, &entries);
        // Node 0 itself is Active with superseded_by=Some(1), so not terminal.
        // The DFS will first check node 0: superseded_by.is_some() → not terminal.
        // Then explore neighbors... eventually reaching node 10 at depth 10.
        // At node 9 (depth 9): depth + 1 = 10, 10 > 10 is false → push node 10.
        // Node 10: Active && superseded_by.is_none() → Some(10).
        assert_eq!(
            result,
            Some(10),
            "chain of 11 entries (terminal at depth 10) must return Some"
        );
    }

    // -- AC-17: Dangling supersedes reference --

    #[test]
    fn dangling_supersedes_ref_is_skipped() {
        // Entry 1 with supersedes=Some(9999) where 9999 is not in the slice
        let entries = vec![make_entry(1, Status::Active, Some(9999), None)];
        let result = build_supersession_graph(&entries);
        assert!(result.is_ok(), "dangling ref must not cause Err or panic");
        let graph = result.unwrap();
        assert_eq!(
            graph.node_index.len(),
            1,
            "graph must have only entry 1, no dangling node"
        );
    }

    // -- Behavioral ordering (AC-15 migration coverage) --

    #[test]
    fn dead_end_softer_than_orphan() {
        assert!(
            DEAD_END_PENALTY < ORPHAN_PENALTY,
            "dead-end ({DEAD_END_PENALTY}) must be softer than orphan ({ORPHAN_PENALTY})"
        );
    }

    #[test]
    fn fallback_softer_than_clean() {
        assert!(
            FALLBACK_PENALTY > CLEAN_REPLACEMENT_PENALTY,
            "fallback ({FALLBACK_PENALTY}) must be softer than clean replacement ({CLEAN_REPLACEMENT_PENALTY})"
        );
    }

    // -- R-12: Decay formula bounds --

    #[test]
    fn decay_formula_depth_1() {
        let entries = vec![
            make_entry(1, Status::Active, None, Some(2)),
            make_entry(2, Status::Active, Some(1), None),
        ];
        let g = build_supersession_graph(&entries).unwrap();
        let p = graph_penalty(1, &g, &entries);
        assert!(
            (p - CLEAN_REPLACEMENT_PENALTY).abs() < 1e-10,
            "depth-1 must equal CLEAN_REPLACEMENT_PENALTY, got {p}"
        );
    }

    #[test]
    fn decay_formula_depth_2() {
        let expected = CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR; // 0.24
        let entries = vec![
            make_entry(1, Status::Active, None, Some(2)),
            make_entry(2, Status::Active, Some(1), Some(3)),
            make_entry(3, Status::Active, Some(2), None),
        ];
        let g = build_supersession_graph(&entries).unwrap();
        let p = graph_penalty(1, &g, &entries);
        assert!(
            (p - expected).abs() < 1e-10,
            "depth-2 must be {expected}, got {p}"
        );
        assert!(p >= 0.10);
    }

    #[test]
    fn decay_formula_depth_5_clamped() {
        // 0.40 * 0.60^4 ≈ 0.0518 → clamped to 0.10
        let mut entries: Vec<EntryRecord> = (1u64..=5)
            .map(|i| {
                make_entry(
                    i,
                    Status::Active,
                    if i > 1 { Some(i - 1) } else { None },
                    Some(i + 1),
                )
            })
            .collect();
        entries.push(make_entry(6, Status::Active, Some(5), None));
        let g = build_supersession_graph(&entries).unwrap();
        let p = graph_penalty(1, &g, &entries);
        assert!(
            (p - 0.10).abs() < 1e-10,
            "depth-5 must clamp to 0.10, got {p}"
        );
    }

    #[test]
    fn decay_formula_depth_10_clamped() {
        let mut entries: Vec<EntryRecord> = (1u64..=10)
            .map(|i| {
                make_entry(
                    i,
                    Status::Active,
                    if i > 1 { Some(i - 1) } else { None },
                    Some(i + 1),
                )
            })
            .collect();
        entries.push(make_entry(11, Status::Active, Some(10), None));
        let g = build_supersession_graph(&entries).unwrap();
        let p = graph_penalty(1, &g, &entries);
        assert!(
            (p - 0.10).abs() < 1e-10,
            "depth-10 must clamp to 0.10, got {p}"
        );
    }

    #[test]
    fn decay_never_exceeds_clean_replacement() {
        // For all depths 1..=10, penalty must be <= CLEAN_REPLACEMENT_PENALTY
        for depth in 1usize..=10 {
            let mut entries: Vec<EntryRecord> = (1u64..=(depth as u64))
                .map(|i| {
                    make_entry(
                        i,
                        Status::Active,
                        if i > 1 { Some(i - 1) } else { None },
                        Some(i + 1),
                    )
                })
                .collect();
            entries.push(make_entry(
                depth as u64 + 1,
                Status::Active,
                Some(depth as u64),
                None,
            ));
            let g = build_supersession_graph(&entries).unwrap();
            let p = graph_penalty(1, &g, &entries);
            assert!(
                p <= CLEAN_REPLACEMENT_PENALTY,
                "depth {depth}: penalty {p} must not exceed CLEAN_REPLACEMENT_PENALTY"
            );
        }
    }

    // -- Edge cases --

    #[test]
    fn all_active_no_penalty() {
        // All Active entries with no supersession — graph has nodes but no edges.
        // The search pipeline guards graph_penalty calls with
        // `superseded_by.is_some() || status == Deprecated`, so graph_penalty
        // is never called on standalone Active entries (tested at integration level).
        // Here we verify graph structure: N nodes, 0 edges.
        let entries: Vec<EntryRecord> = (1u64..=5)
            .map(|i| make_entry(i, Status::Active, None, None))
            .collect();
        let g = build_supersession_graph(&entries).unwrap();
        assert_eq!(g.node_index.len(), 5, "graph must have 5 nodes");
        assert_eq!(
            g.inner.edge_count(),
            0,
            "graph with no supersession links must have 0 edges"
        );
        // graph_penalty returns 1.0 for nodes not in graph (absent node guard)
        assert_eq!(graph_penalty(9999, &g, &entries), 1.0);
    }

    #[test]
    fn terminal_active_starting_node_is_active() {
        // find_terminal_active returns Some(node_id) when the starting node is already terminal
        let entries = vec![make_entry(1, Status::Active, None, None)];
        let g = build_supersession_graph(&entries).unwrap();
        let result = find_terminal_active(1, &g, &entries);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn two_successors_one_active_one_deprecated() {
        // A(1) → B(2, Active) and A(1) → C(3, Deprecated)
        // successor_count > 1 → PARTIAL_SUPERSESSION_PENALTY
        let entries = vec![
            make_entry(1, Status::Active, None, None),
            make_entry(2, Status::Active, Some(1), None),
            make_entry(3, Status::Deprecated, Some(1), None),
        ];
        let g = build_supersession_graph(&entries).unwrap();
        let p = graph_penalty(1, &g, &entries);
        assert_eq!(
            p, PARTIAL_SUPERSESSION_PENALTY,
            "two successors (one active, one deprecated) → PARTIAL_SUPERSESSION_PENALTY"
        );
    }

    #[test]
    fn node_id_zero_not_in_graph() {
        let graph = build_supersession_graph(&[]).unwrap();
        let result = graph_penalty(0, &graph, &[]);
        assert_eq!(
            result, 1.0,
            "node_id=0 not in graph must return 1.0 without panic"
        );
    }

    #[test]
    fn graph_penalty_entry_not_in_slice() {
        // Manually insert a node id into graph that has no corresponding entry in slice
        // Simulate via: entry in graph but look up against empty entries slice
        let entries = vec![make_entry(1, Status::Active, None, None)];
        let g = build_supersession_graph(&entries).unwrap();
        // Pass empty entries slice — entry_by_id returns None → falls through to 1.0
        let result = graph_penalty(1, &g, &[]);
        assert_eq!(
            result, 1.0,
            "entry in graph but not in slice must return 1.0"
        );
    }
}
