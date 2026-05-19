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

/// Outgoing neighbor query: `source_id = id` in `graph_edges`.
async fn run_outgoing_query(
    pool: &SqlitePool,
    id: u64,
    edge_types: &[&str],
) -> Result<Vec<RawEdgeRow>, StoreError> {
    let rows = if edge_types.is_empty() {
        sqlx::query(
            "SELECT source_id, target_id, relation_type
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
            "SELECT source_id, target_id, relation_type
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
async fn run_incoming_query(
    pool: &SqlitePool,
    id: u64,
    edge_types: &[&str],
) -> Result<Vec<RawEdgeRow>, StoreError> {
    let rows = if edge_types.is_empty() {
        sqlx::query(
            "SELECT source_id, target_id, relation_type
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
            "SELECT source_id, target_id, relation_type
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
fn map_edge_row(row: &sqlx::sqlite::SqliteRow) -> Result<RawEdgeRow, StoreError> {
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
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SqlxStore;
    use crate::pool_config::PoolConfig;
    use crate::schema::Status;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.db");
        let store = SqlxStore::open(&path, PoolConfig::test_default())
            .await
            .expect("open test store");
        (store, dir)
    }

    /// Insert a minimal entry into the `entries` table. Returns the assigned id.
    async fn insert_entry(
        pool: &SqlitePool,
        title: &str,
        status: Status,
        supersedes: Option<u64>,
        superseded_by: Option<u64>,
    ) -> u64 {
        // Get next id from counters
        let id: i64 =
            sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'next_entry_id'")
                .fetch_one(pool)
                .await
                .expect("get next_entry_id");
        let new_id = id + 1;
        sqlx::query("UPDATE counters SET value = ?1 WHERE name = 'next_entry_id'")
            .bind(new_id)
            .execute(pool)
            .await
            .expect("update next_entry_id");

        let status_i = match status {
            Status::Active => 0i64,
            Status::Deprecated => 1,
            Status::Proposed => 2,
            Status::Quarantined => 3,
        };
        let now = 1_700_000_000_i64;
        sqlx::query(
            "INSERT INTO entries (id, title, content, topic, category, source, status,
             confidence, created_at, updated_at, last_accessed_at, access_count,
             supersedes, superseded_by, correction_count, embedding_dim,
             created_by, modified_by, content_hash, previous_hash,
             version, feature_cycle, trust_source, helpful_count, unhelpful_count)
             VALUES (?1, ?2, 'content', 'test', 'pattern', 'test', ?3,
             0.5, ?4, ?4, ?4, 0, ?5, ?6, 0, 0, '', '', '', '', 1, '', '', 0, 0)",
        )
        .bind(new_id)
        .bind(title)
        .bind(status_i)
        .bind(now)
        .bind(supersedes.map(|v| v as i64))
        .bind(superseded_by.map(|v| v as i64))
        .execute(pool)
        .await
        .expect("insert entry");

        new_id as u64
    }

    /// Insert a graph_edges row.
    async fn insert_edge(pool: &SqlitePool, source: u64, target: u64, rel: &str) {
        let now = 1_700_000_000_i64;
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
             VALUES (?1, ?2, ?3, 1.0, ?4, '', '', 0)",
        )
        .bind(source as i64)
        .bind(target as i64)
        .bind(rel)
        .bind(now)
        .execute(pool)
        .await
        .expect("insert edge");
    }

    // -----------------------------------------------------------------------
    // query_supersession_chain tests
    // -----------------------------------------------------------------------

    /// R-01, R-05: Cold-start test — no ticks, empty DB, queries the SQL path directly.
    #[tokio::test]
    async fn test_query_supersession_chain_empty_db_returns_empty() {
        let (store, _dir) = open_test_store().await;
        let result = query_supersession_chain(store.read_pool(), 999_999, ChainDirection::Both, 50)
            .await
            .expect("query");

        assert!(
            result.entries.is_empty(),
            "non-existent id must return empty"
        );
        assert!(!result.forward_capped);
        assert!(!result.backward_capped);
        store.close().await.unwrap();
    }

    /// Single isolated entry — no ancestors or descendants.
    #[tokio::test]
    async fn test_query_supersession_chain_single_entry() {
        let (store, _dir) = open_test_store().await;
        let pool = store.read_pool();

        let id = insert_entry(&&store.write_pool, "Entry A", Status::Active, None, None).await;

        let result = query_supersession_chain(pool, id, ChainDirection::Both, 50)
            .await
            .expect("query");

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].id, id);
        assert!(!result.forward_capped);
        assert!(!result.backward_capped);
        store.close().await.unwrap();
    }

    /// AC-01: Five-entry chain A→B→C→D→E queried from C — should return all 5, oldest first.
    #[tokio::test]
    async fn test_query_supersession_chain_five_entry_chain_both() {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        // Insert A, B, C, D, E as a chain where A is oldest, E is newest.
        // superseded_by points toward newer; supersedes points toward older.
        // A.superseded_by = B, B.superseded_by = C, C.superseded_by = D, D.superseded_by = E
        // B.supersedes = A, C.supersedes = B, D.supersedes = C, E.supersedes = D
        let a = insert_entry(wp, "A", Status::Deprecated, None, None).await;
        let b = insert_entry(wp, "B", Status::Deprecated, Some(a), None).await;
        let c = insert_entry(wp, "C", Status::Deprecated, Some(b), None).await;
        let d = insert_entry(wp, "D", Status::Deprecated, Some(c), None).await;
        let e = insert_entry(wp, "E", Status::Active, Some(d), None).await;

        // Set superseded_by links: A → B, B → C, C → D, D → E
        sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
            .bind(b as i64)
            .bind(a as i64)
            .execute(wp)
            .await
            .unwrap();
        sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
            .bind(c as i64)
            .bind(b as i64)
            .execute(wp)
            .await
            .unwrap();
        sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
            .bind(d as i64)
            .bind(c as i64)
            .execute(wp)
            .await
            .unwrap();
        sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
            .bind(e as i64)
            .bind(d as i64)
            .execute(wp)
            .await
            .unwrap();

        let result = query_supersession_chain(store.read_pool(), c, ChainDirection::Both, 50)
            .await
            .expect("query");

        assert_eq!(result.entries.len(), 5, "all 5 entries must be returned");
        let ids: Vec<u64> = result.entries.iter().map(|e| e.id).collect();
        // Oldest (A) must come before seed (C); seed before newest (E).
        let pos_a = ids.iter().position(|&x| x == a).unwrap();
        let pos_c = ids.iter().position(|&x| x == c).unwrap();
        let pos_e = ids.iter().position(|&x| x == e).unwrap();
        assert!(pos_a < pos_c, "A must come before C");
        assert!(pos_c < pos_e, "C must come before E");
        assert!(!result.forward_capped);
        assert!(!result.backward_capped);
        store.close().await.unwrap();
    }

    /// AC-02: Forward direction only returns seed + descendants.
    #[tokio::test]
    async fn test_query_supersession_chain_direction_forward_only() {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        let a = insert_entry(wp, "A", Status::Deprecated, None, None).await;
        let b = insert_entry(wp, "B", Status::Deprecated, Some(a), None).await;
        let c = insert_entry(wp, "C", Status::Active, Some(b), None).await;

        sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
            .bind(b as i64)
            .bind(a as i64)
            .execute(wp)
            .await
            .unwrap();
        sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
            .bind(c as i64)
            .bind(b as i64)
            .execute(wp)
            .await
            .unwrap();

        // Query forward from A — should get A (seed) and B, C (descendants that supersede A).
        let result = query_supersession_chain(store.read_pool(), a, ChainDirection::Forward, 50)
            .await
            .expect("query");

        let ids: Vec<u64> = result.entries.iter().map(|e| e.id).collect();
        assert!(ids.contains(&a), "seed must be in forward result");
        assert!(ids.contains(&b), "B must be in forward result");
        assert!(ids.contains(&c), "C must be in forward result");
        assert!(!result.forward_capped);
        store.close().await.unwrap();
    }

    /// AC-02: Backward direction only returns seed + ancestors.
    #[tokio::test]
    async fn test_query_supersession_chain_direction_backward_only() {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        let a = insert_entry(wp, "A", Status::Deprecated, None, None).await;
        let b = insert_entry(wp, "B", Status::Deprecated, Some(a), None).await;
        let c = insert_entry(wp, "C", Status::Active, Some(b), None).await;

        sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
            .bind(b as i64)
            .bind(a as i64)
            .execute(wp)
            .await
            .unwrap();
        sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
            .bind(c as i64)
            .bind(b as i64)
            .execute(wp)
            .await
            .unwrap();

        // Query backward from C — should get C (seed), B, and A (ancestors).
        let result = query_supersession_chain(store.read_pool(), c, ChainDirection::Backward, 50)
            .await
            .expect("query");

        let ids: Vec<u64> = result.entries.iter().map(|e| e.id).collect();
        assert!(ids.contains(&a), "A must be in backward result");
        assert!(ids.contains(&b), "B must be in backward result");
        assert!(ids.contains(&c), "seed must be in backward result");
        assert!(!result.backward_capped);
        store.close().await.unwrap();
    }

    /// AC-04: Non-existent ID returns empty result, not an error.
    #[tokio::test]
    async fn test_query_supersession_chain_nonexistent_id() {
        let (store, _dir) = open_test_store().await;
        let result = query_supersession_chain(store.read_pool(), 999_999, ChainDirection::Both, 50)
            .await
            .expect("no error for non-existent id");

        assert!(result.entries.is_empty());
        assert!(!result.forward_capped);
        assert!(!result.backward_capped);
        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // query_current_terminal tests
    // -----------------------------------------------------------------------

    /// Existing active entry with no superseded_by returns Some(that entry).
    #[tokio::test]
    async fn test_query_current_terminal_active_entry_returns_some() {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        let id = insert_entry(wp, "Active Entry", Status::Active, None, None).await;

        let result = query_current_terminal(store.read_pool(), id)
            .await
            .expect("query");

        assert!(result.is_some(), "active entry must return Some");
        assert_eq!(result.unwrap().id, id);
        store.close().await.unwrap();
    }

    /// Orphaned deprecated entry (superseded_by IS NULL, status=Deprecated) returns None.
    /// This is the R-20 Critical risk test — validates AND e.status='Active' filter.
    #[tokio::test]
    async fn test_query_current_terminal_orphaned_deprecated_returns_none() {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        // Insert a deprecated entry with no superseded_by — orphaned deprecated terminal.
        let id = insert_entry(wp, "Orphaned Deprecated", Status::Deprecated, None, None).await;

        let result = query_current_terminal(store.read_pool(), id)
            .await
            .expect("query");

        assert!(
            result.is_none(),
            "orphaned deprecated entry (superseded_by IS NULL, status=Deprecated) must return None \
             — AND e.status='Active' filter is mandatory (R-20 Critical)"
        );
        store.close().await.unwrap();
    }

    /// Non-existent ID returns None.
    #[tokio::test]
    async fn test_query_current_terminal_nonexistent_id_returns_none() {
        let (store, _dir) = open_test_store().await;

        let result = query_current_terminal(store.read_pool(), 999_999)
            .await
            .expect("query");

        assert!(result.is_none(), "non-existent id must return None");
        store.close().await.unwrap();
    }

    /// Deprecated entry with active successor: returns the active successor.
    #[tokio::test]
    async fn test_query_current_terminal_deprecated_with_active_successor() {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        let old = insert_entry(wp, "Old Entry", Status::Deprecated, None, None).await;
        let new = insert_entry(wp, "New Entry", Status::Active, Some(old), None).await;

        // Set superseded_by on old → new
        sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
            .bind(new as i64)
            .bind(old as i64)
            .execute(wp)
            .await
            .unwrap();

        let result = query_current_terminal(store.read_pool(), old)
            .await
            .expect("query");

        assert!(result.is_some(), "must resolve to active successor");
        assert_eq!(result.unwrap().id, new);
        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // query_direct_neighbors tests
    // -----------------------------------------------------------------------

    /// AC-08: Outgoing edges with specific type.
    #[tokio::test]
    async fn test_query_direct_neighbors_outgoing_specific_type() {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        let x = insert_entry(wp, "X", Status::Active, None, None).await;
        let y = insert_entry(wp, "Y", Status::Active, None, None).await;
        let z = insert_entry(wp, "Z", Status::Active, None, None).await;

        insert_edge(wp, x, y, "Prerequisite").await;
        insert_edge(wp, x, z, "Prerequisite").await;

        let rows = query_direct_neighbors(
            store.read_pool(),
            x,
            &["Prerequisite"],
            NeighborDirection::Outgoing,
        )
        .await
        .expect("query");

        assert_eq!(rows.len(), 2);
        let targets: Vec<u64> = rows.iter().map(|r| r.target_id).collect();
        assert!(targets.contains(&y));
        assert!(targets.contains(&z));
        assert!(rows.iter().all(|r| r.source_id == x));
        assert!(rows.iter().all(|r| r.relation_type == "Prerequisite"));
        store.close().await.unwrap();
    }

    /// AC-09: Incoming edges with specific type.
    #[tokio::test]
    async fn test_query_direct_neighbors_incoming_specific_type() {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        let x = insert_entry(wp, "X", Status::Active, None, None).await;
        let y = insert_entry(wp, "Y", Status::Active, None, None).await;
        let z_entry = insert_entry(wp, "Z", Status::Active, None, None).await;

        insert_edge(wp, y, x, "Supports").await;
        insert_edge(wp, z_entry, x, "Supports").await;

        let rows = query_direct_neighbors(
            store.read_pool(),
            x,
            &["Supports"],
            NeighborDirection::Incoming,
        )
        .await
        .expect("query");

        assert_eq!(rows.len(), 2);
        let sources: Vec<u64> = rows.iter().map(|r| r.source_id).collect();
        assert!(sources.contains(&y));
        assert!(sources.contains(&z_entry));
        assert!(rows.iter().all(|r| r.target_id == x));
        store.close().await.unwrap();
    }

    /// Both directions union.
    #[tokio::test]
    async fn test_query_direct_neighbors_both_directions() {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        let x = insert_entry(wp, "X", Status::Active, None, None).await;
        let y = insert_entry(wp, "Y", Status::Active, None, None).await;
        let z_entry = insert_entry(wp, "Z", Status::Active, None, None).await;

        insert_edge(wp, x, y, "Supports").await; // outgoing from X
        insert_edge(wp, z_entry, x, "Informs").await; // incoming to X

        let rows = query_direct_neighbors(
            store.read_pool(),
            x,
            &["Supports", "Informs"],
            NeighborDirection::Both,
        )
        .await
        .expect("query");

        assert_eq!(rows.len(), 2);
        let has_outgoing = rows.iter().any(|r| r.source_id == x && r.target_id == y);
        let has_incoming = rows
            .iter()
            .any(|r| r.source_id == z_entry && r.target_id == x);
        assert!(has_outgoing, "outgoing edge X→Y must be included");
        assert!(has_incoming, "incoming edge Z→X must be included");
        store.close().await.unwrap();
    }

    /// AC-10, R-06: Empty type list excludes Supersedes silently.
    #[tokio::test]
    async fn test_query_direct_neighbors_empty_type_list_excludes_supersedes() {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        let x = insert_entry(wp, "X", Status::Active, None, None).await;
        let y = insert_entry(wp, "Y", Status::Active, None, None).await;
        let z_entry = insert_entry(wp, "Z", Status::Active, None, None).await;

        insert_edge(wp, x, y, "Supports").await;
        insert_edge(wp, x, z_entry, "Supersedes").await;

        let rows = query_direct_neighbors(store.read_pool(), x, &[], NeighborDirection::Both)
            .await
            .expect("query");

        let targets: Vec<u64> = rows.iter().map(|r| r.target_id).collect();
        assert!(targets.contains(&y), "Supports edge to Y must be returned");
        assert!(
            !targets.contains(&z_entry),
            "Supersedes edge must be excluded from empty-type-list query"
        );
        assert!(!rows.is_empty(), "result must not be empty");
        store.close().await.unwrap();
    }

    /// R-12, OQ-01: Non-existent anchor returns empty Vec, no error.
    #[tokio::test]
    async fn test_query_direct_neighbors_nonexistent_anchor_returns_empty() {
        let (store, _dir) = open_test_store().await;

        let rows = query_direct_neighbors(store.read_pool(), 999_999, &[], NeighborDirection::Both)
            .await
            .expect("no error for non-existent anchor — OQ-01 resolved");

        assert!(rows.is_empty());
        store.close().await.unwrap();
    }

    /// Entry exists but has no graph_edges rows.
    #[tokio::test]
    async fn test_query_direct_neighbors_zero_edges_from_anchor() {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        let id = insert_entry(wp, "Isolated", Status::Active, None, None).await;

        let rows = query_direct_neighbors(store.read_pool(), id, &[], NeighborDirection::Both)
            .await
            .expect("query");

        assert!(rows.is_empty());
        store.close().await.unwrap();
    }
}
