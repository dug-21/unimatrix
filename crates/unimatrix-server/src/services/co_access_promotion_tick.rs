//! Recurring co_access → GRAPH_EDGES promotion tick (crt-034).
//!
//! Promotes qualifying `co_access` pairs (count >= CO_ACCESS_GRAPH_MIN_COUNT) into
//! `GRAPH_EDGES` as `CoAccess`-typed edges. For already-promoted edges, refreshes
//! the normalized weight when drift exceeds `CO_ACCESS_WEIGHT_UPDATE_DELTA`.
//!
//! # Design constraints
//! - Infallible: `async fn ... -> ()`. All errors logged at `warn!`, tick continues.
//! - Direct write pool path only (ADR-001/#3821): `AnalyticsWrite::GraphEdge` cannot
//!   express conditional UPDATE semantics and must not be used.
//! - No rayon pool: pure SQL, no ML inference.
//! - One-directional edges v1 (ADR-006): `source_id = entry_id_a`, `target_id = entry_id_b`.

use unimatrix_core::Store;
use unimatrix_store::{CO_ACCESS_GRAPH_MIN_COUNT, EDGE_SOURCE_CO_ACCESS};

use crate::infra::config::InferenceConfig;

// ---------------------------------------------------------------------------
// Module-level constants
// ---------------------------------------------------------------------------

/// Minimum absolute weight difference required to trigger an UPDATE on an
/// already-promoted CoAccess edge.
///
/// ADR-003 (#3825): f64, NOT f32. sqlx fetches SQLite REAL columns as f64.
/// Comparing a fetched weight (f64) against 0.1f32 cast to f64 produces
/// 0.100000001490116..., which would incorrectly treat a delta of exactly 0.1
/// as exceeding the threshold. Using f64 avoids this precision noise.
///
/// Not operator-configurable: this is a calibrated noise floor, not a domain
/// policy parameter.
const CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1;

/// Number of early ticks during which a zero qualifying-pair count triggers a
/// `warn!` log (SR-05 signal-loss detectability, ADR-005/#3827).
///
/// Defined here (not in background.rs) to avoid the visibility issue flagged
/// in Gate 3a OQ-4. Consumed by `run_co_access_promotion_tick` via the
/// `current_tick` parameter.
pub(crate) const PROMOTION_EARLY_RUN_WARN_TICKS: u32 = 5;

// ---------------------------------------------------------------------------
// Row type (module-private)
// ---------------------------------------------------------------------------

/// One row from the batch candidate SELECT.
///
/// `max_count` is `Option<i64>` because the scalar subquery returns NULL when
/// the `co_access` table has no rows passing the `WHERE count >= threshold`
/// filter. A NULL `max_count` signals an empty qualifying set → early return
/// (eliminates division-by-zero risk).
#[derive(sqlx::FromRow)]
struct CoAccessBatchRow {
    entry_id_a: i64,
    entry_id_b: i64,
    count: i64,
    max_count: Option<i64>,
}

// ---------------------------------------------------------------------------
// Public tick function
// ---------------------------------------------------------------------------

/// Recurring background tick: promote qualifying `co_access` pairs into
/// `GRAPH_EDGES` as `CoAccess`-typed edges and refresh stale weights.
///
/// Infallible. Write errors are logged at `warn!` and the tick continues.
/// Always emits a `tracing::info!` with inserted/updated counts at the end.
///
/// # SR-05 early-tick detection
/// When `qualifying_count == 0 AND current_tick < PROMOTION_EARLY_RUN_WARN_TICKS`,
/// a `warn!` is emitted to surface the GH #409 race condition (ADR-005/#3827).
pub(crate) async fn run_co_access_promotion_tick(
    store: &Store,
    config: &InferenceConfig,
    current_tick: u32,
) {
    // Phase 1: Batch fetch qualifying pairs with embedded global MAX normalization.
    //
    // Single SQL round-trip (ADR-001/#3823). The scalar subquery computes
    // MAX(count) over ALL qualifying pairs (not just the capped batch), ensuring
    // weight normalization is globally consistent regardless of the LIMIT cap.
    // ORDER BY count DESC guarantees highest-signal pairs are selected first.
    let rows_result = sqlx::query_as::<_, CoAccessBatchRow>(
        "SELECT
             entry_id_a,
             entry_id_b,
             count,
             (SELECT MAX(count) FROM co_access WHERE count >= ?1) AS max_count
         FROM co_access
         WHERE count >= ?1
         ORDER BY count DESC
         LIMIT ?2",
    )
    .bind(CO_ACCESS_GRAPH_MIN_COUNT) // ?1: i64 = 3
    .bind(config.max_co_access_promotion_per_tick as i64) // ?2: i64 cap
    .fetch_all(store.write_pool_server())
    .await;

    let rows = match rows_result {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "co_access promotion tick: batch fetch failed");
            tracing::info!(
                inserted = 0,
                updated = 0,
                "co_access promotion tick complete (fetch error)"
            );
            return;
        }
    };

    let qualifying_count = rows.len();

    // SR-05 early-tick detectability (ADR-005): emit warn! only when BOTH:
    //   1. qualifying_count == 0 (no pairs meet the threshold)
    //   2. current_tick < PROMOTION_EARLY_RUN_WARN_TICKS (within early-run window)
    // Outside this window, zero qualifying rows produces only the info! log.
    if qualifying_count == 0 && current_tick < PROMOTION_EARLY_RUN_WARN_TICKS {
        tracing::warn!(
            current_tick = current_tick,
            warn_window = PROMOTION_EARLY_RUN_WARN_TICKS,
            "co_access promotion tick: zero qualifying pairs in early-tick window — \
             verify GH #409 has not pruned co_access before crt-034 deployed (SR-05)"
        );
    }

    if qualifying_count == 0 {
        tracing::info!(
            inserted = 0,
            updated = 0,
            "co_access promotion tick complete"
        );
        return;
    }

    // Phase 2: Extract global max_count from first row.
    // Safe: rows is non-empty. max_count is Some because the WHERE count >= ?1
    // predicate in the outer query guarantees at least one qualifying row, so the
    // same predicate in the subquery also finds it. unwrap_or(1) is belt-and-suspenders.
    let max_count = rows[0].max_count.unwrap_or(1);

    if max_count <= 0 {
        // Degenerate: counts are 0 or negative despite count >= 3 filter.
        // Guard against data corruption without panicking.
        tracing::warn!("co_access promotion tick: max_count <= 0 despite non-empty rows; skipping");
        tracing::info!(
            inserted = 0,
            updated = 0,
            "co_access promotion tick complete (degenerate max)"
        );
        return;
    }

    // Phase 3: Per-pair two-step write.
    // INSERT OR IGNORE per pair; on no-op (rows_affected == 0), check weight delta
    // and UPDATE only if delta exceeds CO_ACCESS_WEIGHT_UPDATE_DELTA.
    // Errors on individual pairs are logged and skipped (infallible contract).
    // No transaction: pairs are independent; partial completion is acceptable.
    let mut inserted_count: usize = 0;
    let mut updated_count: usize = 0;

    for row in &rows {
        let new_weight: f64 = row.count as f64 / max_count as f64;
        // new_weight is in (0.0, 1.0] given 0 < row.count <= max_count.

        // Step A: INSERT OR IGNORE.
        // Inserts the edge if (source_id, target_id, 'CoAccess') is not already
        // in graph_edges. On UNIQUE constraint conflict, SQLite silently ignores
        // the INSERT and rows_affected = 0.
        let insert_result = sqlx::query(
            "INSERT OR IGNORE INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (?1, ?2, 'CoAccess', ?3, strftime('%s','now'), 'tick', ?4, 0)",
        )
        .bind(row.entry_id_a) // ?1: source_id (ADR-006: one direction only, min-id first)
        .bind(row.entry_id_b) // ?2: target_id
        .bind(new_weight) // ?3: REAL, normalized [0.0, 1.0]
        .bind(EDGE_SOURCE_CO_ACCESS) // ?4: "co_access"
        .execute(store.write_pool_server())
        .await;

        let insert_result = match insert_result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    entry_id_a = row.entry_id_a,
                    entry_id_b = row.entry_id_b,
                    error = %e,
                    "co_access promotion tick: INSERT failed; skipping pair"
                );
                continue;
            }
        };

        if insert_result.rows_affected() > 0 {
            // New edge inserted.
            inserted_count += 1;
            continue;
        }

        // rows_affected == 0: edge already exists (INSERT was a no-op).
        // Step B: Fetch the current stored weight to check for drift.
        let fetch_result = sqlx::query_scalar::<_, f64>(
            "SELECT weight FROM graph_edges
             WHERE source_id = ?1 AND target_id = ?2 AND relation_type = 'CoAccess'",
        )
        .bind(row.entry_id_a)
        .bind(row.entry_id_b)
        .fetch_optional(store.write_pool_server())
        .await;

        let existing_weight = match fetch_result {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(
                    entry_id_a = row.entry_id_a,
                    entry_id_b = row.entry_id_b,
                    error = %e,
                    "co_access promotion tick: weight fetch failed; skipping update"
                );
                continue;
            }
        };

        let existing_weight = match existing_weight {
            Some(w) => w,
            None => {
                // Edge disappeared between INSERT no-op and this fetch (race with deletion).
                // Harmless: skip, will be re-evaluated on next tick.
                continue;
            }
        };

        // Delta guard: suppress churn for small weight changes (ADR-003/#3825).
        // Strict greater-than: delta exactly equal to CO_ACCESS_WEIGHT_UPDATE_DELTA
        // is NOT updated (E-05 boundary: |0.6 - 0.5| = 0.1 → no update).
        let delta = (new_weight - existing_weight).abs();
        if delta <= CO_ACCESS_WEIGHT_UPDATE_DELTA {
            continue;
        }

        // Step C: UPDATE the weight.
        let update_result = sqlx::query(
            "UPDATE graph_edges
             SET weight = ?1
             WHERE source_id = ?2 AND target_id = ?3 AND relation_type = 'CoAccess'",
        )
        .bind(new_weight) // ?1: new normalized weight (f64)
        .bind(row.entry_id_a) // ?2
        .bind(row.entry_id_b) // ?3
        .execute(store.write_pool_server())
        .await;

        match update_result {
            Ok(_) => {
                updated_count += 1;
            }
            Err(e) => {
                tracing::warn!(
                    entry_id_a = row.entry_id_a,
                    entry_id_b = row.entry_id_b,
                    new_weight = new_weight,
                    error = %e,
                    "co_access promotion tick: weight UPDATE failed"
                );
            }
        }
    }

    // Phase 4: Summary log (FR-10). Always emits, even if all writes failed.
    tracing::info!(
        inserted = inserted_count,
        updated = updated_count,
        qualifying = qualifying_count,
        "co_access promotion tick complete"
    );
}

// ---------------------------------------------------------------------------
// Tests (extracted to separate file to keep this module under 500 lines)
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "co_access_promotion_tick_tests.rs"]
mod tests;
