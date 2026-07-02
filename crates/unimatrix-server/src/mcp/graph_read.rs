//! context_graph mode handlers: chain, current, neighbors, subgraph (vnc-018, vnc-019),
//! inverse, filter, path (vnc-020).
//!
//! Entry point: `handle_graph` — called from `tools.rs` after capability check.
//! Submodules:
//! - `graph_read_supersession` — chain, current, follow_to_current
//! - `graph_read_neighbors`    — neighbors, neighbors_sql, neighbors_bfs
//! - `graph_read_subgraph`     — subgraph BFS traversal and metadata hydration (vnc-019)
//! - `graph_read_inverse`      — inverse antijoin SQL handler (vnc-020)
//! - `graph_read_filter`       — filter correlated subquery handler (vnc-020)
//! - `graph_read_path`         — path BFS shortest-path handler (vnc-020)
//!
//! # Execution order (ARCHITECTURE.md §Component Interactions, ADR-003)
//! 1. `require_cap(Read)` — runs in `tools.rs` BEFORE `handle_graph` is called.
//! 2. `validate_no_unsupported_params` — inside `handle_graph`, before mode dispatch.
//! 3. Mode dispatch.
//!
//! # Key constraints
//! - chain and current modes use SQL recursive CTEs (ADR-001).
//!   `find_terminal_active` (in-memory) is PROHIBITED.
//! - `EdgeRecord.metadata` serializes as JSON `null` — no `skip_serializing_if` (ADR-004, R-15).
//! - BFS visited set is `HashSet<u64>` keyed by `node_id` only (AC-11a, R-18).
//! - `TypedGraphStateHandle` wraps `std::sync::RwLock` — use `.read().unwrap_or_else(...)`.

use std::sync::{Arc, RwLock};

use rmcp::model::{CallToolResult, ErrorData};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use unimatrix_core::{EntryRecord, Store};

use crate::error::{ERROR_INTERNAL, ERROR_INVALID_PARAMS};
use crate::mcp::context::ToolContext;
use crate::services::typed_graph::TypedGraphState;

#[path = "graph_read_supersession.rs"]
mod graph_read_supersession;

#[path = "graph_read_neighbors.rs"]
mod graph_read_neighbors;

#[path = "graph_read_subgraph.rs"]
mod graph_read_subgraph;

#[path = "graph_read_inverse.rs"]
mod graph_read_inverse;

#[path = "graph_read_filter.rs"]
mod graph_read_filter;

#[path = "graph_read_path.rs"]
mod graph_read_path;

#[path = "graph_read_validation.rs"]
mod graph_read_validation;

// vnc-042: re-export the canonical supersession-resolution helper so the tools.rs
// handler can bind it at `crate::mcp::graph_read::follow_to_current` (Pattern #4436).
// Mirrors the submodule-symbol re-export idiom used for `EdgeRecord` in mcp/mod.rs.
// NOTE: canonical copy is graph_read_neighbors.rs — NOT the graph_read_supersession.rs duplicate.
pub(crate) use graph_read_neighbors::follow_to_current;

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
    /// neighbors / subgraph / path: resolve deprecated endpoints to their active terminal.
    /// Default TRUE (bugfix-881, aligning with context_get) — pass `false` for raw as-stored
    /// topology (audit). Rejected on chain/current/inverse/filter modes (ADR-003, #616A).
    pub resolve_supersessions: Option<bool>,
    // -- Forward-compat fields — error on misuse in incompatible modes (ADR-003) --
    /// subgraph mode: one or more entry IDs to use as BFS seeds.
    pub seed_ids: Option<Vec<u64>>,
    /// subgraph mode: maximum nodes to return, 1..=200 (default 200).
    pub max_nodes: Option<u32>,
    /// path mode (#598) — not yet supported.
    pub from_id: Option<u64>,
    /// path mode (#598) — not yet supported.
    pub to_id: Option<u64>,
    /// subgraph mode only: BFS max depth 1..=10 (default 3 when absent).
    /// Error if passed to chain, current, or neighbors modes (ADR-001 vnc-019).
    pub max_depth: Option<u8>,
    // -- vnc-020 fields: inverse and filter modes (ADR-002 vnc-020) --
    /// inverse and filter: entry category filter (required for both modes).
    /// REJECTED on all other modes.
    #[serde(default)]
    pub category: Option<String>,
    /// inverse only: edge type(s) whose absence is tested (required, non-empty).
    /// REJECTED on all other modes. Do not confuse with edge_types.
    #[serde(default)]
    pub missing_edge_types: Option<Vec<String>>,
    /// inverse and filter: max entries to return (default 100, range [1, 500]).
    /// REJECTED on all other modes.
    #[serde(default)]
    pub limit: Option<u32>,
    /// filter only: entries where created_at <= (NOW - N days).
    #[serde(default)]
    pub min_age_days: Option<u32>,
    /// filter only: entries where confidence >= N.
    #[serde(default)]
    pub min_confidence: Option<f64>,
    /// filter only: entries where confidence <= N.
    #[serde(default)]
    pub max_confidence: Option<f64>,
    /// filter only: entries with at least this many outgoing edges of edge_types.
    /// Requires edge_types to be present and non-empty.
    #[serde(default)]
    pub min_edge_count: Option<u32>,
    /// filter only: entries with at most this many outgoing edges of edge_types.
    /// max_edge_count=0 is valid: returns entries with zero matching edges.
    /// Requires edge_types to be present and non-empty.
    #[serde(default)]
    pub max_edge_count: Option<u32>,
}

/// A single typed edge from a neighbors traversal (ADR-004, vnc-018).
///
/// `metadata` always serializes as JSON `null` in vnc-018.
/// `skip_serializing_if` is PROHIBITED on this field (ADR-004, R-15).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Truncated {
    pub forward: bool,
    pub backward: bool,
}

/// Response envelope for chain mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainResult {
    pub entries: Vec<EntryRecord>,
    pub truncated: Truncated,
}

/// Response envelope for current mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentResponse {
    pub entry: EntryRecord,
}

/// Response envelope for neighbors mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborsResponse {
    pub edges: Vec<EdgeRecord>,
}

/// Response envelope for subgraph mode (vnc-019, ADR-001, ADR-004).
///
/// `direction` on every `EdgeRecord` is always `"outgoing"` — canonical stored direction
/// (`source_id → target_id`). See FR-12, ADR-004 vnc-018.
///
/// `truncated: true` means the `max_nodes` cap was reached before BFS completed.
/// `depth_reached`: actual maximum BFS depth traversed (0 when no edges discovered).
#[derive(Debug, Clone, Serialize)]
pub struct SubgraphResponse {
    pub nodes: Vec<EntryRecord>,
    pub edges: Vec<EdgeRecord>,
    pub truncated: bool,
    pub seed_ids: Vec<u64>,
    pub depth_reached: u8,
}

/// Response envelope for inverse mode (vnc-020).
#[derive(Debug, Clone, Serialize)]
pub struct InverseResponse {
    pub entries: Vec<EntryRecord>,
    pub total_returned: usize,
}

/// Response envelope for filter mode (vnc-020).
#[derive(Debug, Clone, Serialize)]
pub struct FilterResponse {
    pub entries: Vec<EntryRecord>,
    pub total_returned: usize,
}

/// A single hop in a path traversal result (vnc-020, ADR-005).
///
/// `relation_type` is never null — always one of the 16 RelationType variant name strings.
/// `from_id` is NOT a PathHop — it is a top-level PathResponse field.
#[derive(Debug, Clone, Serialize)]
pub struct PathHop {
    pub entry_id: u64,
    pub relation_type: String,
}

/// Response envelope for path mode (vnc-020, ADR-005).
///
/// `found=false` when: (a) no path within depth hops, or (b) from_id/to_id absent from
/// the current graph snapshot. Both are valid results, NOT errors (FR-13, AC-14, AC-15).
/// `length` always equals `hops.len()`.
/// `from_id` and `to_id` are resolved IDs when `resolve_supersessions=true` (ADR-006).
#[derive(Debug, Clone, Serialize)]
pub struct PathResponse {
    pub found: bool,
    pub from_id: u64,
    pub to_id: u64,
    pub hops: Vec<PathHop>,
    pub length: u8,
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
        return Err(ErrorData::new(ERROR_INVALID_PARAMS, msg, None));
    }

    // Step 2: Mode dispatch.
    // NOTE: subgraph mode uses `seed_ids` rather than `id`; the id-required guard
    // lives inside the chain/current/neighbors arm only.
    match params.mode.as_str() {
        "chain" | "current" | "neighbors" => {
            // All three point-lookup modes require an anchor entry ID.
            let id = match params.id {
                Some(id) => id,
                None => {
                    return Err(ErrorData::new(
                        ERROR_INVALID_PARAMS,
                        "id is required for chain, current, and neighbors modes",
                        None,
                    ));
                }
            };

            match params.mode.as_str() {
                "chain" => {
                    let result = graph_read_supersession::handle_chain(store, &params, id).await?;
                    let json = serde_json::to_string(&result).map_err(|e| {
                        ErrorData::new(ERROR_INTERNAL, format!("serialization error: {e}"), None)
                    })?;
                    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                        json,
                    )]))
                }
                "current" => match graph_read_supersession::handle_current(store, id).await {
                    Ok(resp) => {
                        let json = serde_json::to_string(&resp).map_err(|e| {
                            ErrorData::new(
                                ERROR_INTERNAL,
                                format!("serialization error: {e}"),
                                None,
                            )
                        })?;
                        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                            json,
                        )]))
                    }
                    Err(msg) => Err(ErrorData::new(ERROR_INVALID_PARAMS, msg, None)),
                },
                "neighbors" => {
                    let result = graph_read_neighbors::handle_neighbors(
                        store,
                        typed_graph_state,
                        &params,
                        id,
                    )
                    .await?;
                    let json = serde_json::to_string(&result).map_err(|e| {
                        ErrorData::new(ERROR_INTERNAL, format!("serialization error: {e}"), None)
                    })?;
                    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                        json,
                    )]))
                }
                _ => unreachable!(
                    "validate_no_unsupported_params already caught this before dispatch"
                ),
            }
        }
        "subgraph" => {
            // subgraph mode uses seed_ids, not id. No anchor ID required.
            let result =
                graph_read_subgraph::handle_subgraph(store, typed_graph_state, &params).await?;
            let json = serde_json::to_string(&result).map_err(|e| {
                ErrorData::new(ERROR_INTERNAL, format!("serialization error: {e}"), None)
            })?;
            Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                json,
            )]))
        }
        "inverse" => {
            // Pure SQL antijoin — no graph state needed.
            let result = graph_read_inverse::handle_inverse(store, &params).await?;
            let json = serde_json::to_string(&result).map_err(|e| {
                ErrorData::new(ERROR_INTERNAL, format!("serialization error: {e}"), None)
            })?;
            Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                json,
            )]))
        }
        "filter" => {
            // Pure SQL correlated subquery — no graph state needed.
            let result = graph_read_filter::handle_filter(store, &params).await?;
            let json = serde_json::to_string(&result).map_err(|e| {
                ErrorData::new(ERROR_INTERNAL, format!("serialization error: {e}"), None)
            })?;
            Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                json,
            )]))
        }
        "path" => {
            // In-memory BFS — requires typed_graph_state.
            let result = graph_read_path::handle_path(store, typed_graph_state, &params).await?;
            let json = serde_json::to_string(&result).map_err(|e| {
                ErrorData::new(ERROR_INTERNAL, format!("serialization error: {e}"), None)
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
/// mode dispatch. Delegates to `graph_read_validation` (kept separate per C5 line limit).
///
/// This function is the single contract point for all cross-mode rejection logic.
/// See `graph_read_validation.rs` for the full implementation.
pub(crate) fn validate_no_unsupported_params(params: &GraphParams) -> Result<(), String> {
    graph_read_validation::validate_no_unsupported_params(params)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "graph_read_tests.rs"]
mod tests;
