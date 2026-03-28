//! Background graph inference tick — Supports edges only (crt-029).
//!
//! `run_graph_inference_tick` is the counterpart to `maybe_run_bootstrap_promotion`:
//! one-shot/idempotency-gated vs recurring/cap-throttled. Both share W1-2 + rayon pool.
//!
//! # W1-2 Contract
//! ALL `CrossEncoderProvider::score_batch` calls via `rayon_pool.spawn()`.
//! `spawn_blocking` prohibited. Inline async NLI prohibited.
//!
//! # Supports-Only (C-13 / AC-10a)
//! This module writes ONLY `Supports` edges. No `contradiction_threshold` parameter.
//! The `contradiction` score is discarded. Dedicated contradiction detection path
//! is the sole `Contradicts` writer.
//!
//! # R-09 Rayon/Tokio Boundary (C-14)
//! The rayon closure in Phase 7 MUST be synchronous CPU-bound only.
//! PROHIBITED: `tokio::runtime::Handle::current()`, `.await`, any async call.
//! Rayon worker threads have no Tokio runtime; violations panic at runtime.
//! Detection: `grep -n 'Handle::current' nli_detection_tick.rs` must return empty.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::Arc;

use unimatrix_core::{Store, VectorIndex};
use unimatrix_embed::{CrossEncoderProvider, NliScores};
use unimatrix_store::{EntryRecord, Status};

use crate::infra::config::InferenceConfig;
use crate::infra::nli_handle::NliServiceHandle;
use crate::infra::rayon_pool::RayonPool;

// pub(crate) symbols promoted from nli_detection.rs (R-11):
use crate::services::nli_detection::{current_timestamp_secs, format_nli_metadata, write_nli_edge};

// ---------------------------------------------------------------------------
// Public: background graph inference tick
// ---------------------------------------------------------------------------

/// Background tick filling `Supports` edges via HNSW expansion + rayon NLI scoring.
///
/// Recurring, cap-throttled via `config.max_graph_inference_per_tick`. Infallible.
/// Single rayon spawn per tick (W1-2 / AC-08 / entry #3653).
/// Never writes `Contradicts` edges (C-13 / AC-10a).
pub async fn run_graph_inference_tick(
    store: &Store,
    nli_handle: &NliServiceHandle,
    vector_index: &VectorIndex,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
) {
    // Phase 1 — Guard: silent no-op when NLI not ready (fires every tick when disabled).
    let provider = match nli_handle.get_provider().await {
        Ok(p) => p,
        Err(_) => return,
    };

    // Phase 2 — Data fetch: three sequential async DB reads.
    let all_active = match store.query_by_status(Status::Active).await {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(error = %e, "graph inference tick: failed to fetch active entries");
            return;
        }
    };

    if all_active.is_empty() {
        tracing::debug!("graph inference tick: no active entries, skipping");
        return;
    }

    let isolated_ids: HashSet<u64> = match store.query_entries_without_edges().await {
        Ok(ids) => ids.into_iter().collect(),
        Err(e) => {
            // Degraded: proceed without isolation priority (tier 2 lost).
            tracing::warn!(error = %e, "graph inference tick: failed to fetch isolated IDs; proceeding without isolation priority");
            HashSet::new()
        }
    };

    let existing_supports_pairs = match store.query_existing_supports_pairs().await {
        Ok(pairs) => pairs,
        Err(e) => {
            // Degraded: empty pre-filter; INSERT OR IGNORE is backstop.
            tracing::warn!(error = %e, "graph inference tick: failed to fetch existing Supports pairs; INSERT OR IGNORE dedup");
            HashSet::new()
        }
    };

    // Phase 3 — Source candidate selection (cap BEFORE embedding).
    // AC-06c / R-02: operates on metadata only (IDs + category strings). No get_embedding here.
    // Invariant: source_candidates.len() <= config.max_graph_inference_per_tick (ADR-003).
    //
    // Build the embedded set so select_source_candidates can exclude no-embedding entries
    // (RC-2 fix: prevents no-embedding entries from permanently polluting tier 1).
    let embedded_ids: HashSet<u64> = all_active
        .iter()
        .filter(|e| vector_index.contains(e.id))
        .map(|e| e.id)
        .collect();

    let source_candidates = select_source_candidates(
        &all_active,
        &existing_supports_pairs,
        &isolated_ids,
        &embedded_ids,
        config.max_graph_inference_per_tick,
    );

    if source_candidates.is_empty() {
        tracing::debug!("graph inference tick: no source candidates, skipping");
        return;
    }

    // Phase 4 — HNSW expansion (embeddings for capped list only).
    // VectorIndex::search and get_embedding are synchronous (internal RwLock, no Tokio I/O).
    // get_embedding called at most max_graph_inference_per_tick times (SR-02).
    let mut candidate_pairs: Vec<(u64, u64, f32)> = Vec::new(); // (source, target, similarity)
    let mut seen_pairs: HashSet<(u64, u64)> = HashSet::new();

    const EF_SEARCH: usize = 32;

    for source_id in &source_candidates {
        let embedding = match vector_index.get_embedding(*source_id) {
            Some(emb) => emb,
            None => {
                tracing::debug!(
                    entry_id = source_id,
                    "graph inference tick: no embedding, skipping source"
                );
                continue;
            }
        };

        let search_results = match vector_index.search(
            &embedding,
            config.graph_inference_k,
            EF_SEARCH,
        ) {
            Ok(results) => results,
            Err(e) => {
                tracing::debug!(entry_id = source_id, error = %e, "graph inference tick: HNSW search failed");
                continue;
            }
        };

        for result in search_results {
            let neighbour_id = result.entry_id;
            let similarity = result.similarity as f32;

            if neighbour_id == *source_id {
                continue;
            }
            // Strict >: at-threshold means no candidate.
            if similarity <= config.supports_candidate_threshold {
                continue;
            }

            // Normalise to (min, max) for symmetric dedup.
            let lo = source_id.min(&neighbour_id);
            let hi = source_id.max(&neighbour_id);
            let pair_key = (*lo, *hi);

            if existing_supports_pairs.contains(&pair_key) {
                continue; // pre-filter optimisation; INSERT OR IGNORE is backstop
            }
            if seen_pairs.contains(&pair_key) {
                continue;
            }
            seen_pairs.insert(pair_key);
            candidate_pairs.push((*source_id, neighbour_id, similarity));
        }
    }

    if candidate_pairs.is_empty() {
        tracing::debug!("graph inference tick: no candidate pairs after HNSW expansion");
        return;
    }

    // Phase 5 — Priority sort and truncation.
    // Order: (1) cross-category first, (2) either endpoint isolated, (3) similarity desc.
    let category_map: std::collections::HashMap<u64, &str> = all_active
        .iter()
        .map(|e| (e.id, e.category.as_str()))
        .collect();

    candidate_pairs.sort_by(|(a_src, a_tgt, a_sim), (b_src, b_tgt, b_sim)| {
        let a_cross = category_map.get(a_src) != category_map.get(a_tgt);
        let b_cross = category_map.get(b_src) != category_map.get(b_tgt);
        match b_cross.cmp(&a_cross) {
            Ordering::Equal => {
                let a_iso = isolated_ids.contains(a_src) || isolated_ids.contains(a_tgt);
                let b_iso = isolated_ids.contains(b_src) || isolated_ids.contains(b_tgt);
                match b_iso.cmp(&a_iso) {
                    Ordering::Equal => b_sim.partial_cmp(a_sim).unwrap_or(Ordering::Equal),
                    other => other,
                }
            }
            other => other,
        }
    });
    candidate_pairs.truncate(config.max_graph_inference_per_tick);

    // Phase 6 — Text fetch via write_pool (WAL visibility; bootstrap promotion pattern).
    let mut scored_input: Vec<(u64, u64, String, String)> = Vec::new(); // (src, tgt, src_text, tgt_text)

    for (source_id, target_id, _) in &candidate_pairs {
        let source_text = match store.get_content_via_write_pool(*source_id).await {
            Ok(text) => text,
            Err(e) => {
                tracing::debug!(entry_id = source_id, error = %e, "graph inference tick: source content fetch failed, skipping pair");
                continue;
            }
        };
        let target_text = match store.get_content_via_write_pool(*target_id).await {
            Ok(text) => text,
            Err(e) => {
                tracing::debug!(entry_id = target_id, error = %e, "graph inference tick: target content fetch failed, skipping pair");
                continue;
            }
        };
        scored_input.push((*source_id, *target_id, source_text, target_text));
    }

    if scored_input.is_empty() {
        tracing::debug!("graph inference tick: no pairs with fetchable content, skipping NLI");
        return;
    }

    // Phase 7 — W1-2 dispatch: single rayon spawn (C-01 / AC-08 / entry #3653).
    //
    // C-14 / R-09 CRITICAL: closure body is SYNC-ONLY CPU-bound.
    // PROHIBITED: tokio::runtime::Handle::current(), .await, any async call.
    // Rayon threads have no Tokio runtime; violations panic at runtime (compile-invisible).
    // Pre-merge gate: grep -n 'Handle::current' nli_detection_tick.rs must return empty.
    let nli_pairs: Vec<(String, String)> = scored_input
        .iter()
        .map(|(_, _, src, tgt)| (src.clone(), tgt.clone()))
        .collect();

    let provider_clone = Arc::clone(&provider);

    let nli_result = rayon_pool
        .spawn(move || {
            // SYNC-ONLY CLOSURE — no .await, no Handle::current()
            let pairs_ref: Vec<(&str, &str)> = nli_pairs
                .iter()
                .map(|(q, p)| (q.as_str(), p.as_str()))
                .collect();
            provider_clone.score_batch(&pairs_ref)
        })
        .await;
    // .await is OUTSIDE the closure — on the tokio thread awaiting the rayon result.

    let nli_scores: Vec<NliScores> = match nli_result {
        Ok(Ok(scores)) => scores,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "graph inference tick: score_batch failed");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "graph inference tick: rayon task cancelled");
            return;
        }
    };

    if nli_scores.len() != scored_input.len() {
        tracing::warn!(
            scores_len = nli_scores.len(),
            pairs_len = scored_input.len(),
            "graph inference tick: score_batch length mismatch; skipping write"
        );
        return;
    }

    let n = nli_scores.len();
    let (nli_score_max, nli_score_mean, nli_score_p75) = nli_score_stats(&nli_scores);
    tracing::debug!(
        nli_score_max,
        nli_score_mean,
        nli_score_p75,
        threshold = config.supports_edge_threshold,
        pairs = n,
        "graph inference tick: nli score distribution"
    );

    let write_pairs: Vec<(u64, u64)> = scored_input
        .iter()
        .map(|(src, tgt, _, _)| (*src, *tgt))
        .collect();

    // Phase 8 — Write (Supports only; no contradiction_threshold; C-13 / AC-10a).
    let edges_written = write_inferred_edges_with_cap(
        store,
        &write_pairs,
        &nli_scores,
        config.supports_edge_threshold, // NOT nli_entailment_threshold (C-06)
        config.max_graph_inference_per_tick,
    )
    .await;

    tracing::debug!(
        edges_written,
        pairs_scored = nli_scores.len(),
        source_candidates = source_candidates.len(),
        "graph inference tick complete"
    );
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Select up to `max_sources` source IDs in priority order (AC-06c / R-02).
///
/// Operates on metadata only — no `get_embedding` calls. The cap on this function
/// bounds all Phase 4 `get_embedding` calls to `max_sources` (SR-02).
///
/// Only entries present in `embedded_ids` are eligible — entries without an embedding
/// are permanently excluded so they cannot occupy slots every tick (RC-2 fix).
///
/// Priority: (1) isolated entries with embeddings, (2) non-isolated entries with embeddings.
/// Both tiers are shuffled before selection to prevent deterministic re-selection of the
/// same top-N candidates across ticks (RC-1 fix). Full cross-category pair priority is
/// applied in Phase 5 on expanded candidates.
///
/// `existing_edge_set` is accepted per the architecture's function signature; it is not
/// consumed in source selection — cross-category pair detection is deferred to Phase 5.
fn select_source_candidates(
    all_active: &[EntryRecord],
    _existing_edge_set: &HashSet<(u64, u64)>,
    isolated_ids: &HashSet<u64>,
    embedded_ids: &HashSet<u64>,
    max_sources: usize,
) -> Vec<u64> {
    use rand::seq::SliceRandom;

    if all_active.is_empty() || max_sources == 0 {
        return vec![];
    }

    let mut tier1: Vec<&EntryRecord> = Vec::new(); // isolated + has embedding — highest priority
    let mut tier2: Vec<&EntryRecord> = Vec::new(); // non-isolated + has embedding

    for entry in all_active {
        if !embedded_ids.contains(&entry.id) {
            continue; // no embedding — skip permanently (RC-2 fix)
        }
        if isolated_ids.contains(&entry.id) {
            tier1.push(entry);
        } else {
            tier2.push(entry);
        }
    }

    let mut rng = rand::rng();
    tier1.shuffle(&mut rng);
    tier2.shuffle(&mut rng);

    tier1
        .iter()
        .chain(tier2.iter())
        .map(|e| e.id)
        .take(max_sources)
        .collect()
}

/// Compute (max, mean, p75) of the `.entailment` field across all NLI scores.
///
/// Returns `(0.0, 0.0, 0.0)` for an empty slice. Uses nearest-rank p75 (same formula as
/// `compute_observed_spread` in `status.rs`). f32 throughout — NLI scores are at the ONNX
/// boundary and must not be widened.
fn nli_score_stats(scores: &[NliScores]) -> (f32, f32, f32) {
    if scores.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut vals: Vec<f32> = scores.iter().map(|s| s.entailment).collect();
    vals.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = vals.len();
    let max = vals[n - 1];
    let mean = vals.iter().sum::<f32>() / n as f32;
    let p75_idx = (((0.75 * n as f32).ceil() as usize).saturating_sub(1)).min(n - 1);
    let p75 = vals[p75_idx];
    (max, mean, p75)
}

/// Write NLI-scored `Supports` edges; returns `edges_written`.
///
/// **Supports-ONLY** (C-13 / AC-10a): no `contradiction_threshold` parameter.
/// The `contradiction` score is intentionally discarded. `INSERT OR IGNORE` provides
/// idempotency. Cap stops at `max_edges` (FR-09, AC-11, strict `>` threshold AC-09).
async fn write_inferred_edges_with_cap(
    store: &Store,
    pairs: &[(u64, u64)],
    nli_scores: &[NliScores],
    supports_threshold: f32,
    max_edges: usize,
) -> usize {
    debug_assert_eq!(pairs.len(), nli_scores.len());

    let mut edges_written: usize = 0;
    let timestamp = current_timestamp_secs();

    for i in 0..pairs.len() {
        if edges_written >= max_edges {
            break; // cap reached (FR-09, AC-11)
        }

        let (source_id, target_id) = pairs[i];
        let scores = &nli_scores[i];

        // Evaluate ONLY entailment; contradiction is DISCARDED (C-13 / AC-10a).
        // Strict >: at-threshold means no edge (AC-09).
        if scores.entailment > supports_threshold {
            let metadata_json = format_nli_metadata(scores);
            let written = write_nli_edge(
                store,
                source_id,
                target_id,
                "Supports", // ONLY "Supports" written here (C-13)
                scores.entailment,
                timestamp,
                &metadata_json,
            )
            .await;
            if written {
                edges_written += 1;
            }
        }
    }

    edges_written
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use unimatrix_embed::NliScores;
    use unimatrix_store::Status;

    use super::*;
    use crate::infra::config::InferenceConfig;
    use crate::infra::nli_handle::NliServiceHandle;
    use crate::infra::rayon_pool::RayonPool;

    fn make_rayon_pool() -> Arc<RayonPool> {
        Arc::new(RayonPool::new(2, "test-nli-tick").expect("rayon pool"))
    }

    /// Insert a test entry with a known fixed ID via raw SQL (pattern from nli_detection.rs).
    async fn insert_test_entry(store: &Store, id: u64) {
        sqlx::query(
            "INSERT OR IGNORE INTO entries \
             (id, title, content, topic, category, source, status, confidence, \
              created_at, updated_at, last_accessed_at, access_count, \
              created_by, modified_by, content_hash, previous_hash, \
              version, feature_cycle, trust_source, helpful_count, unhelpful_count, \
              pre_quarantine_status, correction_count, embedding_dim) \
             VALUES (?1, 'test', 'test content', 'test-topic', 'decision', 'test', 0, 0.5, \
                     ?2, ?2, 0, 0, 'test', 'test', 'hash', '', 1, '', '', 0, 0, NULL, 0, 0)",
        )
        .bind(id as i64)
        .bind(id as i64) // created_at = id for deterministic ordering
        .execute(store.write_pool_server())
        .await
        .unwrap();
    }

    fn make_entry(id: u64, category: &str, created_at: u64) -> EntryRecord {
        EntryRecord {
            id,
            title: format!("entry-{id}"),
            content: format!("content-{id}"),
            topic: "test-topic".to_string(),
            category: category.to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            confidence: 0.5,
            created_at,
            updated_at: created_at,
            last_accessed_at: 0,
            access_count: 0,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 0,
            created_by: String::new(),
            modified_by: String::new(),
            content_hash: String::new(),
            previous_hash: String::new(),
            version: 1,
            feature_cycle: String::new(),
            trust_source: String::new(),
            helpful_count: 0,
            unhelpful_count: 0,
            pre_quarantine_status: None,
        }
    }

    fn scores_above(entailment: f32) -> NliScores {
        NliScores {
            entailment,
            neutral: 0.1,
            contradiction: 0.05,
        }
    }

    // -----------------------------------------------------------------------
    // select_source_candidates
    // -----------------------------------------------------------------------

    /// AC-06c / R-02: cap enforced.
    #[test]
    fn test_select_source_candidates_cap_enforced() {
        let entries: Vec<EntryRecord> = (0..200u64).map(|i| make_entry(i, "decision", i)).collect();
        let all_ids: HashSet<u64> = (0..200u64).collect();
        let result =
            select_source_candidates(&entries, &HashSet::new(), &HashSet::new(), &all_ids, 10);
        assert_eq!(result.len(), 10);
        let unique: HashSet<u64> = result.iter().cloned().collect();
        assert_eq!(unique.len(), 10, "no duplicates");
    }

    #[test]
    fn test_select_source_candidates_cap_larger_than_entries() {
        let entries: Vec<EntryRecord> = (0..5u64).map(|i| make_entry(i, "pattern", i)).collect();
        let all_ids: HashSet<u64> = (0..5u64).collect();
        let result =
            select_source_candidates(&entries, &HashSet::new(), &HashSet::new(), &all_ids, 20);
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_select_source_candidates_empty_input() {
        let result =
            select_source_candidates(&[], &HashSet::new(), &HashSet::new(), &HashSet::new(), 10);
        assert!(result.is_empty());
    }

    #[test]
    fn test_select_source_candidates_max_sources_zero() {
        let entries: Vec<EntryRecord> = (0..5u64).map(|i| make_entry(i, "decision", i)).collect();
        let all_ids: HashSet<u64> = (0..5u64).collect();
        let result =
            select_source_candidates(&entries, &HashSet::new(), &HashSet::new(), &all_ids, 0);
        assert!(result.is_empty());
    }

    /// AC-07 / R-12: isolated entries prioritised before non-isolated.
    #[test]
    fn test_select_source_candidates_isolated_second() {
        let entries: Vec<EntryRecord> = (0..5u64).map(|i| make_entry(i, "decision", i)).collect();
        let mut isolated = HashSet::new();
        isolated.insert(3u64);
        isolated.insert(4u64);
        let all_ids: HashSet<u64> = (0..5u64).collect();
        let result = select_source_candidates(&entries, &HashSet::new(), &isolated, &all_ids, 2);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&3));
        assert!(result.contains(&4));
    }

    #[test]
    fn test_select_source_candidates_remainder_by_created_at() {
        let entries: Vec<EntryRecord> = (0..5u64).map(|i| make_entry(i, "decision", i)).collect();
        let all_ids: HashSet<u64> = (0..5u64).collect();
        let result =
            select_source_candidates(&entries, &HashSet::new(), &HashSet::new(), &all_ids, 3);
        // With shuffle, order is non-deterministic — assert set membership and no duplicates.
        assert_eq!(result.len(), 3);
        let result_set: HashSet<u64> = result.iter().cloned().collect();
        assert_eq!(result_set.len(), 3, "no duplicates");
        assert!(
            result_set.iter().all(|id| *id < 5),
            "all IDs in valid range"
        );
    }

    #[test]
    fn test_select_source_candidates_priority_ordering_combined() {
        let entries: Vec<EntryRecord> = (0..6u64).map(|i| make_entry(i, "decision", i)).collect();
        let isolated: HashSet<u64> = vec![3, 4, 5].into_iter().collect();
        let all_ids: HashSet<u64> = (0..6u64).collect();
        let result = select_source_candidates(&entries, &HashSet::new(), &isolated, &all_ids, 5);
        assert_eq!(result.len(), 5);
        let first_three: HashSet<u64> = result[..3].iter().cloned().collect();
        assert_eq!(
            first_three,
            vec![3u64, 4, 5].into_iter().collect::<HashSet<_>>()
        );
    }

    #[test]
    fn test_select_source_candidates_all_isolated() {
        let entries: Vec<EntryRecord> = (0..10u64).map(|i| make_entry(i, "lesson", i)).collect();
        let isolated: HashSet<u64> = (0..10).collect();
        let all_ids: HashSet<u64> = (0..10u64).collect();
        let result = select_source_candidates(&entries, &HashSet::new(), &isolated, &all_ids, 4);
        assert_eq!(result.len(), 4);
    }

    /// RC-2 fix: entries without embeddings are excluded from both tiers.
    #[test]
    fn test_select_source_candidates_excludes_no_embedding_entries() {
        // 6 entries: 0-2 isolated, 3-5 non-isolated; only 1, 2, 4, 5 have embeddings
        let entries: Vec<EntryRecord> = (0..6u64).map(|i| make_entry(i, "decision", i)).collect();
        let isolated: HashSet<u64> = vec![0u64, 1, 2].into_iter().collect();
        let embedded_ids: HashSet<u64> = vec![1u64, 2, 4, 5].into_iter().collect();

        let result =
            select_source_candidates(&entries, &HashSet::new(), &isolated, &embedded_ids, 10);

        // Entry 0 (isolated, no embedding) and entry 3 (non-isolated, no embedding) must be absent.
        assert!(
            !result.contains(&0),
            "no-embedding isolated entry must be excluded"
        );
        assert!(
            !result.contains(&3),
            "no-embedding non-isolated entry must be excluded"
        );
        // Embedded entries must all be present (pool fits within cap).
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&4));
        assert!(result.contains(&5));
        assert_eq!(result.len(), 4);
    }

    /// RC-1 fix: when pool > cap, verify result is valid (correct length, valid IDs, no duplicates).
    /// Shuffle randomises selection; we assert correctness properties rather than non-equality
    /// to avoid a flaky assertion.
    #[test]
    fn test_select_source_candidates_nondeterministic_rotation() {
        let entries: Vec<EntryRecord> = (0..20u64).map(|i| make_entry(i, "decision", i)).collect();
        let all_ids: HashSet<u64> = (0..20u64).collect();

        let result =
            select_source_candidates(&entries, &HashSet::new(), &HashSet::new(), &all_ids, 5);

        assert_eq!(result.len(), 5, "must return exactly cap entries");
        let unique: HashSet<u64> = result.iter().cloned().collect();
        assert_eq!(unique.len(), 5, "no duplicates");
        assert!(unique.iter().all(|id| *id < 20), "all IDs from valid pool");
    }

    // -----------------------------------------------------------------------
    // write_inferred_edges_with_cap
    // -----------------------------------------------------------------------

    /// AC-11 / R-08: cap enforced — only max_edges edges written.
    #[tokio::test]
    async fn test_write_inferred_edges_with_cap_cap_enforced() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        for i in 1u64..=10 {
            insert_test_entry(&store, i).await;
            insert_test_entry(&store, i + 100).await;
        }
        let pairs: Vec<(u64, u64)> = (1u64..=10).map(|i| (i, i + 100)).collect();
        let scores: Vec<NliScores> = (0..10).map(|_| scores_above(0.9)).collect();
        let written = write_inferred_edges_with_cap(&store, &pairs, &scores, 0.7, 3).await;
        assert_eq!(written, 3);
        assert_eq!(store.query_graph_edges().await.unwrap().len(), 3);
    }

    /// AC-09: strict > — at-threshold produces no edge.
    #[tokio::test]
    async fn test_write_inferred_edges_threshold_strict_greater() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        for id in [1u64, 2, 3, 4, 5, 6] {
            insert_test_entry(&store, id).await;
        }
        let pairs = vec![(1u64, 2u64), (3u64, 4u64), (5u64, 6u64)];
        let scores = vec![
            NliScores {
                entailment: 0.71,
                neutral: 0.1,
                contradiction: 0.19,
            },
            NliScores {
                entailment: 0.70,
                neutral: 0.1,
                contradiction: 0.20,
            }, // at-threshold: NOT written
            NliScores {
                entailment: 0.69,
                neutral: 0.1,
                contradiction: 0.21,
            },
        ];
        let written = write_inferred_edges_with_cap(&store, &pairs, &scores, 0.70, 10).await;
        assert_eq!(written, 1, "only 0.71 exceeds strict > 0.70");
    }

    /// AC-10a / R-01: no Contradicts edges even with high contradiction score.
    #[tokio::test]
    async fn test_write_inferred_edges_supports_only_no_contradicts() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        for i in 1u64..=5 {
            insert_test_entry(&store, i).await;
            insert_test_entry(&store, i + 100).await;
        }
        let pairs: Vec<(u64, u64)> = (1u64..=5).map(|i| (i, i + 100)).collect();
        let scores: Vec<NliScores> = (0..5)
            .map(|_| NliScores {
                entailment: 0.9,
                neutral: 0.05,
                contradiction: 0.95,
            })
            .collect();
        let written = write_inferred_edges_with_cap(&store, &pairs, &scores, 0.7, 10).await;
        assert_eq!(written, 5);
        for edge in store.query_graph_edges().await.unwrap() {
            assert_ne!(
                edge.relation_type, "Contradicts",
                "C-13 / AC-10a: no Contradicts"
            );
            assert_eq!(edge.relation_type, "Supports");
        }
    }

    #[tokio::test]
    async fn test_write_inferred_edges_zero_eligible() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        for id in [1u64, 2, 3, 4, 5, 6] {
            insert_test_entry(&store, id).await;
        }
        let pairs = vec![(1u64, 2u64), (3u64, 4u64), (5u64, 6u64)];
        let scores: Vec<NliScores> = (0..3)
            .map(|_| NliScores {
                entailment: 0.5,
                neutral: 0.3,
                contradiction: 0.2,
            })
            .collect();
        let written = write_inferred_edges_with_cap(&store, &pairs, &scores, 0.7, 10).await;
        assert_eq!(written, 0);
        assert!(store.query_graph_edges().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_write_inferred_edges_cap_at_exact_count() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        for i in 1u64..=3 {
            insert_test_entry(&store, i).await;
            insert_test_entry(&store, i + 100).await;
        }
        let pairs: Vec<(u64, u64)> = (1u64..=3).map(|i| (i, i + 100)).collect();
        let scores: Vec<NliScores> = (0..3).map(|_| scores_above(0.9)).collect();
        let written = write_inferred_edges_with_cap(&store, &pairs, &scores, 0.7, 3).await;
        assert_eq!(written, 3);
    }

    /// AC-16: INSERT OR IGNORE idempotency — no duplicate rows on second call.
    #[tokio::test]
    async fn test_write_inferred_edges_insert_or_ignore_idempotency() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry(&store, 10).await;
        insert_test_entry(&store, 20).await;
        let pairs = vec![(10u64, 20u64)];
        let scores = vec![scores_above(0.9)];
        write_inferred_edges_with_cap(&store, &pairs, &scores, 0.7, 10).await;
        let written2 = write_inferred_edges_with_cap(&store, &pairs, &scores, 0.7, 10).await;
        let count = store
            .query_graph_edges()
            .await
            .unwrap()
            .iter()
            .filter(|e| e.source_id == 10 && e.target_id == 20 && e.relation_type == "Supports")
            .count();
        assert_eq!(
            count, 1,
            "INSERT OR IGNORE must not create duplicate rows (written2={written2})"
        );
    }

    /// AC-13: written edges have source = 'nli' and bootstrap_only = false.
    #[tokio::test]
    async fn test_write_inferred_edges_edge_source_nli() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry(&store, 1).await;
        insert_test_entry(&store, 2).await;
        write_inferred_edges_with_cap(&store, &[(1u64, 2u64)], &[scores_above(0.9)], 0.7, 10).await;
        let edges = store.query_graph_edges().await.unwrap();
        assert_eq!(edges.len(), 1);
        let e = &edges[0];
        assert_eq!(e.relation_type, "Supports");
        assert!(
            !e.bootstrap_only,
            "inferred edges must have bootstrap_only = false"
        );
    }

    // -----------------------------------------------------------------------
    // run_graph_inference_tick guard
    // -----------------------------------------------------------------------

    /// AC-05: NLI not ready — tick returns immediately without writing any edges.
    #[tokio::test]
    async fn test_run_graph_inference_tick_nli_not_ready_no_op() {
        let tmp = tempfile::TempDir::new().unwrap();
        let arc_store = Arc::new(unimatrix_store::test_helpers::open_test_store(&tmp).await);
        insert_test_entry(&arc_store, 1).await;
        insert_test_entry(&arc_store, 2).await;

        // NliServiceHandle::new() starts in Loading state — get_provider() returns Err.
        let not_ready_handle = NliServiceHandle::new();
        let vector_index = Arc::new(
            unimatrix_vector::VectorIndex::new(
                Arc::clone(&arc_store),
                unimatrix_core::VectorConfig::default(),
            )
            .expect("VectorIndex"),
        );
        let config = InferenceConfig {
            nli_enabled: true,
            ..InferenceConfig::default()
        };

        run_graph_inference_tick(
            &arc_store,
            &not_ready_handle,
            &vector_index,
            &make_rayon_pool(),
            &config,
        )
        .await;

        assert!(
            arc_store.query_graph_edges().await.unwrap().is_empty(),
            "no edges must be written when NLI is not ready"
        );
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_tick_empty_entry_set_select_candidates() {
        assert!(
            select_source_candidates(&[], &HashSet::new(), &HashSet::new(), &HashSet::new(), 10)
                .is_empty()
        );
    }

    #[test]
    fn test_tick_single_active_entry() {
        let entries = vec![make_entry(42, "decision", 100)];
        let embedded_ids: HashSet<u64> = vec![42u64].into_iter().collect();
        assert_eq!(
            select_source_candidates(
                &entries,
                &HashSet::new(),
                &HashSet::new(),
                &embedded_ids,
                10
            ),
            vec![42]
        );
    }

    /// AC-16: idempotency — second run does not increase row count.
    #[tokio::test]
    async fn test_tick_idempotency() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        for id in [1u64, 2, 3, 4] {
            insert_test_entry(&store, id).await;
        }
        let pairs = vec![(1u64, 2u64), (3u64, 4u64)];
        let scores: Vec<NliScores> = (0..2).map(|_| scores_above(0.9)).collect();
        write_inferred_edges_with_cap(&store, &pairs, &scores, 0.7, 10).await;
        let first = store.query_graph_edges().await.unwrap().len();
        write_inferred_edges_with_cap(&store, &pairs, &scores, 0.7, 10).await;
        let second = store.query_graph_edges().await.unwrap().len();
        assert_eq!(first, second, "row count must not increase on second run");
    }

    /// Pair normalisation: (A,B) and (B,A) must collapse to the same key.
    #[test]
    fn test_tick_pair_dedup_normalization() {
        let (a, b) = (10u64, 20u64);
        assert_eq!((a.min(b), a.max(b)), (b.min(a), b.max(a)));
    }

    // -----------------------------------------------------------------------
    // nli_score_stats
    // -----------------------------------------------------------------------

    /// Empty slice returns (0.0, 0.0, 0.0) without panic.
    #[test]
    fn test_nli_score_stats_empty_returns_zero() {
        let (max, mean, p75) = nli_score_stats(&[]);
        assert_eq!(max, 0.0);
        assert_eq!(mean, 0.0);
        assert_eq!(p75, 0.0);
    }

    /// Single element: all three fields equal that element.
    #[test]
    fn test_nli_score_stats_single_element() {
        let scores = vec![NliScores {
            entailment: 0.42,
            neutral: 0.3,
            contradiction: 0.28,
        }];
        let (max, mean, p75) = nli_score_stats(&scores);
        assert!((max - 0.42).abs() < 1e-6, "max={max}");
        assert!((mean - 0.42).abs() < 1e-6, "mean={mean}");
        assert!((p75 - 0.42).abs() < 1e-6, "p75={p75}");
    }

    /// n=4: exercises p75 index math — p75_idx = ceil(0.75*4)-1 = ceil(3.0)-1 = 2.
    /// Sorted vals: [0.1, 0.4, 0.7, 0.9] → p75 = vals[2] = 0.7.
    #[test]
    fn test_nli_score_stats_four_elements() {
        let vals = [0.4f32, 0.9, 0.1, 0.7];
        let scores: Vec<NliScores> = vals
            .iter()
            .map(|&e| NliScores {
                entailment: e,
                neutral: 0.05,
                contradiction: 0.05,
            })
            .collect();
        let (max, mean, p75) = nli_score_stats(&scores);
        assert!((max - 0.9).abs() < 1e-6, "max={max}");
        let expected_mean = (0.1 + 0.4 + 0.7 + 0.9) / 4.0;
        assert!((mean - expected_mean).abs() < 1e-5, "mean={mean}");
        // p75_idx = (ceil(0.75 * 4.0) as usize).saturating_sub(1).min(3)
        //         = (3 as usize).saturating_sub(1).min(3) = 2
        assert!((p75 - 0.7).abs() < 1e-6, "p75={p75}");
    }
}
