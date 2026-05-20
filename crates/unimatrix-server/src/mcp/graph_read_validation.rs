//! Centralized parameter validation for context_graph modes (vnc-018, vnc-019, vnc-020).
//!
//! This module is declared via `#[path]` in `graph_read.rs` and kept separate to
//! respect the 500-line file limit (C5). All functions are `pub(super)` — callers go
//! through `graph_read.rs`.
//!
//! # Validation contract (R-04, ADR-003 vnc-018)
//! 1. Unrecognized mode error fires BEFORE any field check.
//! 2. Per-mode field rejection errors follow in declaration order.
//! 3. Range / required-field validation is deferred to the handler (not here).

use super::GraphParams;

// ---------------------------------------------------------------------------
// Public entry point (re-exported from graph_read.rs)
// ---------------------------------------------------------------------------

/// Centralized parameter validation. Called at the top of `handle_graph` before
/// mode dispatch. This function is the single contract point for:
/// - Unrecognized mode (fires BEFORE any field check — R-04).
/// - Forward-compat fields on unsupported modes (with future-mode hint).
/// - `resolve_supersessions=Some(true)` on chain mode (semantically circular, R-08).
/// - `max_depth` rejected on chain/current/neighbors modes (ADR-001 vnc-019).
/// - `depth` rejected on chain/current/subgraph/inverse/filter modes (ADR-004 vnc-020, FR-17).
/// - 8 new vnc-020 fields rejected on non-owning modes (ADR-002 vnc-020, R-04, SR-08).
pub(super) fn validate_no_unsupported_params(params: &GraphParams) -> Result<(), String> {
    match params.mode.as_str() {
        "chain" => validate_chain_params(params),
        "current" => validate_current_params(params),
        "neighbors" => validate_neighbors_params(params),
        "subgraph" => validate_subgraph_params(params),
        "inverse" => validate_inverse_params(params),
        "filter" => validate_filter_params(params),
        "path" => validate_path_params(params),
        // _ arm fires BEFORE field checks — unrecognized mode error is the first thing
        // callers see (R-04).
        _ => Err(format!(
            "unrecognized mode '{}' — supported modes: chain, current, neighbors, subgraph, inverse, filter, path",
            params.mode
        )),
    }
}

// ---------------------------------------------------------------------------
// Per-mode validation helpers
// ---------------------------------------------------------------------------

fn validate_chain_params(params: &GraphParams) -> Result<(), String> {
    if params.seed_ids.is_some() {
        return Err("seed_ids is not supported in chain mode — use subgraph mode".to_string());
    }
    if params.max_nodes.is_some() {
        return Err("max_nodes is not supported in chain mode — use subgraph mode".to_string());
    }
    if params.from_id.is_some() {
        return Err("from_id is not supported in chain mode — use path mode".to_string());
    }
    if params.to_id.is_some() {
        return Err("to_id is not supported in chain mode — use path mode".to_string());
    }
    // resolve_supersessions on chain is semantically circular (FR-08, AC-15c, R-08):
    // chain IS the supersession audit. This check must live here, not in handle_chain.
    if params.resolve_supersessions == Some(true) {
        return Err(
            "resolve_supersessions is not applicable to chain mode — chain IS the supersession audit".to_string()
        );
    }
    // max_depth is subgraph-only (ADR-001 vnc-019).
    if params.max_depth.is_some() {
        return Err("max_depth is not supported in chain mode — use subgraph mode".to_string());
    }
    // depth accepted only by neighbors and path modes (ADR-004 vnc-020, FR-17).
    if params.depth.is_some() {
        return Err(
            "depth is not supported in chain mode — use neighbors or path mode".to_string(),
        );
    }
    reject_new_fields_for_mode(params, "chain")
}

fn validate_current_params(params: &GraphParams) -> Result<(), String> {
    if params.seed_ids.is_some() {
        return Err("seed_ids is not supported in current mode — use subgraph mode".to_string());
    }
    if params.max_nodes.is_some() {
        return Err("max_nodes is not supported in current mode — use subgraph mode".to_string());
    }
    if params.from_id.is_some() {
        return Err("from_id is not supported in current mode — use path mode".to_string());
    }
    if params.to_id.is_some() {
        return Err("to_id is not supported in current mode — use path mode".to_string());
    }
    if params.max_depth.is_some() {
        return Err("max_depth is not supported in current mode — use subgraph mode".to_string());
    }
    if params.depth.is_some() {
        return Err(
            "depth is not supported in current mode — use neighbors or path mode".to_string(),
        );
    }
    reject_new_fields_for_mode(params, "current")
}

fn validate_neighbors_params(params: &GraphParams) -> Result<(), String> {
    if params.seed_ids.is_some() {
        return Err("seed_ids is not supported in neighbors mode — use subgraph mode".to_string());
    }
    if params.max_nodes.is_some() {
        return Err("max_nodes is not supported in neighbors mode — use subgraph mode".to_string());
    }
    if params.from_id.is_some() {
        return Err("from_id is not supported in neighbors mode — use path mode".to_string());
    }
    if params.to_id.is_some() {
        return Err("to_id is not supported in neighbors mode — use path mode".to_string());
    }
    if params.max_depth.is_some() {
        return Err("max_depth is not supported in neighbors mode — use subgraph mode".to_string());
    }
    // depth: accepted by neighbors mode — do NOT add rejection here.
    reject_new_fields_for_mode(params, "neighbors")
}

/// subgraph mode: permits seed_ids, max_nodes, max_depth.
/// Rejects from_id, to_id (path mode only — preserved forward-compat guard).
/// Range validation for max_depth/max_nodes happens inside handle_subgraph.
fn validate_subgraph_params(params: &GraphParams) -> Result<(), String> {
    if params.from_id.is_some() {
        return Err("from_id is not supported in subgraph mode — use path mode".to_string());
    }
    if params.to_id.is_some() {
        return Err("to_id is not supported in subgraph mode — use path mode".to_string());
    }
    if params.depth.is_some() {
        return Err(
            "depth is not supported in subgraph mode — use neighbors or path mode".to_string(),
        );
    }
    // seed_ids, max_nodes, max_depth: permitted — range validation inside handle_subgraph.
    reject_new_fields_for_mode(params, "subgraph")
}

/// Validate params for inverse mode.
/// Permits: category, missing_edge_types, limit.
/// Rejects: edge_types (use missing_edge_types — AC-03a), depth, from_id/to_id, subgraph
/// params, and all filter-only fields.
fn validate_inverse_params(params: &GraphParams) -> Result<(), String> {
    if params.edge_types.is_some() {
        return Err(
            "edge_types is not supported in inverse mode — use missing_edge_types instead"
                .to_string(),
        );
    }
    if params.depth.is_some() {
        return Err(
            "depth is not supported in inverse mode — use neighbors or path mode".to_string(),
        );
    }
    if params.from_id.is_some() {
        return Err("from_id is not supported in inverse mode — use path mode".to_string());
    }
    if params.to_id.is_some() {
        return Err("to_id is not supported in inverse mode — use path mode".to_string());
    }
    if params.seed_ids.is_some() {
        return Err("seed_ids is not supported in inverse mode — use subgraph mode".to_string());
    }
    if params.max_nodes.is_some() {
        return Err("max_nodes is not supported in inverse mode — use subgraph mode".to_string());
    }
    if params.max_depth.is_some() {
        return Err("max_depth is not supported in inverse mode — use subgraph mode".to_string());
    }
    if params.min_age_days.is_some() {
        return Err("min_age_days is not supported in inverse mode — use filter mode".to_string());
    }
    if params.min_confidence.is_some() {
        return Err(
            "min_confidence is not supported in inverse mode — use filter mode".to_string(),
        );
    }
    if params.max_confidence.is_some() {
        return Err(
            "max_confidence is not supported in inverse mode — use filter mode".to_string(),
        );
    }
    if params.min_edge_count.is_some() {
        return Err(
            "min_edge_count is not supported in inverse mode — use filter mode".to_string(),
        );
    }
    if params.max_edge_count.is_some() {
        return Err(
            "max_edge_count is not supported in inverse mode — use filter mode".to_string(),
        );
    }
    // resolve_supersessions has no meaning in inverse mode: only active entries (status=0)
    // are returned by definition — there are no deprecated entries to resolve.
    if params.resolve_supersessions.is_some() {
        return Err(
            "resolve_supersessions has no effect in inverse mode — only active entries are returned"
                .to_string(),
        );
    }
    // category, missing_edge_types, limit: accepted — range validation inside handle_inverse.
    Ok(())
}

/// Validate params for filter mode.
/// Permits: category, edge_types, limit, min_age_days, min_confidence, max_confidence,
///          min_edge_count, max_edge_count.
/// Rejects: depth, from_id/to_id, missing_edge_types (use edge_types), subgraph params.
fn validate_filter_params(params: &GraphParams) -> Result<(), String> {
    if params.depth.is_some() {
        return Err(
            "depth is not supported in filter mode — use neighbors or path mode".to_string(),
        );
    }
    if params.from_id.is_some() {
        return Err("from_id is not supported in filter mode — use path mode".to_string());
    }
    if params.to_id.is_some() {
        return Err("to_id is not supported in filter mode — use path mode".to_string());
    }
    if params.missing_edge_types.is_some() {
        return Err(
            "missing_edge_types is not supported in filter mode — use inverse mode".to_string(),
        );
    }
    if params.seed_ids.is_some() {
        return Err("seed_ids is not supported in filter mode — use subgraph mode".to_string());
    }
    if params.max_nodes.is_some() {
        return Err("max_nodes is not supported in filter mode — use subgraph mode".to_string());
    }
    if params.max_depth.is_some() {
        return Err("max_depth is not supported in filter mode — use subgraph mode".to_string());
    }
    // resolve_supersessions has no meaning in filter mode: only active entries (status=0)
    // are returned by definition — there are no deprecated entries to resolve.
    if params.resolve_supersessions.is_some() {
        return Err(
            "resolve_supersessions has no effect in filter mode — only active entries are returned"
                .to_string(),
        );
    }
    // category, edge_types, limit, min_age_days, min_confidence, max_confidence,
    // min_edge_count, max_edge_count: accepted — range/required validation inside handle_filter.
    Ok(())
}

/// Validate params for path mode.
/// Permits: from_id, to_id, depth, edge_types, resolve_supersessions.
/// Rejects: subgraph params, id (use from_id/to_id), inverse/filter-only fields.
fn validate_path_params(params: &GraphParams) -> Result<(), String> {
    if params.seed_ids.is_some() {
        return Err("seed_ids is not supported in path mode — use subgraph mode".to_string());
    }
    if params.max_nodes.is_some() {
        return Err("max_nodes is not supported in path mode — use subgraph mode".to_string());
    }
    if params.max_depth.is_some() {
        return Err("max_depth is not supported in path mode — use subgraph mode".to_string());
    }
    if params.id.is_some() {
        return Err("id is not supported in path mode — use from_id and to_id".to_string());
    }
    if params.category.is_some() {
        return Err(
            "category is not supported in path mode — use inverse or filter mode".to_string(),
        );
    }
    if params.missing_edge_types.is_some() {
        return Err(
            "missing_edge_types is not supported in path mode — use inverse mode".to_string(),
        );
    }
    if params.limit.is_some() {
        return Err("limit is not supported in path mode — use inverse or filter mode".to_string());
    }
    if params.min_age_days.is_some() {
        return Err("min_age_days is not supported in path mode — use filter mode".to_string());
    }
    if params.min_confidence.is_some() {
        return Err("min_confidence is not supported in path mode — use filter mode".to_string());
    }
    if params.max_confidence.is_some() {
        return Err("max_confidence is not supported in path mode — use filter mode".to_string());
    }
    if params.min_edge_count.is_some() {
        return Err("min_edge_count is not supported in path mode — use filter mode".to_string());
    }
    if params.max_edge_count.is_some() {
        return Err("max_edge_count is not supported in path mode — use filter mode".to_string());
    }
    // from_id, to_id, depth, edge_types, resolve_supersessions: accepted.
    // Range/required validation happens inside handle_path.
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared rejection helper
// ---------------------------------------------------------------------------

/// Reject all 8 vnc-020 new fields for modes that do not own any of them
/// (chain, current, neighbors, subgraph).
///
/// Called as the final check in each of those mode arms. Centralizes the 8-field
/// rejection to keep per-arm code compact (C5).
fn reject_new_fields_for_mode(params: &GraphParams, mode: &str) -> Result<(), String> {
    if params.category.is_some() {
        return Err(format!(
            "category is not supported in {mode} mode — use inverse or filter mode"
        ));
    }
    if params.missing_edge_types.is_some() {
        return Err(format!(
            "missing_edge_types is not supported in {mode} mode — use inverse mode"
        ));
    }
    if params.limit.is_some() {
        return Err(format!(
            "limit is not supported in {mode} mode — use inverse or filter mode"
        ));
    }
    if params.min_age_days.is_some() {
        return Err(format!(
            "min_age_days is not supported in {mode} mode — use filter mode"
        ));
    }
    if params.min_confidence.is_some() {
        return Err(format!(
            "min_confidence is not supported in {mode} mode — use filter mode"
        ));
    }
    if params.max_confidence.is_some() {
        return Err(format!(
            "max_confidence is not supported in {mode} mode — use filter mode"
        ));
    }
    if params.min_edge_count.is_some() {
        return Err(format!(
            "min_edge_count is not supported in {mode} mode — use filter mode"
        ));
    }
    if params.max_edge_count.is_some() {
        return Err(format!(
            "max_edge_count is not supported in {mode} mode — use filter mode"
        ));
    }
    Ok(())
}
