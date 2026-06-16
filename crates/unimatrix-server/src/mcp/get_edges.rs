//! vnc-037 — `context_get` edge assembly (get-edge-assembly component).
//!
//! Lives in its own module so `tools.rs` stays under the 500-line guidance and the
//! edge logic is single-responsibility. [`build_edges_view`] issues the ranked select,
//! the split count, and one batched title join, then projects the ranked rows into the
//! thin discovery-list [`EdgesView`] (ADR-001/002/003, FR-1/FR-3/FR-5/FR-19, NFR-1).
//!
//! **FR-19 fail-loud (C-13)**: every query returns `Result` and there is **no**
//! `.unwrap()`/`.expect()` on the edge path. The handler calls [`build_edges_view`]
//! only on the default-on path (`None`/`Some(true)`) and maps any `Err` with the
//! **same** mapping as the primary `entry_store.get` failure — one consistent failure
//! contract, no degrade-with-note, no silent edge omission. On `Some(false)` the
//! handler never calls this, so the opt-out path cannot reach the failure.
//!
//! **Security**: the title `IN (…)` list uses positional binds over the ≤cap displayed
//! `target_id`s — never string-interpolated ids. The ranked select / split count bind
//! the anchor positionally (see `graph_queries_ranked.rs`).

use std::collections::HashMap;

use sqlx::Row;
use sqlx::sqlite::SqlitePool;
use unimatrix_core::Store;
use unimatrix_store::{
    EdgeCountSplit, RankedEdge, StoreError, count_neighbors_split, query_ranked_neighbors,
};

use super::response::edges::{
    DIRECTION_BOTH, DIRECTION_INBOUND, DIRECTION_OUTBOUND, EdgeTotals, EdgesView, GetEdge,
};

/// Assemble the [`EdgesView`] for a `context_get` default-on read.
///
/// Steps (all on `read_pool_server`, NFR-3):
/// 1. ranked ≤`GET_EDGE_DISPLAY_LIMIT` displayed rows (canonicalized, LEFT JOIN confidence).
/// 2. honest uncapped split totals — three buckets `{inbound, outbound, both}` plus the
///    digest-only `authored` tally (same canonicalized set, `↔` once in `both`).
/// 3. one batched title join over the ≤cap displayed targets ONLY.
/// 4. project rows → [`GetEdge`] (`→`/`←`/`↔`, authored, title-map lookup).
/// 5. build [`EdgesView`] — `authored_total` threaded from the FULL-set `split.authored`.
///
/// Returns `Result<EdgesView, StoreError>` so the handler maps the whole assembly once
/// with the identical primary-read mapping (FR-19). No `.unwrap()`/`.expect()`.
pub(crate) async fn build_edges_view(store: &Store, id: u64) -> Result<EdgesView, StoreError> {
    let pool = store.read_pool_server();

    // 1. ranked ≤cap displayed rows (canonicalized, LEFT JOIN confidence, LIMIT cap).
    let rows = query_ranked_neighbors(pool, id).await?;

    // 2. honest uncapped split totals — three buckets + digest-only `authored`.
    let split: EdgeCountSplit = count_neighbors_split(pool, id).await?;

    // 3. batched title join over the ≤cap displayed targets ONLY (never the uncapped set).
    let mut target_ids: Vec<u64> = Vec::with_capacity(rows.len());
    for edge in &rows {
        if !target_ids.contains(&edge.row.target_id) {
            target_ids.push(edge.row.target_id);
        }
    }
    let title_map = if target_ids.is_empty() {
        HashMap::new()
    } else {
        fetch_titles_batch(pool, &target_ids).await?
    };

    // 4. project ranked rows → GetEdge.
    let edges: Vec<GetEdge> = rows
        .iter()
        .map(|edge| project_edge(edge, &title_map))
        .collect();

    // 5. assemble (edges already ≤cap from SQL; totals uncapped, three buckets;
    //    authored_total threaded for the digest from the FULL uncapped set).
    Ok(EdgesView {
        edges,
        totals: EdgeTotals {
            inbound: split.inbound,
            outbound: split.outbound,
            both: split.both,
        },
        authored_total: split.authored,
    })
}

/// Project one ranked row into the discovery-list [`GetEdge`].
///
/// `target_id` is the OTHER endpoint (`RankedEdge.row.target_id`); `target_title` is the
/// title-map lookup (`None` ⇒ dangling, retained — DNB-1); `authored` is derived in
/// [`GetEdge::new`] from the raw `source` string (`== "agent"`). The SQL-computed
/// direction string is mapped to the canonical `&'static str` constant — a `↔` row is
/// never re-derived as `→`/`←` in Rust.
fn project_edge(edge: &RankedEdge, title_map: &HashMap<u64, String>) -> GetEdge {
    let target_id = edge.row.target_id;
    GetEdge::new(
        edge.row.relation_type.clone(),
        map_direction(&edge.direction),
        target_id,
        title_map.get(&target_id).cloned(),
        &edge.row.source,
    )
}

/// Map the SQL-computed canonical direction string to its `&'static str` constant.
///
/// The ranked SQL emits exactly `"both"` / `"inbound"` / `"outbound"`. Any unexpected
/// value falls back to `DIRECTION_BOTH` (the symmetric, arrow-free rendering) rather than
/// panicking — FR-19 forbids `.unwrap()`/`.expect()` on the edge path.
fn map_direction(direction: &str) -> &'static str {
    match direction {
        DIRECTION_OUTBOUND => DIRECTION_OUTBOUND,
        DIRECTION_INBOUND => DIRECTION_INBOUND,
        _ => DIRECTION_BOTH,
    }
}

/// Batched title resolution for the ≤cap displayed targets (precedent:
/// `fetch_nodes_batch`, `graph_read_subgraph.rs:568`).
///
/// One `SELECT id, title FROM entries WHERE id IN (?, …)` with **positional binds** —
/// never a string-built IN-list (Security). Bounded to ≤cap ids, so no chunking is
/// needed. An id absent from the result is simply absent from the map ⇒ `target_title:
/// None` downstream (dangling retained). Returns `Result` — no `.unwrap()` (FR-19).
async fn fetch_titles_batch(
    pool: &SqlitePool,
    ids: &[u64],
) -> Result<HashMap<u64, String>, StoreError> {
    let placeholders: Vec<&str> = std::iter::repeat_n("?", ids.len()).collect();
    let sql = format!(
        "SELECT id, title FROM entries WHERE id IN ({})",
        placeholders.join(", ")
    );

    let mut query = sqlx::query(&sql);
    for &id in ids {
        query = query.bind(id as i64);
    }

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

    let mut map = HashMap::with_capacity(rows.len());
    for row in &rows {
        let id = row
            .try_get::<i64, _>("id")
            .map_err(|e| StoreError::Database(e.into()))? as u64;
        let title = row
            .try_get::<String, _>("title")
            .map_err(|e| StoreError::Database(e.into()))?;
        map.insert(id, title);
    }
    Ok(map)
}

#[cfg(test)]
#[path = "get_edges_tests.rs"]
mod tests;
