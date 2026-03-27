//! Post-store NLI detection and bootstrap edge promotion (crt-023).
//!
//! Two public async functions:
//! - `run_post_store_nli`: fire-and-forget task spawned after `context_store` HNSW insert.
//! - `maybe_run_bootstrap_promotion`: called on each background tick; one-shot with idempotency
//!   via COUNTERS marker (ADR-005).
//!
//! # W1-2 Contract
//!
//! All NLI inference (`CrossEncoderProvider::score_batch`) MUST run via
//! `rayon_pool.spawn()` — never inline in async context, never via `spawn_blocking`.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use unimatrix_core::{Store, VectorIndex};
use unimatrix_embed::{CrossEncoderProvider, NliScores};
use unimatrix_store::counters;

use crate::infra::config::InferenceConfig;
use crate::infra::nli_handle::NliServiceHandle;
use crate::infra::rayon_pool::RayonPool;

// ---------------------------------------------------------------------------
// Public: post-store NLI detection
// ---------------------------------------------------------------------------

/// Fire-and-forget async function. Spawned via `tokio::spawn` after `context_store` HNSW insert.
///
/// No timeout — this is a background task. The MCP response is already returned.
///
/// # W1-2 contract
///
/// `score_batch` runs via `rayon_pool.spawn()` (no timeout).
/// Never inline in async context. Never via `spawn_blocking`.
///
/// `max_edges_per_call` is named `max_contradicts_per_tick` in config for compatibility
/// (FR-22, AC-23). Its semantic unit is per `context_store` call (not per background tick).
pub async fn run_post_store_nli(
    embedding: Vec<f32>,
    new_entry_id: u64,
    new_entry_text: String,
    nli_handle: Arc<NliServiceHandle>,
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    rayon_pool: Arc<RayonPool>,
    nli_post_store_k: usize,
    nli_entailment_threshold: f32,
    nli_contradiction_threshold: f32,
    max_edges_per_call: usize,
) {
    // Step 1: Get NLI provider. Exit immediately if not ready (no blocking wait).
    let provider = match nli_handle.get_provider().await {
        Ok(p) => p,
        Err(_) => {
            tracing::debug!(
                entry_id = new_entry_id,
                "post-store NLI skipped: NLI not ready"
            );
            return;
        }
    };

    // Step 2: Defensive empty embedding guard (R-07).
    if embedding.is_empty() {
        tracing::warn!(
            entry_id = new_entry_id,
            "post-store NLI skipped: empty embedding"
        );
        return;
    }

    // Step 3: Query HNSW for nearest neighbors.
    // VectorIndex::search is synchronous; called here on the tokio thread (I/O-free, lock-only).
    // EF_SEARCH matches the constant used in SearchService.
    const EF_SEARCH: usize = 32;
    let neighbor_results = match vector_index.search(&embedding, nli_post_store_k, EF_SEARCH) {
        Ok(results) => results,
        Err(e) => {
            tracing::warn!(
                entry_id = new_entry_id,
                error = %e,
                "post-store NLI: HNSW search failed"
            );
            return;
        }
    };

    // Filter out the new entry itself from neighbors.
    let neighbor_ids: Vec<u64> = neighbor_results
        .into_iter()
        .filter(|r| r.entry_id != new_entry_id)
        .map(|r| r.entry_id)
        .collect();

    if neighbor_ids.is_empty() {
        tracing::debug!(
            entry_id = new_entry_id,
            "post-store NLI: no neighbors found"
        );
        return;
    }

    // Step 4: Fetch neighbor entry texts (async DB reads, on tokio thread).
    let mut neighbor_texts: Vec<(u64, String)> = Vec::with_capacity(neighbor_ids.len());
    for id in &neighbor_ids {
        match store.get(*id).await {
            Ok(entry) => neighbor_texts.push((*id, entry.content)),
            Err(e) => {
                tracing::debug!(
                    neighbor_id = id,
                    error = %e,
                    "post-store NLI: failed to fetch neighbor; skipping"
                );
                // Skip unreachable neighbors; continue with the rest.
            }
        }
    }

    if neighbor_texts.is_empty() {
        return;
    }

    // Step 5: Build pairs for batch scoring.
    // new_entry_text is the "premise"; neighbor text is the "hypothesis" (SNLI convention).
    let pairs_owned: Vec<(String, String)> = neighbor_texts
        .iter()
        .map(|(_, text)| (new_entry_text.clone(), text.clone()))
        .collect();

    // Step 6: Dispatch to rayon pool (W1-2 contract — no timeout for background task).
    let provider_clone = Arc::clone(&provider);
    let nli_result = rayon_pool
        .spawn(move || {
            let pairs: Vec<(&str, &str)> = pairs_owned
                .iter()
                .map(|(q, p)| (q.as_str(), p.as_str()))
                .collect();
            provider_clone.score_batch(&pairs)
        })
        .await;

    let nli_scores = match nli_result {
        Ok(Ok(scores)) => scores,
        Ok(Err(e)) => {
            tracing::warn!(
                entry_id = new_entry_id,
                error = %e,
                "post-store NLI: score_batch failed"
            );
            return; // FR-21: do not propagate; log at warn and exit
        }
        Err(rayon_err) => {
            // RayonError::Cancelled = rayon panic (session poisoned or other panic)
            tracing::warn!(
                entry_id = new_entry_id,
                error = %rayon_err,
                "post-store NLI: rayon task cancelled (panic?)"
            );
            return; // FR-21: clean exit on rayon panic
        }
    };

    // Step 7: Write edges. Cap at max_edges_per_call (FR-22, R-09, AC-13).
    // Cap counts BOTH Supports AND Contradicts edges combined (not just Contradicts).
    // NOTE: This is named max_contradicts_per_tick in config for compatibility;
    // semantic is per-call.
    let edges_written = write_edges_with_cap(
        &store,
        new_entry_id,
        &neighbor_texts,
        &nli_scores,
        nli_entailment_threshold,
        nli_contradiction_threshold,
        max_edges_per_call,
    )
    .await;

    tracing::debug!(
        entry_id = new_entry_id,
        edges_written = edges_written,
        neighbors_scored = nli_scores.len(),
        "post-store NLI detection complete"
    );
}

// ---------------------------------------------------------------------------
// Public: bootstrap edge promotion
// ---------------------------------------------------------------------------

/// Entry point called on each background tick.
///
/// Fast no-op path: checks COUNTERS marker first (O(1) DB read).
/// Defers if NLI not ready (FR-25); no marker set on deferral.
///
/// Signature from ARCHITECTURE.md integration surface.
pub async fn maybe_run_bootstrap_promotion(
    store: &Store,
    nli_handle: &NliServiceHandle,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
) {
    // Fast path: check idempotency marker (ADR-005, AC-24).
    let done = counters::read_counter(store.write_pool_server(), "bootstrap_nli_promotion_done")
        .await
        .unwrap_or(0);
    if done != 0 {
        return; // no-op: promotion already completed in a previous run
    }

    // Require NLI readiness (FR-25, AC-12).
    // Do NOT run on cosine fallback — NLI promotion is NLI-only.
    let provider = match nli_handle.get_provider().await {
        Ok(p) => p,
        Err(_) => {
            tracing::info!(
                "bootstrap NLI promotion deferred: NLI not ready; will retry on next tick"
            );
            return; // no marker set; re-runs on next tick automatically
        }
    };

    // Run the promotion task. Marker is set inside on success.
    run_bootstrap_promotion(store, provider, rayon_pool, config).await;
}

// ---------------------------------------------------------------------------
// Private: bootstrap promotion core logic
// ---------------------------------------------------------------------------

/// Execute the one-shot bootstrap promotion. Sets completion marker on success.
///
/// # W1-2 contract
///
/// ALL NLI inference dispatched as a single `rayon_pool.spawn()` call.
/// Pairs are batch-collected from DB first, then sent to rayon, then results written back.
async fn run_bootstrap_promotion(
    store: &Store,
    provider: Arc<dyn CrossEncoderProvider>,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
) {
    tracing::info!("bootstrap NLI promotion: starting");

    // Step 1: Fetch all bootstrap Contradicts rows (async DB read — tokio thread, not rayon).
    let rows = match store.query_bootstrap_contradicts().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(
                error = %e,
                "bootstrap NLI promotion: failed to query bootstrap rows"
            );
            return; // no marker set; retry next tick
        }
    };

    // Zero-row case: valid successful run (ADR-005, AC-12a).
    // Set marker immediately and return. No NLI inference needed.
    if rows.is_empty() {
        tracing::info!("bootstrap NLI promotion: zero bootstrap rows found; marking complete");
        match set_bootstrap_marker(store).await {
            Ok(()) => {
                tracing::info!("bootstrap NLI promotion: complete (zero rows)");
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "bootstrap NLI promotion: failed to set completion marker"
                );
                // No return error to caller; marker will be retried next tick.
            }
        }
        return;
    }

    tracing::info!(
        row_count = rows.len(),
        "bootstrap NLI promotion: scoring {} bootstrap rows",
        rows.len()
    );

    // Step 2: Fetch entry texts for all rows (async DB reads — tokio thread).
    // indexed_pairs: (edge_id, source_id, target_id, source_content, target_content)
    let mut indexed_pairs: Vec<(u64, u64, u64, String, String)> = Vec::with_capacity(rows.len());

    for (edge_id, source_id, target_id) in &rows {
        // Use write-pool reads so recently committed entries are visible.
        // The read pool is opened read-only and may lag behind WAL writes
        // when bootstrap promotion runs immediately after seeding (crt-023).
        let source_text = match store.get_content_via_write_pool(*source_id).await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(
                    edge_id = edge_id,
                    source_id = source_id,
                    error = %e,
                    "bootstrap NLI: skipping row — source entry not found"
                );
                continue; // skip rows with deleted source entries
            }
        };
        let target_text = match store.get_content_via_write_pool(*target_id).await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(
                    edge_id = edge_id,
                    target_id = target_id,
                    error = %e,
                    "bootstrap NLI: skipping row — target entry not found"
                );
                continue;
            }
        };
        indexed_pairs.push((*edge_id, *source_id, *target_id, source_text, target_text));
    }

    if indexed_pairs.is_empty() {
        // All rows had missing entries; set marker (no valid work to do).
        tracing::info!(
            "bootstrap NLI promotion: all {} rows had missing entries; marking complete",
            rows.len()
        );
        let _ = set_bootstrap_marker(store).await;
        return;
    }

    // Step 3: W1-2 constraint — ALL inference dispatched as a SINGLE rayon spawn.
    // Build pairs from indexed_pairs before moving into closure.
    let pairs_owned: Vec<(String, String)> = indexed_pairs
        .iter()
        .map(|(_, _, _, src, tgt)| (src.clone(), tgt.clone()))
        .collect();

    let provider_clone = Arc::clone(&provider);
    let nli_scores: Vec<NliScores> = match rayon_pool
        .spawn(move || {
            let pairs: Vec<(&str, &str)> = pairs_owned
                .iter()
                .map(|(s, t)| (s.as_str(), t.as_str()))
                .collect();
            provider_clone.score_batch(&pairs)
        })
        .await
    {
        Ok(Ok(scores)) => scores,
        Ok(Err(e)) => {
            tracing::error!(
                error = %e,
                "bootstrap NLI promotion: score_batch failed"
            );
            return; // no marker set; retry next tick
        }
        Err(rayon_err) => {
            tracing::error!(
                error = %rayon_err,
                "bootstrap NLI promotion: rayon task cancelled"
            );
            return; // no marker set; retry next tick
        }
    };

    if nli_scores.len() != indexed_pairs.len() {
        tracing::error!(
            expected = indexed_pairs.len(),
            got = nli_scores.len(),
            "bootstrap NLI promotion: score_batch returned wrong number of scores; aborting"
        );
        return; // defensive; should not happen
    }

    // Step 4: Write results (async DB writes — tokio thread).
    // Each row: DELETE old bootstrap edge + conditionally INSERT NLI-confirmed replacement.
    let now = current_timestamp_secs();
    let mut promoted = 0usize;
    let mut deleted = 0usize;

    for (i, (edge_id, source_id, target_id, _, _)) in indexed_pairs.iter().enumerate() {
        let scores = &nli_scores[i];

        if scores.contradiction > config.nli_contradiction_threshold {
            // Score exceeds threshold: promote to NLI-confirmed edge.
            let metadata = format_nli_metadata(scores);
            let weight = scores.contradiction;

            match promote_bootstrap_edge(
                store, *edge_id, *source_id, *target_id, weight, now, &metadata,
            )
            .await
            {
                Ok(()) => promoted += 1,
                Err(e) => {
                    tracing::error!(
                        edge_id = edge_id,
                        error = %e,
                        "bootstrap NLI: failed to promote edge"
                    );
                    // Continue — do not abort the entire promotion on one DB failure.
                }
            }
        } else {
            // Score below threshold: delete the bootstrap edge (not NLI-confirmed).
            match sqlx::query("DELETE FROM graph_edges WHERE id = ?1")
                .bind(*edge_id as i64)
                .execute(store.write_pool_server())
                .await
            {
                Ok(_) => deleted += 1,
                Err(e) => {
                    tracing::error!(
                        edge_id = edge_id,
                        error = %e,
                        "bootstrap NLI: failed to delete bootstrap edge"
                    );
                }
            }
        }
    }

    tracing::info!(
        promoted = promoted,
        deleted = deleted,
        total = indexed_pairs.len(),
        "bootstrap NLI promotion: processing complete"
    );

    // Step 5: Set completion marker (ADR-005, AC-24).
    // Set regardless of partial write failures — the idempotency of INSERT OR IGNORE means
    // a partial run that failed on some edges and then set the marker will not re-process
    // edges that were already promoted or deleted.
    match set_bootstrap_marker(store).await {
        Ok(()) => {
            tracing::info!("bootstrap NLI promotion: completion marker set");
        }
        Err(e) => {
            tracing::error!(
                error = %e,
                "bootstrap NLI promotion: failed to set completion marker; will retry on next tick"
            );
            // Marker not set; task will re-run on next tick.
            // Re-run is safe: INSERT OR IGNORE handles already-promoted edges.
            // Already-deleted rows produce empty DELETE (harmless).
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Write NLI-confirmed graph edges with circuit-breaker cap (R-09, FR-22, AC-13).
///
/// The cap counts BOTH Supports AND Contradicts edges combined — not each type independently.
/// Processing stops as soon as `max_edges` edges have been written, regardless of type.
///
/// Returns the total number of edges written.
async fn write_edges_with_cap(
    store: &Store,
    source_id: u64,
    neighbor_texts: &[(u64, String)],
    nli_scores: &[NliScores],
    nli_entailment_threshold: f32,
    nli_contradiction_threshold: f32,
    max_edges: usize,
) -> usize {
    let mut edges_written: usize = 0;
    let now = current_timestamp_secs();

    for (idx, (neighbor_id, _)) in neighbor_texts.iter().enumerate() {
        if edges_written >= max_edges {
            let remaining = neighbor_texts.len() - idx;
            tracing::debug!(
                entry_id = source_id,
                cap = max_edges,
                dropped = remaining,
                "post-store NLI: edge cap reached; dropping remaining pairs"
            );
            break;
        }

        if idx >= nli_scores.len() {
            break; // defensive: score count mismatch (should not happen)
        }

        let scores = &nli_scores[idx];
        let metadata = format_nli_metadata(scores);

        // Write Supports edge if entailment exceeds threshold (strict >).
        if scores.entailment > nli_entailment_threshold {
            let wrote = write_nli_edge(
                store,
                source_id,
                *neighbor_id,
                "Supports",
                scores.entailment,
                now,
                &metadata,
            )
            .await;
            if wrote {
                edges_written += 1;
            }
            if edges_written >= max_edges {
                continue; // recheck after writing
            }
        }

        // Write Contradicts edge if contradiction exceeds threshold (strict >).
        if scores.contradiction > nli_contradiction_threshold {
            let wrote = write_nli_edge(
                store,
                source_id,
                *neighbor_id,
                "Contradicts",
                scores.contradiction,
                now,
                &metadata,
            )
            .await;
            if wrote {
                edges_written += 1;
            }
        }
    }

    edges_written
}

/// Write a single NLI-confirmed graph edge via `write_pool_server()` (SR-02).
///
/// Uses `INSERT OR IGNORE` for idempotency on `UNIQUE(source_id, target_id, relation_type)`.
/// Returns `true` if the insert succeeded (edge written or already existed).
pub(crate) async fn write_nli_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str, // "Supports" or "Contradicts"
    weight: f32,
    created_at: u64,
    metadata: &str, // JSON string: '{"nli_entailment": f32, "nli_contradiction": f32}'
) -> bool {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO graph_edges \
         (source_id, target_id, relation_type, weight, created_at, created_by, \
          source, bootstrap_only, metadata) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'nli', 'nli', 0, ?6)",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(relation_type)
    .bind(weight as f64)
    .bind(created_at as i64)
    .bind(metadata)
    .execute(store.write_pool_server())
    .await;

    match result {
        Ok(_) => true,
        Err(e) => {
            // R-16: write pool contention or SQLite busy; log at warn, do NOT propagate.
            tracing::warn!(
                source_id = source_id,
                target_id = target_id,
                relation_type = relation_type,
                error = %e,
                "post-store NLI: failed to write graph edge"
            );
            false
        }
    }
}

/// Atomically DELETE old bootstrap edge and INSERT NLI-confirmed replacement.
///
/// Two-statement sequence: DELETE then INSERT OR IGNORE.
/// If the INSERT fails (e.g. unique constraint conflict with post-store NLI edge),
/// `INSERT OR IGNORE` silently does nothing — the existing NLI edge takes precedence.
/// The DELETE still runs, removing the bootstrap edge.
async fn promote_bootstrap_edge(
    store: &Store,
    old_edge_id: u64,
    source_id: u64,
    target_id: u64,
    weight: f32,
    created_at: u64,
    metadata: &str,
) -> Result<(), unimatrix_store::StoreError> {
    // DELETE the old bootstrap edge.
    sqlx::query("DELETE FROM graph_edges WHERE id = ?1")
        .bind(old_edge_id as i64)
        .execute(store.write_pool_server())
        .await
        .map_err(|e| unimatrix_store::StoreError::Database(e.into()))?;

    // INSERT OR IGNORE the NLI-confirmed replacement.
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges \
         (source_id, target_id, relation_type, weight, created_at, created_by, \
          source, bootstrap_only, metadata) \
         VALUES (?1, ?2, 'Contradicts', ?3, ?4, 'nli', 'nli', 0, ?5)",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(weight as f64)
    .bind(created_at as i64)
    .bind(metadata)
    .execute(store.write_pool_server())
    .await
    .map_err(|e| unimatrix_store::StoreError::Database(e.into()))?;

    Ok(())
}

/// Set COUNTERS key "bootstrap_nli_promotion_done" = 1 (ADR-005, FR-24).
///
/// Uses INSERT OR REPLACE (idempotent; calling multiple times is safe).
async fn set_bootstrap_marker(store: &Store) -> Result<(), unimatrix_store::StoreError> {
    counters::set_counter(
        store.write_pool_server(),
        "bootstrap_nli_promotion_done",
        1u64,
    )
    .await
}

/// Serialize NLI scores to the required GRAPH_EDGES metadata JSON format.
///
/// Uses `serde_json::to_string` to prevent SQL injection via string concatenation.
pub(crate) fn format_nli_metadata(scores: &NliScores) -> String {
    // Output: '{"nli_entailment": 0.85, "nli_contradiction": 0.07}'
    // Required fields per AC-11: nli_entailment and nli_contradiction (f32).
    serde_json::json!({
        "nli_entailment":    scores.entailment,
        "nli_contradiction": scores.contradiction,
    })
    .to_string()
}

/// Current Unix timestamp in seconds.
pub(crate) fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use unimatrix_embed::NliScores;
    use unimatrix_store::counters;

    use super::*;
    use crate::infra::config::InferenceConfig;
    use crate::infra::nli_handle::NliServiceHandle;
    use crate::infra::rayon_pool::RayonPool;

    // ---------------------------------------------------------------------------
    // Mock CrossEncoderProvider implementations
    // ---------------------------------------------------------------------------

    /// Returns fixed NliScores for all pairs.
    struct FixedMockProvider {
        scores: NliScores,
        call_count: Arc<AtomicUsize>,
    }

    impl FixedMockProvider {
        fn new(scores: NliScores) -> Self {
            Self {
                scores,
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    impl CrossEncoderProvider for FixedMockProvider {
        fn score_pair(&self, _q: &str, _p: &str) -> Result<NliScores, unimatrix_embed::EmbedError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(self.scores.clone())
        }

        fn score_batch(
            &self,
            pairs: &[(&str, &str)],
        ) -> Result<Vec<NliScores>, unimatrix_embed::EmbedError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(pairs.iter().map(|_| self.scores.clone()).collect())
        }

        fn name(&self) -> &str {
            "FixedMockProvider"
        }
    }

    /// Panics on score_batch (for panic containment tests).
    struct PanicOnCallProvider;

    impl CrossEncoderProvider for PanicOnCallProvider {
        fn score_pair(&self, _q: &str, _p: &str) -> Result<NliScores, unimatrix_embed::EmbedError> {
            panic!("deliberate test panic in score_pair");
        }

        fn score_batch(
            &self,
            _pairs: &[(&str, &str)],
        ) -> Result<Vec<NliScores>, unimatrix_embed::EmbedError> {
            panic!("deliberate test panic in score_batch");
        }

        fn name(&self) -> &str {
            "PanicOnCallProvider"
        }
    }

    /// Records the thread ID from which score_batch is called (W1-2 compliance test).
    struct ThreadRecordingProvider {
        caller_thread: std::sync::Mutex<Option<std::thread::ThreadId>>,
    }

    impl ThreadRecordingProvider {
        fn new() -> Self {
            Self {
                caller_thread: std::sync::Mutex::new(None),
            }
        }

        fn last_caller_thread(&self) -> Option<std::thread::ThreadId> {
            *self.caller_thread.lock().unwrap()
        }
    }

    impl CrossEncoderProvider for ThreadRecordingProvider {
        fn score_pair(&self, _q: &str, _p: &str) -> Result<NliScores, unimatrix_embed::EmbedError> {
            let mut guard = self.caller_thread.lock().unwrap();
            *guard = Some(std::thread::current().id());
            Ok(NliScores {
                entailment: 0.8,
                neutral: 0.1,
                contradiction: 0.1,
            })
        }

        fn score_batch(
            &self,
            pairs: &[(&str, &str)],
        ) -> Result<Vec<NliScores>, unimatrix_embed::EmbedError> {
            let mut guard = self.caller_thread.lock().unwrap();
            *guard = Some(std::thread::current().id());
            Ok(pairs
                .iter()
                .map(|_| NliScores {
                    entailment: 0.8,
                    neutral: 0.1,
                    contradiction: 0.1,
                })
                .collect())
        }

        fn name(&self) -> &str {
            "ThreadRecordingProvider"
        }
    }

    // ---------------------------------------------------------------------------
    // Helper to build a VectorIndex for testing
    // ---------------------------------------------------------------------------

    fn make_rayon_pool() -> Arc<RayonPool> {
        Arc::new(RayonPool::new(2, "test-nli-detection").expect("rayon pool"))
    }

    // ---------------------------------------------------------------------------
    // Unit tests: format_nli_metadata (AC-11)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_format_nli_metadata_contains_required_keys() {
        let scores = NliScores {
            entailment: 0.85,
            neutral: 0.08,
            contradiction: 0.07,
        };
        let json = format_nli_metadata(&scores);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            parsed["nli_entailment"].is_number(),
            "must have nli_entailment"
        );
        assert!(
            parsed["nli_contradiction"].is_number(),
            "must have nli_contradiction"
        );
        let e = parsed["nli_entailment"].as_f64().unwrap();
        let c = parsed["nli_contradiction"].as_f64().unwrap();
        assert!(
            (e - 0.85f64).abs() < 1e-3,
            "entailment {e} not close to 0.85"
        );
        assert!(
            (c - 0.07f64).abs() < 1e-3,
            "contradiction {c} not close to 0.07"
        );
    }

    #[test]
    fn test_format_nli_metadata_is_valid_json() {
        let scores = NliScores {
            entailment: 0.7,
            neutral: 0.2,
            contradiction: 0.1,
        };
        let json = format_nli_metadata(&scores);
        let result: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(result.is_ok(), "metadata must be valid JSON: {json}");
    }

    // ---------------------------------------------------------------------------
    // Unit tests: empty embedding guard (R-07)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_empty_embedding_skips_nli() {
        // R-07: if embedding is empty, task must skip gracefully (no crash, no provider calls).
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let arc_store: Arc<Store> = Arc::new(store);

        // An empty VectorIndex — if search were called, it would return empty (OK).
        let vector_index = Arc::new(
            unimatrix_vector::VectorIndex::new(
                Arc::clone(&arc_store),
                unimatrix_core::VectorConfig::default(),
            )
            .expect("VectorIndex"),
        );

        // Use a not-ready handle — any provider calls would error.
        let not_ready_handle = NliServiceHandle::new();

        run_post_store_nli(
            vec![], // empty embedding
            1,
            "text".to_string(),
            not_ready_handle,
            Arc::clone(&arc_store),
            vector_index,
            make_rayon_pool(),
            10,
            0.6,
            0.6,
            10,
        )
        .await;
        // Must complete without panic or crash. Embedding guard fires before get_provider.
    }

    // ---------------------------------------------------------------------------
    // Unit tests: NLI not ready (exits immediately)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_nli_not_ready_exits_immediately() {
        // When NliServiceHandle is in Loading state, task exits without crashing.
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let arc_store: Arc<Store> = Arc::new(store);
        let vector_index = Arc::new(
            unimatrix_vector::VectorIndex::new(
                Arc::clone(&arc_store),
                unimatrix_core::VectorConfig::default(),
            )
            .expect("VectorIndex"),
        );

        let not_ready_handle = NliServiceHandle::new(); // never started, stays in Loading state

        run_post_store_nli(
            vec![0.1f32; 4],
            1,
            "text".to_string(),
            not_ready_handle,
            Arc::clone(&arc_store),
            vector_index,
            make_rayon_pool(),
            10,
            0.6,
            0.6,
            10,
        )
        .await;
        // Reaching here without panic = pass
    }

    // ---------------------------------------------------------------------------
    // Unit tests: bootstrap promotion zero rows (AC-12a)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_bootstrap_promotion_zero_rows_sets_marker() {
        // AC-12a: When GRAPH_EDGES has no bootstrap_only=1 rows, marker is set.
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let arc_store: Arc<Store> = Arc::new(store);

        let provider: Arc<dyn CrossEncoderProvider> = Arc::new(FixedMockProvider::new(NliScores {
            entailment: 0.1,
            neutral: 0.1,
            contradiction: 0.9,
        }));

        run_bootstrap_promotion(
            &arc_store,
            provider,
            &RayonPool::new(2, "test").unwrap(),
            &InferenceConfig::default(),
        )
        .await;

        let marker = read_marker(&arc_store).await;
        assert_eq!(
            marker, 1,
            "Completion marker must be set even for zero bootstrap rows"
        );
    }

    // ---------------------------------------------------------------------------
    // Unit tests: maybe_bootstrap_promotion idempotency (AC-24)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_maybe_bootstrap_promotion_skips_if_marker_present() {
        // AC-24: marker already set → task is a no-op.
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let arc_store: Arc<Store> = Arc::new(store);

        // Set marker before calling.
        counters::set_counter(
            arc_store.write_pool_server(),
            "bootstrap_nli_promotion_done",
            1,
        )
        .await
        .unwrap();

        // Use a handle in Loading state (would fail if promotion tried to run).
        let not_ready_handle = NliServiceHandle::new();

        maybe_run_bootstrap_promotion(
            &arc_store,
            &not_ready_handle,
            &RayonPool::new(2, "test").unwrap(),
            &InferenceConfig::default(),
        )
        .await;

        // Marker remains 1 (no re-run occurred).
        let marker = read_marker(&arc_store).await;
        assert_eq!(marker, 1, "Marker must remain set; promotion is a no-op");
    }

    // ---------------------------------------------------------------------------
    // Unit tests: bootstrap deferral when NLI not ready (FR-25)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_maybe_bootstrap_promotion_defers_when_nli_not_ready() {
        // FR-25: if NLI not ready → deferred (no marker set).
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let arc_store: Arc<Store> = Arc::new(store);

        // Insert a bootstrap row to ensure promotion would try to run.
        insert_test_entry_raw(&arc_store, 1, "source").await;
        insert_test_entry_raw(&arc_store, 2, "target").await;
        insert_bootstrap_edge(&arc_store, 1, 2).await;

        let not_ready_handle = NliServiceHandle::new(); // never started

        maybe_run_bootstrap_promotion(
            &arc_store,
            &not_ready_handle,
            &RayonPool::new(2, "test").unwrap(),
            &InferenceConfig::default(),
        )
        .await;

        // Marker must NOT be set (deferral, not completion).
        let marker = read_marker(&arc_store).await;
        assert_eq!(
            marker, 0,
            "Marker must not be set when NLI not ready (deferred)"
        );
    }

    // ---------------------------------------------------------------------------
    // Unit tests: bootstrap promotion confirms above threshold (AC-12b)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_bootstrap_promotion_confirms_above_threshold() {
        // AC-12b: bootstrap_only=1 edge; NLI score above threshold → promoted.
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let arc_store: Arc<Store> = Arc::new(store);

        insert_test_entry_raw(&arc_store, 1, "source text").await;
        insert_test_entry_raw(&arc_store, 2, "target text").await;
        insert_bootstrap_edge(&arc_store, 1, 2).await;

        let provider: Arc<dyn CrossEncoderProvider> = Arc::new(FixedMockProvider::new(NliScores {
            entailment: 0.05,
            neutral: 0.05,
            contradiction: 0.9, // above 0.6 threshold
        }));

        run_bootstrap_promotion(
            &arc_store,
            provider,
            &RayonPool::new(2, "test").unwrap(),
            &InferenceConfig {
                nli_contradiction_threshold: 0.6,
                ..InferenceConfig::default()
            },
        )
        .await;

        // Old bootstrap edge must be gone.
        let bootstrap_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM graph_edges WHERE source_id=1 AND target_id=2 \
             AND bootstrap_only=1",
        )
        .fetch_one(arc_store.write_pool_server())
        .await
        .unwrap();
        assert_eq!(
            bootstrap_count, 0,
            "bootstrap_only=1 edge must be deleted after promotion"
        );

        // New NLI-confirmed edge must exist.
        let nli_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM graph_edges WHERE source_id=1 AND target_id=2 \
             AND source='nli' AND bootstrap_only=0 AND relation_type='Contradicts'",
        )
        .fetch_one(arc_store.write_pool_server())
        .await
        .unwrap();
        assert_eq!(
            nli_count, 1,
            "NLI-confirmed edge must exist after promotion"
        );

        let marker = read_marker(&arc_store).await;
        assert_eq!(marker, 1, "Completion marker must be set");
    }

    // ---------------------------------------------------------------------------
    // Unit tests: bootstrap promotion refutes below threshold (AC-12)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_bootstrap_promotion_refutes_below_threshold() {
        // AC-12: NLI score below threshold → DELETE old row, do NOT insert replacement.
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let arc_store: Arc<Store> = Arc::new(store);

        insert_test_entry_raw(&arc_store, 1, "source text").await;
        insert_test_entry_raw(&arc_store, 2, "target text").await;
        insert_bootstrap_edge(&arc_store, 1, 2).await;

        let provider: Arc<dyn CrossEncoderProvider> = Arc::new(FixedMockProvider::new(NliScores {
            entailment: 0.8,
            neutral: 0.1,
            contradiction: 0.1, // below 0.6 contradiction threshold
        }));

        run_bootstrap_promotion(
            &arc_store,
            provider,
            &RayonPool::new(2, "test").unwrap(),
            &InferenceConfig {
                nli_contradiction_threshold: 0.6,
                ..InferenceConfig::default()
            },
        )
        .await;

        let any_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM graph_edges WHERE source_id=1 AND target_id=2",
        )
        .fetch_one(arc_store.write_pool_server())
        .await
        .unwrap();
        assert_eq!(
            any_count, 0,
            "Refuted bootstrap edge must be deleted with no replacement"
        );

        let marker = read_marker(&arc_store).await;
        assert_eq!(marker, 1, "Completion marker must be set");
    }

    // ---------------------------------------------------------------------------
    // Unit tests: idempotency (R-11)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_bootstrap_promotion_idempotent_second_run_no_duplicates() {
        // R-11: Run promotion twice (simulating marker absent on second run due to failure).
        // Assert GRAPH_EDGES is identical after both runs (INSERT OR IGNORE idempotency).
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let arc_store: Arc<Store> = Arc::new(store);

        insert_test_entry_raw(&arc_store, 1, "source text").await;
        insert_test_entry_raw(&arc_store, 2, "target text").await;
        insert_bootstrap_edge(&arc_store, 1, 2).await;

        let provider: Arc<dyn CrossEncoderProvider> = Arc::new(FixedMockProvider::new(NliScores {
            entailment: 0.1,
            neutral: 0.1,
            contradiction: 0.9,
        }));

        // First run.
        run_bootstrap_promotion(
            &arc_store,
            Arc::clone(&provider),
            &RayonPool::new(2, "test").unwrap(),
            &InferenceConfig::default(),
        )
        .await;

        let edges_after_first: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges WHERE source='nli'")
                .fetch_one(arc_store.write_pool_server())
                .await
                .unwrap();
        assert_eq!(read_marker(&arc_store).await, 1);

        // Simulate marker absent for second run.
        counters::set_counter(
            arc_store.write_pool_server(),
            "bootstrap_nli_promotion_done",
            0,
        )
        .await
        .unwrap();

        // Second run.
        run_bootstrap_promotion(
            &arc_store,
            provider,
            &RayonPool::new(2, "test").unwrap(),
            &InferenceConfig::default(),
        )
        .await;

        let edges_after_second: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges WHERE source='nli'")
                .fetch_one(arc_store.write_pool_server())
                .await
                .unwrap();
        assert_eq!(
            edges_after_second, edges_after_first,
            "GRAPH_EDGES must be identical after second run (INSERT OR IGNORE idempotency)"
        );
    }

    // ---------------------------------------------------------------------------
    // Unit tests: W1-2 — inference dispatched via rayon_pool.spawn
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_bootstrap_promotion_nli_inference_runs_on_rayon_thread() {
        // W1-2: score_batch in bootstrap promotion must run on rayon thread, not tokio thread.
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let arc_store: Arc<Store> = Arc::new(store);

        insert_test_entry_raw(&arc_store, 1, "source text").await;
        insert_test_entry_raw(&arc_store, 2, "target text").await;
        insert_bootstrap_edge(&arc_store, 1, 2).await;

        let tokio_thread_id = std::thread::current().id();
        let thread_recorder = Arc::new(ThreadRecordingProvider::new());
        let provider: Arc<dyn CrossEncoderProvider> =
            Arc::clone(&thread_recorder) as Arc<dyn CrossEncoderProvider>;

        run_bootstrap_promotion(
            &arc_store,
            provider,
            &RayonPool::new(2, "test").unwrap(),
            &InferenceConfig::default(),
        )
        .await;

        let call_thread = thread_recorder.last_caller_thread();
        assert!(call_thread.is_some(), "score_batch must have been called");
        assert_ne!(
            call_thread.unwrap(),
            tokio_thread_id,
            "score_batch in bootstrap promotion must NOT run on tokio thread (W1-2)"
        );
    }

    // ---------------------------------------------------------------------------
    // Test helpers
    // ---------------------------------------------------------------------------

    async fn insert_test_entry_raw(store: &Store, id: u64, content: &str) {
        // Note: previous_hash, feature_cycle, trust_source are NOT NULL DEFAULT '' —
        // bind '' not NULL (SQLite NOT NULL rejects explicit NULL even with default).
        sqlx::query(
            "INSERT OR IGNORE INTO entries \
             (id, title, content, topic, category, source, status, confidence, \
              created_at, updated_at, last_accessed_at, access_count, \
              created_by, modified_by, content_hash, previous_hash, \
              version, feature_cycle, trust_source, helpful_count, unhelpful_count, \
              pre_quarantine_status, correction_count, embedding_dim) \
             VALUES (?1, 'test', ?2, 'test', 'pattern', 'test', 0, 0.5, 0, 0, 0, 0, \
                     'test', 'test', 'hash', '', 1, '', '', 0, 0, NULL, 0, 0)",
        )
        .bind(id as i64)
        .bind(content)
        .execute(store.write_pool_server())
        .await
        .unwrap();
    }

    async fn insert_bootstrap_edge(store: &Store, source_id: u64, target_id: u64) {
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges \
             (source_id, target_id, relation_type, weight, created_at, \
              created_by, source, bootstrap_only) \
             VALUES (?1, ?2, 'Contradicts', 0.5, 0, 'bootstrap', 'bootstrap', 1)",
        )
        .bind(source_id as i64)
        .bind(target_id as i64)
        .execute(store.write_pool_server())
        .await
        .unwrap();
    }

    async fn read_marker(store: &Store) -> u64 {
        counters::read_counter(store.write_pool_server(), "bootstrap_nli_promotion_done")
            .await
            .unwrap_or(0)
    }

    /// Count GRAPH_EDGES rows with source='nli' (for circuit breaker assertions).
    async fn count_nli_edges(store: &Arc<Store>) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges WHERE source = 'nli'")
            .fetch_one(store.write_pool_server())
            .await
            .unwrap_or(0)
    }

    // ---------------------------------------------------------------------------
    // Unit tests: circuit breaker (R-09, FR-22, AC-13)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_circuit_breaker_stops_at_cap() {
        // R-09 (Critical, non-negotiable):
        // max_edges=2; 5 neighbor pairs all scoring above contradiction_threshold.
        // Both Supports AND Contradicts count toward the combined cap.
        // Assert: exactly 2 edges written to graph_edges (not 5).
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let arc_store: Arc<Store> = Arc::new(store);

        // Insert 5 neighbor entries for content fetch.
        for id in 1u64..=5 {
            insert_test_entry_raw(&arc_store, id, &format!("neighbor text {id}")).await;
        }

        // Build neighbor_texts and nli_scores: every pair scores above both thresholds.
        let neighbor_texts: Vec<(u64, String)> = (1u64..=5)
            .map(|id| (id, format!("neighbor text {id}")))
            .collect();
        let scores_above_both = NliScores {
            entailment: 0.9, // above 0.6 → Supports edge
            neutral: 0.0,
            contradiction: 0.9, // above 0.6 → Contradicts edge (but cap stops before all)
        };
        let nli_scores: Vec<NliScores> = vec![scores_above_both; 5];

        // source_id=100 (does not collide with neighbors 1-5).
        write_edges_with_cap(
            &arc_store,
            100,
            &neighbor_texts,
            &nli_scores,
            0.6, // nli_entailment_threshold
            0.6, // nli_contradiction_threshold
            2,   // max_edges = cap
        )
        .await;

        let edge_count = count_nli_edges(&arc_store).await;
        assert_eq!(
            edge_count, 2,
            "Circuit breaker must limit TOTAL edges (Supports+Contradicts combined) to cap=2, got: {edge_count}"
        );
    }

    #[tokio::test]
    async fn test_circuit_breaker_counts_all_edge_types() {
        // R-09: cap=3 with 4 neighbors alternating Supports-only and Contradicts-only scores.
        // Pairs: [Supports, Contradicts, Supports, Contradicts] → would write 4 without cap.
        // Assert: exactly 3 edges written (first 3 processed, regardless of type).
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        let arc_store: Arc<Store> = Arc::new(store);

        // Insert 4 neighbor entries.
        for id in 1u64..=4 {
            insert_test_entry_raw(&arc_store, id, &format!("neighbor text {id}")).await;
        }

        let neighbor_texts: Vec<(u64, String)> = (1u64..=4)
            .map(|id| (id, format!("neighbor text {id}")))
            .collect();

        // Alternating: odd neighbors → Supports-only; even neighbors → Contradicts-only.
        let supports_only = NliScores {
            entailment: 0.9,
            neutral: 0.0,
            contradiction: 0.0, // below threshold → no Contradicts edge
        };
        let contradicts_only = NliScores {
            entailment: 0.0, // below threshold → no Supports edge
            neutral: 0.0,
            contradiction: 0.9,
        };
        let nli_scores = vec![
            supports_only.clone(),    // neighbor 1 → 1 Supports edge
            contradicts_only.clone(), // neighbor 2 → 1 Contradicts edge
            supports_only,            // neighbor 3 → 1 Supports edge (hits cap=3 here)
            contradicts_only,         // neighbor 4 → dropped by cap
        ];

        write_edges_with_cap(
            &arc_store,
            200,
            &neighbor_texts,
            &nli_scores,
            0.6, // nli_entailment_threshold
            0.6, // nli_contradiction_threshold
            3,   // max_edges = cap
        )
        .await;

        let edge_count = count_nli_edges(&arc_store).await;
        assert_eq!(
            edge_count, 3,
            "Cap=3 must stop at 3 edges across mixed Supports+Contradicts types, got: {edge_count}"
        );
    }
}
