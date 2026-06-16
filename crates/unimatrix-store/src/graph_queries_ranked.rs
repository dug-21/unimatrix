//! Ranked + split-count read path for the `context_get` next-hop edge affordance (vnc-037).
//!
//! Two get-only functions that share **one byte-identical canonicalization CTE**
//! (`nbr` → `canon` → `deduped`) so the displayed set and the totals can never
//! disagree about how a symmetric edge collapses (ADR-007, R-03 parity):
//!
//! - [`query_ranked_neighbors`] — canonicalizes symmetric types (`Contradicts` /
//!   `CoAccess` / `Informs`) to one `↔` row **before** ranking, `LEFT JOIN`s the
//!   target entry's `confidence`, then
//!   `ORDER BY (source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC
//!   LIMIT ?` with `?` bound to [`GET_EDGE_DISPLAY_LIMIT`] (never a literal 3).
//!   Returns the **≤ cap displayed rows** — the hub's full fan-out is never
//!   materialized (C-7 / SR-14).
//! - [`count_neighbors_split`] — the same `deduped` CTE, then a single
//!   `SUM(CASE…)` aggregate splitting the **uncapped** totals into three direction
//!   buckets (`inbound`, `outbound`, `both`) plus a digest-only `authored` tally. A
//!   `↔` edge counts **once** in its own `both` bucket and is **never** folded into
//!   `inbound` (#744 inbound-degree integrity, ADR-005 TOTALS BUCKET CONTRACT).
//!
//! Both functions live in this one module precisely so the canonicalization CTE is
//! authored once ([`CANON_CTE`]) and reused identically — drift between them would
//! re-introduce a double-count on one surface only.
//!
//! **FR-19 fail-loud**: both functions return `Result<_, StoreError>` and use no
//! `.unwrap()`/`.expect()`; any sqlx/`try_get` failure propagates so the
//! `context_get` handler can map it as a primary-read error.
//!
//! **Security**: the anchor and the cap are positional binds (`?1`, `?2`); the
//! canonicalization `CASE`, the symmetric-type `IN (…)` set, the `!= 'Supersedes'`
//! filter, the `ORDER BY`, and the `LIMIT` keyword are static SQL — never assembled
//! from input.

use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::error::StoreError;
use crate::graph_queries::RawEdgeRow;
use crate::read::GET_EDGE_DISPLAY_LIMIT;

/// Split, **uncapped** edge totals over the canonicalized (`deduped`) neighbor set.
///
/// Three direction buckets — each canonical row counted in exactly **one** —
/// plus a digest-only `authored` tally (ADR-005 TOTALS BUCKET CONTRACT, 2026-06-16):
///
/// - `inbound`  — asymmetric inbound **only** (a `↔` edge is NOT folded in; the old
///   `IN ('inbound','both')` fold is retired — #744 inbound-degree integrity).
/// - `outbound` — asymmetric outbound.
/// - `both`     — canonicalized symmetric edges (`↔`), counted **once** in their own
///   bucket.
/// - `authored` — `SUM(source = 'agent')` over the same `deduped` set; **digest only**
///   (feeds the summary line's `(K authored)`), never a JSON/markdown key.
///
/// All four are post-canonicalization, uncapped, and each edge is counted exactly once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeCountSplit {
    /// Asymmetric inbound degree only (`↔` excluded — #744 integrity).
    pub inbound: usize,
    /// Asymmetric outbound degree.
    pub outbound: usize,
    /// Canonicalized symmetric edges (`↔`), counted once.
    pub both: usize,
    /// `source = 'agent'` count over the full deduped set; digest only.
    pub authored: usize,
}

/// The shared canonicalization CTE (`nbr` → `canon` → `deduped`) — D-10 / ADR-007.
///
/// Authored **once** here and embedded byte-identically into both the ranked select
/// and the split count so the two surfaces cannot diverge on symmetric-edge collapse
/// (R-03 parity). `?1` is the anchor (positional bind). The symmetric-type set, the
/// `!= 'Supersedes'` filter, and the canonicalization `CASE` are static SQL.
///
/// The `deduped` projection exposes `relation_type`, `source`, `other_id`, and the
/// computed `direction` (`both` for symmetric, else the matched leg). Symmetric rows
/// of one unordered pair share `(relation_type, pair_lo, pair_hi, 1)` and collapse to
/// a single group row; asymmetric rows group on their distinct `other_id` and are
/// never merged across distinct neighbors.
const CANON_CTE: &str = "WITH nbr AS (
        SELECT source_id, target_id, target_id AS other_id, relation_type, source,
               'outbound' AS leg
        FROM graph_edges
        WHERE source_id = ?1 AND relation_type != 'Supersedes'
        UNION ALL
        SELECT source_id, target_id, source_id AS other_id, relation_type, source,
               'inbound' AS leg
        FROM graph_edges
        WHERE target_id = ?1 AND relation_type != 'Supersedes'
    ),
    canon AS (
        SELECT relation_type, source, other_id,
               CASE WHEN relation_type IN ('Contradicts','CoAccess','Informs')
                    THEN 'both' ELSE leg END AS direction,
               MIN(?1, other_id) AS pair_lo,
               MAX(?1, other_id) AS pair_hi
        FROM nbr
    ),
    deduped AS (
        SELECT relation_type, source, other_id, direction
        FROM canon
        GROUP BY relation_type, pair_lo, pair_hi,
                 CASE WHEN direction = 'both' THEN 1 ELSE other_id END
    )";

/// The **get-only ranked variant** of the depth-1 neighbor read.
///
/// Canonicalizes symmetric edges to one `↔` row, `LEFT JOIN`s the target entry's
/// `confidence`, then ranks authored-first and inferred-by-target-confidence,
/// returning at most [`GET_EDGE_DISPLAY_LIMIT`] rows (ADR-001/006/007, C-7/C-8).
///
/// Lives beside, and never mutates, `query_direct_neighbors` — the canonicalization
/// and confidence JOIN are get-only and must not leak into the shared neighbors path.
///
/// Ranking is by **target-entry `entries.confidence`**, never `graph_edges.weight`
/// (frozen / non-discriminating per ass-079, C-8). The `LIMIT ?` binds the single
/// named cap constant — there is no literal `3` in the statement.
///
/// Each returned [`RawEdgeRow`] carries `source_id = id` (the anchor), `target_id =`
/// the other endpoint, the relation type, the provenance `source`, and
/// `target_confidence` (`None` for a dangling target — the `LEFT JOIN` retains it,
/// ranked last via `NULLS LAST`). The SQL-computed `direction` is returned alongside
/// (see [`RankedEdge`]) so the projection never re-derives a `↔` collapse in Rust.
///
/// A non-existent `id` returns an empty `Vec` — not an error (ADR-001).
pub async fn query_ranked_neighbors(
    pool: &SqlitePool,
    id: u64,
) -> Result<Vec<RankedEdge>, StoreError> {
    let sql = format!(
        "{CANON_CTE}
        SELECT d.relation_type, d.source, d.other_id AS target_id, d.direction,
               t.confidence AS target_confidence
        FROM deduped d
        LEFT JOIN entries t ON t.id = d.other_id
        ORDER BY (d.source = 'agent') DESC,
                 t.confidence DESC NULLS LAST,
                 target_id ASC
        LIMIT ?2"
    );

    let rows = sqlx::query(&sql)
        .bind(id as i64) // ?1 — anchor (both legs + MIN/MAX)
        .bind(GET_EDGE_DISPLAY_LIMIT) // ?2 — cap; bound, never inlined
        .fetch_all(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

    rows.iter().map(|row| map_ranked_row(row, id)).collect()
}

/// A ranked neighbor row: the projected [`RawEdgeRow`] plus the SQL-computed
/// canonical `direction` (`"both"` / `"inbound"` / `"outbound"`).
///
/// The `direction` is carried out of SQL (option (a) in the pseudocode) so the
/// get-edge projection reads the canonicalization decision directly — a `↔` row is
/// never re-derived as `→`/`←` in Rust, and the symmetric-type list stays in exactly
/// one place (the [`CANON_CTE`] `CASE`).
#[derive(Debug, Clone)]
pub struct RankedEdge {
    /// The neighbor row. `source_id` is the anchor; `target_id` is the other endpoint.
    pub row: RawEdgeRow,
    /// Canonical direction computed in SQL: `"both"` (↔), `"inbound"` (←), `"outbound"` (→).
    pub direction: String,
}

/// Map one ranked SQL row to a [`RankedEdge`]. `anchor` is always the projected
/// row's `source_id`. No `.unwrap()` — every `try_get` maps to `StoreError::Database`.
fn map_ranked_row(row: &sqlx::sqlite::SqliteRow, anchor: u64) -> Result<RankedEdge, StoreError> {
    let target_id = row
        .try_get::<i64, _>("target_id")
        .map_err(|e| StoreError::Database(e.into()))? as u64;
    let relation_type = row
        .try_get("relation_type")
        .map_err(|e| StoreError::Database(e.into()))?;
    let source = row
        .try_get("source")
        .map_err(|e| StoreError::Database(e.into()))?;
    let target_confidence = row
        .try_get::<Option<f64>, _>("target_confidence")
        .map_err(|e| StoreError::Database(e.into()))?;
    let direction = row
        .try_get::<String, _>("direction")
        .map_err(|e| StoreError::Database(e.into()))?;

    Ok(RankedEdge {
        row: RawEdgeRow {
            source_id: anchor,
            target_id,
            relation_type,
            source,
            target_confidence,
        },
        direction,
    })
}

/// The **split, uncapped** edge totals over the same canonicalized set as
/// [`query_ranked_neighbors`].
///
/// A single `SUM(CASE…)` aggregate over the shared `deduped` CTE — three direction
/// buckets plus a digest-only `authored` tally. It never references
/// [`GET_EDGE_DISPLAY_LIMIT`] (the totals are uncapped, C-12) and never materializes
/// rows (C-7/SR-14): it is one round trip returning four scalars.
///
/// A `↔` edge contributes `both += 1` and leaves `inbound` unchanged (#744
/// regression guard). `SUM(...)` over zero rows is `NULL` in SQLite, so every
/// aggregate is `COALESCE(…, 0)` and a zero-edge / non-existent `id` yields
/// `{0, 0, 0, 0}` — not an error.
pub async fn count_neighbors_split(
    pool: &SqlitePool,
    id: u64,
) -> Result<EdgeCountSplit, StoreError> {
    let sql = format!(
        "{CANON_CTE}
        SELECT
            COALESCE(SUM(CASE WHEN direction = 'inbound'  THEN 1 ELSE 0 END), 0) AS inbound,
            COALESCE(SUM(CASE WHEN direction = 'outbound' THEN 1 ELSE 0 END), 0) AS outbound,
            COALESCE(SUM(CASE WHEN direction = 'both'     THEN 1 ELSE 0 END), 0) AS both,
            COALESCE(SUM(CASE WHEN source = 'agent'       THEN 1 ELSE 0 END), 0) AS authored
        FROM deduped"
    );

    let row = sqlx::query(&sql)
        .bind(id as i64) // ?1 — anchor (both legs + MIN/MAX)
        .fetch_one(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

    let inbound = row
        .try_get::<i64, _>("inbound")
        .map_err(|e| StoreError::Database(e.into()))? as usize;
    let outbound = row
        .try_get::<i64, _>("outbound")
        .map_err(|e| StoreError::Database(e.into()))? as usize;
    let both = row
        .try_get::<i64, _>("both")
        .map_err(|e| StoreError::Database(e.into()))? as usize;
    let authored = row
        .try_get::<i64, _>("authored")
        .map_err(|e| StoreError::Database(e.into()))? as usize;

    Ok(EdgeCountSplit {
        inbound,
        outbound,
        both,
        authored,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "graph_queries_ranked_tests.rs"]
mod tests;
