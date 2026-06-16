//! Private SQL helpers for `query_direct_neighbors` (depth=1 neighbor queries).
//!
//! Extracted from `graph_queries.rs` to keep that file under the 500-line limit.
//! All functions are `pub(super)` — used exclusively by `graph_queries.rs`.

use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use super::RawEdgeRow;
use crate::error::StoreError;

/// Outgoing neighbor query: `source_id = id` in `graph_edges`.
pub(super) async fn run_outgoing_query(
    pool: &SqlitePool,
    id: u64,
    edge_types: &[&str],
) -> Result<Vec<RawEdgeRow>, StoreError> {
    let rows = if edge_types.is_empty() {
        sqlx::query(
            "SELECT source_id, target_id, relation_type, source
             FROM graph_edges
             WHERE source_id = ?1
               AND relation_type != 'Supersedes'",
        )
        .bind(id as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?
    } else {
        let placeholders: Vec<String> = (2..=edge_types.len() + 1)
            .map(|i| format!("?{i}"))
            .collect();
        let sql = format!(
            "SELECT source_id, target_id, relation_type, source
             FROM graph_edges
             WHERE source_id = ?1
               AND relation_type IN ({})",
            placeholders.join(", ")
        );
        let mut query = sqlx::query(&sql).bind(id as i64);
        for t in edge_types {
            query = query.bind(*t);
        }
        query
            .fetch_all(pool)
            .await
            .map_err(|e| StoreError::Database(e.into()))?
    };

    rows.iter().map(map_edge_row).collect()
}

/// Incoming neighbor query: `target_id = id` in `graph_edges`.
pub(super) async fn run_incoming_query(
    pool: &SqlitePool,
    id: u64,
    edge_types: &[&str],
) -> Result<Vec<RawEdgeRow>, StoreError> {
    let rows = if edge_types.is_empty() {
        sqlx::query(
            "SELECT source_id, target_id, relation_type, source
             FROM graph_edges
             WHERE target_id = ?1
               AND relation_type != 'Supersedes'",
        )
        .bind(id as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?
    } else {
        let placeholders: Vec<String> = (2..=edge_types.len() + 1)
            .map(|i| format!("?{i}"))
            .collect();
        let sql = format!(
            "SELECT source_id, target_id, relation_type, source
             FROM graph_edges
             WHERE target_id = ?1
               AND relation_type IN ({})",
            placeholders.join(", ")
        );
        let mut query = sqlx::query(&sql).bind(id as i64);
        for t in edge_types {
            query = query.bind(*t);
        }
        query
            .fetch_all(pool)
            .await
            .map_err(|e| StoreError::Database(e.into()))?
    };

    rows.iter().map(map_edge_row).collect()
}

/// Map a `graph_edges` row to a `RawEdgeRow`.
///
/// `source` (ADR-004) is read with the same `try_get` + `StoreError::Database`
/// mapping as the existing fields — no `.unwrap()`. The plain path never carries a
/// target confidence, so `target_confidence` is always `None` here; only the ranked
/// variant populates it.
pub(super) fn map_edge_row(row: &sqlx::sqlite::SqliteRow) -> Result<RawEdgeRow, StoreError> {
    Ok(RawEdgeRow {
        source_id: row
            .try_get::<i64, _>("source_id")
            .map_err(|e| StoreError::Database(e.into()))? as u64,
        target_id: row
            .try_get::<i64, _>("target_id")
            .map_err(|e| StoreError::Database(e.into()))? as u64,
        relation_type: row
            .try_get("relation_type")
            .map_err(|e| StoreError::Database(e.into()))?,
        source: row
            .try_get("source")
            .map_err(|e| StoreError::Database(e.into()))?,
        target_confidence: None,
    })
}
