//! Graph enrichment background tick — S1 (tag co-occurrence), S2 (vocabulary),
//! and S8 (search co-retrieval) edge sources (crt-041).
//!
//! All three functions are infallible: errors are logged at warn!, tick continues.
//! S1 and S2 run every tick. S8 runs every `s8_batch_interval_ticks` ticks.
//!
//! Follows the co_access_promotion_tick.rs design pattern: direct write_pool_server(),
//! no rayon, tracing::info! summary on completion.

use std::collections::HashSet;

use unimatrix_core::Store;
use unimatrix_store::{EDGE_SOURCE_S1, EDGE_SOURCE_S2, EDGE_SOURCE_S8, counters};

use crate::infra::config::InferenceConfig;
use crate::services::nli_detection::{current_timestamp_secs, write_graph_edge};

// ---------------------------------------------------------------------------
// Module constants
// ---------------------------------------------------------------------------

/// Counters table key for the S8 audit_log watermark.
/// Stable: once written, must never change or S8 re-scans from 0.
const S8_WATERMARK_KEY: &str = "s8_audit_log_watermark";

/// SQLite parameter binding limit. Chunk IN-clause IDs to stay under this.
/// Reference: entry #3442.
const SQLITE_MAX_VARIABLE_NUMBER: usize = 900;

// ---------------------------------------------------------------------------
// Row types (module-private)
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct S1Row {
    source_id: i64,
    target_id: i64,
    shared_tags: i64,
}

#[derive(sqlx::FromRow)]
struct S2Row {
    source_id: i64,
    target_id: i64,
    shared_terms: i64,
}

#[derive(sqlx::FromRow)]
struct S8AuditRow {
    event_id: i64,
    target_ids: String,
}

// ---------------------------------------------------------------------------
// Public tick entry point
// ---------------------------------------------------------------------------

/// Top-level entry point called from background.rs after run_graph_inference_tick.
/// Runs S1, S2, then conditionally S8 in fixed order. Infallible.
pub(crate) async fn run_graph_enrichment_tick(
    store: &Store,
    config: &InferenceConfig,
    current_tick: u32,
) {
    let s1_written = run_s1_tick(store, config).await;
    let s2_written = run_s2_tick(store, config).await;
    let s8_written = run_s8_tick(store, config, current_tick).await;

    tracing::info!(
        s1_edges = s1_written,
        s2_edges = s2_written,
        s8_edges = s8_written,
        "graph_enrichment_tick complete"
    );
}

// ---------------------------------------------------------------------------
// S1 — tag co-occurrence Informs edges
// ---------------------------------------------------------------------------

/// S1 — tag co-occurrence `Informs` edges. Runs every tick.
/// Returns the count of new edges written.
pub(crate) async fn run_s1_tick(store: &Store, config: &InferenceConfig) -> u64 {
    // Dual-endpoint quarantine guard (C-03): JOIN entries on BOTH source and target
    // with status = 0. t2.entry_id > t1.entry_id produces each unordered pair once.
    // ORDER BY shared_tags DESC fills the cap with highest-signal pairs first.
    let rows_result = sqlx::query_as::<_, S1Row>(
        "SELECT t1.entry_id AS source_id,
                t2.entry_id AS target_id,
                COUNT(*)    AS shared_tags
         FROM entry_tags t1
         JOIN entry_tags t2 ON t2.tag = t1.tag AND t2.entry_id > t1.entry_id
         JOIN entries e1 ON e1.id = t1.entry_id AND e1.status = 0
         JOIN entries e2 ON e2.id = t2.entry_id AND e2.status = 0
         GROUP BY t1.entry_id, t2.entry_id
         HAVING COUNT(*) >= 3
         ORDER BY shared_tags DESC
         LIMIT ?1",
    )
    .bind(config.max_s1_edges_per_tick as i64)
    .fetch_all(store.write_pool_server())
    .await;

    let rows = match rows_result {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "S1 tick: batch fetch failed");
            return 0;
        }
    };

    if rows.is_empty() {
        return 0;
    }

    let now_ts = current_timestamp_secs();
    let mut edges_written: u64 = 0;

    for row in &rows {
        // Weight: min(shared_tag_count * 0.1, 1.0). Range [0.3, 1.0] given HAVING >= 3.
        let weight = f64::min(row.shared_tags as f64 * 0.1, 1.0) as f32;

        if write_graph_edge(
            store,
            row.source_id as u64,
            row.target_id as u64,
            "Informs",
            weight,
            now_ts,
            EDGE_SOURCE_S1,
            "",
        )
        .await
        {
            edges_written += 1;
        }
        // Second direction (crt-044, ADR-002): false on UNIQUE conflict is expected — C-09.
        if write_graph_edge(
            store,
            row.target_id as u64,
            row.source_id as u64,
            "Informs",
            weight,
            now_ts,
            EDGE_SOURCE_S1,
            "",
        )
        .await
        {
            edges_written += 1;
        }
    }

    tracing::info!(edges_written, candidates = rows.len(), "S1 tick complete");
    edges_written
}

// ---------------------------------------------------------------------------
// S2 — structural vocabulary Informs edges
// ---------------------------------------------------------------------------

/// S2 — structural vocabulary `Informs` edges. Runs every tick.
/// Immediate no-op when `config.s2_vocabulary` is empty (operator opt-in).
/// Returns the count of new edges written.
pub(crate) async fn run_s2_tick(store: &Store, config: &InferenceConfig) -> u64 {
    if config.s2_vocabulary.is_empty() {
        tracing::debug!("S2 tick: vocabulary empty, no-op");
        return 0;
    }

    // SECURITY: vocabulary terms are ALWAYS bound via push_bind (sqlx QueryBuilder).
    // Terms are NEVER interpolated into the SQL string via format!() or string concat.
    // A term containing ', --, or ; does not affect SQL structure (ADR-002, SR-01, C-05).
    //
    // Space-padded instr() pattern prevents substring false positives:
    //   "api" does NOT match "capabilities"; " api " does NOT appear in " capabilities ".
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT source_id, target_id, (s1_terms + s2_terms) AS shared_terms FROM (\
         SELECT e1.id AS source_id, e2.id AS target_id, (",
    );

    let term_count = config.s2_vocabulary.len();

    for (i, term) in config.s2_vocabulary.iter().enumerate() {
        qb.push(
            "CASE WHEN instr(lower(' ' || e1.content || ' ' || e1.title || ' '), \
             lower(' ' || ",
        );
        qb.push_bind(term.as_str());
        qb.push(" || ' ')) > 0 THEN 1 ELSE 0 END");
        if i < term_count - 1 {
            qb.push(" + ");
        }
    }
    qb.push(") AS s1_terms, (");

    for (i, term) in config.s2_vocabulary.iter().enumerate() {
        qb.push(
            "CASE WHEN instr(lower(' ' || e2.content || ' ' || e2.title || ' '), \
             lower(' ' || ",
        );
        qb.push_bind(term.as_str());
        qb.push(" || ' ')) > 0 THEN 1 ELSE 0 END");
        if i < term_count - 1 {
            qb.push(" + ");
        }
    }
    // Dual-endpoint quarantine guard: e2.id > e1.id AND both status = 0.
    qb.push(
        ") AS s2_terms \
         FROM entries e1 \
         JOIN entries e2 ON e2.id > e1.id \
         AND e1.status = 0 \
         AND e2.status = 0) \
         WHERE s1_terms + s2_terms >= 2 \
         ORDER BY shared_terms DESC \
         LIMIT ",
    );
    qb.push_bind(config.max_s2_edges_per_tick as i64);

    let rows = match qb
        .build_query_as::<S2Row>()
        .fetch_all(store.write_pool_server())
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(vocabulary_size = config.s2_vocabulary.len(), error = %e, "S2 tick: query failed");
            return 0;
        }
    };

    if rows.is_empty() {
        return 0;
    }

    let now_ts = current_timestamp_secs();
    let mut edges_written: u64 = 0;

    for row in &rows {
        // Weight: min(shared_term_count * 0.1, 1.0). shared_terms >= 2 so weight >= 0.2.
        let weight = f64::min(row.shared_terms as f64 * 0.1, 1.0) as f32;

        if write_graph_edge(
            store,
            row.source_id as u64,
            row.target_id as u64,
            "Informs",
            weight,
            now_ts,
            EDGE_SOURCE_S2,
            "",
        )
        .await
        {
            edges_written += 1;
        }
        // Second direction (crt-044, ADR-002): false on UNIQUE conflict is expected — C-09.
        if write_graph_edge(
            store,
            row.target_id as u64,
            row.source_id as u64,
            "Informs",
            weight,
            now_ts,
            EDGE_SOURCE_S2,
            "",
        )
        .await
        {
            edges_written += 1;
        }
    }

    tracing::info!(
        edges_written,
        candidates = rows.len(),
        vocabulary_size = config.s2_vocabulary.len(),
        "S2 tick complete"
    );
    edges_written
}

// ---------------------------------------------------------------------------
// S8 — search co-retrieval CoAccess edges
// ---------------------------------------------------------------------------

/// S8 — search co-retrieval `CoAccess` edges.
/// Gated: returns 0 immediately when `current_tick % s8_batch_interval_ticks != 0`.
/// Returns the count of new edges written.
pub(crate) async fn run_s8_tick(store: &Store, config: &InferenceConfig, current_tick: u32) -> u64 {
    // Gate: s8_batch_interval_ticks >= 1 guaranteed by validate() — no % 0 risk.
    if !current_tick.is_multiple_of(config.s8_batch_interval_ticks) {
        return 0;
    }

    // Phase 1: Load watermark (0 if absent — first run).
    let watermark: u64 =
        match counters::read_counter(store.write_pool_server(), S8_WATERMARK_KEY).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "S8 tick: failed to read watermark; skipping");
                return 0;
            }
        };

    // Phase 2: Fetch context_search success rows above the watermark.
    let fetch_limit = (config.max_s8_pairs_per_batch * 2) as i64;
    let audit_rows = match sqlx::query_as::<_, S8AuditRow>(
        "SELECT event_id, target_ids
         FROM audit_log
         WHERE operation = 'context_search'
           AND outcome = 0
           AND event_id > ?1
         ORDER BY event_id ASC
         LIMIT ?2",
    )
    .bind(watermark as i64)
    .bind(fetch_limit)
    .fetch_all(store.write_pool_server())
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "S8 tick: audit_log fetch failed; skipping");
            return 0;
        }
    };

    if audit_rows.is_empty() {
        return 0;
    }

    // Phase 3: Expand pairs from audit rows, respecting the pair cap (C-12).
    // Malformed JSON rows advance the watermark but produce no pairs (C-14).
    let mut pairs: Vec<(u64, u64)> = Vec::new();
    let mut new_watermark: u64 = watermark;
    let cap = config.max_s8_pairs_per_batch;

    'rows: for row in &audit_rows {
        let event_id = row.event_id as u64;

        let entry_ids: Vec<u64> = match serde_json::from_str::<Vec<u64>>(&row.target_ids) {
            Ok(ids) => ids,
            Err(e) => {
                // Malformed JSON: advance watermark past this row to prevent infinite re-scan.
                tracing::warn!(
                    event_id,
                    target_ids = %row.target_ids,
                    error = %e,
                    "S8 tick: malformed target_ids JSON; advancing watermark past row"
                );
                new_watermark = event_id;
                continue 'rows;
            }
        };

        // Build unordered pairs (a < b).
        let mut row_pairs: Vec<(u64, u64)> = Vec::new();
        for i in 0..entry_ids.len() {
            for j in (i + 1)..entry_ids.len() {
                let a = entry_ids[i].min(entry_ids[j]);
                let b = entry_ids[i].max(entry_ids[j]);
                row_pairs.push((a, b));
            }
        }

        // Zero-pair rows (singleton/empty): advance watermark and continue.
        if row_pairs.is_empty() {
            new_watermark = event_id;
            continue 'rows;
        }

        let remaining = cap.saturating_sub(pairs.len());
        if remaining == 0 {
            // Cap already reached by prior rows; stop.
            break 'rows;
        }

        if row_pairs.len() <= remaining {
            // Full row fits: accept all pairs and advance watermark.
            pairs.extend(row_pairs);
            new_watermark = event_id;
        } else {
            // Partial row: take only up to the cap; do NOT advance watermark (C-12).
            // Watermark stays at last fully-processed row's event_id.
            pairs.extend(row_pairs.into_iter().take(remaining));
            break 'rows;
        }
    }

    if pairs.is_empty() {
        // Only parse-skipped rows processed; update watermark if it advanced.
        if new_watermark > watermark {
            let _ = counters::set_counter(store.write_pool_server(), S8_WATERMARK_KEY, new_watermark)
                .await
                .map_err(|e| tracing::warn!(error = %e, new_watermark, "S8 tick: failed to update watermark after parse-skip"));
        }
        return 0;
    }

    // Phase 4: Bulk quarantine filter — chunked to stay under SQLite 999-param limit (C-13).
    let all_ids: Vec<u64> = {
        let mut id_set = std::collections::BTreeSet::<u64>::new();
        for (a, b) in &pairs {
            id_set.insert(*a);
            id_set.insert(*b);
        }
        id_set.into_iter().collect()
    };

    let mut valid_ids: HashSet<u64> = HashSet::new();
    for chunk in all_ids.chunks(SQLITE_MAX_VARIABLE_NUMBER) {
        let mut qb = sqlx::QueryBuilder::new("SELECT id FROM entries WHERE status = 0 AND id IN (");
        let mut sep = qb.separated(", ");
        for id in chunk {
            sep.push_bind(*id as i64);
        }
        qb.push(")");

        match qb
            .build_query_scalar::<i64>()
            .fetch_all(store.write_pool_server())
            .await
        {
            Ok(ids) => ids.into_iter().for_each(|id| {
                valid_ids.insert(id as u64);
            }),
            Err(e) => {
                // Cannot safely filter quarantined entries — skip entire batch.
                tracing::warn!(error = %e, "S8 tick: bulk quarantine filter failed; skipping batch");
                return 0;
            }
        }
    }

    // Phase 5: Write edges — ALL writes before watermark update (C-11).
    let now_ts = current_timestamp_secs();
    let mut pairs_written: u64 = 0;
    let mut pairs_skipped: u64 = 0;

    for (a, b) in &pairs {
        if !valid_ids.contains(a) || !valid_ids.contains(b) {
            pairs_skipped += 1;
            continue;
        }

        if write_graph_edge(
            store,
            *a,
            *b,
            "CoAccess",
            0.25_f32,
            now_ts,
            EDGE_SOURCE_S8,
            "",
        )
        .await
        {
            pairs_written += 1;
        }
        // Second direction (crt-044, ADR-002): false on UNIQUE conflict is expected — C-09.
        // pairs_written counts per-edge (C-06): incremented only on true return.
        if write_graph_edge(
            store,
            *b,
            *a,
            "CoAccess",
            0.25_f32,
            now_ts,
            EDGE_SOURCE_S8,
            "",
        )
        .await
        {
            pairs_written += 1;
        }
    }

    // Phase 6: Update watermark after all edge writes (C-11).
    if let Err(e) =
        counters::set_counter(store.write_pool_server(), S8_WATERMARK_KEY, new_watermark).await
    {
        tracing::warn!(error = %e, new_watermark, "S8 tick: failed to update watermark; batch will re-process");
    }

    tracing::info!(
        pairs_written,
        pairs_skipped_quarantined = pairs_skipped,
        new_watermark,
        "S8 tick complete"
    );
    pairs_written
}

// ---------------------------------------------------------------------------
// Tests (extracted to separate file to keep this module under 500 lines)
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "graph_enrichment_tick_tests.rs"]
mod tests;
