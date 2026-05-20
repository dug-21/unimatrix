//! SQL query functions for `context_graph` traversal (vnc-018).
//!
//! Provides:
//! - `query_supersession_chain` — recursive CTE on `entries.supersedes` /
//!   `entries.superseded_by` for the `chain` mode (ADR-001).
//! - `query_current_terminal` — recursive CTE following `superseded_by` to the
//!   terminal `Active` entry for the `current` mode (ADR-001, R-20).
//! - `query_direct_neighbors` — composite-index query on `GRAPH_EDGES` for
//!   `neighbors` mode at depth=1 (ADR-005).
//!
//! All functions use `read_pool()` (C-07). The write pool is never accessed here.

use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::error::StoreError;
use crate::read::{entry_from_row, load_tags_for_entries};
use crate::schema::EntryRecord;

/// Qualified ENTRY_COLUMNS for CTE join queries.
///
/// The CTE always has a column named `id` (the node ID in the traversal). When the
/// final SELECT joins `entries e` with the CTE and selects entry columns, `id` is
/// ambiguous. This constant qualifies only the `id` column with `e.id AS id` so that
/// `entry_from_row` can still retrieve it by the unqualified name `"id"`. All other
/// entry columns are unambiguous because they don't exist in the CTE.
const ENTRY_COLUMNS_E: &str = "e.id AS id, title, content, topic, category, source, status, confidence, \
     created_at, updated_at, last_accessed_at, access_count, \
     supersedes, superseded_by, correction_count, embedding_dim, \
     created_by, modified_by, content_hash, previous_hash, \
     version, feature_cycle, trust_source, helpful_count, unhelpful_count, \
     pre_quarantine_status";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Direction for supersession chain traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainDirection {
    /// Follow `entries.supersedes` links: find entries that supersede X (toward newer).
    Forward,
    /// Follow `entries.superseded_by` links: find ancestors of X (toward older).
    Backward,
    /// Run both directions independently; dedup by entry ID; per-direction cap tracking.
    Both,
}

/// Direction for neighbor queries on `GRAPH_EDGES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeighborDirection {
    /// Edges where `target_id` = anchor (entries pointing at anchor).
    Incoming,
    /// Edges where `source_id` = anchor (entries pointed to by anchor).
    Outgoing,
    /// Union of incoming and outgoing edges.
    Both,
}

/// Result of `query_supersession_chain`.
#[derive(Debug)]
pub struct ChainQueryResult {
    /// All entries in the chain, ordered oldest ancestor → newest descendant.
    pub entries: Vec<EntryRecord>,
    /// `true` when the forward direction hit the 50-hop depth cap.
    pub forward_capped: bool,
    /// `true` when the backward direction hit the 50-hop depth cap.
    pub backward_capped: bool,
}

/// A single row from `graph_edges` before direction annotation.
#[derive(Debug, Clone)]
pub struct RawEdgeRow {
    pub source_id: u64,
    pub target_id: u64,
    pub relation_type: String,
}

// ---------------------------------------------------------------------------
// query_supersession_chain
// ---------------------------------------------------------------------------

/// Walk the supersession chain for `id` using SQL recursive CTEs.
///
/// Used by `chain` mode. Both directions are supported; the `Both` variant runs
/// two independent CTEs and merges results with deduplication. Returns entries
/// ordered oldest ancestor → seed → newest descendant (AC-01).
///
/// A non-existent `id` produces an empty result — no error (AC-04).
pub async fn query_supersession_chain(
    pool: &SqlitePool,
    id: u64,
    direction: ChainDirection,
    depth_cap: u8,
) -> Result<ChainQueryResult, StoreError> {
    let (forward_entries, forward_capped, backward_entries, backward_capped) = match direction {
        ChainDirection::Forward => {
            let (entries, capped) = run_forward_cte(pool, id, depth_cap).await?;
            (entries, capped, vec![], false)
        }
        ChainDirection::Backward => {
            let (entries, capped) = run_backward_cte(pool, id, depth_cap).await?;
            (vec![], false, entries, capped)
        }
        ChainDirection::Both => {
            let (fwd_entries, fwd_capped) = run_forward_cte(pool, id, depth_cap).await?;
            let (bwd_entries, bwd_capped) = run_backward_cte(pool, id, depth_cap).await?;
            (fwd_entries, fwd_capped, bwd_entries, bwd_capped)
        }
    };

    let entries = merge_chain_results(backward_entries, forward_entries);

    Ok(ChainQueryResult {
        entries,
        forward_capped,
        backward_capped,
    })
}

// ---------------------------------------------------------------------------
// query_current_terminal
// ---------------------------------------------------------------------------

/// Follow `superseded_by` from `id` to the terminal entry where
/// `superseded_by IS NULL AND status = 'Active'`.
///
/// Returns `None` for:
/// - Non-existent `id` (zero rows from anchor SELECT).
/// - Orphaned deprecated terminal (`superseded_by IS NULL`, `status = 'Deprecated'`).
/// - Chain exceeding 50 hops (CTE depth cap fires before terminal is reached).
///
/// All three cases map to the same "no active terminal found" error at the handler
/// layer — the CTE cannot distinguish them, and the caller's intent is version
/// resolution, not existence checking. This is intentionally separate from
/// `query_supersession_chain` because the terminal condition (`AND status = 'Active'`)
/// cannot be expressed via `ChainDirection` parameters (FR-05, R-20).
///
/// CRITICAL: `AND e.status = 'Active'` in the final SELECT is MANDATORY.
/// Without it, an orphaned deprecated entry (`superseded_by IS NULL`,
/// `status = 'Deprecated'`) would be silently returned as the terminal (R-20).
pub async fn query_current_terminal(
    pool: &SqlitePool,
    id: u64,
) -> Result<Option<EntryRecord>, StoreError> {
    // The CTE follows superseded_by links until it finds an entry with
    // superseded_by IS NULL. The final SELECT then filters to only Active entries.
    let sql = format!(
        "WITH RECURSIVE chain(id, depth) AS (
             SELECT id, 0 FROM entries WHERE id = ?1
             UNION ALL
             SELECT e.superseded_by, c.depth + 1
             FROM entries e
             JOIN chain c ON e.id = c.id
             WHERE e.superseded_by IS NOT NULL AND c.depth < 50
         )
         SELECT {ENTRY_COLUMNS_E}
         FROM entries e
         JOIN chain c ON e.id = c.id
         WHERE e.superseded_by IS NULL
           AND e.status = 0
         LIMIT 1"
    );

    let row = sqlx::query(&sql)
        .bind(id as i64)
        .fetch_optional(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let mut entry = entry_from_row(&row)?;

    // Load tags for this single entry.
    let tag_map = load_tags_for_entries(pool, &[entry.id]).await?;
    entry.tags = tag_map.into_values().next().unwrap_or_default();

    Ok(Some(entry))
}

// ---------------------------------------------------------------------------
// query_direct_neighbors
// ---------------------------------------------------------------------------

/// Query `GRAPH_EDGES` for depth=1 neighbors of `id`.
///
/// Uses composite indexes `idx_graph_edges_source_type` (outgoing) and
/// `idx_graph_edges_target_type` (incoming) for single-range scans (ADR-005).
///
/// `edge_types`:
/// - Empty slice → all types except `Supersedes` (silent exclusion at SQL level).
/// - Non-empty → specific types; `Supersedes` must already be excluded by the
///   caller (`handle_neighbors`). The `!= 'Supersedes'` filter in the empty-type
///   SQL path is a fallback safety net.
///
/// A non-existent `id` returns an empty `Vec` — no error (OQ-01 resolution).
pub async fn query_direct_neighbors(
    pool: &SqlitePool,
    id: u64,
    edge_types: &[&str],
    direction: NeighborDirection,
) -> Result<Vec<RawEdgeRow>, StoreError> {
    match direction {
        NeighborDirection::Outgoing => run_outgoing_query(pool, id, edge_types).await,
        NeighborDirection::Incoming => run_incoming_query(pool, id, edge_types).await,
        NeighborDirection::Both => {
            let mut outgoing = run_outgoing_query(pool, id, edge_types).await?;
            let incoming = run_incoming_query(pool, id, edge_types).await?;
            outgoing.extend(incoming);
            Ok(outgoing)
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Forward CTE: finds entries that supersede X — toward newer descendants.
///
/// Returns (entries_ordered_by_depth_asc, capped).
/// Cap fires when any returned row has depth == depth_cap - 1 AND has successors.
async fn run_forward_cte(
    pool: &SqlitePool,
    id: u64,
    depth_cap: u8,
) -> Result<(Vec<(EntryRecord, i64)>, bool), StoreError> {
    // The CTE joins on `e.supersedes = c.id`, walking toward newer entries.
    let sql = format!(
        "WITH RECURSIVE chain(id, depth) AS (
             SELECT id, 0 FROM entries WHERE id = ?1
             UNION ALL
             SELECT e.id, c.depth + 1
             FROM entries e
             JOIN chain c ON e.supersedes = c.id
             WHERE c.depth < ?2
         )
         SELECT {ENTRY_COLUMNS_E}, c.depth AS chain_depth
         FROM entries e
         JOIN chain c ON e.id = c.id
         ORDER BY c.depth ASC"
    );

    let rows = sqlx::query(&sql)
        .bind(id as i64)
        .bind(depth_cap as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

    let mut entries_with_depth: Vec<(EntryRecord, i64)> = Vec::with_capacity(rows.len());
    let mut max_depth: i64 = -1;

    for row in &rows {
        let entry = entry_from_row(row)?;
        let depth: i64 = row
            .try_get("chain_depth")
            .map_err(|e| StoreError::Database(e.into()))?;
        if depth > max_depth {
            max_depth = depth;
        }
        entries_with_depth.push((entry, depth));
    }

    // Load tags for all entries.
    let ids: Vec<u64> = entries_with_depth.iter().map(|(e, _)| e.id).collect();
    let tag_map = load_tags_for_entries(pool, &ids).await?;
    for (entry, _) in &mut entries_with_depth {
        entry.tags = tag_map.get(&entry.id).cloned().unwrap_or_default();
    }

    // Detect cap: if max_depth == depth_cap - 1 as i64, check if any of those
    // rows has a successor (an entry whose supersedes = that id).
    let capped = if max_depth == (depth_cap as i64) - 1 && !entries_with_depth.is_empty() {
        let last_depth_ids: Vec<u64> = entries_with_depth
            .iter()
            .filter(|(_, d)| *d == max_depth)
            .map(|(e, _)| e.id)
            .collect();
        check_has_forward_successors(pool, &last_depth_ids).await?
    } else {
        false
    };

    Ok((entries_with_depth, capped))
}

/// Backward CTE: finds ancestors of X — entries that X supersedes, toward older.
///
/// Returns (entries_ordered_by_depth_asc, capped).
async fn run_backward_cte(
    pool: &SqlitePool,
    id: u64,
    depth_cap: u8,
) -> Result<(Vec<(EntryRecord, i64)>, bool), StoreError> {
    // The CTE joins on `e.superseded_by = c.id`, walking toward older entries.
    let sql = format!(
        "WITH RECURSIVE chain(id, depth) AS (
             SELECT id, 0 FROM entries WHERE id = ?1
             UNION ALL
             SELECT e.id, c.depth + 1
             FROM entries e
             JOIN chain c ON e.superseded_by = c.id
             WHERE c.depth < ?2
         )
         SELECT {ENTRY_COLUMNS_E}, c.depth AS chain_depth
         FROM entries e
         JOIN chain c ON e.id = c.id
         ORDER BY c.depth ASC"
    );

    let rows = sqlx::query(&sql)
        .bind(id as i64)
        .bind(depth_cap as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

    let mut entries_with_depth: Vec<(EntryRecord, i64)> = Vec::with_capacity(rows.len());
    let mut max_depth: i64 = -1;

    for row in &rows {
        let entry = entry_from_row(row)?;
        let depth: i64 = row
            .try_get("chain_depth")
            .map_err(|e| StoreError::Database(e.into()))?;
        if depth > max_depth {
            max_depth = depth;
        }
        entries_with_depth.push((entry, depth));
    }

    // Load tags for all entries.
    let ids: Vec<u64> = entries_with_depth.iter().map(|(e, _)| e.id).collect();
    let tag_map = load_tags_for_entries(pool, &ids).await?;
    for (entry, _) in &mut entries_with_depth {
        entry.tags = tag_map.get(&entry.id).cloned().unwrap_or_default();
    }

    // Detect cap: if max_depth == depth_cap - 1, check if any of those rows has
    // an ancestor (an entry whose superseded_by = that id).
    let capped = if max_depth == (depth_cap as i64) - 1 && !entries_with_depth.is_empty() {
        let last_depth_ids: Vec<u64> = entries_with_depth
            .iter()
            .filter(|(_, d)| *d == max_depth)
            .map(|(e, _)| e.id)
            .collect();
        check_has_backward_predecessors(pool, &last_depth_ids).await?
    } else {
        false
    };

    Ok((entries_with_depth, capped))
}

/// Check whether any of the given IDs has a forward successor (`supersedes` pointing to it).
async fn check_has_forward_successors(pool: &SqlitePool, ids: &[u64]) -> Result<bool, StoreError> {
    if ids.is_empty() {
        return Ok(false);
    }
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "SELECT COUNT(*) FROM entries WHERE supersedes IN ({})",
        placeholders.join(", ")
    );
    let mut query = sqlx::query_scalar::<_, i64>(&sql);
    for id in ids {
        query = query.bind(*id as i64);
    }
    let count = query
        .fetch_one(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;
    Ok(count > 0)
}

/// Check whether any of the given IDs has a backward predecessor (`superseded_by` pointing to it).
async fn check_has_backward_predecessors(
    pool: &SqlitePool,
    ids: &[u64],
) -> Result<bool, StoreError> {
    if ids.is_empty() {
        return Ok(false);
    }
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "SELECT COUNT(*) FROM entries WHERE superseded_by IN ({})",
        placeholders.join(", ")
    );
    let mut query = sqlx::query_scalar::<_, i64>(&sql);
    for id in ids {
        query = query.bind(*id as i64);
    }
    let count = query
        .fetch_one(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;
    Ok(count > 0)
}

/// Merge backward (ancestors) and forward (descendants) chain results.
///
/// - Backward entries are at depth > 0 from seed (toward older). They must be
///   reversed so oldest appears first (depth=N before depth=0).
/// - Forward entries are at depth >= 0 from seed (toward newer).
/// - The seed (id) appears in BOTH CTEs at depth=0; deduplication keeps the first.
///
/// Final order: oldest ancestor → ... → seed → ... → newest descendant.
fn merge_chain_results(
    backward: Vec<(EntryRecord, i64)>,
    forward: Vec<(EntryRecord, i64)>,
) -> Vec<EntryRecord> {
    // backward is ordered depth=0 (seed) first, depth=N last.
    // Reverse so oldest ancestors come first.
    let mut result: Vec<EntryRecord> = Vec::with_capacity(backward.len() + forward.len());
    let mut seen_ids: std::collections::HashSet<u64> = std::collections::HashSet::new();

    // Add backward entries in reverse order (oldest first).
    for (entry, _) in backward.into_iter().rev() {
        if seen_ids.insert(entry.id) {
            result.push(entry);
        }
    }

    // Add forward entries in order (seed first, then newer descendants).
    for (entry, _) in forward {
        if seen_ids.insert(entry.id) {
            result.push(entry);
        }
    }

    result
}

// SQL helpers for depth=1 neighbor queries — extracted to keep this file under 500 lines.
#[path = "graph_queries_neighbors.rs"]
mod neighbors;

use neighbors::{run_incoming_query, run_outgoing_query};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "graph_queries_tests.rs"]
mod tests;
