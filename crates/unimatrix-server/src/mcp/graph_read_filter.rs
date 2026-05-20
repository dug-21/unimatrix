//! graph_read_filter — correlated subquery handler for filter mode (vnc-020).
//!
//! `handle_filter` builds a parameterized correlated subquery to find entries matching
//! a category and optional property + edge-count constraints. All SQL is constructed
//! from typed `GraphParams` fields bound as sqlx parameters — no raw SQL from callers
//! (ADR-007, C9).
//!
//! Declared as a sub-module of `graph_read.rs` via `#[path]`.

use rmcp::model::ErrorData;
use sqlx::QueryBuilder;
use unimatrix_core::Store;
use unimatrix_engine::graph::RelationType;

use crate::error::{ERROR_INTERNAL, ERROR_INVALID_PARAMS};

use super::{FilterResponse, GraphParams};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 500;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Handle filter mode: return entries matching category + optional property + edge-count
/// constraints via parameterized correlated subquery SQL (ADR-007 vnc-020).
pub(super) async fn handle_filter(
    store: &Store,
    params: &GraphParams,
) -> Result<FilterResponse, ErrorData> {
    // Step 1: Validate category (required).
    let category: &str = match params.category.as_deref() {
        Some(c) if !c.is_empty() => c,
        _ => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                "filter mode requires category",
                None,
            ));
        }
    };

    // Step 2: Validate edge_types (required when edge-count constraints are present).
    let has_edge_count = params.min_edge_count.is_some() || params.max_edge_count.is_some();

    let edge_types: Option<Vec<RelationType>> = match &params.edge_types {
        None => {
            if has_edge_count {
                return Err(ErrorData::new(
                    ERROR_INVALID_PARAMS,
                    "filter mode requires edge_types when edge_count constraints are specified",
                    None,
                ));
            }
            None
        }
        Some(v) if v.is_empty() => {
            if has_edge_count {
                return Err(ErrorData::new(
                    ERROR_INVALID_PARAMS,
                    "filter mode requires edge_types when edge_count constraints are specified",
                    None,
                ));
            }
            None
        }
        Some(types) => Some(parse_relation_types(types)?),
    };

    // Step 3: Validate limit (default 100, range [1, 500]).
    let limit: u32 = match params.limit {
        None => DEFAULT_LIMIT,
        Some(n) if n >= 1 && n <= MAX_LIMIT => n,
        Some(n) => {
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                format!("limit must be in range 1..=500, got {n}"),
                None,
            ));
        }
    };

    // Step 4: Build parameterized correlated subquery SQL.
    // All caller values bound via push_bind — no string interpolation (ADR-007, NFR-06, SR-A).

    // 4a. Base query: active entries in the specified category.
    let mut qb: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
        "SELECT e.id, e.title, e.content, e.topic, e.category, e.source, e.status, \
         e.confidence, e.created_at, e.updated_at, e.last_accessed_at, e.access_count, \
         e.supersedes, e.superseded_by, e.correction_count, e.embedding_dim, \
         e.created_by, e.modified_by, e.content_hash, e.previous_hash, \
         e.version, e.feature_cycle, e.trust_source, e.helpful_count, e.unhelpful_count, \
         e.pre_quarantine_status \
         FROM entries e \
         WHERE e.category = ",
    );
    qb.push_bind(category);
    qb.push(" AND e.status = 0");

    // 4b. Optional: min_age_days — entries created at least N days ago.
    // created_at is INTEGER (Unix epoch seconds). Use integer arithmetic, NOT datetime().
    if let Some(days) = params.min_age_days {
        // CAST(strftime('%s','now') AS INTEGER) returns current epoch as integer.
        // days * 86400 is seconds in N days (u32 * 86400 fits in i64).
        qb.push(" AND e.created_at < (CAST(strftime('%s','now') AS INTEGER) - ");
        qb.push_bind(days as i64 * 86_400_i64);
        qb.push(")");
    }

    // 4c. Optional: min_confidence — entries where confidence >= N.
    if let Some(min_c) = params.min_confidence {
        qb.push(" AND e.confidence >= ");
        qb.push_bind(min_c);
    }

    // 4d. Optional: max_confidence — entries where confidence <= N.
    if let Some(max_c) = params.max_confidence {
        qb.push(" AND e.confidence <= ");
        qb.push_bind(max_c);
    }

    // 4e. Optional: min_edge_count — entries with >= N outgoing edges of edge_types.
    // edge_types is guaranteed Some and non-empty when has_edge_count=true (Step 2 guard).
    if let Some(min_n) = params.min_edge_count {
        let et = edge_types.as_ref().expect("edge_types guaranteed non-empty by Step 2 validation");
        qb.push(
            " AND (SELECT COUNT(*) FROM graph_edges g \
             WHERE g.source_id = e.id AND g.relation_type IN (",
        );
        push_relation_type_list(&mut qb, et);
        qb.push(")) >= ");
        qb.push_bind(min_n as i64);
    }

    // 4f. Optional: max_edge_count — entries with <= N outgoing edges of edge_types.
    // CRITICAL: max_edge_count=0 is valid and must work correctly (R-02).
    // Use `<= ?` unconditionally — never special-case zero.
    // Two SEPARATE subqueries when both min and max are present (R-08 — not a BETWEEN).
    if let Some(max_n) = params.max_edge_count {
        let et = edge_types.as_ref().expect("edge_types guaranteed non-empty by Step 2 validation");
        qb.push(
            " AND (SELECT COUNT(*) FROM graph_edges g \
             WHERE g.source_id = e.id AND g.relation_type IN (",
        );
        push_relation_type_list(&mut qb, et);
        qb.push(")) <= ");
        qb.push_bind(max_n as i64);
    }

    // 4g. LIMIT clause.
    qb.push(" LIMIT ");
    qb.push_bind(limit as i64);

    // Step 5: Execute query and hydrate EntryRecord rows.
    // EntryRecord does not implement sqlx::FromRow — use entry_from_row for hydration.
    use unimatrix_store::read::{apply_tags, entry_from_row, load_tags_for_entries};

    let pool = store.read_pool_server();
    let rows =
        qb.build().fetch_all(pool).await.map_err(|e| {
            ErrorData::new(ERROR_INTERNAL, format!("filter mode SQL error: {e}"), None)
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
                format!("filter mode tag load failed: {e}"),
                None,
            )
        })?;
        apply_tags(&mut entries, &tag_map);
    }

    // Step 6: Return response.
    let total_returned = entries.len();
    Ok(FilterResponse {
        entries,
        total_returned,
    })
}

// ---------------------------------------------------------------------------
// Helper: parse_relation_types
// ---------------------------------------------------------------------------

/// Validate each element via RelationType::from_str. Returns Err listing all
/// 16 types when any element is unrecognized.
fn parse_relation_types(types: &[String]) -> Result<Vec<RelationType>, ErrorData> {
    let mut result = Vec::with_capacity(types.len());
    for s in types {
        match RelationType::from_str(s) {
            Some(rt) => result.push(rt),
            None => {
                return Err(ErrorData::new(
                    ERROR_INVALID_PARAMS,
                    format!(
                        "unrecognized edge type '{s}' — recognized types: \
                         About, Advances, Asserts, Cites, CoAccess, Contradicts, \
                         DerivedFrom, Informs, Mentions, Motivates, Prerequisite, \
                         Refutes, RelatedTo, Supports, Supersedes, Tests"
                    ),
                    None,
                ));
            }
        }
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Helper: push_relation_type_list
// ---------------------------------------------------------------------------

/// Appends a parameterized IN-clause argument list to a QueryBuilder.
/// Each type string is bound individually via push_bind — no string interpolation
/// of type values (ADR-007, push_bind pattern #4058).
fn push_relation_type_list(qb: &mut QueryBuilder<sqlx::Sqlite>, types: &[RelationType]) {
    for (i, rt) in types.iter().enumerate() {
        if i > 0 {
            qb.push(", ");
        }
        qb.push_bind(rt.as_str());
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "graph_read_filter_tests.rs"]
mod tests;
