//! graph_read_inverse — antijoin SQL handler for inverse mode (vnc-020).
//!
//! `handle_inverse` finds entries of a given category that have NO incoming edges
//! of ALL the specified `missing_edge_types` (AND semantics, ADR-003 vnc-020).
//!
//! This module is purely SQL — no in-memory graph access, no staleness concern.
//! Declared as a sub-module of `graph_read.rs` via `#[path]`.

use rmcp::model::ErrorData;
use sqlx::QueryBuilder;
use unimatrix_core::Store;
use unimatrix_engine::graph::RelationType;

use crate::error::{ERROR_INTERNAL, ERROR_INVALID_PARAMS};

use super::{GraphParams, InverseResponse};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 500;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Handle inverse mode: return active entries of a given category that have no
/// incoming edges of ALL the specified `missing_edge_types` (AND semantics).
///
/// SQL strategy: one LEFT JOIN per missing edge type (aliased g0, g1, ...).
/// WHERE checks that every aliased join produced no match (target_id IS NULL).
/// Only active entries are returned (`e.status = 0`).
pub(super) async fn handle_inverse(
    store: &Store,
    params: &GraphParams,
) -> Result<InverseResponse, ErrorData> {
    // Step 1: Validate category (required).
    let category: &str = match params.category.as_deref() {
        Some(c) if !c.is_empty() => c,
        _ => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                "inverse mode requires category",
                None,
            ));
        }
    };

    // Step 2: Validate missing_edge_types (required, non-empty).
    let raw_types: &[String] = match &params.missing_edge_types {
        Some(v) if !v.is_empty() => v.as_slice(),
        _ => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                "inverse mode requires at least one edge type in missing_edge_types",
                None,
            ));
        }
    };

    // Parse each element via RelationType::from_str (AC-02, SR-B — injection prevention).
    let edge_types: Vec<RelationType> = parse_relation_types(raw_types)?;

    // Step 3: Validate limit (default 100, range [1, 500]).
    let limit: u32 = match params.limit {
        None => DEFAULT_LIMIT,
        Some(n) if (1..=MAX_LIMIT).contains(&n) => n,
        Some(n) => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                format!("limit must be in range 1..=500, got {n}"),
                None,
            ));
        }
    };

    // Step 4: Build parameterized antijoin SQL via QueryBuilder.
    //
    // SQL structure (N=len(edge_types) LEFT JOINs):
    //   SELECT <columns>
    //   FROM entries e
    //   LEFT JOIN graph_edges g0 ON e.id = g0.target_id AND g0.relation_type = ?
    //   LEFT JOIN graph_edges g1 ON e.id = g1.target_id AND g1.relation_type = ?
    //   ...
    //   WHERE e.category = ? AND e.status = 0
    //     AND g0.target_id IS NULL AND g1.target_id IS NULL ...
    //   LIMIT ?
    //
    // Alias names (g0, g1, ...) are ONLY generated from the loop counter — never
    // from caller input (SR-B). The composite index idx_graph_edges_target_type
    // (target_id, relation_type) is used by each LEFT JOIN condition (schema v27).
    let mut qb = QueryBuilder::new(
        "SELECT e.id, e.title, e.content, e.topic, e.category, e.source, e.status, \
         e.confidence, e.created_at, e.updated_at, e.last_accessed_at, e.access_count, \
         e.supersedes, e.superseded_by, e.correction_count, e.embedding_dim, \
         e.created_by, e.modified_by, e.content_hash, e.previous_hash, \
         e.version, e.feature_cycle, e.trust_source, e.helpful_count, e.unhelpful_count, \
         e.pre_quarantine_status \
         FROM entries e",
    );

    // One LEFT JOIN per missing edge type.
    for (i, rel_type) in edge_types.iter().enumerate() {
        qb.push(format!(
            " LEFT JOIN graph_edges g{i} ON e.id = g{i}.target_id AND g{i}.relation_type = "
        ));
        qb.push_bind(rel_type.as_str());
    }

    // WHERE clause — category + active status filter (R-10).
    qb.push(" WHERE e.category = ");
    qb.push_bind(category);
    qb.push(" AND e.status = 0");

    // NULL checks for each aliased join — AND semantics (ADR-003 vnc-020).
    for i in 0..edge_types.len() {
        qb.push(format!(" AND g{i}.target_id IS NULL"));
    }

    // LIMIT bound as parameter (pattern #4058).
    qb.push(" LIMIT ");
    qb.push_bind(limit as i64);

    // Step 5: Execute query and hydrate EntryRecord rows.
    // EntryRecord does not implement sqlx::FromRow — use entry_from_row for hydration.
    use unimatrix_store::read::{apply_tags, entry_from_row, load_tags_for_entries};

    let pool = store.read_pool_server();
    let rows = qb.build().fetch_all(pool).await.map_err(|e| {
        ErrorData::new(ERROR_INTERNAL, format!("inverse mode SQL error: {e}"), None)
    })?;

    let mut entries: Vec<unimatrix_core::EntryRecord> = rows
        .iter()
        .filter_map(|row| entry_from_row(row).ok())
        .collect();

    // Load tags — every code path building EntryRecord MUST call this (ADR-006, C-10).
    if !entries.is_empty() {
        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(pool, &ids).await.map_err(|e| {
            ErrorData::new(
                ERROR_INTERNAL,
                format!("inverse mode tag load failed: {e}"),
                None,
            )
        })?;
        apply_tags(&mut entries, &tag_map);
    }

    // Step 6: Return response.
    let total_returned = entries.len();
    Ok(InverseResponse {
        entries,
        total_returned,
    })
}

// ---------------------------------------------------------------------------
// Helper: parse RelationType strings
// ---------------------------------------------------------------------------

/// Parse a slice of raw edge-type strings into `RelationType` values.
///
/// Returns an `ERROR_INVALID_PARAMS` error naming the first unrecognized string
/// and listing all 16 recognized types. Validates before any SQL construction,
/// so injection via a crafted type string is impossible (SR-B).
pub(super) fn parse_relation_types(raw: &[String]) -> Result<Vec<RelationType>, ErrorData> {
    let mut out = Vec::with_capacity(raw.len());
    for t in raw {
        match RelationType::from_str(t) {
            Some(rt) => out.push(rt),
            None => {
                return Err(ErrorData::new(
                    ERROR_INVALID_PARAMS,
                    format!(
                        "unrecognized edge type '{t}' — recognized types: \
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
    Ok(out)
}

// Tests
// ---------------------------------------------------------------------------

/// Tests extracted to a separate file to stay within the 500-line file limit.
#[cfg(test)]
#[path = "graph_read_inverse_tests.rs"]
mod tests;
