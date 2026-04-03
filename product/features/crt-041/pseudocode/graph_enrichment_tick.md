# Pseudocode: graph_enrichment_tick

## Purpose

New module `crates/unimatrix-server/src/services/graph_enrichment_tick.rs`.

Implements three infallible background tick functions (S1, S2, S8) plus the orchestrating
`run_graph_enrichment_tick` entry point. Follows the `co_access_promotion_tick.rs` pattern:
direct `write_pool_server()`, no rayon, no ML, no spawn_blocking, `tracing::info!` summary on
completion, `tracing::warn!` on errors.

Module must stay under 500 lines (C-09). If tests push it over, extract tests to
`graph_enrichment_tick_tests.rs` sibling file with `#[cfg(test)] #[path = "..."] mod tests;`.

## File Header

```
//! Graph enrichment background tick — S1 (tag co-occurrence), S2 (vocabulary),
//! and S8 (search co-retrieval) edge sources (crt-041).
//!
//! All three functions are infallible: errors are logged at warn!, tick continues.
//! S1 and S2 run every tick. S8 runs every `s8_batch_interval_ticks` ticks.
//!
//! Follows the co_access_promotion_tick.rs design pattern: direct write_pool_server(),
//! no rayon, tracing::info! summary on completion.
```

## Imports

```
use std::collections::HashSet;
use serde_json;
use unimatrix_core::Store;
use unimatrix_store::{EDGE_SOURCE_S1, EDGE_SOURCE_S2, EDGE_SOURCE_S8, counters};
use crate::infra::config::InferenceConfig;
use crate::services::nli_detection::write_graph_edge;
```

## Module Constants

```
/// Counters table key for the S8 audit_log watermark.
/// Stores the last-processed event_id as a u64 (SQLite INTEGER).
/// Key is stable: once written, must never change or S8 re-scans from 0.
const S8_WATERMARK_KEY: &str = "s8_audit_log_watermark";

/// SQLite parameter binding limit. Chunk IN-clause IDs to stay under this.
/// Reference: entry #3442. Must be < 999 to leave room for other params in the query.
const SQLITE_MAX_VARIABLE_NUMBER: usize = 900;
```

## Row Types (module-private)

```
/// One row from the S1 self-join query.
#[derive(sqlx::FromRow)]
struct S1Row {
    source_id: i64,
    target_id: i64,
    shared_tags: i64,
}

/// One row from the S2 vocabulary query.
#[derive(sqlx::FromRow)]
struct S2Row {
    source_id: i64,
    target_id: i64,
    shared_terms: i64,
}

/// One row from the S8 audit_log query.
#[derive(sqlx::FromRow)]
struct S8AuditRow {
    event_id: i64,
    target_ids: String,
}
```

---

## Function: `run_graph_enrichment_tick`

Top-level entry point called from `background.rs` after `run_graph_inference_tick`.

```
pub(crate) async fn run_graph_enrichment_tick(
    store: &Store,
    config: &InferenceConfig,
    current_tick: u64,
) {
    // S1: always
    run_s1_tick(store, config).await;

    // S2: always (run_s2_tick is a no-op when s2_vocabulary is empty)
    run_s2_tick(store, config).await;

    // S8: gated by tick interval
    // current_tick cast to u64 for modulo; s8_batch_interval_ticks is u32.
    // Safe: u64 % u64 with s8_batch_interval_ticks >= 1 (validate() guarantee).
    if current_tick % (config.s8_batch_interval_ticks as u64) == 0 {
        run_s8_tick(store, config).await;
    }
}
```

---

## Function: `run_s1_tick`

S1 — tag co-occurrence `Informs` edges. Runs every tick.

```
pub(crate) async fn run_s1_tick(store: &Store, config: &InferenceConfig) {
    // Phase 1: Fetch qualifying tag co-occurrence pairs.
    //
    // Self-join on entry_tags: t2.entry_id > t1.entry_id ensures each unordered
    // pair appears exactly once (avoids (a,b) and (b,a) as separate rows).
    //
    // Dual-endpoint quarantine guard (C-03): JOIN entries on BOTH source and target,
    // filtering status = 0 (Active). Status::Active = 0 as i64.
    //
    // ORDER BY shared_tags DESC ensures highest-signal pairs fill the cap first.
    // LIMIT applies the per-tick cap before rows are returned to Rust.
    //
    // NOTE (R-04 / OQ-01): Verify via EXPLAIN QUERY PLAN that idx_entry_tags_tag
    // is used for the self-join equality. If the query planner materializes the
    // full Cartesian before HAVING + GROUP BY, refactor to a two-phase approach:
    //   Phase A: SELECT DISTINCT t1.entry_id, t2.entry_id WHERE t1.tag = t2.tag
    //   Phase B: score only Phase A pairs for HAVING count >= 3
    // Document the query plan choice in the implementation PR description.
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
         LIMIT ?1"
    )
    .bind(config.max_s1_edges_per_tick as i64)
    .fetch_all(store.write_pool_server())
    .await;

    let rows = match rows_result {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "S1 tick: batch fetch failed");
            tracing::info!(edges_written = 0, candidates = 0, "S1 tick complete (fetch error)");
            return;
        }
    };

    let candidate_count = rows.len();
    if candidate_count == 0 {
        tracing::info!(edges_written = 0, candidates = 0, "S1 tick complete");
        return;
    }

    // Phase 2: Write edges.
    let now_ts = current_timestamp_secs(); // reuse helper from nli_detection.rs or use
                                           // std::time::SystemTime::now() inline
    let mut edges_written: usize = 0;

    for row in &rows {
        // Weight formula (SCOPE.md §Design Decision 2):
        //   min(shared_tag_count * 0.1, 1.0)
        // Range: [0.3, 1.0] given HAVING >= 3.
        // Computed in Rust, not in SQL. Cast to f32 for write_graph_edge signature.
        let weight = f64::min(row.shared_tags as f64 * 0.1, 1.0) as f32;

        let written = write_graph_edge(
            store,
            row.source_id as u64,
            row.target_id as u64,
            "Informs",
            weight,
            now_ts,
            EDGE_SOURCE_S1,   // source AND created_by (write_graph_edge binds to both columns)
            "",               // metadata: empty string (no metadata for S1)
        ).await;

        if written {
            edges_written += 1;
        }
    }

    tracing::info!(
        edges_written = edges_written,
        candidates = candidate_count,
        "S1 tick complete"
    );
}
```

---

## Function: `run_s2_tick`

S2 — structural vocabulary `Informs` edges. Runs every tick. No-op when vocabulary is empty.

```
pub(crate) async fn run_s2_tick(store: &Store, config: &InferenceConfig) {
    // Early return: no-op when vocabulary is empty.
    // No SQL issued. No log output (debug trace only per FR-08).
    // This is the correct default behavior (empty default, operator opt-in).
    if config.s2_vocabulary.is_empty() {
        tracing::debug!("S2 tick: vocabulary empty, no-op");
        return;
    }

    // Phase 1: Build and execute the dynamic vocabulary-matching SQL.
    //
    // SECURITY: vocabulary terms are ALWAYS bound via push_bind (sqlx QueryBuilder).
    // Terms are NEVER interpolated into the SQL string via format!() or string concat.
    // A term containing ', --, or ; does not affect SQL structure — push_bind
    // transmits them as literal parameter values (ADR-002, SR-01, C-05).
    //
    // Query structure:
    //   SELECT source_id, target_id, (s1_terms + s2_terms) AS shared_terms
    //   FROM (
    //     SELECT e1.id AS source_id, e2.id AS target_id,
    //            <SUM of CASE WHEN term in e1> AS s1_terms,
    //            <SUM of CASE WHEN term in e2> AS s2_terms
    //     FROM entries e1
    //     JOIN entries e2 ON e2.id > e1.id
    //          AND e1.status = 0
    //          AND e2.status = 0
    //   )
    //   WHERE s1_terms + s2_terms >= 2
    //   ORDER BY shared_terms DESC
    //   LIMIT ?
    //
    // The space-padded instr() pattern (FR-09):
    //   instr(lower(' ' || e.content || ' ' || e.title || ' '), lower(' ' || ? || ' ')) > 0
    // This prevents substring false positives (e.g., "api" matching "capabilities").

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT source_id, target_id, (s1_terms + s2_terms) AS shared_terms FROM (\
         SELECT e1.id AS source_id, e2.id AS target_id, ("
    );

    // Sum of CASE WHEN for e1 (left entry)
    let term_count = config.s2_vocabulary.len();
    for (i, term) in config.s2_vocabulary.iter().enumerate() {
        qb.push("CASE WHEN instr(lower(' ' || e1.content || ' ' || e1.title || ' '), \
                 lower(' ' || ");
        qb.push_bind(term.as_str());
        qb.push(" || ' ')) > 0 THEN 1 ELSE 0 END");
        if i < term_count - 1 {
            qb.push(" + ");
        }
    }
    qb.push(") AS s1_terms, (");

    // Sum of CASE WHEN for e2 (right entry)
    for (i, term) in config.s2_vocabulary.iter().enumerate() {
        qb.push("CASE WHEN instr(lower(' ' || e2.content || ' ' || e2.title || ' '), \
                 lower(' ' || ");
        qb.push_bind(term.as_str());
        qb.push(" || ' ')) > 0 THEN 1 ELSE 0 END");
        if i < term_count - 1 {
            qb.push(" + ");
        }
    }
    qb.push(") AS s2_terms ");
    qb.push("FROM entries e1 \
              JOIN entries e2 ON e2.id > e1.id \
              AND e1.status = 0 \
              AND e2.status = 0) \
              WHERE s1_terms + s2_terms >= 2 \
              ORDER BY shared_terms DESC \
              LIMIT ");
    qb.push_bind(config.max_s2_edges_per_tick as i64);

    let query = qb.build_query_as::<S2Row>();
    let rows_result = query.fetch_all(store.write_pool_server()).await;

    let rows = match rows_result {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                vocabulary_size = config.s2_vocabulary.len(),
                error = %e,
                "S2 tick: query failed"
            );
            tracing::info!(
                edges_written = 0,
                candidates = 0,
                "S2 tick complete (query error)"
            );
            return;
        }
    };

    let candidate_count = rows.len();
    if candidate_count == 0 {
        tracing::info!(edges_written = 0, candidates = 0, "S2 tick complete");
        return;
    }

    // Phase 2: Write edges.
    let now_ts = current_timestamp_secs();
    let mut edges_written: usize = 0;

    for row in &rows {
        // Weight formula: min(shared_term_count * 0.1, 1.0), cast to f32.
        // shared_terms >= 2 (WHERE clause), so weight >= 0.2.
        let weight = f64::min(row.shared_terms as f64 * 0.1, 1.0) as f32;

        let written = write_graph_edge(
            store,
            row.source_id as u64,
            row.target_id as u64,
            "Informs",
            weight,
            now_ts,
            EDGE_SOURCE_S2,
            "",
        ).await;

        if written {
            edges_written += 1;
        }
    }

    tracing::info!(
        edges_written = edges_written,
        candidates = candidate_count,
        vocabulary_size = config.s2_vocabulary.len(),
        "S2 tick complete"
    );
}
```

---

## Function: `run_s8_tick`

S8 — search co-retrieval `CoAccess` edges. Gated by `current_tick % s8_batch_interval_ticks == 0`.
Called from `run_graph_enrichment_tick` only when the gate fires.

```
pub(crate) async fn run_s8_tick(store: &Store, config: &InferenceConfig) {
    // Phase 1: Load watermark.
    // counters::read_counter returns u64 (0 when absent).
    // On error: log warn! and abort — watermark must be known for correct batch selection.
    let watermark: u64 = match counters::read_counter(
        store.write_pool_server(),
        S8_WATERMARK_KEY
    ).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "S8 tick: failed to read watermark; skipping");
            return;
        }
    };

    // Phase 2: Fetch audit_log rows.
    // Only context_search rows with outcome=0 (Success) above the watermark.
    // Fetch up to max_s8_pairs_per_batch * 2 rows (generous upper bound).
    // ORDER BY event_id ASC ensures monotonic watermark advance.
    let fetch_limit = (config.max_s8_pairs_per_batch * 2) as i64;

    let audit_rows_result = sqlx::query_as::<_, S8AuditRow>(
        "SELECT event_id, target_ids
         FROM audit_log
         WHERE operation = 'context_search'
           AND outcome = 0
           AND event_id > ?1
         ORDER BY event_id ASC
         LIMIT ?2"
    )
    .bind(watermark as i64)
    .bind(fetch_limit)
    .fetch_all(store.write_pool_server())
    .await;

    let audit_rows = match audit_rows_result {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "S8 tick: audit_log fetch failed; skipping");
            return;
        }
    };

    if audit_rows.is_empty() {
        tracing::info!(
            pairs_written = 0,
            pairs_skipped = 0,
            watermark = watermark,
            "S8 tick complete (no new audit rows)"
        );
        return;
    }

    // Phase 3: Expand pairs from audit rows.
    // Cap on pairs (not on rows). Partial-row: track last fully-processed event_id.
    //
    // Invariant (ADR-003 C-12):
    //   - If a row's pairs would push total OVER the cap, that row is not processed.
    //   - new_watermark advances to the event_id of the last row where ALL pairs
    //     were counted (even if some pairs are skipped for quarantine later).
    //   - Rows with malformed JSON are logged at warn! and their event_id IS included
    //     in new_watermark (C-14: no stuck watermark behind malformed rows).

    let mut pairs: Vec<(u64, u64)> = Vec::new();
    let mut new_watermark: u64 = watermark; // advances as rows are fully processed
    let cap = config.max_s8_pairs_per_batch;

    'rows: for row in &audit_rows {
        let event_id = row.event_id as u64;

        // Parse target_ids JSON as Vec<u64>.
        let entry_ids: Vec<u64> = match serde_json::from_str::<Vec<u64>>(&row.target_ids) {
            Ok(ids) => ids,
            Err(e) => {
                // Malformed JSON: advance watermark past this row (C-14).
                // Do NOT leave watermark stuck — perpetual re-scan would occur.
                tracing::warn!(
                    event_id = event_id,
                    target_ids = %row.target_ids,
                    error = %e,
                    "S8 tick: malformed target_ids JSON; advancing watermark past row"
                );
                new_watermark = event_id;
                continue 'rows;
            }
        };

        // Build unordered pairs (a < b) for this row.
        let mut row_pairs: Vec<(u64, u64)> = Vec::new();
        for i in 0..entry_ids.len() {
            for j in (i + 1)..entry_ids.len() {
                let a = entry_ids[i].min(entry_ids[j]);
                let b = entry_ids[i].max(entry_ids[j]);
                // a < b guaranteed. a == b excluded by i != j.
                row_pairs.push((a, b));
            }
        }

        // Zero-pair rows (singleton or empty target_ids): advance watermark, continue.
        if row_pairs.is_empty() {
            new_watermark = event_id;
            continue 'rows;
        }

        // Cap check: if adding this row's pairs would exceed cap, stop processing.
        // Do NOT advance new_watermark for this row (partial-row semantics).
        if pairs.len() + row_pairs.len() > cap {
            break 'rows;
        }

        // Full row accepted: extend pairs, advance watermark.
        pairs.extend(row_pairs);
        new_watermark = event_id;
    }

    if pairs.is_empty() {
        // All rows were parse-skipped or cap triggered on first row.
        // Still update watermark if parse-skips occurred.
        if new_watermark > watermark {
            let _ = counters::set_counter(store.write_pool_server(), S8_WATERMARK_KEY, new_watermark).await;
        }
        tracing::info!(
            pairs_written = 0,
            pairs_skipped = 0,
            new_watermark = new_watermark,
            "S8 tick complete (no valid pairs)"
        );
        return;
    }

    // Phase 4: Bulk quarantine filter.
    // Collect all unique IDs referenced by the candidate pairs.
    // Query entries table to find which IDs are Active (status = 0).
    // Chunked to stay under SQLITE_MAX_VARIABLE_NUMBER (C-13, entry #3442).
    //
    // Build HashSet<u64> of valid (Active) entry IDs.
    let all_ids: Vec<u64> = {
        let mut id_set: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
        for (a, b) in &pairs {
            id_set.insert(*a);
            id_set.insert(*b);
        }
        id_set.into_iter().collect()
    };

    let mut valid_ids: HashSet<u64> = HashSet::new();
    for chunk in all_ids.chunks(SQLITE_MAX_VARIABLE_NUMBER) {
        let mut qb = sqlx::QueryBuilder::new(
            "SELECT id FROM entries WHERE status = 0 AND id IN ("
        );
        let mut sep = qb.separated(", ");
        for id in chunk {
            sep.push_bind(*id as i64);
        }
        qb.push(")");

        let chunk_result: Result<Vec<i64>, _> = qb
            .build_query_scalar()
            .fetch_all(store.write_pool_server())
            .await;

        match chunk_result {
            Ok(ids) => {
                for id in ids {
                    valid_ids.insert(id as u64);
                }
            }
            Err(e) => {
                // Bulk validation query failed: cannot safely filter quarantined entries.
                // Safe behavior: skip entire batch (do not write edges without endpoint check).
                tracing::warn!(
                    error = %e,
                    "S8 tick: bulk quarantine filter failed; skipping batch, watermark unchanged"
                );
                return;
            }
        }
    }

    // Phase 5: Write edges.
    // INVARIANT (C-11): ALL edge writes happen BEFORE watermark update.
    let now_ts = current_timestamp_secs();
    let mut pairs_written: usize = 0;
    let mut pairs_skipped: usize = 0;

    for (a, b) in &pairs {
        // Both endpoints must be in valid_ids (Active, not quarantined).
        if !valid_ids.contains(a) || !valid_ids.contains(b) {
            pairs_skipped += 1;
            continue;
        }

        let written = write_graph_edge(
            store,
            *a,
            *b,
            "CoAccess",
            0.25_f32,       // fixed weight for S8 CoAccess edges
            now_ts,
            EDGE_SOURCE_S8,
            "",
        ).await;

        if written {
            pairs_written += 1;
        }
        // write_graph_edge returns false on UNIQUE conflict (INSERT OR IGNORE): not an error.
    }

    // Phase 6: Update watermark AFTER all edge writes (C-11).
    // If set_counter fails: log warn!, edges are already written.
    // Same batch will re-process on next S8 run; INSERT OR IGNORE handles duplicates.
    match counters::set_counter(store.write_pool_server(), S8_WATERMARK_KEY, new_watermark).await {
        Ok(()) => {}
        Err(e) => {
            tracing::warn!(
                error = %e,
                new_watermark = new_watermark,
                "S8 tick: failed to update watermark; batch will re-process on next run"
            );
        }
    }

    tracing::info!(
        pairs_written = pairs_written,
        pairs_skipped_quarantined = pairs_skipped,
        new_watermark = new_watermark,
        "S8 tick complete"
    );
}
```

---

## Helper: `current_timestamp_secs`

S1/S2/S8 all need a current Unix timestamp for `created_at`. Two options:

Option A: Import `format_nli_metadata` and `current_timestamp_secs` from `nli_detection.rs`
if those are already `pub(crate)`.

Option B: Inline the standard pattern used throughout the codebase:
```
fn current_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
```

Delivery agent: grep for `current_timestamp_secs` in `nli_detection.rs`. If `pub(crate)`,
import it. Otherwise, define it as a module-private helper in `graph_enrichment_tick.rs`.

---

## State Machine: S8 Watermark

```
States: {watermark=W}

On tick gate fires (current_tick % s8_batch_interval_ticks == 0):
    read watermark W from counters
    fetch audit_log WHERE event_id > W ORDER BY event_id ASC LIMIT max_pairs*2
    for each row in order:
        if malformed JSON:  advance W to row.event_id; log warn!; continue
        if zero pairs:      advance W to row.event_id; continue
        if pairs would exceed cap: STOP (do not advance W past this row)
        else: accumulate pairs; advance W to row.event_id
    write all edges (INSERT OR IGNORE)
    write W to counters   ← MUST be after edge writes (C-11)
    
On crash between edge writes and watermark write:
    W is unchanged; next run re-fetches same batch; INSERT OR IGNORE handles duplicates.
    At-most-once gap: not possible (watermark-after-writes ordering).
    At-least-once re-processing: acceptable (idempotent).
```

---

## Tests (extracted to `graph_enrichment_tick_tests.rs` if needed)

### T-GET-01: S1 quarantine guard — source endpoint (R-01, AC-01 variant)
Corpus: entry A (Active), entry B (Active, then Quarantined after first run).
First run: A-B shares 3 tags → edge written.
Quarantine B. Second run: assert no new edge to B as source_id.

### T-GET-02: S1 quarantine guard — target endpoint (R-01)
Same as T-GET-01 but B as target_id (entry_id_a with t2.entry_id > t1.entry_id ordering).

### T-GET-03: S1 weight formula (edge case — exact 3 shared tags and capping at 1.0)
Pair with 3 shared tags: weight = 0.3.
Pair with 10 shared tags: weight = 1.0 (not 1.0+).
Pair with 11 shared tags: weight = 1.0 (cap applied).
Pair with 2 shared tags: no edge (HAVING >= 3).

### T-GET-04: S1 cap (max_s1_edges_per_tick = 1)
Corpus producing 5 qualifying pairs (sorted DESC by shared_tags).
Assert exactly 1 edge written; assert it has the highest shared_tag count.

### T-GET-05: S1 source-value assertion (R-07)
After S1 tick: assert all written edges have `source = 'S1'`.
Assert `graph_edges WHERE source = 'nli'` count unchanged.

### T-GET-06: S2 no-op on empty vocabulary (R-14, AC-07)
Set `s2_vocabulary = []`. Run S2 tick. Assert zero edges in graph_edges with source='S2'.
No panic, no SQL error.

### T-GET-07: S2 SQL injection resistance (R-02, AC-11)
Set `s2_vocabulary = ["it's", "drop--table"]`. Run S2 tick.
Assert no SQL error. Assert edges are for entries actually containing those terms.

### T-GET-08: S2 false-positive suppression (R-11, AC-10)
Term "api" in vocabulary. Entry content = "capabilities only". Assert no edge for that entry.
Term "api" in vocabulary. Entry content = "the api is documented". Assert edge IS written.

### T-GET-09: S2 quarantine guard (R-01)
Pair of entries both matching ≥2 vocabulary terms, one is Quarantined.
Assert no edge written.

### T-GET-10: S2 shared_terms threshold (min = 2)
e1 has 1 term match, e2 has 1 term match → total = 2 → edge written.
e1 has 2 matches, e2 has 0 → total = 2 → edge written.
e1 has 1 match, e2 has 0 → total = 1 → no edge.

### T-GET-11: S8 watermark advance on malformed JSON (R-05, AC-20)
3 audit_log rows: row 1 valid, row 2 malformed JSON, row 3 valid.
Assert: row 1 and row 3 pairs written, warn! for row 2,
watermark = event_id of row 3.
Second run: assert zero new edges, watermark unchanged.

### T-GET-12: S8 watermark write-after-edges ordering (R-06, AC-16)
Use a mock/instrumented store to capture call order.
Assert counters::set_counter is called after all write_graph_edge calls.

### T-GET-13: S8 quarantine filter — endpoint filtering (R-01, AC-14)
audit_log row with target_ids containing one Quarantined entry ID.
Assert the pair involving that entry is not written.
Assert the other pairs in the same row ARE written.

### T-GET-14: S8 batch cap on pairs, not rows (R-10, AC-21)
max_s8_pairs_per_batch = 5. One row with 5 entry IDs (10 pairs).
Assert exactly 5 edges written (cap on pairs, not 10).

### T-GET-15: S8 partial-row watermark (R-10)
max_s8_pairs_per_batch = 3. Two rows: row A produces 2 pairs, row B produces 4 pairs.
Row A is fully processed (2 pairs). Row B would push to 6 — exceeds cap.
Assert exactly 2 edges written. Assert watermark = event_id of row A only.

### T-GET-16: S8 singleton target_ids (no pairs) (edge case)
audit_log row with target_ids = '[42]' (single ID → 0 pairs).
Assert no edge written. Assert watermark advances past that row.

### T-GET-17: S8 source-value assertion (R-07)
After S8 tick: assert all written edges have `source = 'S8'` and `relation_type = 'CoAccess'`.

### T-GET-18: inferred_edge_count isolation (R-13, AC-30)
Insert edges for S1, S2, S8, and NLI sources.
Run compute_graph_cohesion_metrics(). Assert inferred_edge_count equals only NLI count.
Run S1/S2/S8 ticks again. Assert inferred_edge_count is unchanged.

### T-GET-19: S2 word-boundary — hyphenated compound (edge case, doc only)
Term "api" should NOT match "api-gateway" due to hyphen breaking space-boundary.
This is expected behavior. Document in test comments; no assertion required unless
the space-padding regex is extended to handle hyphens.

---

## Notes for Delivery Agent

1. `write_graph_edge` actual signature: `weight: f32`, `metadata: &str` (not Option).
   Call sites must pass `weight as f32` and `""` for metadata.

2. `counters::read_counter` and `counters::set_counter` both accept `impl Executor`.
   Pass `store.write_pool_server()` directly (same pool as edges, same WAL file).

3. S8 pairs loop: `pairs.len() + row_pairs.len() > cap` comparison uses `>` (strict),
   meaning a row that would hit exactly the cap IS accepted. Use `>=` if the cap should
   exclude the row at exactly-equal. The BRIEF says "stop at cap" — strict `>` means
   the cap is a maximum inclusive count, so `> cap` is correct.

4. The `e2.id > e1.id` JOIN condition in S2 is the canonical unordered-pair approach,
   matching S1's `t2.entry_id > t1.entry_id`. Both give (lower_id → higher_id) direction.
   S1/S2 Informs edges are not bidirectional (unlike CoAccess co_access_promotion).

5. File size budget: main module + S1/S2/S8 functions + helpers ≈ 280-320 lines.
   Tests will push to ~600+ lines. Extract tests at PR time.
