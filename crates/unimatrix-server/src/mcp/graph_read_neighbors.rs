//! Neighbors mode handler for context_graph (vnc-018, ADR-005).
//!
//! depth=1 → live SQL on GRAPH_EDGES (always fresh).
//! depth>1 → BFS over in-memory TypedRelationGraph (tick-window staleness) when warm;
//!           falls back to live SQL via `neighbors_via_db` when `use_fallback=true`
//!           (cold-start or cycle-detected) — GH #623.
//!
//! Declared as a sub-module of `graph_read.rs` via `#[path]`.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, RwLock};

use petgraph::stable_graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rmcp::model::ErrorData;
use unimatrix_core::Store;
use unimatrix_engine::graph::{RelationType, TypedRelationGraph};
use unimatrix_store::{NeighborDirection, query_direct_neighbors};

use unimatrix_core::Status;

use crate::error::{ERROR_INTERNAL, ERROR_INVALID_PARAMS};
use crate::services::typed_graph::TypedGraphState;

use super::{EdgeRecord, GraphParams, NeighborsResponse};

// ---------------------------------------------------------------------------
// follow_to_current — supersession resolution helper (also in supersession.rs)
// ---------------------------------------------------------------------------

/// Follow `superseded_by` from `id` to the terminal Active entry using the store.
///
/// 50-hop safety cap enforced by loop bound. Returns `None` when chain exceeds 50
/// hops, entry is an orphaned deprecated terminal, or store lookup fails.
/// Caller uses the original ID as a fallback (ADR-005, R-10).
pub(super) async fn follow_to_current(store: &Store, id: u64) -> Option<u64> {
    let mut current = id;
    for _ in 0..50 {
        let entry = match store.get(current).await {
            Ok(e) => e,
            Err(_) => return None,
        };
        match entry.superseded_by {
            None => {
                if entry.status == Status::Active {
                    return Some(current);
                } else {
                    return None;
                }
            }
            Some(next_id) => current = next_id,
        }
    }
    None
}

// ---------------------------------------------------------------------------
// All 15 non-Supersedes types
// ---------------------------------------------------------------------------

/// All 15 non-Supersedes relation types.
///
/// Used when `edge_types` is absent or empty — Supersedes is always silently excluded
/// (AC-10, AC-10a). No warning, no extra field in the response.
pub(super) fn all_non_supersedes_types() -> Vec<RelationType> {
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

// ---------------------------------------------------------------------------
// Neighbor traversal entry point
// ---------------------------------------------------------------------------

/// Neighbor traversal entry point — dispatches to SQL (depth=1) or BFS (depth>1).
pub(super) async fn handle_neighbors(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: &GraphParams,
    id: u64,
) -> Result<NeighborsResponse, ErrorData> {
    // Step 1: Validate depth (R-11). Range is 1..=10. Default 1.
    let depth = params.depth.unwrap_or(1);
    if depth == 0 || depth > 10 {
        return Err(ErrorData::new(
            ERROR_INVALID_PARAMS,
            format!("depth must be in range 1..=10, got {depth}"),
            None,
        ));
    }

    // Step 2: Validate direction for neighbors mode (R-17).
    // neighbors uses "incoming"|"outgoing"|"both" — NOT "forward"/"backward".
    let direction = match params.direction.as_deref().unwrap_or("both") {
        "incoming" => NeighborDirection::Incoming,
        "outgoing" => NeighborDirection::Outgoing,
        "both" => NeighborDirection::Both,
        other => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                format!(
                    "invalid direction '{other}' for neighbors mode — valid values: incoming, outgoing, both"
                ),
                None,
            ));
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
                    return Err(ErrorData::new(
                        ERROR_INVALID_PARAMS,
                        "Supersedes edges are not traversable via neighbors mode — use chain or current modes for supersession navigation",
                        None,
                    ));
                }
                // Validate each type via from_str (FR-07, AC-15).
                match RelationType::from_str(s) {
                    Some(rel_type) => resolved.push(rel_type),
                    None => {
                        return Err(ErrorData::new(
                            ERROR_INVALID_PARAMS,
                            format!(
                                "unknown edge type '{s}' — valid types: \
                                 Advances, Asserts, About, Cites, CoAccess, \
                                 Contradicts, DerivedFrom, Informs, Mentions, \
                                 Motivates, Prerequisite, Refutes, RelatedTo, \
                                 Supports, Tests"
                            ),
                            None,
                        ));
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
    // Note: neighbors_bfs reads use_fallback inside the lock guard and delegates to
    // neighbors_via_db when use_fallback=true (cold-start / cycle-detected) — GH #623.

    Ok(NeighborsResponse { edges })
}

// ---------------------------------------------------------------------------
// depth=1 SQL path
// ---------------------------------------------------------------------------

/// depth=1 live SQL path (ADR-005).
async fn neighbors_sql(
    store: &Store,
    id: u64,
    types: &[RelationType],
    direction: NeighborDirection,
) -> Result<Vec<EdgeRecord>, ErrorData> {
    let type_strs: Vec<&str> = types.iter().map(|t| t.as_str()).collect();

    let raw_rows = query_direct_neighbors(store.read_pool_server(), id, &type_strs, direction)
        .await
        .map_err(|e| {
            tracing::error!(id, error = %e, "query_direct_neighbors failed");
            ErrorData::new(ERROR_INTERNAL, format!("graph query failed: {e}"), None)
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

// ---------------------------------------------------------------------------
// depth>1 BFS path
// ---------------------------------------------------------------------------

/// depth>1 in-memory BFS path (ADR-005).
///
/// Uses `std::sync::RwLock` (NOT tokio) — TypedGraphStateHandle is std::sync::Arc<RwLock<_>>.
/// Graph and use_fallback are cloned/read out from under the lock before any async work.
/// When use_fallback=true, delegates to `neighbors_via_db` (GH #623).
/// Visited set is `HashSet<u64>` keyed by node_id only (AC-11a, R-18).
async fn neighbors_bfs(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    id: u64,
    types: &[RelationType],
    direction: NeighborDirection,
    depth: u8,
    resolve_supersessions: bool,
) -> Result<Vec<EdgeRecord>, ErrorData> {
    use petgraph::Direction;

    // Acquire std::sync::RwLock and clone graph + use_fallback to release the lock before
    // async work. Extract both fields in the same lock guard (GH #623 fix).
    let (graph, use_fallback): (TypedRelationGraph, bool) = {
        let guard = typed_graph_state.read().unwrap_or_else(|e| e.into_inner());
        (guard.typed_graph.clone(), guard.use_fallback)
    };

    // When use_fallback=true (cold-start or cycle-detected), typed_graph is empty.
    // node_index_for(id) returns None, BFS never starts, depth_reached=0 for all seeds.
    // Delegate to live DB path instead (GH #623).
    if use_fallback {
        return neighbors_via_db(store, id, types, direction, depth, resolve_supersessions).await;
    }

    // Find anchor node in the in-memory graph (ADR-008).
    let start_node = match graph.node_index_for(id) {
        Some(idx) => idx,
        None => {
            // Anchor ID not in current tick's graph (genuinely absent after warm start).
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
    let mut frontier: VecDeque<(NodeIndex, u64, u8)> = VecDeque::new();

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
                // Use node_id_for_index to get u64 IDs without accessing inner (ADR-008).
                let edges: Vec<_> = graph
                    .edges_of_type(current_idx, *rel_type, petgraph_dir)
                    .filter_map(|e| {
                        let neighbor_idx = match petgraph_dir {
                            Direction::Outgoing => e.target(),
                            Direction::Incoming => e.source(),
                        };
                        let neighbor_id = graph.node_id_for_index(neighbor_idx)?;
                        Some((neighbor_id, petgraph_dir))
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
                                frontier.push_back((neighbor_node_idx, effective_id, hop_depth));
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
// DB-fallback BFS: neighbors_via_db (GH #623)
// ---------------------------------------------------------------------------

/// Per-node fan-out cap for DB-fallback BFS (neighbors_via_db).
///
/// After each `query_direct_neighbors` call the returned Vec is truncated to this
/// limit before enqueuing. Mirrors `MAX_DB_NEIGHBORS_PER_NODE` in `graph_read_path.rs`.
const MAX_DB_NEIGHBORS_PER_NODE: usize = 1000;

/// BFS over live SQL when `use_fallback = true` (cold-start or cycle-detected).
///
/// `direction` is forwarded from the caller as-is — never hard-coded to Outgoing
/// (hard requirement: callers specifying direction="incoming" must get correct results).
///
/// Mirrors `path_via_db` in graph_read_path.rs. Key differences:
/// - Multi-direction support (`direction` parameter forwarded).
/// - Records both source_id/target_id and direction string on each EdgeRecord.
/// - Visited set keyed by node_id only (AC-11a, R-18): first encounter wins.
async fn neighbors_via_db(
    store: &Store,
    id: u64,
    types: &[RelationType],
    direction: NeighborDirection,
    depth: u8,
    resolve_supersessions: bool,
) -> Result<Vec<EdgeRecord>, ErrorData> {
    let type_strs: Vec<&str> = types.iter().map(|t| t.as_str()).collect();

    let mut result_edges: Vec<EdgeRecord> = Vec::new();
    // Visited set keyed by node_id only (AC-11a, R-18).
    let mut visited: HashSet<u64> = HashSet::new();
    // Queue: (current_entry_id, current_depth)
    let mut frontier: VecDeque<(u64, u8)> = VecDeque::new();

    visited.insert(id);
    frontier.push_back((id, 0));

    while let Some((current_id, current_depth)) = frontier.pop_front() {
        if current_depth >= depth {
            continue;
        }

        // Query live DB — direction is forwarded as-is (hard requirement).
        let mut raw_neighbors =
            query_direct_neighbors(store.read_pool_server(), current_id, &type_strs, direction)
                .await
                .map_err(|e| {
                    tracing::error!(current_id, error = %e, "neighbors_via_db: query_direct_neighbors failed");
                    ErrorData::new(ERROR_INTERNAL, format!("graph query failed: {e}"), None)
                })?;

        // Fan-out cap: prevent runaway BFS on high-degree nodes.
        raw_neighbors.truncate(MAX_DB_NEIGHBORS_PER_NODE);

        for row in raw_neighbors {
            // Determine neighbor_id and direction string from the raw row.
            let (neighbor_id, edge_dir_str, record_src, record_tgt) = if row.source_id == current_id
            {
                // outgoing edge: current_id → row.target_id
                (row.target_id, "outgoing", current_id, row.target_id)
            } else {
                // incoming edge: row.source_id → current_id
                (row.source_id, "incoming", row.source_id, current_id)
            };

            // Supersession resolution (ADR-005, R-10).
            let effective_id = if resolve_supersessions {
                follow_to_current(store, neighbor_id)
                    .await
                    .unwrap_or(neighbor_id)
            } else {
                neighbor_id
            };

            // Visited set keyed by node_id only (AC-11a, R-18).
            if !visited.contains(&effective_id) {
                visited.insert(effective_id);
                let hop_depth = current_depth + 1;

                result_edges.push(EdgeRecord {
                    source_id: record_src,
                    target_id: record_tgt,
                    relation_type: row.relation_type.clone(),
                    direction: edge_dir_str.to_string(),
                    depth: hop_depth,
                    metadata: None,
                });

                // Enqueue for further expansion if not at max depth.
                if hop_depth < depth {
                    frontier.push_back((effective_id, hop_depth));
                }
            }
        }
    }

    Ok(result_edges)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "graph_read_neighbors_tests.rs"]
mod tests;
