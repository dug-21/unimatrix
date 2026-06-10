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
//! via `edges_of_type`. Non-Supersedes edges (CoAccess, Contradicts, Supports, Prerequisite,
//! Informs) are present in the graph but invisible to all penalty logic (SR-01 mitigation).

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

#[path = "graph_expand.rs"]
mod graph_expand;
pub use graph_expand::graph_expand;

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

/// Hop-decay clamp lower bound. A literal floor (NOT a tunable lever — ADR-001 nan-018).
const HOP_DECAY_CLAMP_FLOOR: f64 = 0.10;

/// Explicit penalty parameters threaded through [`graph_penalty_with`].
///
/// nan-018 ADR-001 (#4897): the crt-014 penalty `const`s become sweepable per-profile
/// levers for the eval harness. This `Copy` struct carries one resolved set of values;
/// its [`Default`] impl references the existing `pub const`s, making them the **single
/// source of truth** for default behavior (dual-default discipline, #4064).
///
/// `fallback` rides on the struct even though [`graph_penalty_with`] never reads it —
/// the fallback branch is applied at the search layer (`search.rs:727`), not inside the
/// engine fn. Keeping it here lets the search layer resolve one params object.
///
/// `hop_decay` and `max_traversal_depth` are **shape** parameters, not severities; the
/// server-side multiplier overlay must never scale them (ADR-001 §3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphPenaltyParams {
    /// Deprecated entry with no successors (orphan). Default: [`ORPHAN_PENALTY`].
    pub orphan: f64,
    /// Cleanly replaced at depth 1; also the hop-decay clamp ceiling.
    /// Default: [`CLEAN_REPLACEMENT_PENALTY`].
    pub clean_replacement: f64,
    /// Per-additional-hop decay multiplier. Default: [`HOP_DECAY_FACTOR`].
    pub hop_decay: f64,
    /// Ambiguous (multi-successor) supersession. Default: [`PARTIAL_SUPERSESSION_PENALTY`].
    pub partial_supersession: f64,
    /// Chain leads nowhere active. Default: [`DEAD_END_PENALTY`].
    pub dead_end: f64,
    /// Flat fallback applied by the search layer on cycle detection.
    /// Default: [`FALLBACK_PENALTY`].
    pub fallback: f64,
    /// Maximum traversal depth (shape param). Default: [`MAX_TRAVERSAL_DEPTH`].
    pub max_traversal_depth: usize,
}

impl Default for GraphPenaltyParams {
    fn default() -> Self {
        // SINGLE SOURCE OF TRUTH: every field references the existing const so that
        // graph_penalty(..) == graph_penalty_with(.., &Default::default()) bit-for-bit
        // (NFR-01), and the server config's Default triangulates to these (#4064).
        GraphPenaltyParams {
            orphan: ORPHAN_PENALTY,
            clean_replacement: CLEAN_REPLACEMENT_PENALTY,
            hop_decay: HOP_DECAY_FACTOR,
            partial_supersession: PARTIAL_SUPERSESSION_PENALTY,
            dead_end: DEAD_END_PENALTY,
            fallback: FALLBACK_PENALTY,
            max_traversal_depth: MAX_TRAVERSAL_DEPTH,
        }
    }
}

// -- Error type --

/// Error returned when the supersession graph contains a cycle.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("supersession cycle detected")]
    CycleDetected,
}

// -- Typed edge classification --

/// Sixteen edge types covering the full relationship taxonomy (6 existing + 10 new in vnc-015).
///
/// Stored as strings in GRAPH_EDGES — NOT integer discriminants.
/// String encoding allows extension without schema migration or GNN retraining.
///
/// `Prerequisite` is reserved for W3-1; no write path exists in crt-021.
/// `Informs` bridges empirical knowledge (lesson-learned, pattern) from earlier feature
/// cycles to normative knowledge (decision, convention) in later cycles (crt-037).
///
/// The 10 new variants (vnc-015) cover SDLC goal-tracing and research domain semantics.
/// All new variants are write-only in this feature except `RelatedTo` which is also added
/// to PPR and graph_expand positive types. `Advances` and `Motivates` are deferred to Phase 2
/// for directed-edge PPR semantics (ADR-006).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationType {
    // ── Existing 6 (UNCHANGED) ───────────────────────────────────────────────
    Supersedes,
    Contradicts,
    Supports,
    CoAccess,
    Prerequisite,
    /// Empirical→normative cross-feature bridge; positive PPR (crt-037).
    /// `graph_penalty` and `find_terminal_active` do NOT traverse Informs edges (SR-01).
    Informs,
    // ── 10 New Variants (vnc-015) ────────────────────────────────────────────
    // SDLC goal-tracing
    /// Source advances or contributes toward target goal/objective.
    /// Write-only in vnc-015; PPR semantics deferred to Phase 2 (ADR-006).
    Advances,
    /// Source is the motivation or rationale behind target decision.
    /// Write-only in vnc-015; PPR semantics deferred to Phase 2 (ADR-006).
    Motivates,
    // Research domain
    /// Source cites or references target as a primary source.
    Cites,
    /// Source makes or contains the target claim.
    Asserts,
    /// Source mentions target entity.
    Mentions,
    /// Source provides evidence contradicting or falsifying target.
    Refutes,
    /// Source tests or experimentally evaluates target thesis/claim.
    Tests,
    /// Source is derived from or originated in target.
    DerivedFrom,
    /// Source concerns or governs target entity/concept.
    About,
    // General fallback — the only new PPR-positive variant (ADR-006)
    /// Weak semantic relatedness; no more specific type available. Added to PPR/BFS (ADR-006).
    RelatedTo,
}

impl RelationType {
    /// Returns the canonical string representation stored in GRAPH_EDGES.
    pub fn as_str(&self) -> &'static str {
        match self {
            // Existing 6 variants (UNCHANGED)
            Self::Supersedes => "Supersedes",
            Self::Contradicts => "Contradicts",
            Self::Supports => "Supports",
            Self::CoAccess => "CoAccess",
            Self::Prerequisite => "Prerequisite",
            Self::Informs => "Informs",
            // 10 new variants (vnc-015)
            Self::Advances => "Advances",
            Self::Motivates => "Motivates",
            Self::Cites => "Cites",
            Self::Asserts => "Asserts",
            Self::Mentions => "Mentions",
            Self::Refutes => "Refutes",
            Self::Tests => "Tests",
            Self::DerivedFrom => "DerivedFrom",
            Self::About => "About",
            Self::RelatedTo => "RelatedTo",
        }
    }

    /// Parses a string into a `RelationType`. Case-sensitive. Returns `None` for unknown strings.
    ///
    /// Note: This method intentionally has the same name as `std::str::FromStr::from_str` per
    /// the architecture integration surface (ARCHITECTURE.md §Integration Surface).
    ///
    /// CRITICAL (ADR-007, R-01): All named arms MUST appear before the wildcard `_ => None`.
    /// A variant added after the wildcard compiles but silently returns None — edges written with
    /// that type are silently dropped by `build_typed_relation_graph` Pass 2b (R-10 guard).
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            // Existing 6 variants (UNCHANGED)
            "Supersedes" => Some(RelationType::Supersedes),
            "Contradicts" => Some(RelationType::Contradicts),
            "Supports" => Some(RelationType::Supports),
            "CoAccess" => Some(RelationType::CoAccess),
            "Prerequisite" => Some(RelationType::Prerequisite),
            "Informs" => Some(RelationType::Informs),
            // 10 new variants (vnc-015) — all BEFORE the wildcard arm
            "Advances" => Some(RelationType::Advances),
            "Motivates" => Some(RelationType::Motivates),
            "Cites" => Some(RelationType::Cites),
            "Asserts" => Some(RelationType::Asserts),
            "Mentions" => Some(RelationType::Mentions),
            "Refutes" => Some(RelationType::Refutes),
            "Tests" => Some(RelationType::Tests),
            "DerivedFrom" => Some(RelationType::DerivedFrom),
            "About" => Some(RelationType::About),
            "RelatedTo" => Some(RelationType::RelatedTo),
            // Wildcard MUST remain last — see critical note above (ADR-007)
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

    /// Look up the petgraph `NodeIndex` for a given entry ID.
    ///
    /// Returns `None` when the entry is not present in the current tick's graph
    /// (cold-start, unknown ID, or entry not loaded into the snapshot).
    ///
    /// This is the cross-crate visibility solution for BFS traversal in
    /// `unimatrix-server` (ADR-008). The internal `node_index` field remains
    /// `pub(crate)`; this accessor exposes only the minimum necessary surface.
    pub fn node_index_for(&self, id: u64) -> Option<NodeIndex> {
        self.node_index.get(&id).copied()
    }

    /// Look up the entry ID (u64) stored at a `NodeIndex`.
    ///
    /// Companion to `node_index_for` — provides the reverse mapping needed by BFS
    /// traversal in `unimatrix-server` to convert edge target/source `NodeIndex` values
    /// back to entry IDs without exposing the `inner` StableGraph field (ADR-008).
    ///
    /// Returns `None` when the `NodeIndex` has no corresponding node (e.g., removed node
    /// in a StableGraph or an index from a different graph instance).
    pub fn node_id_for_index(&self, idx: NodeIndex) -> Option<u64> {
        self.inner.node_weight(idx).copied()
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
///
/// This is a **thin wrapper** over [`graph_penalty_with`] with
/// [`GraphPenaltyParams::default()`] (the crt-014 consts). It exists so every existing
/// caller and ordering-invariant test stays bit-for-bit identical (nan-018 ADR-001,
/// NFR-01).
pub fn graph_penalty(node_id: u64, graph: &TypedRelationGraph, entries: &[EntryRecord]) -> f64 {
    graph_penalty_with(node_id, graph, entries, &GraphPenaltyParams::default())
}

/// Topology-derived penalty multiplier for a node, with explicit penalty parameters.
///
/// Identical branch structure to [`graph_penalty`]; every const is replaced by the
/// matching `params.*` field. The hop-decay clamp **ceiling tracks
/// `params.clean_replacement`** (NOT the const) — see the clamp-coupling note below.
///
/// nan-018 ADR-001 (#4897): this is the parameterized entry point the eval harness
/// sweeps per profile. `params.fallback` is carried for the search layer but unread here.
///
/// ## Clamp coupling (LOAD-BEARING — ADR-001, R-13)
///
/// At depth `d >= 2`: `raw = clean_replacement * hop_decay^(d-1)`, clamped to
/// `[0.10, params.clean_replacement]`. The upper bound is **`params.clean_replacement`
/// itself**, not the const: because `hop_decay < 1`, `raw <= clean_replacement` so the
/// ceiling is the monotonicity cap (depth-2 never harsher than depth-1). If the ceiling
/// stayed the const while `clean_replacement` is swept higher, a depth-2 entry could be
/// clamped MORE harshly than depth-1, inverting the formula. The lower bound `0.10` stays
/// a literal floor. Consequence: `clean_replacement` is an **amplified** sweep knob (base
/// and ceiling move together).
pub fn graph_penalty_with(
    node_id: u64,
    graph: &TypedRelationGraph,
    entries: &[EntryRecord],
    params: &GraphPenaltyParams,
) -> f64 {
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
        return params.orphan;
    }

    // Signal 2: active_reachable via Supersedes edges
    let active_reachable =
        dfs_active_reachable(node_idx, graph, entries, params.max_traversal_depth);

    // Priority 2: no active terminal reachable
    if !active_reachable {
        return params.dead_end;
    }

    // Priority 3: partial supersession — multiple direct Supersedes successors
    if successor_count > 1 {
        return params.partial_supersession;
    }

    // Signal 3: chain_depth via Supersedes edges
    let chain_depth = bfs_chain_depth(node_idx, graph, entries, params.max_traversal_depth);

    // Priority 4: clean replacement at depth 1
    if chain_depth == Some(1) {
        return params.clean_replacement;
    }

    // Priority 5: hop decay at depth >= 2
    if let Some(d) = chain_depth
        && d >= 2
    {
        let raw = params.clean_replacement * params.hop_decay.powi((d - 1) as i32);
        // Clamp ceiling = params.clean_replacement (NOT the const) — clamp coupling, ADR-001.
        // Guard the `min > max` case: a swept clean_replacement below the literal floor
        // would make `clamp(0.10, clean_replacement)` panic (std::f64::clamp requires
        // min <= max). When the ceiling sits below the floor, the ceiling dominates (the
        // base penalty is already smaller than the floor) — never panic.
        let ceiling = params.clean_replacement;
        if ceiling <= HOP_DECAY_CLAMP_FLOOR {
            return ceiling;
        }
        return raw.clamp(HOP_DECAY_CLAMP_FLOOR, ceiling);
    }

    // Priority 6: defensive fallback — should not be reached in valid data
    params.dead_end
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
///
/// Traversal is capped at `max_traversal_depth` hops from `start_idx` (a **shape**
/// parameter, default [`MAX_TRAVERSAL_DEPTH`]). A successor beyond the cap is not
/// visited, so a depth set below the deepest chain truncates the search to a defined
/// "no active terminal reachable" result — never a panic (nan-018 R-TEST).
fn dfs_active_reachable(
    start_idx: NodeIndex,
    graph: &TypedRelationGraph,
    entries: &[EntryRecord],
    max_traversal_depth: usize,
) -> bool {
    // Stack entries: (NodeIndex, depth_from_start).
    let mut stack: Vec<(NodeIndex, usize)> = vec![(start_idx, 0)];
    let mut visited: HashSet<NodeIndex> = HashSet::new();

    while let Some((current_idx, depth)) = stack.pop() {
        if !visited.insert(current_idx) {
            continue;
        }

        // Do not traverse beyond the depth cap (same convention as bfs_chain_depth).
        if depth > max_traversal_depth {
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
            stack.push((neighbor_idx, depth + 1));
        }
    }

    false
}

/// BFS from `start_idx` to find the shortest hop distance to the nearest
/// `Active && superseded_by.is_none()` node via Supersedes edges only (SR-01).
///
/// Returns `Some(depth)` where depth >= 1 (start node not counted as terminal
/// since `graph_penalty` is called on entries needing penalizing).
/// Returns `None` if no active terminal reachable or depth exceeds `max_traversal_depth`
/// (a **shape** parameter, default [`MAX_TRAVERSAL_DEPTH`]).
fn bfs_chain_depth(
    start_idx: NodeIndex,
    graph: &TypedRelationGraph,
    entries: &[EntryRecord],
    max_traversal_depth: usize,
) -> Option<usize> {
    let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::new();
    let mut visited: HashSet<NodeIndex> = HashSet::new();

    queue.push_back((start_idx, 0));
    visited.insert(start_idx);

    while let Some((current_idx, depth)) = queue.pop_front() {
        if depth > max_traversal_depth {
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

#[cfg(test)]
#[path = "graph_penalty_params_tests.rs"]
mod penalty_params_tests;
