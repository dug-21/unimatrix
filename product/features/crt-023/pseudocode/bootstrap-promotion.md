# Bootstrap Edge Promotion — Pseudocode

**File**: `crates/unimatrix-server/src/services/nli_detection.rs` (same file as post-store)
**Caller**: `crates/unimatrix-server/src/background.rs` (modified — calls `maybe_run_bootstrap_promotion`)

**Purpose**: One-shot task run on the first background tick after server startup where NLI is
ready. Fetches all `GRAPH_EDGES` rows with `bootstrap_only=1 AND relation_type='Contradicts'`,
batch-scores them through NLI, then replaces confirmed rows with NLI-confirmed edges and deletes
refuted rows. A durable completion marker in the `COUNTERS` table prevents re-runs (ADR-005).

**Critical constraint (W1-2)**: ALL NLI inference across the entire bootstrap set is dispatched
as a SINGLE `rayon_pool.spawn()` call. Pairs are collected first (async DB reads on tokio thread),
then dispatched to rayon as one batch, then results are written back (async DB writes on tokio
thread). This is the W1-2 pattern for background tasks.

---

## `maybe_run_bootstrap_promotion` (public async fn in nli_detection.rs)

```
/// Entry point called on each background tick.
/// Fast no-op path: checks COUNTERS marker first (O(1) DB read).
/// Defers if NLI not ready (FR-25); no marker set on deferral.
///
/// Signature from ARCHITECTURE.md integration surface.
pub async fn maybe_run_bootstrap_promotion(
    store: &Store,
    nli_handle: &NliServiceHandle,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
)
    // Fast path: check idempotency marker (ADR-005, AC-24).
    let done = counters::read_counter(
        store.write_pool_server(),
        "bootstrap_nli_promotion_done"
    ).await.unwrap_or(0)
    if done != 0:
        return  // no-op: promotion already completed in a previous run

    // Require NLI readiness (FR-25, AC-12).
    // Do NOT run on cosine fallback — NLI promotion is NLI-only.
    let provider = match nli_handle.get_provider().await:
        Ok(p)  -> p
        Err(_) ->
            tracing::info!("bootstrap NLI promotion deferred: NLI not ready; will retry on next tick")
            return  // no marker set; re-runs on next tick automatically

    // Run the promotion task. Marker is set inside on success.
    run_bootstrap_promotion(store, provider, rayon_pool, config).await
```

---

## `run_bootstrap_promotion` (private async fn in nli_detection.rs)

```
/// Execute the one-shot bootstrap promotion. Sets completion marker on success.
///
/// W1-2 contract: ALL NLI inference dispatched as a single rayon_pool.spawn() call.
/// Pairs are batch-collected from DB first, then sent to rayon, then results written back.
async fn run_bootstrap_promotion(
    store: &Store,
    provider: Arc<dyn CrossEncoderProvider>,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
)
    tracing::info!("bootstrap NLI promotion: starting")

    // Step 1: Fetch all bootstrap Contradicts rows (async DB read — tokio thread, not rayon)
    let rows = match store.query_bootstrap_contradicts().await:
        Ok(r)  -> r
        Err(e) ->
            tracing::error!(error=%e, "bootstrap NLI promotion: failed to query bootstrap rows")
            return  // no marker set; retry next tick

    // Zero-row case: valid successful run (ADR-005, AC-12a).
    // Set marker immediately and return. No NLI inference needed.
    if rows.is_empty():
        tracing::info!("bootstrap NLI promotion: zero bootstrap rows found; marking complete")
        match set_bootstrap_marker(store).await:
            Ok(()) ->
                tracing::info!("bootstrap NLI promotion: complete (zero rows)")
            Err(e) ->
                tracing::error!(error=%e, "bootstrap NLI promotion: failed to set completion marker")
                // No return error to caller; marker will be retried next tick
        return

    tracing::info!(row_count=rows.len(), "bootstrap NLI promotion: scoring {} bootstrap rows", rows.len())

    // Step 2: Fetch entry texts for all rows (async DB reads — tokio thread)
    // query_bootstrap_contradicts returns (edge_id, source_id, target_id).
    // We need the content text of each entry for NLI input.
    let mut indexed_pairs: Vec<(u64, String, String)> = Vec::with_capacity(rows.len())
    // indexed_pairs: (edge_id, source_content, target_content)

    for (edge_id, source_id, target_id) in &rows:
        let source_text = match store.get(*source_id).await:
            Ok(e)  -> e.content
            Err(e) ->
                tracing::debug!(edge_id=edge_id, source_id=source_id, error=%e,
                               "bootstrap NLI: skipping row — source entry not found")
                continue  // skip rows with deleted source entries
        let target_text = match store.get(*target_id).await:
            Ok(e)  -> e.content
            Err(e) ->
                tracing::debug!(edge_id=edge_id, target_id=target_id, error=%e,
                               "bootstrap NLI: skipping row — target entry not found")
                continue
        indexed_pairs.push((*edge_id, source_text, target_text))

    if indexed_pairs.is_empty():
        // All rows had missing entries; set marker (no valid work to do).
        tracing::info!("bootstrap NLI promotion: all {} rows had missing entries; marking complete", rows.len())
        let _ = set_bootstrap_marker(store).await
        return

    // Step 3: W1-2 constraint — ALL inference dispatched as a SINGLE rayon spawn.
    // Build pairs from indexed_pairs before moving into closure.
    let pairs_owned: Vec<(String, String)> = indexed_pairs.iter()
        .map(|(_, src, tgt)| (src.clone(), tgt.clone()))
        .collect()

    let provider_clone = Arc::clone(&provider)
    let nli_scores: Vec<NliScores> = match rayon_pool.spawn(move || {
        let pairs: Vec<(&str, &str)> = pairs_owned.iter()
            .map(|(s, t)| (s.as_str(), t.as_str()))
            .collect()
        provider_clone.score_batch(&pairs)
    }).await:
        Ok(Ok(scores)) -> scores
        Ok(Err(e))     ->
            tracing::error!(error=%e, "bootstrap NLI promotion: score_batch failed")
            return  // no marker set; retry next tick
        Err(rayon_err) ->
            tracing::error!(error=%rayon_err, "bootstrap NLI promotion: rayon task cancelled")
            return  // no marker set; retry next tick

    if nli_scores.len() != indexed_pairs.len():
        tracing::error!(
            expected=indexed_pairs.len(), got=nli_scores.len(),
            "bootstrap NLI promotion: score_batch returned wrong number of scores; aborting"
        )
        return  // defensive; should not happen

    // Step 4: Write results (async DB writes — tokio thread).
    // Each row: DELETE old bootstrap edge + conditionally INSERT NLI-confirmed replacement.
    // All in a single transaction per edge (ADR-005).
    let now = current_timestamp_secs()
    let mut promoted = 0usize
    let mut deleted  = 0usize

    for (i, (edge_id, _, _)) in indexed_pairs.iter().enumerate():
        let scores = &nli_scores[i]

        if scores.contradiction > config.nli_contradiction_threshold:
            // Score exceeds threshold: promote to NLI-confirmed edge.
            let metadata = format_nli_metadata(scores)

            // Find source_id and target_id for this edge from the original rows lookup.
            // indexed_pairs index i corresponds to rows in insertion order.
            // Need source_id and target_id for the INSERT.
            // Solution: store them in indexed_pairs tuple as well.
            //
            // CORRECTION: extend indexed_pairs tuple to also carry (source_id, target_id):
            // indexed_pairs: (edge_id, source_id, target_id, source_content, target_content)
            // [See note below about tuple fields]

            let (edge_id, source_id, target_id) = get_ids_for_index(i, &rows, &indexed_pairs)
            let weight = scores.contradiction

            match promote_bootstrap_edge(store, edge_id, source_id, target_id, weight, now, &metadata).await:
                Ok(())  -> promoted += 1
                Err(e)  ->
                    tracing::error!(edge_id=edge_id, error=%e, "bootstrap NLI: failed to promote edge")
                    // Continue — do not abort the entire promotion on one DB failure
        else:
            // Score below threshold: delete the bootstrap edge (not NLI-confirmed).
            match store.write_pool_server().execute(
                "DELETE FROM graph_edges WHERE id = ?1",
                params![edge_id]
            ).await:
                Ok(_)  -> deleted += 1
                Err(e) ->
                    tracing::error!(edge_id=edge_id, error=%e, "bootstrap NLI: failed to delete bootstrap edge")

    tracing::info!(
        promoted=promoted, deleted=deleted,
        total=indexed_pairs.len(),
        "bootstrap NLI promotion: processing complete"
    )

    // Step 5: Set completion marker (ADR-005, AC-24).
    // Set regardless of partial write failures — the idempotency of INSERT OR IGNORE means
    // a partial run that failed on some edges and then set the marker will not re-process
    // edges that were already promoted or deleted.
    match set_bootstrap_marker(store).await:
        Ok(()) ->
            tracing::info!("bootstrap NLI promotion: completion marker set")
        Err(e) ->
            tracing::error!(error=%e, "bootstrap NLI promotion: failed to set completion marker; will retry on next tick")
            // Marker not set; task will re-run on next tick.
            // Re-run is safe: INSERT OR IGNORE handles already-promoted edges.
            // Already-deleted rows produce empty DELETE (harmless).
```

**Note on indexed_pairs tuple**: To avoid a second pass through `rows` to look up source_id and
target_id during the write phase, extend the `indexed_pairs` Vec to carry all required data:

```
// Use this tuple type instead:
struct BootstrapRow {
    edge_id:      u64,
    source_id:    u64,
    target_id:    u64,
    source_text:  String,
    target_text:  String,
}
// Or equivalently (edge_id, source_id, target_id, source_text, target_text) as a tuple.
```

---

## `promote_bootstrap_edge` (private async fn in nli_detection.rs)

```
/// Atomically DELETE old bootstrap edge and INSERT NLI-confirmed replacement.
/// Two-statement transaction: DELETE then INSERT OR IGNORE.
async fn promote_bootstrap_edge(
    store: &Store,
    old_edge_id: u64,
    source_id: u64,
    target_id: u64,
    weight: f32,
    created_at: u64,
    metadata: &str,
) -> Result<(), Error>
    // Execute as a transaction to ensure atomicity (ADR-005).
    // If the INSERT fails (e.g. unique constraint conflict with post-store NLI edge),
    // INSERT OR IGNORE silently does nothing — the existing NLI edge takes precedence.
    // The DELETE still runs (in the same transaction), removing the bootstrap edge.
    store.write_pool_server().transaction(|conn| {
        conn.execute("DELETE FROM graph_edges WHERE id = ?1", params![old_edge_id])?
        conn.execute(
            "INSERT OR IGNORE INTO graph_edges \
             (source_id, target_id, relation_type, weight, created_at, created_by, \
              source, bootstrap_only, metadata) \
             VALUES (?1, ?2, 'Contradicts', ?3, ?4, 'nli', 'nli', 0, ?5)",
            params![source_id, target_id, weight as f64, created_at, metadata]
        )?
        Ok(())
    }).await
```

---

## `set_bootstrap_marker` (private async fn in nli_detection.rs)

```
/// Set COUNTERS key "bootstrap_nli_promotion_done" = 1 (ADR-005, FR-24).
/// Uses INSERT OR REPLACE (idempotent; calling multiple times is safe).
async fn set_bootstrap_marker(store: &Store) -> Result<(), Error>
    counters::set_counter(
        store.write_pool_server(),
        "bootstrap_nli_promotion_done",
        1u64
    ).await
```

---

## `store.query_bootstrap_contradicts` (new Store method in unimatrix-store)

```
/// Fetch all GRAPH_EDGES rows with bootstrap_only=1 AND relation_type='Contradicts'.
/// Returns (edge_id, source_id, target_id) for all bootstrap contradiction edges.
pub async fn query_bootstrap_contradicts(&self) -> Result<Vec<(u64, u64, u64)>, StoreError>
    let conn = self.read_pool().acquire().await?
    let rows = conn.query(
        "SELECT id, source_id, target_id FROM graph_edges \
         WHERE bootstrap_only = 1 AND relation_type = 'Contradicts'",
        []
    )?
    .map(|row| Ok((row.get::<u64>(0)?, row.get::<u64>(1)?, row.get::<u64>(2)?)))
    .collect::<Result<Vec<_>, _>>()?
    Ok(rows)
```

---

## `background.rs` Modification

Add call to `maybe_run_bootstrap_promotion` in the background tick function.

```
// In background.rs tick function, after existing maintenance steps:

// crt-023: Bootstrap NLI promotion (one-shot, idempotent after first successful run).
// No-op if: already done (COUNTERS marker set), NLI not ready (deferred), or NLI disabled.
if app_state.config.inference.nli_enabled {
    maybe_run_bootstrap_promotion(
        &app_state.store,
        &app_state.nli_handle,
        &app_state.rayon_pool,
        &app_state.config.inference,
    ).await
}
```

---

## Error Handling Summary

| Failure | Log Level | Behavior |
|---------|-----------|----------|
| `read_counter` fails | warn (unwrap_or(0)) | Treated as "not done"; task proceeds |
| NLI not ready | info | Defer (no marker set); retry next tick |
| `query_bootstrap_contradicts` fails | error | Return; retry next tick |
| Entry fetch fails (source or target) | debug | Skip that row; continue |
| `score_batch` fails | error | Return; retry next tick |
| Rayon task cancelled | error | Return; retry next tick |
| Individual DELETE/INSERT fails | error | Continue with remaining rows |
| `set_bootstrap_marker` fails | error | Return (no marker set); retry next tick |

**Idempotency guarantee (ADR-005)**: Re-runs are safe because:
- Already-deleted bootstrap edges produce empty DELETE (no error).
- Already-promoted NLI edges hit `INSERT OR IGNORE` UNIQUE constraint (no error, no duplicate).
- The marker check (`read_counter != 0`) skips the task after first successful completion.

---

## Key Test Scenarios

1. **AC-12a / zero-row case**: Call `run_bootstrap_promotion` on DB with no `bootstrap_only=1` rows; assert marker is set; assert function returns without error.
2. **AC-12b / promotion**: Insert synthetic `bootstrap_only=1` Contradicts rows; run promotion with mock provider returning `contradiction=0.9`; assert rows are replaced with `source='nli'`, `bootstrap_only=0` rows; assert marker is set.
3. **AC-12b / deletion**: Insert synthetic rows; mock provider returns `contradiction=0.3` (below threshold); assert rows are deleted; marker is set.
4. **AC-24 / idempotency**: Run promotion to completion; restart (or call again); assert `maybe_run_bootstrap_promotion` returns immediately (no-op) and `GRAPH_EDGES` is unchanged.
5. **FR-25 / NLI not ready deferral**: Call `maybe_run_bootstrap_promotion` when NLI handle is in Loading state; assert info log "deferred"; assert marker NOT set; assert next tick with Ready NLI runs normally.
6. **R-11 / partial failure**: Inject write pool error on final `set_bootstrap_marker`; restart; assert no duplicate rows (INSERT OR IGNORE safety); assert re-run eventually sets marker.
7. **R-12 / no HNSW dependency**: Call `run_bootstrap_promotion` with cold (empty) VectorIndex; assert promotion completes using only entry text (no HNSW calls in promotion path).
8. **R-20 / post-store race**: Insert `bootstrap_only=1` edge; trigger post-store NLI for an adjacent entry; run bootstrap promotion concurrently; assert no duplicate edges (INSERT OR IGNORE handles the race).
9. **W1-2 compliance**: All NLI inference (score_batch) is inside the rayon_pool.spawn closure; assert no NLI scoring happens on the tokio thread.
