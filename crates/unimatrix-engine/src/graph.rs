//! Typed Relationship Graph — topology-aware penalty computation and multi-hop traversal.
//!
//! Builds a directed typed graph from entry relationships, backed by `GRAPH_EDGES`.
//! Provides:
//! - `build_typed_relation_graph` — constructs the typed graph and detects Supersedes cycles
//! - `graph_penalty` — derives a topology-informed penalty multiplier per entry
//! - `find_terminal_active` — traverses Supersedes edges to the terminal active node
//!
//! All functions are synchronous and pure (no I/O).
//!
//! Edge direction: `pred_id → entry.id` when `entry.supersedes == Some(pred_id)`.
//! Outgoing Supersedes edges point toward newer knowledge.
//!
//! `graph_penalty` and `find_terminal_active` filter exclusively to Supersedes edges
//! via `edges_of_type`. Non-Supersedes edges (CoAccess, Contradicts, Supports, Prerequisite)
//! are present in the graph but invisible to all penalty logic (SR-01 mitigation).

use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::Direction;
use petgraph::algo::is_cyclic_directed;
use petgraph::stable_graph::{EdgeReference, NodeIndex, StableGraph};
use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use unimatrix_core::{EntryRecord, Status};

#[path = "graph_suppression.rs"]
mod graph_suppression;
pub use graph_suppression::suppress_contradicts;

#[path = "graph_ppr.rs"]
mod graph_ppr;
pub use graph_ppr::personalized_pagerank;

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

// -- Typed edge classification --

/// Five edge types covering the full relationship taxonomy.
///
/// Stored as strings in GRAPH_EDGES — NOT integer discriminants.
/// String encoding allows extension without schema migration or GNN retraining.
///
/// `Prerequisite` is reserved for W3-1; no write path exists in crt-021.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationType {
    Supersedes,
    Contradicts,
    Supports,
    CoAccess,
    Prerequisite,
}

impl RelationType {
    /// Returns the canonical string representation stored in GRAPH_EDGES.
    pub fn as_str(&self) -> &'static str {
        match self {
            RelationType::Supersedes => "Supersedes",
            RelationType::Contradicts => "Contradicts",
            RelationType::Supports => "Supports",
            RelationType::CoAccess => "CoAccess",
            RelationType::Prerequisite => "Prerequisite",
        }
    }

    /// Parses a string into a `RelationType`. Case-sensitive. Returns `None` for unknown strings.
    ///
    /// Note: This method intentionally has the same name as `std::str::FromStr::from_str` per
    /// the architecture integration surface (ARCHITECTURE.md §Integration Surface).
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Supersedes" => Some(RelationType::Supersedes),
            "Contradicts" => Some(RelationType::Contradicts),
            "Supports" => Some(RelationType::Supports),
            "CoAccess" => Some(RelationType::CoAccess),
            "Prerequisite" => Some(RelationType::Prerequisite),
            _ => None,
        }
    }
}

// -- Typed edge weight --

/// Typed edge weight carried by `StableGraph<u64, RelationEdge>`.
///
/// `relation_type` stores `RelationType::as_str()` — never an integer discriminant.
/// `bootstrap_only = true` means the edge was created from heuristic bootstrap data
/// and is excluded structurally from `TypedRelationGraph.inner` during rebuild.
#[derive(Debug, Clone)]
pub struct RelationEdge {
    /// `RelationType::as_str()` value — string, never integer.
    pub relation_type: String,
    /// Validated finite weight. Supersedes=1.0, CoAccess=count/MAX(count).
    pub weight: f32,
    /// Unix epoch seconds at creation.
    pub created_at: i64,
    /// Agent id or `"bootstrap"`.
    pub created_by: String,
    /// `"entries.supersedes"` | `"co_access"` | `"nli"` | `"bootstrap"`.
    pub source: String,
    /// When `true`, excluded structurally in `build_typed_relation_graph` (never added to inner).
    pub bootstrap_only: bool,
}

// -- Row type for GRAPH_EDGES query results --

/// A row loaded from the `GRAPH_EDGES` table by `Store::query_graph_edges`.
///
/// Passed to `build_typed_relation_graph` as the `edges` slice.
/// Defined here so `unimatrix-engine` can compile independently of the store-analytics
/// crt-021 component. `unimatrix-store` will re-export this type once `store-analytics`
/// is implemented (build sequencing: engine-types first per OVERVIEW.md §Build Sequencing).
#[derive(Debug, Clone)]
pub struct GraphEdgeRow {
    pub source_id: u64,
    pub target_id: u64,
    pub relation_type: String,
    pub weight: f32,
    pub created_at: i64,
    pub created_by: String,
    pub source: String,
    pub bootstrap_only: bool,
}

// -- Graph type --

/// Typed relationship graph. Replaces `SupersessionGraph`.
///
/// `StableGraph` chosen for crt-017 forward compatibility — node indices remain
/// stable when nodes are removed in future phases (ADR-001, entry #1601).
///
/// `graph_penalty`, `find_terminal_active`, and all private helpers filter exclusively
/// to Supersedes edges via `edges_of_type`. Non-Supersedes edges are present but
/// invisible to all penalty logic (SR-01 mitigation: single filter-boundary method).
///
/// `pub(crate)` fields allow unit tests to inspect graph structure directly.
///
/// `Clone` is derived to allow the search hot path to clone the pre-built graph out from
/// under a short read lock, releasing the lock before any graph traversal (FR-22).
#[derive(Debug, Clone)]
pub struct TypedRelationGraph {
    /// Directed petgraph StableGraph with typed edge weights.
    pub(crate) inner: StableGraph<u64, RelationEdge>,
    /// Maps entry id → NodeIndex for O(1) lookup.
    pub(crate) node_index: HashMap<u64, NodeIndex>,
}

impl TypedRelationGraph {
    /// Create an empty `TypedRelationGraph` for cold-start state.
    ///
    /// Used by `TypedGraphState::new()` to create a valid zero-node, zero-edge
    /// graph without any I/O.
    pub fn empty() -> Self {
        TypedRelationGraph {
            inner: StableGraph::new(),
            node_index: HashMap::new(),
        }
    }

    /// Iterator over edges of the specified type from a given node in a given direction.
    ///
    /// This is the SOLE filter boundary (SR-01 mitigation). All traversal in
    /// `graph_penalty`, `find_terminal_active`, `dfs_active_reachable`, and `bfs_chain_depth`
    /// MUST call this method. Direct calls to `.edges_directed()` or `.neighbors_directed()`
    /// are prohibited at those sites.
    pub fn edges_of_type(
        &self,
        node_idx: NodeIndex,
        relation_type: RelationType,
        direction: Direction,
    ) -> impl Iterator<Item = EdgeReference<'_, RelationEdge>> {
        let type_str = relation_type.as_str();
        self.inner
            .edges_directed(node_idx, direction)
            .filter(move |e| e.weight().relation_type == type_str)
    }
}

// -- Public API --

/// Build directed typed relationship graph from a slice of entries and persisted edge rows.
///
/// **Pass 1**: Add one node per unique entry id (from entries + edge endpoints).
///
/// **Pass 2a**: Add Supersedes edges from `entries.supersedes` (authoritative source).
/// Supersedes topology is derived from the canonical `entries` field, not from
/// `GRAPH_EDGES` rows, to preserve correct cycle-detection semantics.
/// Dangling references (pred_id not in entries) are skipped with `tracing::warn!`.
///
/// **Pass 2b**: Add non-Supersedes edges from `edges` (GRAPH_EDGES rows).
/// `bootstrap_only=true` rows are excluded structurally — never added to inner.
/// Supersedes rows from GRAPH_EDGES are skipped (already derived in Pass 2a).
/// Unrecognized `relation_type` strings are skipped with `tracing::warn!`.
/// Endpoints absent from `node_index` are skipped with `tracing::warn!`.
///
/// **Pass 3**: Cycle detection on a temporary Supersedes-only sub-graph.
/// CoAccess bidirectional pairs (A↔B) would false-positive with `is_cyclic_directed`
/// on the full graph; the temp graph isolates only Supersedes edges.
///
/// Returns `Err(GraphError::CycleDetected)` if a Supersedes cycle is found.
/// Returns `Ok` with zero nodes for an empty entries slice.
pub fn build_typed_relation_graph(
    entries: &[EntryRecord],
    edges: &[GraphEdgeRow],
) -> Result<TypedRelationGraph, GraphError> {
    let mut graph = TypedRelationGraph {
        inner: StableGraph::new(),
        node_index: HashMap::with_capacity(entries.len()),
    };

    // Pass 1: add one node per entry
    for entry in entries {
        let idx = graph.inner.add_node(entry.id);
        graph.node_index.insert(entry.id, idx);
    }

    // Pass 2a: add Supersedes edges from entries.supersedes (authoritative source).
    // These are NOT derived from GRAPH_EDGES Supersedes rows — entries.supersedes is canonical.
    for entry in entries {
        if let Some(pred_id) = entry.supersedes {
            match graph.node_index.get(&pred_id) {
                None => {
                    tracing::warn!(
                        entry_id = entry.id,
                        missing_pred_id = pred_id,
                        "build_typed_relation_graph: dangling supersedes reference, skipping edge"
                    );
                }
                Some(&pred_idx) => {
                    let succ_idx = graph.node_index[&entry.id];
                    let edge = RelationEdge {
                        relation_type: "Supersedes".to_string(),
                        weight: 1.0,
                        created_at: 0,
                        created_by: "bootstrap".to_string(),
                        source: "entries.supersedes".to_string(),
                        bootstrap_only: false,
                    };
                    graph.inner.add_edge(pred_idx, succ_idx, edge);
                }
            }
        }
    }

    // Pass 2b: add non-Supersedes edges from GRAPH_EDGES rows.
    // bootstrap_only=true → structural exclusion, never added to inner (C-13, ADR-001 §3).
    // Supersedes rows skipped — authoritative Supersedes already handled in Pass 2a.
    for row in edges {
        if row.bootstrap_only {
            continue;
        }

        // Skip Supersedes rows from GRAPH_EDGES: already derived from entries.supersedes above.
        if row.relation_type == "Supersedes" {
            continue;
        }

        // Validate relation_type string; skip unrecognized types (R-10).
        if RelationType::from_str(&row.relation_type).is_none() {
            tracing::warn!(
                relation_type = %row.relation_type,
                source_id = row.source_id,
                target_id = row.target_id,
                "build_typed_relation_graph: unrecognized relation_type, skipping edge"
            );
            continue;
        }

        // Resolve source node index; skip if missing from snapshot.
        let source_idx = match graph.node_index.get(&row.source_id) {
            None => {
                tracing::warn!(
                    source_id = row.source_id,
                    target_id = row.target_id,
                    relation_type = %row.relation_type,
                    "build_typed_relation_graph: source_id not in entries snapshot, skipping edge"
                );
                continue;
            }
            Some(&idx) => idx,
        };

        // Resolve target node index; skip if missing from snapshot.
        let target_idx = match graph.node_index.get(&row.target_id) {
            None => {
                tracing::warn!(
                    source_id = row.source_id,
                    target_id = row.target_id,
                    relation_type = %row.relation_type,
                    "build_typed_relation_graph: target_id not in entries snapshot, skipping edge"
                );
                continue;
            }
            Some(&idx) => idx,
        };

        let edge = RelationEdge {
            relation_type: row.relation_type.clone(),
            weight: row.weight,
            created_at: row.created_at,
            created_by: row.created_by.clone(),
            source: row.source.clone(),
            bootstrap_only: false, // already filtered above
        };
        graph.inner.add_edge(source_idx, target_idx, edge);
    }

    // Pass 3: cycle detection on a temporary Supersedes-only sub-graph.
    // The full inner graph may contain CoAccess bidirectional pairs (A↔B) which would
    // cause is_cyclic_directed to false-positive. Build a temp graph with Supersedes edges
    // only and run cycle detection on it.
    let mut temp_graph: StableGraph<u64, ()> = StableGraph::new();
    let mut temp_nodes: HashMap<u64, NodeIndex> = HashMap::new();

    for &entry_id in graph.node_index.keys() {
        let tidx = temp_graph.add_node(entry_id);
        temp_nodes.insert(entry_id, tidx);
    }

    for edge_ref in graph.inner.edge_references() {
        if edge_ref.weight().relation_type == "Supersedes" {
            let src_id = graph.inner[edge_ref.source()];
            let tgt_id = graph.inner[edge_ref.target()];
            let tsrc = temp_nodes[&src_id];
            let ttgt = temp_nodes[&tgt_id];
            temp_graph.add_edge(tsrc, ttgt, ());
        }
    }

    if is_cyclic_directed(&temp_graph) {
        return Err(GraphError::CycleDetected);
    }

    Ok(graph)
}

/// Topology-derived penalty multiplier for a node.
///
/// Filters exclusively to Supersedes edges via `edges_of_type` (SR-01).
/// Returns `1.0` (no penalty) for node IDs absent from the graph.
///
/// Priority order:
/// 1. `is_orphan` (Deprecated + zero outgoing Supersedes edges) → `ORPHAN_PENALTY`
/// 2. `!active_reachable` → `DEAD_END_PENALTY`
/// 3. `successor_count > 1` → `PARTIAL_SUPERSESSION_PENALTY`
/// 4. `chain_depth == Some(1)` → `CLEAN_REPLACEMENT_PENALTY`
/// 5. `chain_depth == Some(d >= 2)` → `CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR^(d-1)`,
///    clamped to `[0.10, CLEAN_REPLACEMENT_PENALTY]`
/// 6. Defensive fallback → `DEAD_END_PENALTY`
///
/// Pure function: no I/O, deterministic, no side effects.
pub fn graph_penalty(node_id: u64, graph: &TypedRelationGraph, entries: &[EntryRecord]) -> f64 {
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

    // Signal 1: outgoing Supersedes edge count (uses edges_of_type boundary — SR-01)
    let outgoing_count = graph
        .edges_of_type(node_idx, RelationType::Supersedes, Direction::Outgoing)
        .count();
    let successor_count = outgoing_count;

    // Signal: is_orphan — Deprecated with no outgoing Supersedes edges
    let is_orphan = entry.status == Status::Deprecated && outgoing_count == 0;

    // Priority 1: orphan
    if is_orphan {
        return ORPHAN_PENALTY;
    }

    // Signal 2: active_reachable via Supersedes edges
    let active_reachable = dfs_active_reachable(node_idx, graph, entries);

    // Priority 2: no active terminal reachable
    if !active_reachable {
        return DEAD_END_PENALTY;
    }

    // Priority 3: partial supersession — multiple direct Supersedes successors
    if successor_count > 1 {
        return PARTIAL_SUPERSESSION_PENALTY;
    }

    // Signal 3: chain_depth via Supersedes edges
    let chain_depth = bfs_chain_depth(node_idx, graph, entries);

    // Priority 4: clean replacement at depth 1
    if chain_depth == Some(1) {
        return CLEAN_REPLACEMENT_PENALTY;
    }

    // Priority 5: hop decay at depth >= 2
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
/// Filters exclusively to Supersedes edges via `edges_of_type` (SR-01).
/// Depth-capped at `MAX_TRAVERSAL_DEPTH`. Returns `None` if no active terminal
/// is reachable or if `node_id` is not in the graph.
///
/// The starting node itself is checked (depth 0), allowing callers to pass an
/// already-terminal node.
pub fn find_terminal_active(
    node_id: u64,
    graph: &TypedRelationGraph,
    entries: &[EntryRecord],
) -> Option<u64> {
    let start_idx = match graph.node_index.get(&node_id) {
        Some(&idx) => idx,
        None => return None,
    };

    // Iterative DFS — no recursion, no stack overflow risk on pathological chains.
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
        if depth + 1 > MAX_TRAVERSAL_DEPTH {
            continue;
        }

        // Traverse only Supersedes edges (SR-01 — edges_of_type boundary).
        for edge_ref in
            graph.edges_of_type(current_idx, RelationType::Supersedes, Direction::Outgoing)
        {
            let neighbor_idx = edge_ref.target();
            if !visited.contains(&neighbor_idx) {
                visited.insert(neighbor_idx);
                stack.push((neighbor_idx, depth + 1));
            }
        }
    }

    None
}

// -- Private helpers --

/// DFS following outgoing Supersedes edges from `start_idx`.
/// Returns `true` if any reachable successor is `Active && superseded_by.is_none()`.
/// Does NOT check `start_idx` itself — checks successors only.
fn dfs_active_reachable(
    start_idx: NodeIndex,
    graph: &TypedRelationGraph,
    entries: &[EntryRecord],
) -> bool {
    let mut stack: Vec<NodeIndex> = vec![start_idx];
    let mut visited: HashSet<NodeIndex> = HashSet::new();

    while let Some(current_idx) = stack.pop() {
        if !visited.insert(current_idx) {
            continue;
        }

        // Traverse only Supersedes edges (SR-01 — edges_of_type boundary).
        for edge_ref in
            graph.edges_of_type(current_idx, RelationType::Supersedes, Direction::Outgoing)
        {
            let neighbor_idx = edge_ref.target();
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
/// `Active && superseded_by.is_none()` node via Supersedes edges only (SR-01).
///
/// Returns `Some(depth)` where depth >= 1 (start node not counted as terminal
/// since `graph_penalty` is called on entries needing penalizing).
/// Returns `None` if no active terminal reachable or depth exceeds `MAX_TRAVERSAL_DEPTH`.
fn bfs_chain_depth(
    start_idx: NodeIndex,
    graph: &TypedRelationGraph,
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

        // Traverse only Supersedes edges (SR-01 — edges_of_type boundary).
        for edge_ref in
            graph.edges_of_type(current_idx, RelationType::Supersedes, Direction::Outgoing)
        {
            let neighbor_idx = edge_ref.target();
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
/// O(n) per call — acceptable for expected slice sizes (≤1,000 entries).
fn entry_by_id(id: u64, entries: &[EntryRecord]) -> Option<&EntryRecord> {
    entries.iter().find(|e| e.id == id)
}

// -- Transition shims (crt-021 in-progress) --
//
// These aliases and wrapper functions preserve backward compatibility with
// `unimatrix-server` code that has not yet been updated by the server-state
// -- Tests --

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
