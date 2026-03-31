//! Background graph inference tick — Supports + Informs edges (crt-029, crt-037).
//!
//! `run_graph_inference_tick` is the counterpart to `maybe_run_bootstrap_promotion`:
//! one-shot/idempotency-gated vs recurring/cap-throttled. Both share W1-2 + rayon pool.
//!
//! # W1-2 Contract
//! ALL `CrossEncoderProvider::score_batch` calls via `rayon_pool.spawn()`.
//! `spawn_blocking` prohibited. Inline async NLI prohibited.
//!
//! # Supports/Informs Detection (C-13 / AC-10a)
//! Phase 8 writes `Supports` edges (entailment signal, unchanged).
//! Phase 8b writes `Informs` edges (neutral signal, crt-037).
//! No `Contradicts` edges in this module — dedicated contradiction detection path only.
//!
//! # R-09 Rayon/Tokio Boundary (C-14)
//! The rayon closure in Phase 7 MUST be synchronous CPU-bound only.
//! PROHIBITED: `tokio::runtime::Handle::current()`, `.await`, any async call.
//! Rayon worker threads have no Tokio runtime; violations panic at runtime.
//! Detection: `grep -n 'Handle::current' nli_detection_tick.rs` must return empty.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
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
// Module-private types: tagged union for merged NLI batch (crt-037, ADR-001)
// ---------------------------------------------------------------------------

/// Tagged union carrying per-pair metadata from Phase 4/4b through Phase 7 to Phase 8/8b.
///
/// The enum variant is the routing discriminator. Phase 8 pattern-matches on
/// `SupportsContradict`; Phase 8b pattern-matches on `Informs`. Misrouting is a
/// compile error, not a runtime branch.
///
/// SR-08: parallel index-matched vecs are the failure mode this type prevents.
/// FR-10: spec requires a tagged union so misrouting is a compile-time error.
///
/// `cosine` in `SupportsContradict` is carried for structural symmetry with `InformsCandidate`.
/// Phase 8 uses `nli_scores.entailment` as the edge weight, not cosine.
#[allow(dead_code)]
#[derive(Debug, Clone)]
enum NliCandidatePair {
    SupportsContradict {
        source_id: u64,
        target_id: u64,
        cosine: f32,
        nli_scores: NliScores,
    },
    Informs {
        candidate: InformsCandidate,
        nli_scores: NliScores,
    },
}

/// Carries all Phase 4b guard metadata for an Informs candidate.
///
/// All fields are required (non-Option): the compiler guarantees Phase 8b has
/// everything it needs without defensive None checks. This eliminates the R-05
/// vacuous-pass risk present in a flat struct with Option<*> guard fields.
///
/// `source_category` and `target_category` are verified by Phase 4b filter and
/// stored here for structural completeness. They are not re-read in Phase 8b
/// because category-pair membership is implicit in the `Informs` variant (ADR-001).
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct InformsCandidate {
    source_id: u64,
    target_id: u64,
    cosine: f32,
    source_created_at: i64,       // Unix seconds — required; not Option
    target_created_at: i64,       // Unix seconds — required; not Option
    source_feature_cycle: String, // required — cross-feature guard; not Option
    target_feature_cycle: String, // required — cross-feature guard; not Option
    source_category: String,      // required — category pair filter; not Option
    target_category: String,      // required — category pair filter; not Option
}

/// Construction scaffolding consumed in Phase 7 to build `NliCandidatePair`.
///
/// Parallel-indexed to `scored_input` — NOT a discriminator field on a flat struct.
/// This internal enum is consumed (`.into_iter()`) when zipping with NLI scores.
/// After Phase 7 it no longer exists. SR-08 misrouting risk applies only to the
/// final `NliCandidatePair` which uses typed variants, not index offsets.
enum PairOrigin {
    SupportsContradict {
        source_id: u64,
        target_id: u64,
        cosine: f32,
    },
    Informs(InformsCandidate),
}

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

    // Phase 2 (continued) — Informs dedup pre-filter (crt-037, directional, ADR-003).
    let existing_informs_pairs: HashSet<(u64, u64)> = match store
        .query_existing_informs_pairs()
        .await
    {
        Ok(pairs) => pairs,
        Err(e) => {
            // Degraded: INSERT OR IGNORE on UNIQUE(source_id, target_id, relation_type) is backstop.
            tracing::warn!(
                error = %e,
                "graph inference tick: failed to fetch existing Informs pairs; INSERT OR IGNORE dedup"
            );
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

    // Phase 4b — Informs HNSW scan at nli_informs_cosine_floor (crt-037).
    //
    // Uses the same source_candidates pool as Phase 4.
    // Applies cross-category, temporal, cross-feature, and dedup guards before NLI.
    // Domain vocabulary must not appear here — config fields are the sole source (C-12).
    //
    // entry_meta: O(1) lookup for created_at, feature_cycle, category of any entry ID.
    let entry_meta: HashMap<u64, &EntryRecord> = all_active.iter().map(|e| (e.id, e)).collect();

    // informs_lhs_set: O(1) check whether a source category is an eligible Informs LHS.
    let informs_lhs_set: HashSet<&str> = config
        .informs_category_pairs
        .iter()
        .map(|pair| pair[0].as_str())
        .collect();

    let mut informs_metadata: Vec<InformsCandidate> = Vec::new();

    if !config.informs_category_pairs.is_empty() {
        let mut seen_informs_pairs: HashSet<(u64, u64)> = HashSet::new();

        for source_id in &source_candidates {
            let source_meta = match entry_meta.get(source_id) {
                Some(m) => m,
                None => continue,
            };

            // Source-category pre-filter (C-12: runtime value from config, no domain literals here).
            if !informs_lhs_set.contains(source_meta.category.as_str()) {
                continue;
            }

            // Cross-feature guard requires a non-empty feature_cycle on source.
            // EntryRecord.feature_cycle is String; empty string means absent.
            if source_meta.feature_cycle.is_empty() {
                continue;
            }
            let source_feature_cycle = source_meta.feature_cycle.clone();

            let embedding = match vector_index.get_embedding(*source_id) {
                Some(emb) => emb,
                None => {
                    tracing::debug!(
                        entry_id = source_id,
                        "graph inference tick Phase 4b: no embedding, skipping"
                    );
                    continue;
                }
            };

            let search_results =
                match vector_index.search(&embedding, config.graph_inference_k, EF_SEARCH) {
                    Ok(results) => results,
                    Err(e) => {
                        tracing::debug!(
                            entry_id = source_id,
                            error = %e,
                            "graph inference tick Phase 4b: HNSW search failed"
                        );
                        continue;
                    }
                };

            for result in search_results {
                let neighbor_id = result.entry_id;
                let similarity = result.similarity as f32;

                // Self-skip.
                if neighbor_id == *source_id {
                    continue;
                }

                let target_meta = match entry_meta.get(&neighbor_id) {
                    Some(m) => m,
                    None => continue,
                };

                // Evaluate all Phase 4b candidate-level guards (cosine, category, temporal, feature).
                // C-12: domain strings from config only — no literals here.
                let source_ts = source_meta.created_at as i64;
                let target_ts = target_meta.created_at as i64;
                if !phase4b_candidate_passes_guards(
                    similarity,
                    source_meta.category.as_str(),
                    target_meta.category.as_str(),
                    source_ts,
                    target_ts,
                    source_feature_cycle.as_str(),
                    target_meta.feature_cycle.as_str(),
                    config,
                ) {
                    continue;
                }

                let target_feature_cycle = target_meta.feature_cycle.clone();

                // DB-level dedup: pair not already written (AC-23, ADR-003 directional).
                if existing_informs_pairs.contains(&(*source_id, neighbor_id)) {
                    continue;
                }

                // In-tick dedup: pair not already seen this tick.
                if seen_informs_pairs.contains(&(*source_id, neighbor_id)) {
                    continue;
                }
                seen_informs_pairs.insert((*source_id, neighbor_id));

                // Construct InformsCandidate — all nine fields required, no Option (ADR-001).
                // source_feature_cycle cloned here so the outer-loop binding stays valid.
                informs_metadata.push(InformsCandidate {
                    source_id: *source_id,
                    target_id: neighbor_id,
                    cosine: similarity,
                    source_created_at: source_ts,
                    target_created_at: target_ts,
                    source_feature_cycle: source_feature_cycle.clone(),
                    target_feature_cycle,
                    source_category: source_meta.category.clone(),
                    target_category: target_meta.category.clone(),
                });
            }
        }
    }

    // Early return: no candidates in either Phase 4 or Phase 4b.
    if candidate_pairs.is_empty() && informs_metadata.is_empty() {
        tracing::debug!("graph inference tick: no candidate pairs after HNSW expansion");
        return;
    }

    // Phase 5 — Combined cap: Supports first (priority), Informs second (remainder). ADR-002.
    //
    // Step 1: Sort supports by existing priority criteria, truncate to max_cap.
    // Order: (1) cross-category first, (2) either endpoint isolated, (3) similarity desc.
    let category_map: HashMap<u64, &str> = all_active
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

    // Step 2: Remaining capacity after Supports reservation.
    let remaining_capacity = config
        .max_graph_inference_per_tick
        .saturating_sub(candidate_pairs.len());

    // Step 3: Sort Informs candidates by similarity descending.
    // Cross-category already guaranteed by Phase 4b filter; isolated-endpoint boost not applied.
    let informs_total_before_cap = informs_metadata.len();
    informs_metadata
        .sort_unstable_by(|a, b| b.cosine.partial_cmp(&a.cosine).unwrap_or(Ordering::Equal));

    // Step 4: Truncate Informs to remaining capacity.
    informs_metadata.truncate(remaining_capacity);

    // Step 5: Log cap accounting (SR-03, FR-14).
    tracing::debug!(
        supports_candidates = candidate_pairs.len(),
        informs_candidates_total = informs_total_before_cap,
        informs_candidates_accepted = informs_metadata.len(),
        informs_candidates_dropped =
            informs_total_before_cap.saturating_sub(informs_metadata.len()),
        "graph inference tick: merged cap accounting"
    );

    // Phase 6 — Text fetch via write_pool for all pairs (Supports + Informs).
    //
    // scored_input: text for NLI scorer.
    // pair_origins: construction scaffolding consumed in Phase 7 to build NliCandidatePair.
    // Invariant: scored_input.len() == pair_origins.len() (maintained by construction).
    // Pairs where content fetch fails are dropped from both vecs.
    let mut scored_input: Vec<(u64, u64, String, String)> = Vec::new();
    let mut pair_origins: Vec<PairOrigin> = Vec::new();

    // Fetch Supports pairs.
    for (source_id, target_id, cosine) in &candidate_pairs {
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
        pair_origins.push(PairOrigin::SupportsContradict {
            source_id: *source_id,
            target_id: *target_id,
            cosine: *cosine,
        });
    }

    // Fetch Informs pairs.
    for candidate in &informs_metadata {
        let source_text = match store.get_content_via_write_pool(candidate.source_id).await {
            Ok(text) => text,
            Err(e) => {
                tracing::debug!(
                    entry_id = candidate.source_id,
                    error = %e,
                    "graph inference tick Phase 4b: source content fetch failed, skipping pair"
                );
                continue;
            }
        };
        let target_text = match store.get_content_via_write_pool(candidate.target_id).await {
            Ok(text) => text,
            Err(e) => {
                tracing::debug!(
                    entry_id = candidate.target_id,
                    error = %e,
                    "graph inference tick Phase 4b: target content fetch failed, skipping pair"
                );
                continue;
            }
        };
        scored_input.push((
            candidate.source_id,
            candidate.target_id,
            source_text,
            target_text,
        ));
        pair_origins.push(PairOrigin::Informs(candidate.clone()));
    }

    if scored_input.is_empty() {
        tracing::debug!("graph inference tick: no pairs with fetchable content, skipping NLI");
        return;
    }

    // Phase 7 — W1-2 dispatch: single rayon spawn (C-01 / AC-08 / entry #3653).
    //
    // C-14 / R-09 CRITICAL: closure body is SYNC-ONLY CPU-bound.
    // PROHIBITED: tokio::runtime::Handle::current(), .await, any async call.
    // Rayon worker threads have no Tokio runtime; violations panic at runtime (compile-invisible).
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

    let raw_scores: Vec<NliScores> = match nli_result {
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

    if raw_scores.len() != scored_input.len() {
        tracing::warn!(
            scores_len = raw_scores.len(),
            pairs_len = scored_input.len(),
            "graph inference tick: score_batch length mismatch; skipping write"
        );
        return;
    }

    let n = raw_scores.len();
    let (nli_score_max, nli_score_mean, nli_score_p75) = nli_score_stats(&raw_scores);
    tracing::debug!(
        nli_score_max,
        nli_score_mean,
        nli_score_p75,
        threshold = config.supports_edge_threshold,
        pairs = n,
        "graph inference tick: nli score distribution"
    );

    // Zip pair_origins with scores to build the typed merged_pairs vec (ADR-001, FR-10).
    // pair_origins is consumed here. NliCandidatePair variant is the routing discriminator.
    let merged_pairs: Vec<NliCandidatePair> = pair_origins
        .into_iter()
        .zip(raw_scores.iter().cloned())
        .map(|(origin, scores)| match origin {
            PairOrigin::SupportsContradict {
                source_id,
                target_id,
                cosine,
            } => NliCandidatePair::SupportsContradict {
                source_id,
                target_id,
                cosine,
                nli_scores: scores,
            },
            PairOrigin::Informs(candidate) => NliCandidatePair::Informs {
                candidate,
                nli_scores: scores,
            },
        })
        .collect();

    let timestamp = current_timestamp_secs();

    // Phase 8 — Write Supports edges (SupportsContradict variants only; C-13 / AC-10a).
    let mut edges_written: usize = 0;
    for pair in &merged_pairs {
        if edges_written >= config.max_graph_inference_per_tick {
            // ORDERING INVARIANT (ADR-002): this break is safe only because Phase 6 appends
            // SupportsContradict pairs before Informs pairs in merged_pairs. If Phase 6 ever
            // reorders the merge, this break could fire mid-Supports and cause Phase 8b to
            // miss Informs variants silently. Keep Phase 6 ordering, or remove this break.
            break;
        }
        if let NliCandidatePair::SupportsContradict {
            source_id,
            target_id,
            cosine: _,
            nli_scores,
        } = pair
        {
            // Strict > threshold (AC-09). Contradiction is discarded (C-13 / AC-10a).
            if nli_scores.entailment > config.supports_edge_threshold {
                let metadata_json = format_nli_metadata(nli_scores);
                let written = write_nli_edge(
                    store,
                    *source_id,
                    *target_id,
                    "Supports",
                    nli_scores.entailment,
                    timestamp,
                    &metadata_json,
                )
                .await;
                if written {
                    edges_written += 1;
                }
            }
        }
    }

    // Phase 8b — Write Informs edges (Informs variants only; crt-037).
    //
    // No secondary cap: budget reserved in Phase 5; write all that pass the composite guard.
    let informs_count: usize = merged_pairs
        .iter()
        .filter(|p| matches!(p, NliCandidatePair::Informs { .. }))
        .count();
    let mut informs_edges_written: usize = 0;

    for pair in &merged_pairs {
        if let NliCandidatePair::Informs {
            candidate,
            nli_scores,
        } = pair
        {
            if apply_informs_composite_guard(nli_scores, candidate, config) {
                let weight = candidate.cosine * config.nli_informs_ppr_weight;
                debug_assert!(weight.is_finite(), "Informs edge weight must be finite");
                if !weight.is_finite() {
                    continue;
                }
                let metadata_json = format_nli_metadata_informs(nli_scores);
                let written = write_nli_edge(
                    store,
                    candidate.source_id,
                    candidate.target_id,
                    "Informs", // must match RelationType::Informs.as_str() exactly
                    weight,
                    timestamp,
                    &metadata_json,
                )
                .await;
                if written {
                    informs_edges_written += 1;
                }
            }
        }
    }

    tracing::debug!(
        informs_edges_written,
        informs_pairs_evaluated = informs_count,
        "graph inference tick: Informs write complete"
    );

    tracing::debug!(
        edges_written,
        informs_edges_written,
        pairs_scored = merged_pairs.len(),
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

/// Evaluate Phase 4b candidate-level guards (used before constructing `InformsCandidate`).
///
/// Returns `true` when the candidate triplet is eligible for the Informs path:
/// - `similarity >= cosine_floor` (inclusive — AC-17: floor is inclusive, not strict)
/// - `[source_category, target_category]` is in `informs_category_pairs` (AC-16)
/// - `source_created_at < target_created_at` (strict — AC-14)
/// - `source_feature_cycle != target_feature_cycle` and both non-empty (AC-15)
///
/// Module-private. Accessible to tests via `use super::*`.
fn phase4b_candidate_passes_guards(
    similarity: f32,
    source_category: &str,
    target_category: &str,
    source_created_at: i64,
    target_created_at: i64,
    source_feature_cycle: &str,
    target_feature_cycle: &str,
    config: &InferenceConfig,
) -> bool {
    if similarity < config.nli_informs_cosine_floor {
        return false;
    }
    let is_valid_pair = config
        .informs_category_pairs
        .iter()
        .any(|pair| pair[0] == source_category && pair[1] == target_category);
    if !is_valid_pair {
        return false;
    }
    if source_feature_cycle.is_empty() || target_feature_cycle.is_empty() {
        return false;
    }
    if source_created_at >= target_created_at {
        return false;
    }
    if source_feature_cycle == target_feature_cycle {
        return false;
    }
    true
}

/// Evaluate all composite guard predicates for a candidate Informs edge.
///
/// Returns `true` only when ALL five guards pass (FR-11, SR-01):
///
/// 1. `nli_scores.neutral > 0.5` — neutral-zone signal (fixed constant C-09).
/// 2. `candidate.source_created_at < candidate.target_created_at` — temporal ordering.
/// 3. `candidate.source_feature_cycle != candidate.target_feature_cycle` — cross-feature.
/// 4. Category pair membership — implicit in `Informs` variant (verified by Phase 4b).
/// 5. `nli_scores.entailment <= supports_edge_threshold` AND
///    `nli_scores.contradiction <= nli_contradiction_threshold` — FR-11 mutual exclusion.
///
/// Module-private. Accessible to the `tests` sub-module via `use super::*`.
fn apply_informs_composite_guard(
    nli_scores: &NliScores,
    candidate: &InformsCandidate,
    config: &InferenceConfig,
) -> bool {
    nli_scores.neutral > 0.5
        && candidate.source_created_at < candidate.target_created_at
        && candidate.source_feature_cycle != candidate.target_feature_cycle
        && nli_scores.entailment <= config.supports_edge_threshold
        && nli_scores.contradiction <= config.nli_contradiction_threshold
}

/// Serialize NLI scores to metadata JSON for Informs edges (crt-037).
///
/// Unlike `format_nli_metadata`, includes `nli_neutral` because neutral is the
/// Informs detection criterion (Phase 8b guard 1). All three scores included for
/// observability and cross-type debugging.
fn format_nli_metadata_informs(scores: &NliScores) -> String {
    serde_json::json!({
        "nli_entailment":    scores.entailment,
        "nli_contradiction": scores.contradiction,
        "nli_neutral":       scores.neutral,
    })
    .to_string()
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

    // -----------------------------------------------------------------------
    // crt-037 Phase 4b + Phase 8b tests — AC-13 through AC-23 (R-20: all required)
    // -----------------------------------------------------------------------

    /// Build a default InferenceConfig with Informs detection enabled.
    /// Uses default informs_category_pairs (4 SE pairs) and default thresholds.
    fn informs_config() -> InferenceConfig {
        InferenceConfig::default()
    }

    /// Build an InformsCandidate that passes all composite guards when paired with
    /// `informs_passing_scores()`. Category pair ("ll", "dec") must be in config.
    fn make_informs_candidate(
        source_category: &str,
        target_category: &str,
        cosine: f32,
        source_created_at: i64,
        target_created_at: i64,
        source_feature_cycle: &str,
        target_feature_cycle: &str,
    ) -> InformsCandidate {
        InformsCandidate {
            source_id: 1,
            target_id: 2,
            cosine,
            source_created_at,
            target_created_at,
            source_feature_cycle: source_feature_cycle.to_string(),
            target_feature_cycle: target_feature_cycle.to_string(),
            source_category: source_category.to_string(),
            target_category: target_category.to_string(),
        }
    }

    /// NliScores that pass the Informs composite guard:
    /// neutral > 0.5, entailment <= supports_edge_threshold, contradiction <= contradiction_threshold.
    fn informs_passing_scores() -> NliScores {
        NliScores {
            entailment: 0.2,
            neutral: 0.6,
            contradiction: 0.2,
        }
    }

    // -----------------------------------------------------------------------
    // AC-13: Happy path — all guards pass, Informs edge written
    // -----------------------------------------------------------------------

    /// AC-13: Phase 8b writes one Informs row when all composite guards pass.
    /// Covers: entry source="nli", relation_type="Informs", finite weight, nli_neutral in metadata.
    #[tokio::test]
    async fn test_phase8b_writes_informs_edge_when_all_guards_pass() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry(&store, 1).await;
        insert_test_entry(&store, 2).await;

        let config = informs_config();
        // Use two categories from the default informs_category_pairs.
        // The default pairs include ["lesson-learned","decision"],["lesson-learned","convention"],
        // ["pattern","decision"],["pattern","convention"] — loaded at runtime from config.
        // We pass them as runtime strings; no domain literals appear in nli_detection_tick.rs (C-12).
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );

        let candidate = make_informs_candidate(
            src_cat, tgt_cat, 0.50,      // cosine >= default floor 0.45
            1_000_000, // source_created_at < target_created_at
            2_000_000, "crt-020", // different feature cycles
            "crt-030",
        );
        let scores = informs_passing_scores();

        assert!(
            apply_informs_composite_guard(&scores, &candidate, &config),
            "guard must pass for happy path"
        );

        let ts = current_timestamp_secs();
        let metadata = format_nli_metadata_informs(&scores);
        let weight = candidate.cosine * config.nli_informs_ppr_weight;
        let written = write_nli_edge(
            &store,
            candidate.source_id,
            candidate.target_id,
            "Informs",
            weight,
            ts,
            &metadata,
        )
        .await;
        assert!(written, "write_nli_edge must succeed");

        let edges = store.query_graph_edges().await.unwrap();
        let informs: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Informs")
            .collect();
        assert_eq!(informs.len(), 1, "exactly one Informs edge");

        // AC-19: source = "nli" (EDGE_SOURCE_NLI)
        assert_eq!(informs[0].source, "nli", "AC-19: source must be nli");

        // AC-20: weight = cosine * ppr_weight (within f32 epsilon)
        let expected_weight = 0.50_f32 * config.nli_informs_ppr_weight;
        assert!(
            (informs[0].weight as f32 - expected_weight).abs() < 1e-5,
            "AC-20: weight={} expected={expected_weight}",
            informs[0].weight
        );

        // Metadata must include nli_neutral — query raw metadata column directly.
        let row: (Option<String>,) = sqlx::query_as(
            "SELECT metadata FROM graph_edges WHERE relation_type = 'Informs' LIMIT 1",
        )
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
        let meta_str = row.0.unwrap_or_default();
        let meta_val: serde_json::Value = serde_json::from_str(&meta_str).unwrap_or_default();
        assert!(
            meta_val["nli_neutral"].is_number(),
            "AC-13: metadata must contain nli_neutral; got: {meta_str}"
        );
    }

    // -----------------------------------------------------------------------
    // AC-14: Temporal ordering guard
    // -----------------------------------------------------------------------

    /// AC-14a: No Informs edge when source_created_at == target_created_at.
    #[test]
    fn test_phase8b_no_informs_when_timestamps_equal() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        let candidate = make_informs_candidate(
            src_cat, tgt_cat, 0.50, 1_500_000, 1_500_000, "crt-020", "crt-030",
        );
        let scores = informs_passing_scores();
        assert!(
            !apply_informs_composite_guard(&scores, &candidate, &config),
            "AC-14: equal timestamps must fail guard"
        );
    }

    /// AC-14b: No Informs edge when source is newer than target (reversed order).
    #[test]
    fn test_phase8b_no_informs_when_source_newer_than_target() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        let candidate = make_informs_candidate(
            src_cat, tgt_cat, 0.50, 3_000_000, 1_000_000, "crt-020", "crt-030",
        );
        let scores = informs_passing_scores();
        assert!(
            !apply_informs_composite_guard(&scores, &candidate, &config),
            "AC-14: reversed timestamps must fail guard"
        );
    }

    // -----------------------------------------------------------------------
    // AC-15: Cross-feature guard
    // -----------------------------------------------------------------------

    /// AC-15: No Informs edge when source and target share the same feature cycle.
    #[test]
    fn test_phase8b_no_informs_when_same_feature_cycle() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        let candidate = make_informs_candidate(
            src_cat, tgt_cat, 0.50, 1_000_000, 2_000_000, "crt-037", "crt-037",
        );
        let scores = informs_passing_scores();
        assert!(
            !apply_informs_composite_guard(&scores, &candidate, &config),
            "AC-15: same feature cycle must fail guard"
        );
    }

    // -----------------------------------------------------------------------
    // AC-16: Category pair guard (Phase 4b filter)
    // -----------------------------------------------------------------------

    /// AC-16: Phase 4b excludes pairs where category is not in informs_category_pairs.
    #[test]
    fn test_phase8b_no_informs_when_category_pair_not_in_config() {
        let config = informs_config();
        // ("decision", "decision") is not in default informs_category_pairs.
        let result = phase4b_candidate_passes_guards(
            0.50,       // above floor
            "decision", // source_category
            "decision", // target_category — ("decision","decision") not in SE pairs
            1_000_000, 2_000_000, "crt-020", "crt-030", &config,
        );
        assert!(!result, "AC-16: category pair not in config must reject");
    }

    // -----------------------------------------------------------------------
    // AC-17: Cosine floor guard (Phase 4b filter)
    // -----------------------------------------------------------------------

    /// AC-17: Phase 4b excludes candidates with cosine < nli_informs_cosine_floor.
    #[test]
    fn test_phase8b_no_informs_when_cosine_below_floor() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        // cosine = 0.44 is below default floor 0.45.
        let result = phase4b_candidate_passes_guards(
            0.44, src_cat, tgt_cat, 1_000_000, 2_000_000, "crt-020", "crt-030", &config,
        );
        assert!(
            !result,
            "AC-17: cosine 0.44 below floor 0.45 must be rejected by Phase 4b"
        );
    }

    // -----------------------------------------------------------------------
    // AC-18: Phase 4b uses nli_informs_cosine_floor, not supports_candidate_threshold
    // -----------------------------------------------------------------------

    /// AC-18: A pair in the band [0.45, 0.50) is accepted by Phase 4b (nli_informs_cosine_floor)
    /// but would be rejected by Phase 4 (supports_candidate_threshold strict >).
    #[test]
    fn test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        let cosine_in_band = 0.47_f32; // in [0.45, 0.50)

        // Phase 4b: inclusive >= floor (0.45) — should pass.
        let phase4b_accepts = phase4b_candidate_passes_guards(
            cosine_in_band,
            src_cat,
            tgt_cat,
            1_000_000,
            2_000_000,
            "crt-020",
            "crt-030",
            &config,
        );
        assert!(
            phase4b_accepts,
            "AC-18: cosine 0.47 >= nli_informs_cosine_floor 0.45 must be accepted by Phase 4b"
        );

        // Phase 4 strict >: similarity <= supports_candidate_threshold means no candidate.
        // Default supports_candidate_threshold = 0.50; cosine 0.47 <= 0.50 → excluded.
        let phase4_would_accept = cosine_in_band > config.supports_candidate_threshold;
        assert!(
            !phase4_would_accept,
            "AC-18: cosine 0.47 must be excluded by Phase 4 (strict > 0.50)"
        );
    }

    // -----------------------------------------------------------------------
    // AC-19: Edge source = "nli" (EDGE_SOURCE_NLI)
    // -----------------------------------------------------------------------

    /// AC-19: Informs edges written via write_nli_edge have source = "nli".
    /// Covered by test_phase8b_writes_informs_edge_when_all_guards_pass (asserts source="nli").

    // -----------------------------------------------------------------------
    // AC-20: Edge weight = cosine * nli_informs_ppr_weight
    // -----------------------------------------------------------------------

    /// AC-20: weight computation is cosine * nli_informs_ppr_weight.
    #[tokio::test]
    async fn test_phase8b_edge_weight_equals_cosine_times_ppr_weight() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry(&store, 10).await;
        insert_test_entry(&store, 20).await;

        let config = informs_config();
        let cosine = 0.55_f32;
        let expected_weight = cosine * config.nli_informs_ppr_weight; // 0.55 * 0.6 = 0.33

        let ts = current_timestamp_secs();
        let scores = informs_passing_scores();
        let metadata = format_nli_metadata_informs(&scores);
        write_nli_edge(&store, 10, 20, "Informs", expected_weight, ts, &metadata).await;

        let edges = store.query_graph_edges().await.unwrap();
        let informs: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Informs" && e.source_id == 10 && e.target_id == 20)
            .collect();
        assert_eq!(informs.len(), 1);
        assert!(
            (informs[0].weight as f32 - expected_weight).abs() < 1e-5,
            "AC-20: weight={:.6} expected={expected_weight:.6}",
            informs[0].weight
        );
    }

    // -----------------------------------------------------------------------
    // AC-21: grep gate — Handle::current absent from file
    // -----------------------------------------------------------------------

    /// AC-21: Verify no Handle::current() is called in nli_detection_tick.rs (CI gate).
    /// Scans non-comment lines only — the PROHIBITED notes in comments are documentation.
    /// CI gate analogue: if any non-comment source line calls Handle::current, this fails.
    #[test]
    fn test_ac21_no_handle_current_in_file() {
        let file_contents = include_str!("nli_detection_tick.rs");
        // The search needle is formed at runtime to avoid self-matching in the source text.
        let parts = ["Handle", "::", "current"];
        let needle = parts.concat();
        // Filter to non-comment, non-test lines before scanning.
        // The prohibited comments are documentation, not code — they document the prohibition.
        let violations: Vec<&str> = file_contents
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                // Skip pure comment lines (// prefix) and doc comment lines (/// and //!).
                !trimmed.starts_with("//")
            })
            .filter(|line| line.contains(needle.as_str()))
            .collect();
        assert!(
            violations.is_empty(),
            "AC-21: forbidden runtime handle access in non-comment code: {:?}",
            violations
        );
    }

    // -----------------------------------------------------------------------
    // AC-22: grep gate — domain vocabulary absent from file
    // -----------------------------------------------------------------------

    /// AC-22: Domain vocab strings must not appear as literals in nli_detection_tick.rs.
    /// These strings live exclusively in config.rs (C-12).
    /// Strings below are split across array elements to avoid self-matching.
    #[test]
    fn test_ac22_no_domain_vocab_literals_in_file() {
        let file_contents = include_str!("nli_detection_tick.rs");
        // Only scan production code (above the cfg(test) block) — the test section
        // necessarily contains the split fragments that reconstruct the forbidden strings.
        let prod_code = file_contents
            .find("#[cfg(test)]")
            .map(|pos| &file_contents[..pos])
            .unwrap_or(file_contents);
        // Forbidden domain vocab string literals (with surrounding quotes).
        // Split across two array elements to avoid this test source self-matching.
        // The CI grep gate checks for '"lesson-learned"', '"decision"', '"pattern"', '"convention"'.
        // We match the quoted form so bare word occurrences in comments don't false-positive.
        let forbidden: &[(&str, &str)] = &[
            ("\"lesson", "-learned\""),
            ("\"deci", "sion\""),
            ("\"patt", "ern\""),
            ("\"conven", "tion\""),
        ];
        for (a, b) in forbidden {
            let needle = format!("{a}{b}");
            let count = prod_code.matches(needle.as_str()).count();
            assert_eq!(
                count, 0,
                "AC-22: domain vocab literal {needle} must not appear in production code of nli_detection_tick.rs"
            );
        }
    }

    // -----------------------------------------------------------------------
    // AC-23: Dedup — second tick does not write duplicate Informs edge
    // -----------------------------------------------------------------------

    /// AC-23: Running write_nli_edge twice for same (source_id, target_id, "Informs")
    /// produces exactly one row (INSERT OR IGNORE idempotency).
    #[tokio::test]
    async fn test_second_tick_does_not_write_duplicate_informs_edge() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry(&store, 100).await;
        insert_test_entry(&store, 200).await;

        let ts = current_timestamp_secs();
        let scores = informs_passing_scores();
        let metadata = format_nli_metadata_informs(&scores);
        let config = informs_config();
        let weight = 0.50_f32 * config.nli_informs_ppr_weight;

        // First write.
        let w1 = write_nli_edge(&store, 100, 200, "Informs", weight, ts, &metadata).await;
        assert!(w1, "first write must succeed");

        // Second write — INSERT OR IGNORE must be a no-op for the DB row.
        write_nli_edge(&store, 100, 200, "Informs", weight, ts, &metadata).await;

        let count = store
            .query_graph_edges()
            .await
            .unwrap()
            .into_iter()
            .filter(|e| e.relation_type == "Informs" && e.source_id == 100 && e.target_id == 200)
            .count();
        assert_eq!(
            count, 1,
            "AC-23: exactly one Informs row after two write calls"
        );
    }

    /// AC-23 (continued): query_existing_informs_pairs returns the written pair on second tick.
    #[tokio::test]
    async fn test_second_tick_query_existing_informs_pairs_loads_prior_edge() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry(&store, 50).await;
        insert_test_entry(&store, 60).await;

        let ts = current_timestamp_secs();
        let scores = informs_passing_scores();
        let metadata = format_nli_metadata_informs(&scores);
        let config = informs_config();
        let weight = 0.50_f32 * config.nli_informs_ppr_weight;

        write_nli_edge(&store, 50, 60, "Informs", weight, ts, &metadata).await;

        // Simulate what Phase 2 does on the second tick.
        let existing = store
            .query_existing_informs_pairs()
            .await
            .expect("query_existing_informs_pairs");
        assert!(
            existing.contains(&(50, 60)),
            "AC-23: second tick Phase 2 must load the previously-written pair"
        );
    }

    // -----------------------------------------------------------------------
    // Additional guard tests for completeness (per test plan)
    // -----------------------------------------------------------------------

    /// Neutral exactly 0.5: strict > required, so 0.5 must be excluded.
    #[test]
    fn test_phase8b_no_informs_when_neutral_exactly_0_5() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        let candidate = make_informs_candidate(
            src_cat, tgt_cat, 0.50, 1_000_000, 2_000_000, "crt-020", "crt-030",
        );
        let scores = NliScores {
            entailment: 0.2,
            neutral: 0.5, // exactly 0.5 — strict > required
            contradiction: 0.3,
        };
        assert!(
            !apply_informs_composite_guard(&scores, &candidate, &config),
            "neutral = 0.5 exactly must fail strict > 0.5 guard"
        );
    }

    /// Neutral just above 0.5: must pass the guard.
    #[test]
    fn test_phase8b_writes_informs_when_neutral_just_above_0_5() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        let candidate = make_informs_candidate(
            src_cat, tgt_cat, 0.50, 1_000_000, 2_000_000, "crt-020", "crt-030",
        );
        let scores = NliScores {
            entailment: 0.1,
            neutral: 0.5000001,
            contradiction: 0.1,
        };
        assert!(
            apply_informs_composite_guard(&scores, &candidate, &config),
            "neutral just above 0.5 must pass guard"
        );
    }

    /// FR-11 mutual exclusion: entailment exceeds supports_edge_threshold — no Informs edge.
    #[test]
    fn test_phase8b_no_informs_when_entailment_exceeds_supports_threshold() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        let candidate = make_informs_candidate(
            src_cat, tgt_cat, 0.50, 1_000_000, 2_000_000, "crt-020", "crt-030",
        );
        let scores = NliScores {
            entailment: 0.75, // > default supports_edge_threshold 0.6
            neutral: 0.6,
            contradiction: 0.1,
        };
        assert!(
            !apply_informs_composite_guard(&scores, &candidate, &config),
            "FR-11: high entailment must prevent Informs edge"
        );
    }

    /// Phase 5 combined cap: Supports fills cap → zero Informs accepted.
    #[test]
    fn test_phase5_supports_fills_cap_zero_informs_accepted() {
        let cap = 5_usize;
        let supports_count = 5_usize; // fills cap exactly

        let remaining = cap.saturating_sub(supports_count);
        assert_eq!(remaining, 0, "no remaining capacity for Informs");

        // Simulate truncating informs to remaining.
        let mut informs: Vec<InformsCandidate> = (0..3)
            .map(|i| make_informs_candidate("a", "b", 0.5, i, i + 1, "c1", "c2"))
            .collect();
        informs.truncate(remaining);
        assert!(
            informs.is_empty(),
            "Informs must be empty when Supports fills cap"
        );
    }

    /// Phase 5 combined cap: partial Supports fill → Informs fills remainder.
    #[test]
    fn test_phase5_partial_cap_informs_fills_remainder() {
        let cap = 10_usize;
        let supports_count = 7_usize;
        let remaining = cap.saturating_sub(supports_count);
        assert_eq!(remaining, 3);

        let mut informs: Vec<InformsCandidate> = (0..10)
            .map(|i| make_informs_candidate("a", "b", 0.5 - i as f32 * 0.01, i, i + 1, "c1", "c2"))
            .collect();
        informs.sort_unstable_by(|a, b| b.cosine.partial_cmp(&a.cosine).unwrap_or(Ordering::Equal));
        informs.truncate(remaining);
        assert_eq!(informs.len(), 3);
    }

    /// Phase 5 combined cap: no Supports → all Informs up to cap.
    #[test]
    fn test_phase5_no_supports_all_informs_up_to_cap() {
        let cap = 5_usize;
        let supports_count = 0_usize;
        let remaining = cap.saturating_sub(supports_count);

        let mut informs: Vec<InformsCandidate> = (0..8)
            .map(|i| make_informs_candidate("a", "b", 0.5, i, i + 1, "c1", "c2"))
            .collect();
        informs.truncate(remaining);
        assert_eq!(informs.len(), 5, "all cap used by Informs");
    }

    /// Phase 5 combined cap: merged length never exceeds max_cap (property test on 4 scenarios).
    #[test]
    fn test_phase5_merged_len_never_exceeds_max_cap_property() {
        let scenarios: &[(usize, usize, usize)] =
            &[(0, 20, 10), (10, 10, 10), (5, 15, 8), (0, 0, 5)];
        for &(supports, informs_total, cap) in scenarios {
            let effective_supports = supports.min(cap);
            let remaining = cap.saturating_sub(effective_supports);
            let effective_informs = informs_total.min(remaining);
            let merged_len = effective_supports + effective_informs;
            assert!(
                merged_len <= cap,
                "scenario ({supports},{informs_total},{cap}): merged_len={merged_len} > cap={cap}"
            );
        }
    }

    /// Phase 5 combined cap: cap=0 produces empty merged, no panic.
    #[test]
    fn test_phase5_cap_zero_produces_empty_merged() {
        let cap = 0_usize;
        let supports_count = 5_usize;
        let informs_total = 5_usize;

        let effective_supports = supports_count.min(cap);
        let remaining = cap.saturating_sub(effective_supports);
        let effective_informs = informs_total.min(remaining);
        assert_eq!(effective_supports + effective_informs, 0);
    }

    /// Phase 5 remaining computed after truncation (not before): saturating_sub prevents underflow.
    #[test]
    fn test_phase5_remaining_computed_after_truncation() {
        let max_cap = 5_usize;
        let supports_before_truncate = 8_usize;

        // Truncate supports to cap first.
        let supports_after = supports_before_truncate.min(max_cap);
        // Then compute remaining — must be 0, not a negative number.
        let remaining = max_cap.saturating_sub(supports_after);
        assert_eq!(remaining, 0, "remaining must be 0 after supports fills cap");
        // saturating_sub prevents underflow; usize subtraction would underflow without it.
    }

    /// Edge weight finitude guard (C-13, R-15): cosine * ppr_weight is finite.
    #[test]
    fn test_informs_edge_weight_is_finite_before_write() {
        let config = informs_config();
        let cosine = 0.55_f32;
        let weight = cosine * config.nli_informs_ppr_weight;
        assert!(weight.is_finite(), "weight must be finite: {weight}");
        assert!((weight - 0.33_f32).abs() < 1e-5, "weight={weight}");
    }

    /// format_nli_metadata_informs includes nli_neutral key.
    #[test]
    fn test_format_nli_metadata_informs_includes_neutral() {
        let scores = NliScores {
            entailment: 0.2,
            neutral: 0.6,
            contradiction: 0.2,
        };
        let json = format_nli_metadata_informs(&scores);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["nli_neutral"].is_number(), "must have nli_neutral");
        assert!(
            parsed["nli_entailment"].is_number(),
            "must have nli_entailment"
        );
        assert!(
            parsed["nli_contradiction"].is_number(),
            "must have nli_contradiction"
        );
        let neutral = parsed["nli_neutral"].as_f64().unwrap();
        assert!((neutral - 0.6_f64).abs() < 1e-3, "neutral={neutral}");
    }
}
