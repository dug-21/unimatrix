//! graph_read_path — BFS shortest-path handler for path mode (vnc-020).
//!
//! `handle_path` finds the shortest outgoing-edge path between two entries using
//! path-carrying BFS over the in-memory `TypedRelationGraph`. Tick-window staleness
//! applies (same contract as neighbors depth>1 and subgraph modes — C8, ADR-006).
//!
//! When `use_fallback = true` (cold-start or cycle-detected), the in-memory graph is
//! empty or stale. In that case `path_via_db()` performs BFS directly against live SQL
//! using `query_direct_neighbors` so path mode returns correct results immediately
//! after `context_edge add` without waiting for the next background tick (GH #612).
//!
//! # Key invariants
//! - Outgoing edges only (FR-15). No `direction` parameter on path mode.
//! - Visited set keyed on RESOLVED entry ID (not raw/deprecated) — R-03, pattern #4494.
//! - from_id is a top-level response field, NOT in hops (ADR-005).
//! - length == hops.len() (ADR-005).
//! - from_id == to_id → found: false without BFS (FR-18a, AC-32).
//! - from/to absent from snapshot → found: false, NOT ErrorData (AC-14, AC-15).
//! - RwLock acquired once (lock → clone → release) BEFORE any async work (lock discipline).

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, RwLock};

use petgraph::Direction as PetgraphDirection;
use petgraph::stable_graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rmcp::model::ErrorData;
use unimatrix_core::Store;
use unimatrix_engine::graph::RelationType;
use unimatrix_store::NeighborDirection;

use crate::error::{ERROR_INTERNAL, ERROR_INVALID_PARAMS};
use crate::services::typed_graph::TypedGraphState;

use super::graph_read_neighbors::{all_non_supersedes_types, follow_to_current};
use super::{GraphParams, PathHop, PathResponse};

use unimatrix_store::query_direct_neighbors;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_DEPTH: u8 = 5;
const MAX_DEPTH_UPPER: u8 = 10;

/// Per-node fan-out cap for DB-fallback BFS (path_via_db).
///
/// After each `query_direct_neighbors` call the returned Vec is truncated to
/// this limit before enqueuing. This diverges subtly from the in-memory path,
/// which has no per-hop fan-out cap (petgraph iterates ALL edges). Operators
/// should be aware: extremely high-degree nodes (> MAX_DB_NEIGHBORS_PER_NODE
/// outgoing edges of the requested type) may cause the DB path to miss routes
/// that the in-memory BFS would find once the cache is warm.
const MAX_DB_NEIGHBORS_PER_NODE: usize = 1000;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Handle path mode: BFS shortest outgoing-edge path from `from_id` to `to_id` over
/// the in-memory `TypedRelationGraph` (ADR-005, ADR-006 vnc-020).
///
/// # Staleness
/// The in-memory graph cache is rebuilt each tick (typically 30-60 seconds). Edges
/// written within the current tick interval may not appear in the result. This is the
/// same staleness contract as neighbors mode at depth>1 and subgraph mode.
/// If from_id or to_id is not present in the current graph snapshot, the result is
/// `{ found: false }` — not an error. Use `resolve_supersessions=true` to have deprecated
/// endpoints resolved to their active successors before BFS begins.
pub(super) async fn handle_path(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: &GraphParams,
) -> Result<PathResponse, ErrorData> {
    // Step 1: Validate from_id (required — AC-16).
    let from_id: u64 = match params.from_id {
        Some(id) => id,
        None => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                "path mode requires from_id",
                None,
            ));
        }
    };

    // Step 2: Validate to_id (required — AC-17).
    let to_id: u64 = match params.to_id {
        Some(id) => id,
        None => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                "path mode requires to_id",
                None,
            ));
        }
    };

    // Step 3: Self-path guard — return found: false immediately (FR-18a, AC-32).
    // BFS never fires the destination check on the seed node, so this is consistent
    // with BFS behavior. Resolution happens AFTER this check (per-spec: raw IDs).
    if from_id == to_id {
        return Ok(PathResponse {
            found: false,
            from_id,
            to_id,
            hops: vec![],
            length: 0,
        });
    }

    // Step 4: Validate depth (default 5, range [1, 10] — AC-18, ADR-004).
    let max_depth: u8 = match params.depth {
        None => DEFAULT_DEPTH,
        Some(d) if (1..=MAX_DEPTH_UPPER).contains(&d) => d,
        Some(d) => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                format!("depth must be in range 1..=10, got {d}"),
                None,
            ));
        }
    };

    // Step 5: Validate and resolve edge_types (optional, default = all non-Supersedes).
    let edge_types: Vec<RelationType> = match &params.edge_types {
        None => all_non_supersedes_types(),
        Some(v) if v.is_empty() => all_non_supersedes_types(),
        Some(types) => parse_relation_types(types)?,
    };

    // Step 6: Resolve supersession endpoints BEFORE acquiring the graph lock
    // (lock discipline: all Store async calls happen before lock is held — ADR-006).
    // resolve_supersessions defaults TRUE (bugfix-881, overrides vnc-042 ADR-001 for
    // context_graph per issue #881): deprecated endpoints resolve to their active terminal
    // by default, aligning with context_get. Explicit `false` remains the raw/audit opt-out.
    let resolve_supersessions = params.resolve_supersessions.unwrap_or(true);

    let effective_from: u64 = if resolve_supersessions {
        // follow_to_current returns None when chain exceeds 50 hops or entry is
        // an orphaned deprecated terminal — fall back to original ID (ADR-006 §Consequences).
        follow_to_current(store, from_id).await.unwrap_or(from_id)
    } else {
        from_id
    };

    let effective_to: u64 = if resolve_supersessions {
        follow_to_current(store, to_id).await.unwrap_or(to_id)
    } else {
        to_id
    };

    // Step 7: Acquire graph snapshot (lock → clone → release) BEFORE BFS.
    // std::sync::RwLock — NOT tokio. Poison recovery via unwrap_or_else (same pattern
    // as graph_read_subgraph.rs line 147-149 and graph_read_neighbors.rs).
    // Extract both typed_graph and use_fallback in the same lock guard, then release.
    //
    // `use_fallback = true` covers both cold-start (TypedGraphState::new()) and
    // cycle-detected state (background.rs). This is not purely a cold-start path.
    // When true the in-memory graph is empty or stale — branch to DB-backed BFS.
    let (graph, use_fallback) = {
        let state = typed_graph_state.read().unwrap_or_else(|e| e.into_inner());
        (state.typed_graph.clone(), state.use_fallback)
    };
    // Lock is now released. All subsequent BFS (including async follow_to_current calls)
    // operates on the owned snapshot clone — no lock held during async (lock discipline).

    // When use_fallback=true the snapshot is empty (cold-start) or corrupt (cycle-detected).
    // Fall back to live DB BFS so path mode works immediately after context_edge add
    // without waiting for the next background tick (GH #612).
    if use_fallback {
        return path_via_db(
            store,
            effective_from,
            effective_to,
            &edge_types,
            max_depth,
            resolve_supersessions,
        )
        .await;
    }

    // Step 8: Not-in-snapshot guard for effective_from.
    // Absent from snapshot is NOT an error (AC-15, FR-13).
    let from_idx: NodeIndex = match graph.node_index_for(effective_from) {
        Some(idx) => idx,
        None => {
            return Ok(PathResponse {
                found: false,
                from_id: effective_from,
                to_id: effective_to,
                hops: vec![],
                length: 0,
            });
        }
    };

    // Step 9: Not-in-snapshot guard for effective_to.
    let to_idx: NodeIndex = match graph.node_index_for(effective_to) {
        Some(idx) => idx,
        None => {
            return Ok(PathResponse {
                found: false,
                from_id: effective_from,
                to_id: effective_to,
                hops: vec![],
                length: 0,
            });
        }
    };

    // Post-resolution self-path check (effective IDs may resolve to the same node).
    if from_idx == to_idx {
        return Ok(PathResponse {
            found: false,
            from_id: effective_from,
            to_id: effective_to,
            hops: vec![],
            length: 0,
        });
    }

    // Step 10: Path-carrying BFS (outgoing only — FR-15).
    //
    // Frontier entries: (current_node_idx, path_hops_so_far, current_depth).
    // path_hops_so_far carries the complete path FROM from_id TO current_node,
    // enabling path reconstruction on first arrival at the target without a
    // back-pointer table (ARCHITECTURE.md §Memory bound).
    //
    // CRITICAL (R-03, pattern #4494): The visited set is keyed on the RESOLVED entry
    // ID (effective neighbor after follow_to_current), NOT the raw/deprecated ID.
    // This prevents double-enqueue when multiple deprecated nodes resolve to the same
    // terminal successor (e.g. D1_dep and D2_dep both superseded by C_active).

    // visited: keyed on RESOLVED entry ID (not raw) — R-03.
    let mut visited: HashSet<u64> = HashSet::new();
    // frontier: (current_node_idx, path_so_far, current_depth)
    let mut frontier: VecDeque<(NodeIndex, Vec<PathHop>, u8)> = VecDeque::new();

    // Seed the frontier with the start node.
    // from_id itself is NOT added to hops (ADR-005 — from_id is the top-level field).
    visited.insert(effective_from);
    frontier.push_back((from_idx, vec![], 0));

    while let Some((current_idx, path_so_far, current_depth)) = frontier.pop_front() {
        if current_depth >= max_depth {
            // Depth limit exhausted — do not expand further.
            continue;
        }

        for &rel_type in &edge_types {
            // Collect outgoing edges eagerly (Vec) to avoid borrow conflicts across
            // async follow_to_current calls (same pattern as graph_read_subgraph.rs
            // and graph_read_neighbors.rs — ADR-008).
            let outgoing_pairs: Vec<(NodeIndex, NodeIndex)> = graph
                .edges_of_type(current_idx, rel_type, PetgraphDirection::Outgoing)
                .map(|e| (e.source(), e.target()))
                .collect();

            for (_, tgt_idx) in outgoing_pairs {
                // Resolve raw target ID from NodeIndex.
                let raw_neighbor_id: u64 = match graph.node_id_for_index(tgt_idx) {
                    Some(id) => id,
                    None => continue, // stale index — skip
                };

                // Per-hop supersession resolution (ADR-006 — follows graph_read_subgraph pattern).
                let effective_neighbor: u64 = if resolve_supersessions {
                    follow_to_current(store, raw_neighbor_id)
                        .await
                        .unwrap_or(raw_neighbor_id)
                } else {
                    raw_neighbor_id
                };

                // Build the hop that leads to effective_neighbor.
                let hop = PathHop {
                    entry_id: effective_neighbor,
                    relation_type: rel_type.as_str().to_string(),
                };

                // Check whether we have reached the destination.
                // Compare effective_neighbor ID against effective_to ID.
                if effective_neighbor == effective_to {
                    // Path found. Build and return the complete path.
                    let mut full_path = path_so_far.clone();
                    full_path.push(hop);
                    let length = full_path.len() as u8;
                    return Ok(PathResponse {
                        found: true,
                        from_id: effective_from,
                        to_id: effective_to,
                        hops: full_path,
                        length,
                    });
                }

                // Not the destination: enqueue if not yet visited.
                // CRITICAL: visited check uses effective_neighbor (resolved ID) — R-03.
                if !visited.contains(&effective_neighbor) {
                    // Mark BEFORE enqueue to prevent re-enqueue from alternate paths.
                    visited.insert(effective_neighbor);

                    // Resolve effective_neighbor back to a NodeIndex for BFS continuation.
                    // If effective_neighbor is not in this graph snapshot, still mark visited
                    // (prevents re-enqueue from another path), but do not add to frontier.
                    if let Some(neighbor_idx) = graph.node_index_for(effective_neighbor) {
                        let mut new_path = path_so_far.clone();
                        new_path.push(hop);
                        frontier.push_back((neighbor_idx, new_path, current_depth + 1));
                    }
                }
            }
        }
    }

    // Frontier exhausted without finding target within depth hops.
    Ok(PathResponse {
        found: false,
        from_id: effective_from,
        to_id: effective_to,
        hops: vec![],
        length: 0,
    })
}

// ---------------------------------------------------------------------------
// DB-fallback BFS: path_via_db
// ---------------------------------------------------------------------------

/// BFS over live SQL when `use_fallback = true` (cold-start or cycle-detected).
///
/// Uses `query_direct_neighbors` with `NeighborDirection::Outgoing` — the same
/// function used by neighbors depth=1 live SQL path in `graph_read_neighbors.rs`.
/// The index `idx_graph_edges_source_type(source_id, relation_type)` means this
/// is not a full table scan.
///
/// Preserves all in-memory BFS invariants:
/// - Visited set keyed on resolved ID (R-03, pattern #4494).
/// - Outgoing direction only (FR-15).
/// - max_depth enforced identically to the in-memory path.
/// - Same PathResponse shape and hop structure.
/// - Per-hop follow_to_current / supersession resolution mirrors the in-memory path.
///
/// NOTE: Fan-out is capped at MAX_DB_NEIGHBORS_PER_NODE per hop. This creates a
/// subtle divergence from the in-memory path (no per-hop fan-out cap). Future
/// operators should be aware: nodes with > MAX_DB_NEIGHBORS_PER_NODE outgoing
/// edges of the requested type may have some neighbors silently skipped in DB mode.
async fn path_via_db(
    store: &Store,
    effective_from: u64,
    effective_to: u64,
    edge_types: &[RelationType],
    max_depth: u8,
    resolve_supersessions: bool,
) -> Result<PathResponse, ErrorData> {
    let type_strs: Vec<&str> = edge_types.iter().map(|t| t.as_str()).collect();

    // visited: keyed on RESOLVED entry ID (not raw) — R-03.
    let mut visited: HashSet<u64> = HashSet::new();
    // frontier: (current_entry_id, path_so_far, current_depth)
    let mut frontier: VecDeque<(u64, Vec<PathHop>, u8)> = VecDeque::new();

    // Seed: from_id itself is NOT added to hops (ADR-005).
    visited.insert(effective_from);
    frontier.push_back((effective_from, vec![], 0));

    while let Some((current_id, path_so_far, current_depth)) = frontier.pop_front() {
        if current_depth >= max_depth {
            continue;
        }

        // Query outgoing neighbors from live DB — no full table scan (idx_graph_edges_source_type).
        let mut raw_neighbors = query_direct_neighbors(
            store.read_pool_server(),
            current_id,
            &type_strs,
            NeighborDirection::Outgoing,
        )
        .await
        .map_err(|e| {
            tracing::error!(current_id, error = %e, "path_via_db: query_direct_neighbors failed");
            ErrorData::new(ERROR_INTERNAL, format!("graph query failed: {e}"), None)
        })?;

        // Fan-out cap: prevent runaway BFS on extremely high-degree nodes.
        // Creates a subtle divergence from in-memory path (see module doc).
        raw_neighbors.truncate(MAX_DB_NEIGHBORS_PER_NODE);

        for row in raw_neighbors {
            let raw_neighbor_id = row.target_id;

            // Per-hop supersession resolution (mirrors in-memory BFS — ADR-006).
            let effective_neighbor: u64 = if resolve_supersessions {
                follow_to_current(store, raw_neighbor_id)
                    .await
                    .unwrap_or(raw_neighbor_id)
            } else {
                raw_neighbor_id
            };

            // Build the hop toward effective_neighbor.
            let hop = PathHop {
                entry_id: effective_neighbor,
                relation_type: row.relation_type.clone(),
            };

            // Destination check.
            if effective_neighbor == effective_to {
                let mut full_path = path_so_far.clone();
                full_path.push(hop);
                let length = full_path.len() as u8;
                return Ok(PathResponse {
                    found: true,
                    from_id: effective_from,
                    to_id: effective_to,
                    hops: full_path,
                    length,
                });
            }

            // Enqueue if not yet visited — CRITICAL: keyed on resolved ID (R-03).
            if !visited.contains(&effective_neighbor) {
                visited.insert(effective_neighbor);
                let mut new_path = path_so_far.clone();
                new_path.push(hop);
                frontier.push_back((effective_neighbor, new_path, current_depth + 1));
            }
        }
    }

    // Frontier exhausted.
    Ok(PathResponse {
        found: false,
        from_id: effective_from,
        to_id: effective_to,
        hops: vec![],
        length: 0,
    })
}

// ---------------------------------------------------------------------------
// Helper: parse and validate a slice of edge type name strings
// ---------------------------------------------------------------------------

/// Parse a slice of relation type name strings, returning a `Vec<RelationType>`.
/// Returns `Err(ErrorData)` on the first unrecognized name, listing all valid types.
fn parse_relation_types(types: &[String]) -> Result<Vec<RelationType>, ErrorData> {
    let mut parsed = Vec::with_capacity(types.len());
    for t in types {
        match RelationType::from_str(t) {
            Some(rt) => parsed.push(rt),
            None => {
                return Err(ErrorData::new(
                    ERROR_INVALID_PARAMS,
                    format!(
                        "unrecognized edge type '{t}' \u{2014} recognized types: \
                         About, Advances, Asserts, Cites, CoAccess, \
                         Contradicts, DerivedFrom, Informs, Mentions, \
                         Motivates, Prerequisite, Refutes, RelatedTo, \
                         Supersedes, Supports, Tests"
                    ),
                    None,
                ));
            }
        }
    }
    Ok(parsed)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "graph_read_path_tests.rs"]
mod tests;
