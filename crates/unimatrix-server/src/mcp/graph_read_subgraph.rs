//! subgraph mode BFS traversal and metadata hydration (vnc-019).
//!
//! This module implements `handle_subgraph` -- the BFS-based multi-hop traversal
//! for `context_graph` subgraph mode. See IMPLEMENTATION-BRIEF.md §BFS Algorithm Contract.
//!
//! # Key invariants (ARCHITECTURE.md, IMPLEMENTATION-BRIEF.md)
//! - BFS uses `TypedRelationGraph` (in-memory); no SQL fallback for depth > 1 (C-05).
//! - `visited` set keyed on `effective_id` (post-supersession), not original (R-01).
//! - Edge dedup uses canonical stored direction `(source_id -> target_id)` (R-02).
//! - `direction` on all returned `EdgeRecord`s is always `"outgoing"` (FR-12, R-02).
//! - Empty OR-chain guard: metadata batch query skipped when no edges collected (R-04).
//! - Post-BFS dangling-edge filter required for correctness when cap fires mid-hop (R-05).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, RwLock};

use petgraph::Direction as PetgraphDirection;
use petgraph::stable_graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rmcp::model::ErrorData;
use unimatrix_core::{EntryRecord, Store};
use unimatrix_engine::graph::RelationType;

use crate::error::{ERROR_INTERNAL, ERROR_INVALID_PARAMS};
use crate::services::typed_graph::TypedGraphState;

use super::{EdgeRecord, GraphParams, SubgraphResponse};
use super::graph_read_neighbors::{all_non_supersedes_types, follow_to_current};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_MAX_DEPTH: u8 = 3;
const MAX_DEPTH_UPPER: u8 = 10;
const MAX_NODES_UPPER: u32 = 200;
const DEFAULT_MAX_NODES: u32 = 200;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// BFS-based subgraph traversal from one or more seed entry IDs.
///
/// Validates all parameters, performs BFS over the in-memory `TypedRelationGraph`,
/// hydrates nodes in a single batch query, and populates `EdgeRecord.metadata` via
/// a post-BFS OR-chain query against `GRAPH_EDGES`.
///
/// # Staleness
/// The in-memory graph cache is rebuilt each tick (typically 30-60 seconds).
/// Edges written within the current tick interval may not appear in the result.
/// This is the same staleness contract as neighbors mode at depth>1.
pub(super) async fn handle_subgraph(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: &GraphParams,
) -> Result<SubgraphResponse, ErrorData> {
    // Step 1: Validate parameters.

    // seed_ids required and non-empty.
    let seed_ids: Vec<u64> = match &params.seed_ids {
        Some(ids) if !ids.is_empty() => ids.clone(),
        _ => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                "subgraph mode requires at least one entry ID in seed_ids",
                None,
            ));
        }
    };

    // max_depth: default 3, valid range [1, 10].
    let max_depth: u8 = match params.max_depth {
        None => DEFAULT_MAX_DEPTH,
        Some(d) if (1..=MAX_DEPTH_UPPER).contains(&d) => d,
        Some(d) => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                format!("max_depth must be in range 1..=10, got {d}"),
                None,
            ));
        }
    };

    // max_nodes: default 200, valid range [1, 200]. Values above 200 are rejected.
    let max_nodes: u32 = match params.max_nodes {
        None => DEFAULT_MAX_NODES,
        Some(n) if (1..=MAX_NODES_UPPER).contains(&n) => n,
        Some(n) => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                format!("max_nodes must be in range 1..=200, got {n}"),
                None,
            ));
        }
    };
    let max_nodes_usize = max_nodes as usize;

    // direction: default "outgoing", must be one of incoming/outgoing/both.
    let direction_str = params.direction.as_deref().unwrap_or("outgoing");
    let petgraph_dirs: Vec<PetgraphDirection> = match direction_str {
        "incoming" => vec![PetgraphDirection::Incoming],
        "outgoing" => vec![PetgraphDirection::Outgoing],
        "both" => vec![PetgraphDirection::Outgoing, PetgraphDirection::Incoming],
        other => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                format!("direction must be one of: incoming, outgoing, both; got '{other}'"),
                None,
            ));
        }
    };

    // edge_types: expand absent/empty to all non-supersedes types.
    let edge_types: Vec<RelationType> = match &params.edge_types {
        None => all_non_supersedes_types(),
        Some(v) if v.is_empty() => all_non_supersedes_types(),
        Some(types) => {
            let mut parsed = Vec::with_capacity(types.len());
            for t in types {
                match RelationType::from_str(t) {
                    Some(rt) => parsed.push(rt),
                    None => {
                        return Err(ErrorData::new(
                            ERROR_INVALID_PARAMS,
                            format!(
                                "unknown edge_type '{}' -- valid types: \
                                 Advances, Asserts, About, Cites, CoAccess, \
                                 Contradicts, DerivedFrom, Informs, Mentions, \
                                 Motivates, Prerequisite, Refutes, RelatedTo, \
                                 Supports, Tests",
                                t
                            ),
                            None,
                        ));
                    }
                }
            }
            parsed
        }
    };

    let resolve_supersessions = params.resolve_supersessions.unwrap_or(false);

    // Step 2: Acquire graph (lock -> clone -> release before any async work).
    let graph = {
        let state = typed_graph_state
            .read()
            .unwrap_or_else(|e| e.into_inner());
        state.typed_graph.clone()
    };

    // Step 3: BFS state.
    let mut visited: HashSet<u64> = HashSet::new();
    let mut frontier: VecDeque<(NodeIndex, u64, u8)> = VecDeque::new();
    // (source_id, target_id, relation_type_str, depth)
    let mut collected_edges: Vec<(u64, u64, String, u8)> = Vec::new();
    let mut collected_node_ids: Vec<u64> = Vec::new();
    // dedup by canonical triple (source_id, target_id, rel_type)
    let mut edge_set: HashSet<(u64, u64, String)> = HashSet::new();
    let mut truncated = false;

    // Step 4: Seed phase.
    // Supersession resolution BEFORE visited check (R-01).
    for &seed_id in &seed_ids {
        let effective_id = if resolve_supersessions {
            follow_to_current(store, seed_id).await.unwrap_or(seed_id)
        } else {
            seed_id
        };

        if visited.contains(&effective_id) {
            continue;
        }

        if collected_node_ids.len() >= max_nodes_usize {
            // Seeds alone saturated max_nodes -- BFS must not run (R-03).
            truncated = true;
            break;
        }

        visited.insert(effective_id);
        collected_node_ids.push(effective_id);

        if let Some(node_idx) = graph.node_index_for(effective_id) {
            frontier.push_back((node_idx, effective_id, 0));
        }
        // Seeds absent from the graph are collected as nodes but skip BFS enqueue.
    }

    // Step 5: BFS phase.
    if !truncated {
        'bfs: while let Some((current_idx, _current_id, current_depth)) = frontier.pop_front() {
            if current_depth >= max_depth {
                continue;
            }

            for &rel_type in &edge_types {
                for &petgraph_dir in &petgraph_dirs {
                    // Collect edges eagerly to avoid borrow conflicts with async calls below
                    // (same pattern as graph_read_neighbors.rs, ADR-008).
                    let edge_pairs: Vec<(NodeIndex, NodeIndex)> = graph
                        .edges_of_type(current_idx, rel_type, petgraph_dir)
                        .map(|e| -> (NodeIndex, NodeIndex) { (e.source(), e.target()) })
                        .collect();

                    for (src_idx, tgt_idx) in edge_pairs {
                        // Resolve NodeIndex -> u64 using canonical stored direction (R-02).
                        let edge_src = match graph.node_id_for_index(src_idx) {
                            Some(id) => id,
                            None => continue,
                        };
                        let edge_tgt = match graph.node_id_for_index(tgt_idx) {
                            Some(id) => id,
                            None => continue,
                        };
                        let rel_type_str = rel_type.as_str().to_string();
                        let edge_key = (edge_src, edge_tgt, rel_type_str.clone());

                        // Neighbor from traversal perspective.
                        let neighbor_id = match petgraph_dir {
                            PetgraphDirection::Outgoing => edge_tgt,
                            PetgraphDirection::Incoming => edge_src,
                        };

                        // Supersession resolution on neighbor (R-01).
                        let effective_neighbor = if resolve_supersessions {
                            follow_to_current(store, neighbor_id)
                                .await
                                .unwrap_or(neighbor_id)
                        } else {
                            neighbor_id
                        };

                        // Dedup edge by canonical stored triple.
                        if edge_set.insert(edge_key) {
                            collected_edges.push((
                                edge_src,
                                edge_tgt,
                                rel_type_str,
                                current_depth + 1,
                            ));
                        }

                        // Enqueue neighbor if not yet visited.
                        if !visited.contains(&effective_neighbor) {
                            if collected_node_ids.len() >= max_nodes_usize {
                                truncated = true;
                                break 'bfs;
                            }
                            visited.insert(effective_neighbor);
                            collected_node_ids.push(effective_neighbor);
                            if let Some(neighbor_idx) = graph.node_index_for(effective_neighbor) {
                                frontier.push_back((
                                    neighbor_idx,
                                    effective_neighbor,
                                    current_depth + 1,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // Step 6: Dangling-edge filter (POST_BFS correctness -- R-05).
    // Remove edges whose source or target is not in collected_node_ids.
    let node_set: HashSet<u64> = collected_node_ids.iter().copied().collect();
    collected_edges.retain(|(src, tgt, _, _)| node_set.contains(src) && node_set.contains(tgt));

    // Step 7: Batch node hydration (single IN-clause query).
    let nodes: Vec<EntryRecord> = if collected_node_ids.is_empty() {
        Vec::new()
    } else {
        fetch_nodes_batch(store, &collected_node_ids).await?
    };

    // Step 8: Post-BFS metadata batch query.
    // Skipped entirely when collected_edges is empty (R-04: empty OR-chain guard).
    let metadata_map: HashMap<(u64, u64, String), Option<serde_json::Value>> =
        if collected_edges.is_empty() {
            HashMap::new()
        } else {
            fetch_edge_metadata(store, &collected_edges).await?
        };

    // Step 9: Compute depth_reached.
    let depth_reached: u8 = collected_edges.iter().map(|e| e.3).max().unwrap_or(0);

    // Step 10: Build EdgeRecord list.
    // direction is always "outgoing" -- canonical stored direction (FR-12, R-02, ADR-004).
    let edges: Vec<EdgeRecord> = collected_edges
        .iter()
        .map(|(src, tgt, rel_type, depth)| {
            let meta_key = (*src, *tgt, rel_type.clone());
            let metadata = metadata_map.get(&meta_key).cloned().flatten();
            EdgeRecord {
                source_id: *src,
                target_id: *tgt,
                relation_type: rel_type.clone(),
                direction: "outgoing".to_string(),
                depth: *depth,
                metadata,
            }
        })
        .collect();

    Ok(SubgraphResponse {
        nodes,
        edges,
        truncated,
        seed_ids,
        depth_reached,
    })
}

// ---------------------------------------------------------------------------
// Batch node hydration (Step 7)
// ---------------------------------------------------------------------------

/// Hydrates a batch of entry IDs to `EntryRecord`s in a single SQL query.
///
/// Uses an IN-clause with positional bindings. Returns entries in arbitrary order
/// (matching the store's natural order, not the input slice order). Tags are loaded
/// via a separate query per the store's ADR-006 tagging requirement.
async fn fetch_nodes_batch(
    store: &Store,
    node_ids: &[u64],
) -> Result<Vec<EntryRecord>, ErrorData> {
    use unimatrix_store::read::{ENTRY_COLUMNS, apply_tags, entry_from_row, load_tags_for_entries};

    let pool = store.read_pool_server();

    // Build IN-clause: (?, ?, ...) with one placeholder per ID.
    let placeholders: Vec<&str> = std::iter::repeat_n("?", node_ids.len()).collect();
    let sql = format!(
        "SELECT {} FROM entries WHERE id IN ({})",
        ENTRY_COLUMNS,
        placeholders.join(", ")
    );

    let mut query = sqlx::query(&sql);
    for &id in node_ids {
        query = query.bind(id as i64);
    }

    let rows = query.fetch_all(pool).await.map_err(|e| {
        ErrorData::new(ERROR_INTERNAL, format!("node batch query failed: {e}"), None)
    })?;

    let mut entries: Vec<EntryRecord> = rows
        .iter()
        .filter_map(|row| entry_from_row(row).ok())
        .collect();

    // Load tags (ADR-006, C-10: every code path building EntryRecord must call this).
    let tag_map = load_tags_for_entries(pool, node_ids)
        .await
        .map_err(|e| ErrorData::new(ERROR_INTERNAL, format!("tag load failed: {e}"), None))?;
    apply_tags(&mut entries, &tag_map);

    Ok(entries)
}

// ---------------------------------------------------------------------------
// Post-BFS metadata batch query (ADR-003, FR-14)
// ---------------------------------------------------------------------------

/// Fetches metadata for all collected edges in a single OR-chain SQL query.
///
/// Must only be called when `collected_edges` is non-empty (R-04).
/// Metadata deserialization: `serde_json::from_str(...).ok()` returns `None`
/// on malformed JSON without panic (SEC-05).
async fn fetch_edge_metadata(
    store: &Store,
    collected_edges: &[(u64, u64, String, u8)],
) -> Result<HashMap<(u64, u64, String), Option<serde_json::Value>>, ErrorData> {
    use sqlx::Row;

    let pool = store.read_pool_server();

    // Build OR-chain with SQLite ? positional placeholders.
    // Each edge contributes three bindings: source_id, target_id, relation_type.
    let clause = "(source_id = ? AND target_id = ? AND relation_type = ?)";
    let where_clauses: Vec<&str> = std::iter::repeat_n(clause, collected_edges.len()).collect();

    let sql = format!(
        "SELECT source_id, target_id, relation_type, metadata FROM graph_edges WHERE {}",
        where_clauses.join(" OR ")
    );

    let mut query = sqlx::query(&sql);
    for (src, tgt, rel_type, _) in collected_edges {
        query = query.bind(*src as i64).bind(*tgt as i64).bind(rel_type);
    }

    let rows = query.fetch_all(pool).await.map_err(|e| {
        ErrorData::new(ERROR_INTERNAL, format!("metadata query failed: {e}"), None)
    })?;

    let mut map: HashMap<(u64, u64, String), Option<serde_json::Value>> =
        HashMap::with_capacity(rows.len());

    for row in rows {
        let src: i64 = row.try_get("source_id").unwrap_or(0);
        let tgt: i64 = row.try_get("target_id").unwrap_or(0);
        let rel: String = row.try_get("relation_type").unwrap_or_default();
        let meta_text: Option<String> = row.try_get("metadata").ok().flatten();
        let meta_value: Option<serde_json::Value> =
            meta_text.as_deref().and_then(|s| serde_json::from_str(s).ok());
        map.insert((src as u64, tgt as u64, rel), meta_value);
    }

    Ok(map)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "graph_read_subgraph_tests.rs"]
mod tests;
