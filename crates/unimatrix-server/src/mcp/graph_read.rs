//! context_graph mode handlers: chain, current, neighbors (vnc-018).
//!
//! Exposes a single public entry point (`handle_graph`) called from `tools.rs`.
//! All validation, dispatch, and result serialization lives here.
//!
//! # Execution order (ARCHITECTURE.md §Component Interactions, ADR-003)
//! 1. `require_cap(Read)` — runs in `tools.rs` BEFORE `handle_graph` is called.
//! 2. `validate_no_unsupported_params` — inside `handle_graph`, before mode dispatch.
//! 3. Mode dispatch — chain / current / neighbors.
//!
//! # Key constraints
//! - chain and current modes use SQL recursive CTEs (ADR-001).
//!   `find_terminal_active` (in-memory) is PROHIBITED for both modes.
//! - `EdgeRecord.metadata` serializes as JSON `null` — no `skip_serializing_if` (ADR-004, R-15).
//! - BFS visited set is `HashSet<u64>` keyed by `node_id` only (AC-11a, R-18).
//! - `TypedGraphStateHandle` wraps `std::sync::RwLock` — use `.read().unwrap_or_else(...)`.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, RwLock};

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use unimatrix_core::{EntryRecord, Status, Store};
use unimatrix_engine::graph::{RelationType, TypedRelationGraph};
use unimatrix_store::{
    ChainDirection, NeighborDirection, query_current_terminal, query_direct_neighbors,
    query_supersession_chain,
};

use crate::mcp::context::ToolContext;
use crate::services::typed_graph::TypedGraphState;

// ---------------------------------------------------------------------------
// Wire types (ADR-003, ADR-004)
// ---------------------------------------------------------------------------

/// Wire parameters for the context_graph tool (layout locked — ADR-003).
///
/// Forward-compat fields (`seed_ids`, `max_nodes`, `from_id`, `to_id`) error on
/// misuse via `validate_no_unsupported_params`. Never remove or reorder fields.
#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
pub struct GraphParams {
    /// Traversal mode: "chain" | "current" | "neighbors".
    pub mode: String,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
    /// Anchor entry ID — required for all three modes.
    #[schemars(with = "Option<u64>")]
    pub id: Option<u64>,
    /// chain: "forward"|"backward"|"both"; neighbors: "incoming"|"outgoing"|"both".
    pub direction: Option<String>,
    /// neighbors only: edge types to traverse (absent/[] = all except Supersedes).
    pub edge_types: Option<Vec<String>>,
    /// neighbors only: hop depth 1..=10 (default 1).
    pub depth: Option<u8>,
    /// neighbors only: resolve deprecated endpoints to active terminal (default false).
    /// Rejected on chain mode (ADR-003).
    pub resolve_supersessions: Option<bool>,
    // -- Forward-compat fields — error on misuse in current modes (ADR-003) --
    /// subgraph mode (#597) — not yet supported.
    pub seed_ids: Option<Vec<u64>>,
    /// subgraph mode (#597) — not yet supported.
    pub max_nodes: Option<u32>,
    /// path mode (#598) — not yet supported.
    pub from_id: Option<u64>,
    /// path mode (#598) — not yet supported.
    pub to_id: Option<u64>,
}

/// A single typed edge from a neighbors traversal (ADR-004, vnc-018).
///
/// `metadata` always serializes as JSON `null` in vnc-018.
/// `skip_serializing_if` is PROHIBITED on this field (ADR-004, R-15).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EdgeRecord {
    pub source_id: u64,
    pub target_id: u64,
    pub relation_type: String,
    /// "incoming" | "outgoing" relative to the traversal anchor.
    pub direction: String,
    pub depth: u8,
    /// Always `null` in vnc-018. Do NOT add skip_serializing_if here (ADR-004).
    pub metadata: Option<serde_json::Value>,
}

/// Per-direction truncation status for chain mode (ADR-002).
///
/// Wire format: `{"forward": bool, "backward": bool}` — NEVER a flat bool (R-02).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Truncated {
    pub forward: bool,
    pub backward: bool,
}

/// Response envelope for chain mode.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChainResult {
    pub entries: Vec<EntryRecord>,
    pub truncated: Truncated,
}

/// Response envelope for current mode.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CurrentResponse {
    pub entry: EntryRecord,
}

/// Response envelope for neighbors mode.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NeighborsResponse {
    pub edges: Vec<EdgeRecord>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Main handler for `context_graph`. Called from `tools.rs` after capability check.
///
/// Execution order (per ARCHITECTURE.md §Component Interactions):
/// 1. `validate_no_unsupported_params` (centralized, before mode dispatch).
/// 2. Require anchor `id` for all three modes.
/// 3. Mode dispatch.
pub(crate) async fn handle_graph(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: GraphParams,
    _ctx: &ToolContext,
) -> Result<CallToolResult, rmcp::ErrorData> {
    // Step 1: Centralized parameter validation (ADR-003).
    // Capability check (require_cap) already ran in tools.rs before this is called.
    if let Err(msg) = validate_no_unsupported_params(&params) {
        return Err(rmcp::ErrorData {
            code: rmcp::ErrorCode::INVALID_PARAMS,
            message: msg.into(),
            data: None,
        });
    }

    // Step 2: All three modes require an anchor ID.
    let id = match params.id {
        Some(id) => id,
        None => {
            return Err(rmcp::ErrorData {
                code: rmcp::ErrorCode::INVALID_PARAMS,
                message: "id is required for chain, current, and neighbors modes".into(),
                data: None,
            });
        }
    };

    // Step 3: Mode dispatch.
    match params.mode.as_str() {
        "chain" => {
            let result = handle_chain(store, &params, id).await;
            let json = serde_json::to_string(&result).map_err(|e| rmcp::ErrorData {
                code: rmcp::ErrorCode::INTERNAL_ERROR,
                message: format!("serialization error: {e}").into(),
                data: None,
            })?;
            Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                json,
            )]))
        }
        "current" => match handle_current(store, id).await {
            Ok(resp) => {
                let json = serde_json::to_string(&resp).map_err(|e| rmcp::ErrorData {
                    code: rmcp::ErrorCode::INTERNAL_ERROR,
                    message: format!("serialization error: {e}").into(),
                    data: None,
                })?;
                Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                    json,
                )]))
            }
            Err(msg) => Err(rmcp::ErrorData {
                code: rmcp::ErrorCode::INVALID_PARAMS,
                message: msg.into(),
                data: None,
            }),
        },
        "neighbors" => {
            let result =
                handle_neighbors(store, typed_graph_state, &params, id).await?;
            let json = serde_json::to_string(&result).map_err(|e| rmcp::ErrorData {
                code: rmcp::ErrorCode::INTERNAL_ERROR,
                message: format!("serialization error: {e}").into(),
                data: None,
            })?;
            Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                json,
            )]))
        }
        // validate_no_unsupported_params already caught unrecognized modes above;
        // this arm is unreachable under normal flow but required for exhaustiveness.
        _ => unreachable!(
            "validate_no_unsupported_params must catch unrecognized modes before reaching dispatch"
        ),
    }
}

// ---------------------------------------------------------------------------
// Centralized parameter validation (ADR-003)
// ---------------------------------------------------------------------------

/// Centralized parameter validation. Called at the top of `handle_graph` before
/// mode dispatch. This function is the single contract point for:
/// - Unrecognized mode (fires BEFORE any field check — R-04).
/// - Forward-compat fields on unsupported modes (with future-mode hint).
/// - `resolve_supersessions=Some(true)` on chain mode (semantically circular, R-08).
///
/// When #597 ships, add a `"subgraph"` arm permitting `seed_ids` and `max_nodes`.
pub(crate) fn validate_no_unsupported_params(params: &GraphParams) -> Result<(), String> {
    match params.mode.as_str() {
        "chain" => {
            // chain rejects all forward-compat fields AND resolve_supersessions=true.
            if params.seed_ids.is_some() {
                return Err(
                    "seed_ids is not supported in chain mode — use subgraph mode (#597)"
                        .to_string(),
                );
            }
            if params.max_nodes.is_some() {
                return Err(
                    "max_nodes is not supported in chain mode — use subgraph mode (#597)"
                        .to_string(),
                );
            }
            if params.from_id.is_some() {
                return Err(
                    "from_id is not supported in chain mode — use path mode (#598)".to_string(),
                );
            }
            if params.to_id.is_some() {
                return Err(
                    "to_id is not supported in chain mode — use path mode (#598)".to_string(),
                );
            }
            // resolve_supersessions on chain is semantically circular (FR-08, AC-15c, R-08):
            // chain IS the supersession audit. This check must live here, not in handle_chain.
            if params.resolve_supersessions == Some(true) {
                return Err(
                    "resolve_supersessions is not applicable to chain mode — chain IS the supersession audit".to_string()
                );
            }
            Ok(())
        }
        "current" => {
            if params.seed_ids.is_some() {
                return Err(
                    "seed_ids is not supported in current mode — use subgraph mode (#597)"
                        .to_string(),
                );
            }
            if params.max_nodes.is_some() {
                return Err(
                    "max_nodes is not supported in current mode — use subgraph mode (#597)"
                        .to_string(),
                );
            }
            if params.from_id.is_some() {
                return Err(
                    "from_id is not supported in current mode — use path mode (#598)".to_string(),
                );
            }
            if params.to_id.is_some() {
                return Err(
                    "to_id is not supported in current mode — use path mode (#598)".to_string(),
                );
            }
            Ok(())
        }
        "neighbors" => {
            if params.seed_ids.is_some() {
                return Err(
                    "seed_ids is not supported in neighbors mode — use subgraph mode (#597)"
                        .to_string(),
                );
            }
            if params.max_nodes.is_some() {
                return Err(
                    "max_nodes is not supported in neighbors mode — use subgraph mode (#597)"
                        .to_string(),
                );
            }
            if params.from_id.is_some() {
                return Err(
                    "from_id is not supported in neighbors mode — use path mode (#598)"
                        .to_string(),
                );
            }
            if params.to_id.is_some() {
                return Err(
                    "to_id is not supported in neighbors mode — use path mode (#598)".to_string(),
                );
            }
            Ok(())
        }
        // _ arm fires BEFORE field checks — unrecognized mode error is the first thing
        // callers see (R-04). When #597 ships, add "subgraph" arm that permits seed_ids/max_nodes.
        _ => Err(format!(
            "unrecognized mode '{}' — supported modes: chain, current, neighbors",
            params.mode
        )),
    }
}

// ---------------------------------------------------------------------------
// chain mode (FR-04, ADR-001)
// ---------------------------------------------------------------------------

/// Walk the supersession chain from `id` using SQL recursive CTEs.
///
/// Returns empty `ChainResult` for non-existent IDs — no error (AC-04).
/// INTENTIONALLY asymmetric with `handle_current` (which returns an error).
/// See R-21 and AC-04. Do not unify these behaviors.
async fn handle_chain(store: &Store, params: &GraphParams, id: u64) -> ChainResult {
    // Validate direction for chain mode: forward/backward/both only.
    // "incoming"/"outgoing" are neighbors-mode vocabulary.
    let direction = match params.direction.as_deref().unwrap_or("both") {
        "forward" => ChainDirection::Forward,
        "backward" => ChainDirection::Backward,
        "both" => ChainDirection::Both,
        other => {
            tracing::warn!(
                direction = %other,
                "invalid direction for chain mode — expected forward|backward|both"
            );
            return ChainResult {
                entries: vec![],
                truncated: Truncated {
                    forward: false,
                    backward: false,
                },
            };
        }
    };

    // ADR-001: SQL recursive CTE path is mandatory. find_terminal_active is PROHIBITED.
    match query_supersession_chain(store.read_pool_server(), id, direction, 50).await {
        Ok(chain_result) => ChainResult {
            entries: chain_result.entries,
            truncated: Truncated {
                forward: chain_result.forward_capped,
                backward: chain_result.backward_capped,
            },
        },
        Err(e) => {
            tracing::error!(id, error = %e, "query_supersession_chain failed");
            ChainResult {
                entries: vec![],
                truncated: Truncated {
                    forward: false,
                    backward: false,
                },
            }
        }
    }
}

// ---------------------------------------------------------------------------
// current mode (FR-05, ADR-001, R-20)
// ---------------------------------------------------------------------------

/// Follow `superseded_by` from `id` to the terminal Active entry.
///
/// Returns `Err("No active terminal found for entry {id}")` for:
/// - Non-existent ID (AC-05a).
/// - Orphaned deprecated terminal (R-20) — `AND e.status = 0` filter is MANDATORY.
/// - Chain exceeds 50 hops (AC-07).
///
/// INTENTIONALLY asymmetric with `handle_chain`:
/// - `chain` on non-existent ID → empty result (AC-04).
/// - `current` on non-existent ID → error (AC-05a).
/// Asking for the current version of something that doesn't exist is a semantic error,
/// not an empty set. Do NOT unify these behaviors. See R-21.
async fn handle_current(store: &Store, id: u64) -> Result<CurrentResponse, String> {
    // ADR-001: SQL recursive CTE path. find_terminal_active (in-memory) is PROHIBITED.
    // query_current_terminal includes `AND e.status = 0` (Active) — guards against orphaned
    // deprecated terminals (R-20 Critical). Without this filter, a deprecated entry with
    // superseded_by IS NULL would be silently returned as the terminal.
    match query_current_terminal(store.read_pool_server(), id).await {
        Ok(Some(entry)) => Ok(CurrentResponse { entry }),
        Ok(None) => {
            // All three failure cases (non-existent ID, orphaned deprecated, chain > 50 hops)
            // produce zero rows at SQL level — same error intentionally (FR-05).
            Err(format!("No active terminal found for entry {id}"))
        }
        Err(e) => {
            tracing::error!(id, error = %e, "query_current_terminal failed");
            Err(format!("No active terminal found for entry {id}"))
        }
    }
}

// ---------------------------------------------------------------------------
// neighbors mode (FR-06, ADR-005)
// ---------------------------------------------------------------------------

/// All 15 non-Supersedes relation types.
///
/// Used when `edge_types` is absent or empty — Supersedes is always silently excluded
/// (AC-10, AC-10a). No warning, no extra field in the response.
fn all_non_supersedes_types() -> Vec<RelationType> {
    vec![
        RelationType::Contradicts,
        RelationType::Supports,
        RelationType::CoAccess,
        RelationType::Prerequisite,
        RelationType::Informs,
        RelationType::Advances,
        RelationType::Motivates,
        RelationType::Cites,
        RelationType::Asserts,
        RelationType::Mentions,
        RelationType::Refutes,
        RelationType::Tests,
        RelationType::DerivedFrom,
        RelationType::About,
        RelationType::RelatedTo,
    ]
}

/// Neighbor traversal entry point — dispatches to SQL (depth=1) or BFS (depth>1).
///
/// ADR-005: depth=1 → live SQL on GRAPH_EDGES (always fresh).
/// depth>1 → BFS over in-memory TypedRelationGraph (tick-window staleness).
async fn handle_neighbors(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: &GraphParams,
    id: u64,
) -> Result<NeighborsResponse, rmcp::ErrorData> {
    // Step 1: Validate depth (R-11). Range is 1..=10. Default 1.
    let depth = params.depth.unwrap_or(1);
    if depth == 0 || depth > 10 {
        return Err(rmcp::ErrorData {
            code: rmcp::ErrorCode::INVALID_PARAMS,
            message: format!("depth must be in range 1..=10, got {depth}").into(),
            data: None,
        });
    }

    // Step 2: Validate direction for neighbors mode (R-17).
    // neighbors uses "incoming"|"outgoing"|"both" — NOT "forward"/"backward".
    let direction = match params.direction.as_deref().unwrap_or("both") {
        "incoming" => NeighborDirection::Incoming,
        "outgoing" => NeighborDirection::Outgoing,
        "both" => NeighborDirection::Both,
        other => {
            return Err(rmcp::ErrorData {
                code: rmcp::ErrorCode::INVALID_PARAMS,
                message: format!(
                    "invalid direction '{other}' for neighbors mode — valid values: incoming, outgoing, both"
                )
                .into(),
                data: None,
            });
        }
    };

    // Step 3: Validate and resolve edge_types (FR-07, R-06).
    let requested_types: Vec<RelationType> = match &params.edge_types {
        None => all_non_supersedes_types(),
        Some(type_strings) if type_strings.is_empty() => all_non_supersedes_types(),
        Some(type_strings) => {
            let mut resolved = Vec::new();
            for s in type_strings {
                // Reject Supersedes explicitly (FR-07, AC-15a).
                if s.eq_ignore_ascii_case("Supersedes") {
                    return Err(rmcp::ErrorData {
                        code: rmcp::ErrorCode::INVALID_PARAMS,
                        message: "Supersedes edges are not traversable via neighbors mode — use chain or current modes for supersession navigation".into(),
                        data: None,
                    });
                }
                // Validate each type via from_str (FR-07, AC-15).
                match RelationType::from_str(s) {
                    Some(rel_type) => resolved.push(rel_type),
                    None => {
                        return Err(rmcp::ErrorData {
                            code: rmcp::ErrorCode::INVALID_PARAMS,
                            message: format!(
                                "unknown edge type '{s}' — valid types: \
                                 Advances, Asserts, About, Cites, CoAccess, \
                                 Contradicts, DerivedFrom, Informs, Mentions, \
                                 Motivates, Prerequisite, Refutes, RelatedTo, \
                                 Supports, Tests"
                            )
                            .into(),
                            data: None,
                        });
                    }
                }
            }
            resolved
        }
    };

    // Step 4: Dispatch to SQL (depth=1) or BFS (depth>1) per ADR-005.
    let edges = if depth == 1 {
        neighbors_sql(store, id, &requested_types, direction).await?
    } else {
        let resolve = params.resolve_supersessions.unwrap_or(false);
        neighbors_bfs(
            store,
            typed_graph_state,
            id,
            &requested_types,
            direction,
            depth,
            resolve,
        )
        .await?
    };

    Ok(NeighborsResponse { edges })
}

/// depth=1 live SQL path (ADR-005).
async fn neighbors_sql(
    store: &Store,
    id: u64,
    types: &[RelationType],
    direction: NeighborDirection,
) -> Result<Vec<EdgeRecord>, rmcp::ErrorData> {
    let type_strs: Vec<&str> = types.iter().map(|t| t.as_str()).collect();

    let raw_rows =
        query_direct_neighbors(store.read_pool_server(), id, &type_strs, direction)
            .await
            .map_err(|e| {
                tracing::error!(id, error = %e, "query_direct_neighbors failed");
                rmcp::ErrorData {
                    code: rmcp::ErrorCode::INTERNAL_ERROR,
                    message: format!("graph query failed: {e}").into(),
                    data: None,
                }
            })?;

    let edges = raw_rows
        .into_iter()
        .map(|row| {
            let dir_str = if row.source_id == id {
                "outgoing"
            } else {
                "incoming"
            };
            EdgeRecord {
                source_id: row.source_id,
                target_id: row.target_id,
                relation_type: row.relation_type,
                direction: dir_str.to_string(),
                depth: 1,
                metadata: None, // always None in vnc-018 (ADR-004, R-15)
            }
        })
        .collect();

    Ok(edges)
}

/// depth>1 in-memory BFS path (ADR-005).
///
/// Uses `std::sync::RwLock` (NOT tokio) — TypedGraphStateHandle is std::sync::Arc<RwLock<_>>.
/// Graph is cloned out from under the lock before any async work (poison-recovered).
/// Visited set is `HashSet<u64>` keyed by node_id only (AC-11a, R-18).
async fn neighbors_bfs(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    id: u64,
    types: &[RelationType],
    direction: NeighborDirection,
    depth: u8,
    resolve_supersessions: bool,
) -> Result<Vec<EdgeRecord>, rmcp::ErrorData> {
    use petgraph::Direction;

    // Acquire std::sync::RwLock and clone the graph to release the lock before async work.
    // TypedGraphStateHandle uses std::sync::RwLock — do NOT use .read().await.
    let graph: TypedRelationGraph = {
        let guard = typed_graph_state
            .read()
            .unwrap_or_else(|e| e.into_inner());
        guard.typed_graph.clone()
    };

    // Find anchor node in the in-memory graph (ADR-008).
    let start_node = match graph.node_index_for(id) {
        Some(idx) => idx,
        None => {
            // Anchor ID not in current tick's graph (cold-start or genuinely absent).
            // Return empty result — no error (consistent with depth=1 behavior).
            return Ok(vec![]);
        }
    };

    let mut result_edges: Vec<EdgeRecord> = Vec::new();
    // BFS visited set keyed by node_id ONLY (AC-11a, R-18).
    // Each node appears at most once, at its minimum hop depth.
    // Do NOT key by (node_id, depth) — that produces duplicates (R-18).
    let mut visited: HashSet<u64> = HashSet::new();
    // Queue: (NodeIndex, node_id, depth_so_far)
    let mut frontier: VecDeque<(petgraph::stable_graph::NodeIndex, u64, u8)> =
        VecDeque::new();

    visited.insert(id);
    frontier.push_back((start_node, id, 0));

    while let Some((current_idx, current_id, current_depth)) = frontier.pop_front() {
        if current_depth >= depth {
            continue;
        }

        for rel_type in types {
            let petgraph_dirs: &[Direction] = match direction {
                NeighborDirection::Outgoing => &[Direction::Outgoing],
                NeighborDirection::Incoming => &[Direction::Incoming],
                NeighborDirection::Both => &[Direction::Outgoing, Direction::Incoming],
            };

            for &petgraph_dir in petgraph_dirs {
                // Collect edges to avoid borrow issues with async follow_to_current.
                let edges: Vec<_> = graph
                    .edges_of_type(current_idx, *rel_type, petgraph_dir)
                    .map(|e| {
                        let neighbor_id = match petgraph_dir {
                            Direction::Outgoing => graph.inner[e.target()],
                            Direction::Incoming => graph.inner[e.source()],
                        };
                        (neighbor_id, petgraph_dir)
                    })
                    .collect();

                for (neighbor_node_id, edge_dir) in edges {
                    let edge_dir_str = match edge_dir {
                        Direction::Outgoing => "outgoing",
                        Direction::Incoming => "incoming",
                    };

                    // Resolve supersession if requested (ADR-005, R-10).
                    let effective_id = if resolve_supersessions {
                        // follow_to_current returns None on 50-hop cap or orphaned deprecated
                        // — use original ID as fallback (ADR-005, R-10 acceptance).
                        follow_to_current(store, neighbor_node_id)
                            .await
                            .unwrap_or(neighbor_node_id)
                    } else {
                        neighbor_node_id
                    };

                    // Visited set keyed by node_id only (AC-11a, R-18).
                    // First encounter at shallowest depth wins; longer-path duplicates skipped.
                    if !visited.contains(&effective_id) {
                        visited.insert(effective_id);
                        let hop_depth = current_depth + 1;

                        let (record_src, record_tgt) = match edge_dir {
                            Direction::Outgoing => (current_id, effective_id),
                            Direction::Incoming => (effective_id, current_id),
                        };

                        result_edges.push(EdgeRecord {
                            source_id: record_src,
                            target_id: record_tgt,
                            relation_type: rel_type.as_str().to_string(),
                            direction: edge_dir_str.to_string(),
                            depth: hop_depth,
                            metadata: None,
                        });

                        // Enqueue for further expansion if not at max depth.
                        if hop_depth < depth {
                            if let Some(neighbor_node_idx) = graph.node_index_for(effective_id) {
                                frontier.push_back((
                                    neighbor_node_idx,
                                    effective_id,
                                    hop_depth,
                                ));
                            }
                            // node_index_for returns None: effective_id not in current tick's graph.
                            // BFS stops there — no error, tracing::warn is optional.
                        }
                    }
                    // Already visited: skip. Shallowest depth wins (node_id keying invariant).
                }
            }
        }
    }

    Ok(result_edges)
}

// ---------------------------------------------------------------------------
// follow_to_current — supersession resolution helper
// ---------------------------------------------------------------------------

/// Follow `superseded_by` from `id` to the terminal Active entry using the store.
///
/// 50-hop safety cap enforced by loop bound.
/// Returns `None` when:
/// - Chain exceeds 50 hops.
/// - Orphaned deprecated terminal (`superseded_by IS NULL`, `status != Active`).
/// - Store error during lookup.
///
/// Caller uses the original ID as a fallback when `None` is returned (ADR-005, R-10).
async fn follow_to_current(store: &Store, id: u64) -> Option<u64> {
    let mut current = id;
    for _ in 0..50 {
        let entry = match store.get(current).await {
            Ok(e) => e,
            Err(_) => return None, // Store error — treat as unresolvable.
        };
        match entry.superseded_by {
            None => {
                // Terminal: check status. Active = valid; anything else = orphaned.
                if entry.status == Status::Active {
                    return Some(current);
                } else {
                    // Orphaned deprecated terminal (superseded_by IS NULL, status != Active).
                    // No valid substitution (R-10 edge case).
                    return None;
                }
            }
            Some(next_id) => current = next_id,
        }
    }
    // Loop exhausted: chain exceeds 50 hops.
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // validate_no_unsupported_params tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_chain_rejects_resolve_supersessions() {
        // AC-15c, R-08: resolve_supersessions on chain mode is semantically circular.
        // This check must fire inside validate_no_unsupported_params, NOT handle_chain.
        let params = GraphParams {
            mode: "chain".to_string(),
            resolve_supersessions: Some(true),
            ..Default::default()
        };
        let result = validate_no_unsupported_params(&params);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "resolve_supersessions is not applicable to chain mode — chain IS the supersession audit"
        );
    }

    #[test]
    fn test_validate_unrecognized_mode_fires_before_field_check() {
        // R-04: unrecognized mode must fire BEFORE any field-level check.
        // mode="subgraph" with seed_ids present must return "unrecognized mode",
        // NOT "seed_ids not supported".
        let params = GraphParams {
            mode: "subgraph".to_string(),
            seed_ids: Some(vec![1, 2, 3]),
            ..Default::default()
        };
        let result = validate_no_unsupported_params(&params);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("unrecognized mode"), "got: {msg}");
        assert!(
            !msg.contains("seed_ids"),
            "forward-compat check must not fire first, got: {msg}"
        );
    }

    #[test]
    fn test_validate_walk_mode_error_lists_valid_modes() {
        // AC-14: unrecognized mode error must list the supported modes.
        let params = GraphParams {
            mode: "walk".to_string(),
            ..Default::default()
        };
        let result = validate_no_unsupported_params(&params);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("chain"), "got: {msg}");
        assert!(msg.contains("current"), "got: {msg}");
        assert!(msg.contains("neighbors"), "got: {msg}");
    }

    #[test]
    fn test_validate_neighbors_rejects_seed_ids() {
        // AC-15b, R-16: seed_ids in neighbors mode → error with "seed_ids" and "subgraph".
        let params = GraphParams {
            mode: "neighbors".to_string(),
            seed_ids: Some(vec![1]),
            ..Default::default()
        };
        let result = validate_no_unsupported_params(&params);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("seed_ids"), "got: {msg}");
        assert!(msg.contains("subgraph"), "got: {msg}");
    }

    #[test]
    fn test_validate_neighbors_rejects_from_id() {
        // AC-15b: from_id in neighbors mode → error with "from_id" and "path".
        let params = GraphParams {
            mode: "neighbors".to_string(),
            from_id: Some(1),
            ..Default::default()
        };
        let result = validate_no_unsupported_params(&params);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("from_id"), "got: {msg}");
        assert!(msg.contains("path"), "got: {msg}");
    }

    #[test]
    fn test_validate_neighbors_rejects_to_id() {
        // AC-15b: to_id in neighbors mode → error with "to_id" and "path".
        let params = GraphParams {
            mode: "neighbors".to_string(),
            to_id: Some(1),
            ..Default::default()
        };
        let result = validate_no_unsupported_params(&params);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("to_id"), "got: {msg}");
        assert!(msg.contains("path"), "got: {msg}");
    }

    #[test]
    fn test_validate_neighbors_rejects_max_nodes() {
        // AC-15b: max_nodes in neighbors mode → error with "max_nodes" and "subgraph".
        let params = GraphParams {
            mode: "neighbors".to_string(),
            max_nodes: Some(10),
            ..Default::default()
        };
        let result = validate_no_unsupported_params(&params);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("max_nodes"), "got: {msg}");
        assert!(msg.contains("subgraph"), "got: {msg}");
    }

    #[test]
    fn test_validate_chain_rejects_seed_ids() {
        // Forward-compat: seed_ids rejected on chain mode.
        let params = GraphParams {
            mode: "chain".to_string(),
            seed_ids: Some(vec![1]),
            ..Default::default()
        };
        let result = validate_no_unsupported_params(&params);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("seed_ids"), "got: {msg}");
        assert!(msg.contains("chain"), "got: {msg}");
    }

    #[test]
    fn test_validate_chain_rejects_from_id() {
        let params = GraphParams {
            mode: "chain".to_string(),
            from_id: Some(1),
            ..Default::default()
        };
        let result = validate_no_unsupported_params(&params);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("from_id"), "got: {msg}");
        assert!(msg.contains("path"), "got: {msg}");
    }

    #[test]
    fn test_validate_chain_rejects_to_id() {
        let params = GraphParams {
            mode: "chain".to_string(),
            to_id: Some(1),
            ..Default::default()
        };
        let result = validate_no_unsupported_params(&params);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("to_id"), "got: {msg}");
    }

    #[test]
    fn test_validate_chain_rejects_max_nodes() {
        let params = GraphParams {
            mode: "chain".to_string(),
            max_nodes: Some(100),
            ..Default::default()
        };
        let result = validate_no_unsupported_params(&params);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("max_nodes"), "got: {msg}");
    }

    #[test]
    fn test_validate_valid_modes_pass() {
        for mode in &["chain", "current", "neighbors"] {
            let params = GraphParams {
                mode: mode.to_string(),
                ..Default::default()
            };
            let result = validate_no_unsupported_params(&params);
            assert!(result.is_ok(), "mode={mode} should be valid, got: {result:?}");
        }
    }

    // -----------------------------------------------------------------------
    // EdgeRecord serialization tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_edge_record_metadata_serializes_as_null() {
        // R-15, NFR-07: metadata must appear in JSON as `null`, not be absent.
        // skip_serializing_if = "Option::is_none" is PROHIBITED on this field (ADR-004).
        let record = EdgeRecord {
            source_id: 1,
            target_id: 2,
            relation_type: "Supports".to_string(),
            direction: "outgoing".to_string(),
            depth: 1,
            metadata: None,
        };
        let json = serde_json::to_string(&record).expect("serialize");
        assert!(
            json.contains("\"metadata\":null"),
            "metadata must serialize as null, not be absent. JSON: {json}"
        );
        assert!(
            !json.contains(r#""metadata":{"#),
            "metadata must be null, not an object. JSON: {json}"
        );
    }

    #[test]
    fn test_truncated_serializes_as_struct_not_flat_bool() {
        // R-02, AC-03b: Truncated must serialize as {"forward":bool,"backward":bool}.
        // A flat bool would break the wire format contract (ADR-002).
        let t = Truncated {
            forward: true,
            backward: false,
        };
        let json = serde_json::to_string(&t).expect("serialize");
        assert!(
            json.contains("\"forward\":true"),
            "truncated.forward missing. JSON: {json}"
        );
        assert!(
            json.contains("\"backward\":false"),
            "truncated.backward missing. JSON: {json}"
        );
        // Must not be a flat bool — key is present as object field
        assert_ne!(json, "true", "Truncated must not serialize as a flat bool");
    }

    // -----------------------------------------------------------------------
    // Store-dependent tests (require a live database)
    // -----------------------------------------------------------------------

    use unimatrix_store::db::SqlxStore;
    use unimatrix_store::pool_config::PoolConfig;
    use unimatrix_store::schema::Status;

    async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.db");
        let store = SqlxStore::open(&path, PoolConfig::test_default())
            .await
            .expect("open test store");
        (store, dir)
    }

    async fn insert_entry_direct(
        pool: &sqlx::sqlite::SqlitePool,
        title: &str,
        status: Status,
        supersedes: Option<u64>,
        superseded_by: Option<u64>,
    ) -> u64 {
        let id: i64 =
            sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'next_entry_id'")
                .fetch_one(pool)
                .await
                .expect("get next_entry_id");
        let new_id = id + 1;
        sqlx::query("UPDATE counters SET value = ?1 WHERE name = 'next_entry_id'")
            .bind(new_id)
            .execute(pool)
            .await
            .expect("update counter");

        let status_i = status as i64;
        let now = 1_700_000_000_i64;
        sqlx::query(
            "INSERT INTO entries (id, title, content, topic, category, source, status,
             confidence, created_at, updated_at, last_accessed_at, access_count,
             supersedes, superseded_by, correction_count, embedding_dim,
             created_by, modified_by, content_hash, previous_hash,
             version, feature_cycle, trust_source, helpful_count, unhelpful_count)
             VALUES (?1, ?2, 'content', 'test', 'pattern', 'test', ?3,
             0.5, ?4, ?4, ?4, 0, ?5, ?6, 0, 0, '', '', '', '', 1, '', '', 0, 0)",
        )
        .bind(new_id)
        .bind(title)
        .bind(status_i)
        .bind(now)
        .bind(supersedes.map(|v| v as i64))
        .bind(superseded_by.map(|v| v as i64))
        .execute(pool)
        .await
        .expect("insert entry");

        new_id as u64
    }

    // -----------------------------------------------------------------------
    // handle_chain tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_handle_chain_nonexistent_id_returns_empty() {
        // AC-04, R-21: chain mode returns empty for non-existent ID.
        // INTENTIONALLY asymmetric with current mode which returns error.
        // See R-21 and AC-04. Do not unify these behaviors.
        let (store_impl, _dir) = open_test_store().await;

        let params = GraphParams {
            mode: "chain".to_string(),
            id: Some(999_999),
            ..Default::default()
        };
        let result = handle_chain(&store_impl, &params, 999_999).await;

        assert!(
            result.entries.is_empty(),
            "non-existent id must return empty entries"
        );
        assert!(!result.truncated.forward, "forward truncated must be false");
        assert!(
            !result.truncated.backward,
            "backward truncated must be false"
        );
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_chain_five_entry_chain_both_directions() {
        // AC-01: five-entry chain queried from middle returns all 5 ordered oldest→newest.
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        // Create chain: A → B → C → D → E (A oldest, E newest).
        let a = insert_entry_direct(wp, "A", Status::Deprecated, None, None).await;
        let b = insert_entry_direct(wp, "B", Status::Deprecated, Some(a), None).await;
        let c = insert_entry_direct(wp, "C", Status::Deprecated, Some(b), None).await;
        let d = insert_entry_direct(wp, "D", Status::Deprecated, Some(c), None).await;
        let e = insert_entry_direct(wp, "E", Status::Active, Some(d), None).await;

        // Set superseded_by links.
        for (old, new) in [(a, b), (b, c), (c, d), (d, e)] {
            sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
                .bind(new as i64)
                .bind(old as i64)
                .execute(wp)
                .await
                .unwrap();
        }

        let params = GraphParams {
            mode: "chain".to_string(),
            id: Some(c),
            direction: Some("both".to_string()),
            ..Default::default()
        };
        let result = handle_chain(&store_impl, &params, c).await;

        assert_eq!(result.entries.len(), 5, "all 5 entries must be returned");
        let ids: Vec<u64> = result.entries.iter().map(|entry| entry.id).collect();
        let pos_a = ids.iter().position(|&x| x == a).unwrap();
        let pos_c = ids.iter().position(|&x| x == c).unwrap();
        let pos_e = ids.iter().position(|&x| x == e).unwrap();
        assert!(pos_a < pos_c, "A must come before C");
        assert!(pos_c < pos_e, "C must come before E");
        assert!(!result.truncated.forward);
        assert!(!result.truncated.backward);
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_chain_direction_forward_from_mid_chain() {
        // AC-02: forward direction from mid-chain returns seed + descendants.
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let a = insert_entry_direct(wp, "A", Status::Deprecated, None, None).await;
        let b = insert_entry_direct(wp, "B", Status::Deprecated, Some(a), None).await;
        let c = insert_entry_direct(wp, "C", Status::Active, Some(b), None).await;

        for (old, new) in [(a, b), (b, c)] {
            sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
                .bind(new as i64)
                .bind(old as i64)
                .execute(wp)
                .await
                .unwrap();
        }

        let params = GraphParams {
            mode: "chain".to_string(),
            id: Some(a),
            direction: Some("forward".to_string()),
            ..Default::default()
        };
        let result = handle_chain(&store_impl, &params, a).await;

        let ids: Vec<u64> = result.entries.iter().map(|entry| entry.id).collect();
        assert!(ids.contains(&a), "seed A must be included");
        assert!(ids.contains(&b), "B must be in forward result");
        assert!(ids.contains(&c), "C must be in forward result");
        store_impl.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // handle_current tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_handle_current_active_entry_returns_self() {
        // AC-05: current on active entry returns same entry.
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let id = insert_entry_direct(wp, "Active Entry", Status::Active, None, None).await;

        let result = handle_current(&store_impl, id).await;
        assert!(result.is_ok(), "active entry must return Ok");
        let resp = result.unwrap();
        assert_eq!(resp.entry.id, id);
        assert_eq!(resp.entry.status, Status::Active);
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_current_nonexistent_id_returns_error() {
        // AC-05a, R-21: current on non-existent ID returns error.
        // INTENTIONALLY asymmetric with chain mode (returns empty for same ID).
        // This asymmetry is correct by design — current is a lookup that must
        // succeed or fail, not a traversal that can return empty. See R-21.
        let (store_impl, _dir) = open_test_store().await;

        let result = handle_current(&store_impl, 999_999).await;
        assert!(result.is_err(), "non-existent id must return error");
        let msg = result.unwrap_err();
        assert!(
            msg.to_lowercase().contains("no active terminal"),
            "error must mention 'no active terminal', got: {msg}"
        );
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_current_deprecated_resolves_to_active_terminal() {
        // AC-06: deprecated entry with valid chain resolves to active terminal.
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let a = insert_entry_direct(wp, "A", Status::Deprecated, None, None).await;
        let b = insert_entry_direct(wp, "B", Status::Deprecated, Some(a), None).await;
        let c = insert_entry_direct(wp, "C", Status::Active, Some(b), None).await;

        for (old, new) in [(a, b), (b, c)] {
            sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
                .bind(new as i64)
                .bind(old as i64)
                .execute(wp)
                .await
                .unwrap();
        }

        let result = handle_current(&store_impl, a).await;
        assert!(
            result.is_ok(),
            "deprecated entry must resolve to active terminal"
        );
        assert_eq!(result.unwrap().entry.id, c);
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_current_orphaned_deprecated_returns_error() {
        // AC-06b, R-20: orphaned deprecated entry (superseded_by IS NULL, status=Deprecated)
        // must return error — NOT the deprecated entry as the terminal.
        // COMMENT: This is the only test that catches an accidentally omitted
        // `AND e.status = 0` (Active) filter in the CTE. Without this filter,
        // the deprecated entry would be returned as if it were an active terminal (R-20 Critical).
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let id =
            insert_entry_direct(wp, "Orphaned Deprecated", Status::Deprecated, None, None).await;

        let result = handle_current(&store_impl, id).await;
        assert!(
            result.is_err(),
            "orphaned deprecated entry must return error, not the entry itself"
        );
        store_impl.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // handle_neighbors validation tests (no full DB traversal needed for error paths)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_handle_neighbors_supersedes_explicit_rejection() {
        // AC-15a, R-06: Supersedes in edge_types must produce exact error string.
        let (store_impl, _dir) = open_test_store().await;
        let handle = Arc::new(crate::services::typed_graph::TypedGraphState::new_handle());

        let params = GraphParams {
            mode: "neighbors".to_string(),
            id: Some(1),
            edge_types: Some(vec!["Supersedes".to_string()]),
            ..Default::default()
        };

        let result = handle_neighbors(&store_impl, &handle, &params, 1).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("Supersedes edges are not traversable via neighbors mode"),
            "exact error string required, got: {}",
            err.message
        );
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_neighbors_supersedes_in_mixed_list_rejected() {
        // R-06: one valid type alongside Supersedes does not bypass rejection.
        let (store_impl, _dir) = open_test_store().await;
        let handle = Arc::new(crate::services::typed_graph::TypedGraphState::new_handle());

        let params = GraphParams {
            mode: "neighbors".to_string(),
            id: Some(1),
            edge_types: Some(vec!["Supersedes".to_string(), "Supports".to_string()]),
            ..Default::default()
        };

        let result = handle_neighbors(&store_impl, &handle, &params, 1).await;
        assert!(result.is_err());
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_neighbors_unknown_edge_type() {
        // AC-15: unknown edge type string rejected before traversal.
        let (store_impl, _dir) = open_test_store().await;
        let handle = Arc::new(crate::services::typed_graph::TypedGraphState::new_handle());

        let params = GraphParams {
            mode: "neighbors".to_string(),
            id: Some(1),
            edge_types: Some(vec!["BogusEdge".to_string()]),
            ..Default::default()
        };

        let result = handle_neighbors(&store_impl, &handle, &params, 1).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("unknown edge type"),
            "error must mention unknown edge type, got: {}",
            err.message
        );
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_neighbors_direction_invalid_for_mode() {
        // R-17: direction="forward" is chain-mode vocabulary; rejected for neighbors.
        let (store_impl, _dir) = open_test_store().await;
        let handle = Arc::new(crate::services::typed_graph::TypedGraphState::new_handle());

        let params = GraphParams {
            mode: "neighbors".to_string(),
            id: Some(1),
            direction: Some("forward".to_string()),
            ..Default::default()
        };

        let result = handle_neighbors(&store_impl, &handle, &params, 1).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("invalid direction"),
            "error must mention invalid direction, got: {}",
            err.message
        );
        // Valid neighbors directions must be listed in the error.
        assert!(
            err.message.contains("incoming") || err.message.contains("outgoing"),
            "error must list valid directions, got: {}",
            err.message
        );
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_neighbors_depth_out_of_range() {
        // R-11: depth=0 and depth=11 rejected; depth=1 is a valid boundary.
        let (store_impl, _dir) = open_test_store().await;
        let handle = Arc::new(crate::services::typed_graph::TypedGraphState::new_handle());

        // depth=0 → error
        let params_zero = GraphParams {
            mode: "neighbors".to_string(),
            id: Some(1),
            depth: Some(0),
            ..Default::default()
        };
        let result = handle_neighbors(&store_impl, &handle, &params_zero, 1).await;
        assert!(result.is_err(), "depth=0 must be rejected");

        // depth=11 → error
        let params_eleven = GraphParams {
            mode: "neighbors".to_string(),
            id: Some(1),
            depth: Some(11),
            ..Default::default()
        };
        let result = handle_neighbors(&store_impl, &handle, &params_eleven, 1).await;
        assert!(result.is_err(), "depth=11 must be rejected");

        // depth=1 → OK (may return empty, but no validation error)
        let params_one = GraphParams {
            mode: "neighbors".to_string(),
            id: Some(999_999),
            depth: Some(1),
            ..Default::default()
        };
        let result = handle_neighbors(&store_impl, &handle, &params_one, 999_999).await;
        assert!(result.is_ok(), "depth=1 is valid boundary");

        store_impl.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // follow_to_current tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_follow_to_current_active_entry_returns_self() {
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let id = insert_entry_direct(wp, "Active", Status::Active, None, None).await;
        let result = follow_to_current(&store_impl, id).await;
        assert_eq!(result, Some(id), "active entry must resolve to itself");
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_follow_to_current_chain_resolves() {
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let a = insert_entry_direct(wp, "A", Status::Deprecated, None, None).await;
        let b = insert_entry_direct(wp, "B", Status::Deprecated, Some(a), None).await;
        let c = insert_entry_direct(wp, "C", Status::Active, Some(b), None).await;

        for (old, new) in [(a, b), (b, c)] {
            sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
                .bind(new as i64)
                .bind(old as i64)
                .execute(wp)
                .await
                .unwrap();
        }

        let result = follow_to_current(&store_impl, a).await;
        assert_eq!(result, Some(c), "chain must resolve to terminal active C");
        store_impl.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_follow_to_current_orphaned_returns_none() {
        // R-10: orphaned deprecated entry (superseded_by IS NULL, status=Deprecated) → None.
        let (store_impl, _dir) = open_test_store().await;
        let wp = store_impl.write_pool_server();

        let id =
            insert_entry_direct(wp, "Orphaned Deprecated", Status::Deprecated, None, None).await;
        let result = follow_to_current(&store_impl, id).await;
        assert!(result.is_none(), "orphaned deprecated entry must return None");
        store_impl.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // TypedRelationGraph::node_index_for tests (ADR-008)
    // -----------------------------------------------------------------------

    fn make_entry_for_test(id: u64) -> EntryRecord {
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

    #[test]
    fn test_node_index_for_known_node_returns_index() {
        // R-07, AC-11: node_index_for on a known ID returns Some(index).
        use unimatrix_engine::graph::build_typed_relation_graph;

        let entry = make_entry_for_test(42);
        let graph = build_typed_relation_graph(&[entry], &[]).expect("build graph");
        let result = graph.node_index_for(42);
        assert!(result.is_some(), "known node 42 must return Some(NodeIndex)");
    }

    #[test]
    fn test_node_index_for_unknown_node_returns_none() {
        // R-07: node_index_for on unknown ID returns None.
        let graph = unimatrix_engine::graph::TypedRelationGraph::empty();
        let result = graph.node_index_for(999_999);
        assert!(result.is_none(), "unknown node must return None");
    }
}
