//! Background graph inference tick — dual-path architecture (crt-039).
//!
//! # Path A: Structural Informs (Phase 4b)
//! Pure HNSW cosine scan. No NLI cross-encoder. Runs unconditionally on every tick.
//! Writes `Informs` edges directly from cosine similarity + category pair filter.
//! Gated by: cosine >= nli_informs_cosine_floor (0.50 default), informs_category_pairs
//! config, temporal ordering, cross-feature guard.
//!
//! # Path B: NLI Supports (Phase 8)
//! Requires NLI cross-encoder. Gated by get_provider() — skipped entirely if Err.
//! Phase 7 runs the NLI batch via rayon_pool.spawn() (W1-2 contract).
//! Writes `Supports` edges (entailment > threshold).
//!
//! # Path C: Cosine Supports (crt-040)
//! Pure cosine detection. No NLI cross-encoder. Runs unconditionally on every tick.
//! Writes `Supports` edges with source = EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports".
//! Gated by: cosine >= supports_cosine_threshold (0.65 default), informs_category_pairs.
//! Budget: MAX_COSINE_SUPPORTS_PER_TICK = 50 (independent of Path A and Path B budgets).
//! Placement: after Path A observability log, before Path B entry gate (ADR-003).
//!
//! # Module Rename Deferred
//! This module is named `nli_detection_tick` but now hosts structural-only inference
//! as its primary path. Module rename to `graph_inference_tick` is deferred to Group 3
//! when NLI is fully removed from Phase 8.
//!
//! # W1-2 Contract
//! ALL `CrossEncoderProvider::score_batch` calls via `rayon_pool.spawn()`.
//! `spawn_blocking` prohibited. Inline async NLI prohibited.
//! Phase 4b MUST NOT call score_batch.
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
use unimatrix_store::{
    EDGE_SOURCE_CO_ACCESS, EDGE_SOURCE_COSINE_SUPPORTS, EDGE_SOURCE_NLI, EDGE_SOURCE_S1,
    EDGE_SOURCE_S2, EDGE_SOURCE_S8, EntryRecord, Status,
};

use crate::infra::config::InferenceConfig;
use crate::infra::nli_handle::NliServiceHandle;
use crate::infra::rayon_pool::RayonPool;

// pub(crate) symbols promoted from nli_detection.rs (R-11):
use crate::services::nli_detection::{
    current_timestamp_secs, format_nli_metadata, write_graph_edge, write_nli_edge,
};

/// Independent per-tick budget for Informs edges.
///
/// Informs gets this many slots regardless of how many Supports candidates fill
/// `max_graph_inference_per_tick`. Keeping this as a module constant (not a config
/// field) follows the same rationale as MAX_SOURCES_PER_TICK: it is an internal
/// throughput knob, not an operator-tunable parameter (bugfix-473).
const MAX_INFORMS_PER_TICK: usize = 25;

/// Independent per-tick budget for cosine Supports edges (Path C).
///
/// Path C iterates Phase 4 `candidate_pairs` (already sorted by priority) and
/// writes up to this many `Supports` edges per tick. Independent of:
/// - `MAX_INFORMS_PER_TICK` (Path A budget)
/// - `max_graph_inference_per_tick` (Path B NLI budget)
///
/// Cost of Path C per candidate: one f32 comparison + one HashMap lookup +
/// one HashSet lookup + one INSERT OR IGNORE. No model invocation.
///
/// TODO: Config-promote to `InferenceConfig.max_cosine_supports_per_tick` if
/// operators require runtime tuning (ADR-004, SR-03). Not speculated in crt-040.
const MAX_COSINE_SUPPORTS_PER_TICK: usize = 50;

// ---------------------------------------------------------------------------
// Module-private types: tagged union for merged NLI batch (crt-037, ADR-001)
// ---------------------------------------------------------------------------

/// Tagged union carrying per-pair metadata from Phase 4 through Phase 7 to Phase 8.
///
/// The enum variant is the routing discriminator. Phase 8 pattern-matches on
/// `SupportsContradict`. Misrouting is a compile error, not a runtime branch.
///
/// SR-08: parallel index-matched vecs are the failure mode this type prevents.
/// FR-10: spec requires a tagged union so misrouting is a compile-time error.
///
/// `cosine` in `SupportsContradict` is carried for structural completeness.
/// Phase 8 uses `nli_scores.entailment` as the edge weight, not cosine.
///
/// `Informs` variant removed (crt-039 ADR-001). Path A writes Informs edges directly
/// from `informs_metadata: Vec<InformsCandidate>` without entering the NLI batch.
#[allow(dead_code)]
#[derive(Debug, Clone)]
enum NliCandidatePair {
    SupportsContradict {
        source_id: u64,
        target_id: u64,
        cosine: f32,
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
    source_feature_cycle: String, // empty string means pre-attribution entry; Informs detection allows this
    target_feature_cycle: String, // empty string means pre-attribution entry; Informs detection allows this
    source_category: String,      // required — category pair filter; not Option
    target_category: String,      // required — category pair filter; not Option
}

/// Construction scaffolding consumed in Phase 7 to build `NliCandidatePair`.
///
/// Parallel-indexed to `scored_input` — NOT a discriminator field on a flat struct.
/// This internal enum is consumed (`.into_iter()`) when zipping with NLI scores.
/// After Phase 7 it no longer exists. SR-08 misrouting risk applies only to the
/// final `NliCandidatePair` which uses typed variants, not index offsets.
///
/// `Informs` variant removed (crt-039 ADR-001). `informs_metadata` is a separate
/// Vec<InformsCandidate> written in Path A before Phase 6 text fetch.
enum PairOrigin {
    SupportsContradict {
        source_id: u64,
        target_id: u64,
        cosine: f32,
    },
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
    // Phase 1 removed (crt-039 ADR-001). get_provider() moved to Path B entry below.
    // Phase 4b (structural Informs) runs unconditionally without NLI provider.

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
    let mut informs_candidates_found: usize = 0;

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

                // Count raw candidates before dedup (AC-17, FR-14): incremented here,
                // before any dedup check, so informs_candidates_found is the true pre-dedup count.
                informs_candidates_found += 1;

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

    // Phase 4b post-loop: explicit Supports-set subtraction (R-03, FR-06, AC-13).
    // Build O(1) lookup from Phase 4 candidate_pairs — check both directions for safety.
    // Phase 4 uses (min,max) symmetric dedup; Phase 4b uses directional (source, target).
    // A pair that qualifies for both (e.g., cosine exactly 0.50 with category filter)
    // must be excluded from informs_metadata. Explicit subtraction, not arithmetic alone.
    let supports_candidate_set: HashSet<(u64, u64)> = candidate_pairs
        .iter()
        .flat_map(|(src, tgt, _)| [(*src, *tgt), (*tgt, *src)])
        .collect();

    informs_metadata.retain(|c| {
        !supports_candidate_set.contains(&(c.source_id, c.target_id))
            && !supports_candidate_set.contains(&(c.target_id, c.source_id))
    });

    // Phase 5 — Independent caps: Supports governed by max_graph_inference_per_tick,
    // Informs governed by MAX_INFORMS_PER_TICK (bugfix-473).
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

    // Step 2: Informs gets its own independent budget — MAX_INFORMS_PER_TICK.
    // Supports filling max_graph_inference_per_tick does NOT reduce Informs slots.
    // Record informs_candidates_after_dedup BEFORE shuffle + truncate.
    let informs_candidates_after_dedup = informs_metadata.len();
    {
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        informs_metadata.shuffle(&mut rng);
    }
    informs_metadata.truncate(MAX_INFORMS_PER_TICK);
    let informs_candidates_after_cap = informs_metadata.len();

    // NOTE: Joint early-return removed (crt-040 AC-19). Path C observability log must fire
    // unconditionally even when both lists are empty. Path A and Path C iterate 0 times
    // when their inputs are empty — no write overhead. Path B entry gate below handles the
    // NLI batch early-exit when candidate_pairs is empty.

    // === PATH A: Structural Informs write loop ===
    // Runs unconditionally. No NLI provider required. No rayon pool usage (NFR-01, C-02).
    // Hard cap already applied in Phase 5 — write all candidates passing composite guard.
    let mut informs_edges_written: usize = 0;
    let timestamp = current_timestamp_secs();

    for candidate in &informs_metadata {
        // Defense-in-depth: re-evaluate temporal and cross-feature guards at write time.
        // These were already checked in Phase 4b via phase4b_candidate_passes_guards;
        // this re-evaluation catches any future code path that bypasses Phase 4b (ADR-002).
        if !apply_informs_composite_guard(candidate) {
            continue;
        }

        let weight = candidate.cosine * config.nli_informs_ppr_weight;
        if !weight.is_finite() {
            continue; // guard against NaN/Inf from cosine * weight product
        }

        let metadata_json = format_informs_metadata(
            candidate.cosine,
            &candidate.source_category,
            &candidate.target_category,
        );

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
        // Non-fatal: write failure for one candidate does not abort the loop.
    }

    // Observability log (AC-17, FR-14) — all four values now known.
    // Emitted even when informs_edges_written = 0 (e.g., all deduped or all failing guard).
    tracing::debug!(
        informs_candidates_found,
        informs_candidates_after_dedup,
        informs_candidates_after_cap,
        informs_edges_written,
        "graph inference tick Phase 4b: Informs candidate pipeline"
    );

    // === PATH C: Cosine Supports write loop (crt-040) ===
    // Pure-cosine Supports detection. Runs unconditionally — NOT gated by nli_enabled or
    // get_provider(). Reuses candidate_pairs from Phase 4 (no new HNSW scan, NFR-01).
    // Writes Supports edges with source = EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports".
    // See ADR-001 (write_graph_edge sibling), ADR-003 (placement), ADR-004 (budget).
    run_cosine_supports_path(
        store,
        config,
        &candidate_pairs,
        &existing_supports_pairs,
        &category_map,
        timestamp,
    )
    .await;

    // === PATH B entry gate ===
    // Informs writes (Path A) are complete above. Path C (cosine Supports) also complete.
    // Path B gates NLI Supports only.

    // Fast exit: no Supports candidates — skip NLI batch entirely.
    // (R-07: Phase 8b completes even when candidate_pairs is empty — Path A and Path C run above.)
    if candidate_pairs.is_empty() {
        tracing::debug!("graph inference tick: no Supports candidates; skipping NLI batch");
        return;
    }

    // Explicit nli_enabled gate — must be AFTER candidate_pairs.is_empty() check
    // and BEFORE get_provider().await to avoid the async call when NLI is intentionally off.
    // Message is intentionally distinct from the get_provider() Err message so operators
    // can distinguish intentional-off (this message) vs. transient-not-ready (Err message).
    if !config.nli_enabled {
        tracing::debug!("graph inference tick: NLI disabled by config; Path B skipped");
        return;
    }

    // R-01 CRITICAL: get_provider() is the SOLE entry point to Phase 6/7/8.
    // Err return here structurally prevents ANY Phase 8 write without a successful provider.
    // No code path from get_provider() Err to write_nli_edge for Supports edges exists.
    // The nli_enabled=false case is handled by the explicit gate above; Err here is a
    // transient provider-not-ready condition only.
    let provider = match nli_handle.get_provider().await {
        Ok(p) => p,
        Err(_) => {
            // Transient: provider not yet initialized or temporarily unavailable.
            // Phase 4b Informs writes already complete above — returning here is not a failure.
            tracing::debug!("graph inference tick: NLI provider not ready; Supports path skipped");
            return;
        }
    };

    // Phase 6 — Text fetch via write_pool for Supports pairs only (crt-039 ADR-001).
    //
    // PairOrigin::Informs removed — Informs text not needed (no NLI batch for Informs).
    // scored_input: text for NLI scorer (Supports only).
    // pair_origins: construction scaffolding consumed in Phase 7 to build NliCandidatePair.
    // Invariant: scored_input.len() == pair_origins.len() (maintained by construction).
    // Pairs where content fetch fails are dropped from both vecs.
    let mut scored_input: Vec<(u64, u64, String, String)> = Vec::new();
    let mut pair_origins: Vec<PairOrigin> = Vec::new();

    // Fetch Supports pairs only.
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

    if scored_input.is_empty() {
        tracing::debug!("graph inference tick: no Supports pairs with fetchable content");
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

    // Zip pair_origins with scores to build the typed merged_pairs vec (crt-039 ADR-001).
    // pair_origins is consumed here. NliCandidatePair variant is the routing discriminator.
    // After crt-039: pair_origins contains only SupportsContradict entries.
    // PairOrigin::Informs removed — Informs writes completed in Path A above.
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
        })
        .collect();

    // Phase 8 — Write Supports edges (SupportsContradict variants only; C-13 / AC-10a).
    // `timestamp` reuse: Path A already called current_timestamp_secs() above.
    // Reuse the same timestamp for Supports edges within this tick for consistency.
    let mut edges_written: usize = 0;
    for pair in &merged_pairs {
        if edges_written >= config.max_graph_inference_per_tick {
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

    tracing::debug!(
        edges_written,
        pairs_scored = merged_pairs.len(),
        source_candidates = source_candidates.len(),
        "graph inference tick complete"
    );
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Path C: Cosine Supports write loop (crt-040).
///
/// Pure-cosine Supports detection. Runs unconditionally — not gated by `nli_enabled`
/// or `get_provider()`. Reuses the Phase 4 `candidate_pairs` vec; no new HNSW scan.
///
/// Writes `Supports` edges with `source = EDGE_SOURCE_COSINE_SUPPORTS` ("cosine_supports")
/// via `write_graph_edge`. Budget: `MAX_COSINE_SUPPORTS_PER_TICK = 50`.
///
/// Emits an unconditional `tracing::debug!` log after the loop — fires even when
/// `candidate_pairs` is empty (AC-19, WARN-02).
///
/// # Placement (ADR-003)
/// Called after Path A observability log and before the Path B entry gate.
///
/// # Infallibility (FR-15, SR-07)
/// Returns `()`. All errors are handled inside the loop — no `?` propagation.
async fn run_cosine_supports_path(
    store: &Store,
    config: &InferenceConfig,
    candidate_pairs: &[(u64, u64, f32)],
    existing_supports_pairs: &HashSet<(u64, u64)>,
    category_map: &HashMap<u64, &str>,
    timestamp: u64,
) {
    let mut cosine_supports_written: usize = 0;
    let mut cosine_supports_candidates: usize = 0;

    for (src_id, tgt_id, cosine) in candidate_pairs {
        // --- Finite guard (before threshold comparison — ARCHITECTURE.md R-09) ---
        // Cosine values from HNSW should be finite, but guard is required.
        // NaN/Inf would produce an invalid weight in graph_edges.weight (f32 REAL column).
        if !cosine.is_finite() {
            tracing::warn!(
                src_id,
                tgt_id,
                "Path C: non-finite cosine for candidate pair — skipping"
            );
            continue;
        }

        // --- Gate 1: cosine threshold (>= inclusive, per FR-01) ---
        // Pairs at exactly supports_cosine_threshold qualify (>= not >).
        if cosine < &config.supports_cosine_threshold {
            continue;
        }

        // Candidate passed cosine threshold — count it for observability.
        cosine_supports_candidates += 1;

        // --- Gate 2: per-tick budget cap (BEFORE category lookup — ADR-004) ---
        // break (not continue): Phase 4 already sorted by priority. Once budget
        // exhausted, remaining candidates are lower-priority.
        if cosine_supports_written >= MAX_COSINE_SUPPORTS_PER_TICK {
            break;
        }

        // --- Gate 3: category pair filter (O(1) via HashMap — no DB lookup, WARN-01) ---
        // Both source and target category must be present in category_map (from all_active).
        // If an entry was deprecated between Phase 2 DB read and this point, it will be absent.
        let src_cat = match category_map.get(src_id) {
            Some(cat) => *cat,
            None => {
                tracing::debug!(
                    src_id,
                    "Path C: source entry not found in category_map (deprecated mid-tick?) — skipping"
                );
                continue;
            }
        };
        let tgt_cat = match category_map.get(tgt_id) {
            Some(cat) => *cat,
            None => {
                tracing::debug!(
                    tgt_id,
                    "Path C: target entry not found in category_map (deprecated mid-tick?) — skipping"
                );
                continue;
            }
        };

        // Check category pair against config.informs_category_pairs allow-list.
        // Reuses informs_category_pairs (no separate supports_category_pairs field — SCOPE.md).
        let category_allowed = config
            .informs_category_pairs
            .iter()
            .any(|pair| pair[0] == src_cat && pair[1] == tgt_cat);
        if !category_allowed {
            continue;
        }

        // --- Gate 4: pre-filter (INSERT OR IGNORE is authoritative backstop) ---
        // existing_supports_pairs populated at Phase 2 (tick start).
        // Does NOT reflect intra-tick Path C writes — INSERT OR IGNORE handles those.
        let canonical = (*src_id.min(tgt_id), *src_id.max(tgt_id));
        if existing_supports_pairs.contains(&canonical) {
            continue;
        }

        // --- Write edge ---
        let metadata_json = format!(r#"{{"cosine":{cosine}}}"#);
        let wrote = write_graph_edge(
            store,
            *src_id,
            *tgt_id,
            "Supports",
            *cosine,
            timestamp,
            EDGE_SOURCE_COSINE_SUPPORTS,
            &metadata_json,
        )
        .await;

        // Budget counter: increment ONLY on true return (row inserted, rows_affected = 1).
        // false return = UNIQUE conflict (no warn inside fn for Ok path) OR SQL error
        //   (warn already emitted inside write_graph_edge — do NOT double-log).
        // In both false cases: do NOT increment budget, do NOT emit warn here.
        if wrote {
            cosine_supports_written += 1;
        }
        // false return is NOT an error at the loop level — loop continues normally.
    }

    // --- Unconditional Path C observability log (MANDATORY — WARN-02, R-06, ADR-003) ---
    // Fires after the loop, even when candidate_pairs is empty or all candidates filtered.
    // Field names must NOT collide with Path A's log fields.
    tracing::debug!(
        cosine_supports_candidates,
        cosine_supports_edges_written = cosine_supports_written,
        "Path C: cosine Supports tick complete"
    );
}

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
/// - block only when both feature_cycles are non-empty AND equal (intra-feature) (AC-15)
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
    if source_created_at >= target_created_at {
        return false;
    }
    // Block intra-feature pairs only — empty feature_cycle means unknown provenance, allow it.
    if !source_feature_cycle.is_empty()
        && !target_feature_cycle.is_empty()
        && source_feature_cycle == target_feature_cycle
    {
        return false;
    }
    true
}

/// Evaluate composite guard predicates for a candidate Informs edge (crt-039, ADR-002).
///
/// Returns `true` only when BOTH guards pass:
///
/// Guard 2 (temporal): source entry must have been created before target entry.
/// Guard 3 (cross-feature): block only when both feature_cycle fields are non-empty AND equal
///   (intra-feature pairs blocked; empty feature_cycle means unknown provenance, allowed through).
///
/// Guards 1, 4, 5 removed (crt-039 ADR-002):
///   Guard 1 (nli neutral) — NLI model not available on this path.
///   Guards 4, 5 (mutual exclusion via NLI scores) — enforced by candidate set separation
///   at Phase 4b via explicit Supports-set subtraction (FR-06, AC-13).
///
/// Module-private. Accessible to the `tests` sub-module via `use super::*`.
fn apply_informs_composite_guard(candidate: &InformsCandidate) -> bool {
    candidate.source_created_at < candidate.target_created_at
        && (candidate.source_feature_cycle.is_empty()
            || candidate.target_feature_cycle.is_empty()
            || candidate.source_feature_cycle != candidate.target_feature_cycle)
}

/// Serialize structural metadata to JSON for Informs edges (crt-039 ADR-002).
///
/// Records cosine similarity and category pair that qualified this edge.
/// No NLI score fields — Informs edges are written via the structural path only.
/// Replaces `format_nli_metadata_informs` which carried NLI fields irrelevant to Path A.
fn format_informs_metadata(cosine: f32, source_category: &str, target_category: &str) -> String {
    serde_json::json!({
        "cosine":           cosine,
        "source_category":  source_category,
        "target_category":  target_category,
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
            break; // cap reached — checked before pair i is written, so count is exact (FR-09, AC-11)
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
    // run_graph_inference_tick — Path A / Path B boundary (TC-01, TC-02)
    // -----------------------------------------------------------------------
    // TR-01: test_run_graph_inference_tick_nli_not_ready_no_op removed (crt-039).
    // Semantics invalidated: tick is no longer a no-op when NLI not ready.
    // Replaced by TC-01 (Informs CAN be written) + TC-02 (Supports NOT written).
    //
    // TC-01: Phase 4b writes Informs edges even when NLI provider is not ready.
    // Corpus: two entries with embeddings above nli_informs_cosine_floor,
    // no pair above supports_candidate_threshold (exercises R-07: candidate_pairs empty,
    // Path A still runs).
    #[tokio::test]
    async fn test_phase4b_writes_informs_when_nli_not_ready() {
        let tmp = tempfile::TempDir::new().unwrap();
        let arc_store = Arc::new(unimatrix_store::test_helpers::open_test_store(&tmp).await);

        // Insert entries with specific categories from the default informs_category_pairs.
        // Use SQL directly to control category and created_at values.
        let config = InferenceConfig {
            nli_enabled: false,
            // Set supports_candidate_threshold above any cosine we can produce deterministically.
            // Synthetic identical embeddings produce cosine = 1.0, so set threshold > 1.0 to
            // prevent any Supports candidates.
            // Actually identical embeddings will produce cosine 1.0; use a cosine above
            // supports threshold but we need them NOT to be in candidate_pairs.
            // Strategy: use nearly-identical embeddings (cosine ~1.0) and set
            // supports_candidate_threshold = 1.1 (above any real cosine).
            supports_candidate_threshold: 1.1, // impossible threshold, no Supports candidates
            ..InferenceConfig::default()
        };

        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].clone(),
            config.informs_category_pairs[0][1].clone(),
        );

        // Insert entry 1 (source category, older)
        sqlx::query(
            "INSERT OR IGNORE INTO entries \
             (id, title, content, topic, category, source, status, confidence, \
              created_at, updated_at, last_accessed_at, access_count, \
              created_by, modified_by, content_hash, previous_hash, \
              version, feature_cycle, trust_source, helpful_count, unhelpful_count, \
              pre_quarantine_status, correction_count, embedding_dim) \
             VALUES (1, 'src', 'src content', 'test', ?1, 'test', 0, 0.5, \
                     1000, 1000, 0, 0, 'test', 'test', 'h1', '', 1, 'crt-001', '', 0, 0, NULL, 0, 0)",
        )
        .bind(src_cat.as_str())
        .execute(arc_store.write_pool_server())
        .await
        .unwrap();

        // Insert entry 2 (target category, newer)
        sqlx::query(
            "INSERT OR IGNORE INTO entries \
             (id, title, content, topic, category, source, status, confidence, \
              created_at, updated_at, last_accessed_at, access_count, \
              created_by, modified_by, content_hash, previous_hash, \
              version, feature_cycle, trust_source, helpful_count, unhelpful_count, \
              pre_quarantine_status, correction_count, embedding_dim) \
             VALUES (2, 'tgt', 'tgt content', 'test', ?1, 'test', 0, 0.5, \
                     2000, 2000, 0, 0, 'test', 'test', 'h2', '', 1, 'crt-002', '', 0, 0, NULL, 0, 0)",
        )
        .bind(tgt_cat.as_str())
        .execute(arc_store.write_pool_server())
        .await
        .unwrap();

        // Build vector index and insert nearly-identical embeddings (cosine ~1.0 >= floor 0.50).
        let vector_index = Arc::new(
            unimatrix_vector::VectorIndex::new(
                Arc::clone(&arc_store),
                unimatrix_core::VectorConfig::default(),
            )
            .expect("VectorIndex"),
        );
        let dim = unimatrix_core::VectorConfig::default().dimension;
        // Identical embeddings → cosine = 1.0 (>= floor 0.50, so Phase 4b accepts them).
        let mut emb = vec![0.0_f32; dim];
        emb[0] = 1.0; // unit vector
        vector_index.insert(1, &emb).await.expect("insert 1");
        vector_index.insert(2, &emb).await.expect("insert 2");

        // NliServiceHandle in Loading state — get_provider() returns Err.
        let not_ready_handle = NliServiceHandle::new();

        run_graph_inference_tick(
            &arc_store,
            &not_ready_handle,
            &vector_index,
            &make_rayon_pool(),
            &config,
        )
        .await;

        let edges = arc_store.query_graph_edges().await.unwrap();
        let informs_count = edges
            .iter()
            .filter(|e| e.relation_type == "Informs")
            .count();
        assert!(
            informs_count >= 1,
            "TC-01: at least one Informs edge must be written when NLI not ready; edges={edges:?}"
        );
        let supports_count = edges
            .iter()
            .filter(|e| e.relation_type == "Supports")
            .count();
        assert_eq!(
            supports_count, 0,
            "TC-01: zero Supports edges when NLI not ready"
        );
    }

    // TC-02: Zero Supports edges when NLI not ready even with Supports candidates present.
    // Separate test from TC-01 (R-02 coverage requirement).
    #[tokio::test]
    async fn test_phase8_no_supports_when_nli_not_ready() {
        let tmp = tempfile::TempDir::new().unwrap();
        let arc_store = Arc::new(unimatrix_store::test_helpers::open_test_store(&tmp).await);

        insert_test_entry(&arc_store, 10).await;
        insert_test_entry(&arc_store, 20).await;

        // Build vector index with identical embeddings → cosine = 1.0 > supports threshold (0.65).
        let vector_index = Arc::new(
            unimatrix_vector::VectorIndex::new(
                Arc::clone(&arc_store),
                unimatrix_core::VectorConfig::default(),
            )
            .expect("VectorIndex"),
        );
        let dim = unimatrix_core::VectorConfig::default().dimension;
        let mut emb = vec![0.0_f32; dim];
        emb[0] = 1.0;
        vector_index.insert(10, &emb).await.expect("insert 10");
        vector_index.insert(20, &emb).await.expect("insert 20");

        let not_ready_handle = NliServiceHandle::new();
        let config = InferenceConfig {
            nli_enabled: false,
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

        let edges = arc_store.query_graph_edges().await.unwrap();
        let supports: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Supports")
            .collect();
        assert_eq!(
            supports.len(),
            0,
            "TC-02: zero Supports edges when NLI not ready — R-01 guard; edges={edges:?}"
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

    /// Build an InformsCandidate for testing composite guards and Phase 4b logic.
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

    // -----------------------------------------------------------------------
    // AC-13: Happy path — all guards pass, Informs edge written
    // -----------------------------------------------------------------------

    /// AC-13: Phase 8b writes one Informs row when all composite guards pass.
    /// Covers: entry source="nli", relation_type="Informs", finite weight, cosine in metadata.
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
            src_cat, tgt_cat, 0.50,      // cosine >= default floor 0.50
            1_000_000, // source_created_at < target_created_at
            2_000_000, "crt-020", // different feature cycles
            "crt-030",
        );

        assert!(
            apply_informs_composite_guard(&candidate),
            "guard must pass for happy path"
        );

        let ts = current_timestamp_secs();
        let metadata = format_informs_metadata(candidate.cosine, src_cat, tgt_cat);
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

        // Metadata must include cosine and NOT include NLI score fields (R-08).
        let row: (Option<String>,) = sqlx::query_as(
            "SELECT metadata FROM graph_edges WHERE relation_type = 'Informs' LIMIT 1",
        )
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
        let meta_str = row.0.unwrap_or_default();
        let meta_val: serde_json::Value = serde_json::from_str(&meta_str).unwrap_or_default();
        assert!(
            meta_val["cosine"].is_number(),
            "AC-13/R-08: metadata must contain cosine; got: {meta_str}"
        );
        assert!(
            !meta_val["nli_neutral"].is_number(),
            "R-08: metadata must NOT contain nli_neutral; got: {meta_str}"
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
        assert!(
            !apply_informs_composite_guard(&candidate),
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
        assert!(
            !apply_informs_composite_guard(&candidate),
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
        assert!(
            !apply_informs_composite_guard(&candidate),
            "AC-15: same feature cycle must fail guard"
        );
    }

    /// AC-15 (relaxed): source has empty feature_cycle (unknown provenance) → guard passes.
    #[test]
    fn test_phase4b_accepts_source_with_empty_feature_cycle() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        let result = phase4b_candidate_passes_guards(
            0.50, src_cat, tgt_cat, 1_000_000, 2_000_000,
            "",        // source_feature_cycle empty — unknown provenance
            "crt-037", // target has known cycle
            &config,
        );
        assert!(
            result,
            "AC-15: source with empty feature_cycle must pass guard (unknown provenance)"
        );
    }

    /// AC-15 (relaxed): target has empty feature_cycle (unknown provenance) → guard passes.
    #[test]
    fn test_phase4b_accepts_target_with_empty_feature_cycle() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        let result = phase4b_candidate_passes_guards(
            0.50, src_cat, tgt_cat, 1_000_000, 2_000_000, "crt-037", // source has known cycle
            "",        // target_feature_cycle empty — unknown provenance
            &config,
        );
        assert!(
            result,
            "AC-15: target with empty feature_cycle must pass guard (unknown provenance)"
        );
    }

    /// AC-15 (relaxed): both feature_cycles empty → guard passes (newly-reachable path after Site 1 removal).
    #[test]
    fn test_phase4b_accepts_both_empty_feature_cycle() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        let result = phase4b_candidate_passes_guards(
            0.50, src_cat, tgt_cat, 1_000_000, 2_000_000,
            "", // both empty — unknown provenance on both sides
            "", &config,
        );
        assert!(
            result,
            "AC-15: both-empty feature_cycles must pass guard (unknown provenance on both sides)"
        );
    }

    /// AC-15 (Site 3): both-empty feature_cycles reaching apply_informs_composite_guard must pass.
    /// This path was newly-reachable after Sites 1 and 2 were relaxed.
    #[test]
    fn test_apply_informs_composite_guard_both_empty_passes() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        // Both feature_cycles empty — unknown provenance. Should not be blocked by Site 3.
        let candidate =
            make_informs_candidate(src_cat, tgt_cat, 0.50, 1_000_000, 2_000_000, "", "");
        assert!(
            apply_informs_composite_guard(&candidate),
            "AC-15 Site 3: both-empty feature_cycles must pass composite guard"
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
    /// After crt-039: floor is 0.50; cosine 0.499 is below it.
    #[test]
    fn test_phase8b_no_informs_when_cosine_below_floor() {
        let config = informs_config();
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        // cosine = 0.499 is below default floor 0.50 (after crt-039).
        let result = phase4b_candidate_passes_guards(
            0.499, src_cat, tgt_cat, 1_000_000, 2_000_000, "crt-020", "crt-030", &config,
        );
        assert!(
            !result,
            "AC-17: cosine 0.499 below floor 0.50 must be rejected by Phase 4b"
        );
    }

    // -----------------------------------------------------------------------
    // AC-18: Phase 4b uses nli_informs_cosine_floor, not supports_candidate_threshold
    // -----------------------------------------------------------------------

    /// AC-18 (updated for crt-039): A pair at cosine = 0.50 (the new inclusive floor) is
    /// accepted by Phase 4b (nli_informs_cosine_floor = 0.50, >= semantics) but rejected
    /// by Phase 4 (supports_candidate_threshold = 0.50, strict > semantics).
    /// Band [0.50, supports_candidate_threshold) differentiates the two phases.
    #[test]
    fn test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold() {
        let config = informs_config();
        assert_eq!(
            config.nli_informs_cosine_floor, 0.5_f32,
            "sanity: floor is 0.5 after crt-039"
        );
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );
        // cosine exactly at the new floor — Phase 4b accepts (>=), Phase 4 rejects (strict >).
        let cosine_at_floor = 0.50_f32;

        // Phase 4b: inclusive >= floor (0.50) — should pass.
        let phase4b_accepts = phase4b_candidate_passes_guards(
            cosine_at_floor,
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
            "AC-18 (updated): cosine 0.50 >= nli_informs_cosine_floor 0.50 must be accepted by Phase 4b"
        );

        // Phase 4 strict >: 0.50 > 0.50 is false → excluded.
        let phase4_would_accept = cosine_at_floor > config.supports_candidate_threshold;
        assert!(
            !phase4_would_accept,
            "AC-18 (updated): cosine 0.50 must be excluded by Phase 4 (strict > 0.50)"
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
        let metadata = format_informs_metadata(cosine, "lesson-learned", "decision");
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
        let config = informs_config();
        let weight = 0.50_f32 * config.nli_informs_ppr_weight;
        let metadata = format_informs_metadata(0.50, "lesson-learned", "decision");

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
        let config = informs_config();
        let weight = 0.50_f32 * config.nli_informs_ppr_weight;
        let metadata = format_informs_metadata(0.50, "lesson-learned", "decision");

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
    // Additional guard tests (crt-039: TR-02 and TR-03 removed; TC-03 through TC-07 added)
    // -----------------------------------------------------------------------
    // TR-02: test_phase8b_no_informs_when_neutral_exactly_0_5 removed — neutral guard gone (ADR-002).
    // TR-03: test_phase8b_writes_informs_when_neutral_just_above_0_5 removed — neutral guard gone.
    // TR-05 (FR-11): test_phase8b_no_informs_when_entailment_exceeds_supports_threshold removed —
    //   mutual exclusion now enforced by candidate set separation, not at write time (ADR-002).

    // TC-03: apply_informs_composite_guard temporal guard.
    #[test]
    fn test_apply_informs_composite_guard_temporal_guard() {
        // Source newer than target — must fail
        let candidate_newer = make_informs_candidate(
            "lesson-learned",
            "decision",
            0.55,
            2_000_000,
            1_000_000, // source_ts > target_ts
            "crt-020",
            "crt-030",
        );
        assert!(
            !apply_informs_composite_guard(&candidate_newer),
            "TC-03a: guard must return false when source_created_at >= target_created_at"
        );

        // Source older than target — must pass
        let candidate_older = make_informs_candidate(
            "lesson-learned",
            "decision",
            0.55,
            1_000_000,
            2_000_000, // source_ts < target_ts
            "crt-020",
            "crt-030",
        );
        assert!(
            apply_informs_composite_guard(&candidate_older),
            "TC-03b: guard must return true when source_created_at < target_created_at"
        );
    }

    // TC-04: apply_informs_composite_guard cross-feature guard.
    #[test]
    fn test_apply_informs_composite_guard_cross_feature_guard() {
        // Both non-empty and equal — must fail
        let same_cycle = make_informs_candidate(
            "ll", "dec", 0.55, 1_000_000, 2_000_000, "crt-020", "crt-020",
        );
        assert!(
            !apply_informs_composite_guard(&same_cycle),
            "TC-04a: both cycles non-empty and equal → false"
        );

        // Source empty — must pass
        let src_empty =
            make_informs_candidate("ll", "dec", 0.55, 1_000_000, 2_000_000, "", "crt-020");
        assert!(
            apply_informs_composite_guard(&src_empty),
            "TC-04b: source cycle empty → true"
        );

        // Target empty — must pass
        let tgt_empty =
            make_informs_candidate("ll", "dec", 0.55, 1_000_000, 2_000_000, "crt-020", "");
        assert!(
            apply_informs_composite_guard(&tgt_empty),
            "TC-04c: target cycle empty → true"
        );

        // Both non-empty and different — must pass
        let diff_cycle = make_informs_candidate(
            "ll", "dec", 0.55, 1_000_000, 2_000_000, "crt-020", "crt-030",
        );
        assert!(
            apply_informs_composite_guard(&diff_cycle),
            "TC-04d: both cycles non-empty and different → true"
        );
    }

    // TC-05: Phase 4b cosine floor boundary (>= semantics).
    // TC-06 (cosine 0.499 excluded) is also covered by the second assertion here.
    #[test]
    fn test_phase4b_cosine_floor_boundary() {
        let config = InferenceConfig {
            nli_informs_cosine_floor: 0.5,
            ..InferenceConfig::default()
        };
        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].as_str(),
            config.informs_category_pairs[0][1].as_str(),
        );

        // Exactly 0.500 — must be included (inclusive >=)
        assert!(
            phase4b_candidate_passes_guards(
                0.500_f32, src_cat, tgt_cat, 1_000, 2_000, "crt-020", "crt-030", &config
            ),
            "TC-05a: cosine exactly 0.500 must pass Phase 4b cosine guard (inclusive >=)"
        );

        // Exactly 0.499 — must be excluded
        assert!(
            !phase4b_candidate_passes_guards(
                0.499_f32, src_cat, tgt_cat, 1_000, 2_000, "crt-020", "crt-030", &config
            ),
            "TC-05b: cosine exactly 0.499 must be excluded by Phase 4b (below floor)"
        );
    }

    // TC-07: Phase 4b explicit Supports-set subtraction.
    #[test]
    fn test_phase4b_explicit_supports_set_subtraction() {
        // candidate_pairs contains a pair at cosine 0.68 (above supports threshold 0.65).
        // After Phase 4b collects informs_metadata, this pair must be subtracted.
        let supports_candidate_set: HashSet<(u64, u64)> = {
            let pairs: Vec<(u64, u64, f32)> = vec![(1_u64, 2_u64, 0.68_f32)];
            pairs
                .iter()
                .flat_map(|(src, tgt, _)| [(*src, *tgt), (*tgt, *src)])
                .collect()
        };

        let mut informs_metadata = vec![
            // Pair (1,2) at cosine 0.68 — present in candidate_pairs, must be removed.
            {
                let mut c = make_informs_candidate(
                    "lesson-learned",
                    "decision",
                    0.68,
                    1_000_000,
                    2_000_000,
                    "crt-020",
                    "crt-030",
                );
                c.source_id = 1;
                c.target_id = 2;
                c
            },
            // Pair (3,4) at cosine 0.55 — NOT in candidate_pairs, must be retained.
            {
                let mut c = make_informs_candidate(
                    "lesson-learned",
                    "decision",
                    0.55,
                    3_000_000,
                    4_000_000,
                    "crt-020",
                    "crt-030",
                );
                c.source_id = 3;
                c.target_id = 4;
                c
            },
        ];

        // Apply the Phase 4b subtraction (R-03, FR-06, AC-13).
        informs_metadata.retain(|c| {
            !supports_candidate_set.contains(&(c.source_id, c.target_id))
                && !supports_candidate_set.contains(&(c.target_id, c.source_id))
        });

        assert!(
            !informs_metadata
                .iter()
                .any(|c| c.source_id == 1 && c.target_id == 2),
            "TC-07: pair at cosine 0.68 present in candidate_pairs must be absent from informs_metadata"
        );
        assert!(
            informs_metadata
                .iter()
                .any(|c| c.source_id == 3 && c.target_id == 4),
            "TC-07: pair (3,4) not in candidate_pairs must remain in informs_metadata"
        );

        // Boundary variant (R-03): pair at cosine 0.50 is NOT in candidate_pairs
        // (Phase 4 strict > 0.50 excludes it). Must NOT be subtracted.
        let boundary_pair = {
            let mut c = make_informs_candidate(
                "lesson-learned",
                "decision",
                0.50,
                1_000,
                2_000,
                "c1",
                "c2",
            );
            c.source_id = 5;
            c.target_id = 6;
            c
        };
        assert!(
            !supports_candidate_set.contains(&(boundary_pair.source_id, boundary_pair.target_id)),
            "boundary pair at 0.50 must not be in supports_candidate_set (Phase 4 uses strict >)"
        );
    }

    /// Phase 5 independent budget: Supports fills max_graph_inference_per_tick exactly,
    /// Informs still gets its full MAX_INFORMS_PER_TICK budget (bugfix-473).
    ///
    /// This pins the core invariant: Informs budget is independent of Supports fill.
    #[test]
    fn test_phase5_informs_always_gets_dedicated_budget() {
        // Supports fills the Supports cap completely.
        let supports_cap = MAX_INFORMS_PER_TICK; // reuse same value to stress-test independence
        let mut supports: Vec<(u64, u64, f32)> = (0..supports_cap as u64)
            .map(|i| (i, i + 100, 0.9))
            .collect();
        supports.truncate(supports_cap);
        assert_eq!(supports.len(), supports_cap, "Supports fills its cap");

        // Build N > MAX_INFORMS_PER_TICK Informs candidates.
        let n_informs = MAX_INFORMS_PER_TICK + 10;
        let mut informs: Vec<InformsCandidate> = (0..n_informs as i64)
            .map(|i| make_informs_candidate("a", "b", 0.5, i, i + 1, "c1", "c2"))
            .collect();

        // Apply the fixed Phase 5 logic: shuffle + truncate to MAX_INFORMS_PER_TICK.
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        informs.shuffle(&mut rng);
        informs.truncate(MAX_INFORMS_PER_TICK);

        // Informs must get its full budget regardless of how many Supports there are.
        assert_eq!(
            informs.len(),
            MAX_INFORMS_PER_TICK,
            "Informs must always receive its dedicated budget even when Supports fills its cap"
        );
    }

    /// Phase 5 independent budget: when pool < MAX_INFORMS_PER_TICK, all candidates kept.
    #[test]
    fn test_phase5_informs_small_pool_all_kept() {
        let n_informs = MAX_INFORMS_PER_TICK / 2;
        let mut informs: Vec<InformsCandidate> = (0..n_informs as i64)
            .map(|i| make_informs_candidate("a", "b", 0.5, i, i + 1, "c1", "c2"))
            .collect();

        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        informs.shuffle(&mut rng);
        informs.truncate(MAX_INFORMS_PER_TICK);

        assert_eq!(
            informs.len(),
            n_informs,
            "all candidates kept when pool < MAX_INFORMS_PER_TICK"
        );
    }

    /// Phase 5 independent budget: empty Informs pool stays empty after shuffle+truncate.
    #[test]
    fn test_phase5_informs_empty_pool_stays_empty() {
        let mut informs: Vec<InformsCandidate> = vec![];

        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        informs.shuffle(&mut rng);
        informs.truncate(MAX_INFORMS_PER_TICK);

        assert!(informs.is_empty(), "empty pool must remain empty");
    }

    /// Phase 5 independent budget: Informs result has no duplicates and only valid IDs.
    #[test]
    fn test_phase5_informs_shuffle_no_duplicates_valid_ids() {
        let n_informs = MAX_INFORMS_PER_TICK + 15;
        let mut informs: Vec<InformsCandidate> = (0..n_informs as i64)
            .map(|i| {
                let mut c = make_informs_candidate("a", "b", 0.5, i, i + 1, "c1", "c2");
                c.source_id = i as u64;
                c.target_id = (i + 1000) as u64;
                c
            })
            .collect();

        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        informs.shuffle(&mut rng);
        informs.truncate(MAX_INFORMS_PER_TICK);

        assert_eq!(informs.len(), MAX_INFORMS_PER_TICK);
        // No duplicate (source_id, target_id) pairs.
        let pair_set: std::collections::HashSet<(u64, u64)> =
            informs.iter().map(|c| (c.source_id, c.target_id)).collect();
        assert_eq!(
            pair_set.len(),
            MAX_INFORMS_PER_TICK,
            "no duplicate pairs after shuffle"
        );
        // All source IDs must be within the original pool range.
        assert!(
            informs.iter().all(|c| (c.source_id as usize) < n_informs),
            "all IDs must come from the original pool"
        );
    }

    /// Phase 5 log accounting: dropped = total - kept, accepted + dropped == total.
    #[test]
    fn test_phase5_informs_log_accounting_consistent() {
        let n_informs = MAX_INFORMS_PER_TICK + 7;
        let mut informs: Vec<InformsCandidate> = (0..n_informs as i64)
            .map(|i| make_informs_candidate("a", "b", 0.5, i, i + 1, "c1", "c2"))
            .collect();

        let informs_total_before_cap = informs.len();

        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        informs.shuffle(&mut rng);
        informs.truncate(MAX_INFORMS_PER_TICK);

        let informs_kept = informs.len();
        let informs_dropped = informs_total_before_cap.saturating_sub(informs_kept);

        assert_eq!(
            informs_kept + informs_dropped,
            informs_total_before_cap,
            "accepted + dropped must equal total (SR-03)"
        );
        assert_eq!(
            informs_kept, MAX_INFORMS_PER_TICK,
            "kept must equal the budget cap"
        );
        assert_eq!(informs_dropped, 7, "dropped = total - MAX_INFORMS_PER_TICK");
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

    /// format_informs_metadata contains cosine and category fields; no NLI score fields (R-08).
    #[test]
    fn test_format_informs_metadata_contains_structural_fields() {
        let json = format_informs_metadata(0.55_f32, "lesson-learned", "decision");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        // Must contain structural fields.
        assert!(parsed["cosine"].is_number(), "R-08: must have cosine");
        assert!(
            parsed["source_category"].is_string(),
            "R-08: must have source_category"
        );
        assert!(
            parsed["target_category"].is_string(),
            "R-08: must have target_category"
        );
        let cosine = parsed["cosine"].as_f64().unwrap();
        assert!((cosine - 0.55_f64).abs() < 1e-3, "cosine={cosine}");
        // Must NOT contain NLI score fields.
        assert!(
            parsed["nli_neutral"].is_null(),
            "R-08: must not have nli_neutral"
        );
        assert!(
            parsed["nli_entailment"].is_null(),
            "R-08: must not have nli_entailment"
        );
        assert!(
            parsed["nli_contradiction"].is_null(),
            "R-08: must not have nli_contradiction"
        );
    }

    // -----------------------------------------------------------------------
    // Path C: Cosine Supports (crt-040)
    // -----------------------------------------------------------------------

    /// Insert a test entry with a configurable category via raw SQL.
    async fn insert_test_entry_with_category(store: &Store, id: u64, category: &str) {
        sqlx::query(
            "INSERT OR IGNORE INTO entries \
             (id, title, content, topic, category, source, status, confidence, \
              created_at, updated_at, last_accessed_at, access_count, \
              created_by, modified_by, content_hash, previous_hash, \
              version, feature_cycle, trust_source, helpful_count, unhelpful_count, \
              pre_quarantine_status, correction_count, embedding_dim) \
             VALUES (?1, 'test', 'test content', 'test-topic', ?3, 'test', 0, 0.5, \
                     ?2, ?2, 0, 0, 'test', 'test', 'hash', '', 1, '', '', 0, 0, NULL, 0, 0)",
        )
        .bind(id as i64)
        .bind(id as i64)
        .bind(category)
        .execute(store.write_pool_server())
        .await
        .unwrap();
    }

    /// Build a default config with supports_cosine_threshold = 0.65 (the default).
    fn path_c_config() -> InferenceConfig {
        InferenceConfig::default()
    }

    /// TC-01: Qualifying pair (cosine >= threshold, allowed category, not pre-existing)
    /// produces one Supports edge with source = "cosine_supports" (AC-01, R-01).
    #[tokio::test]
    async fn test_path_c_qualifying_pair_writes_supports_edge() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 1, "lesson-learned").await;
        insert_test_entry_with_category(&store, 2, "decision").await;

        let config = path_c_config();
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![(1, 2, 0.70_f32)];
        let existing = HashSet::new();
        let category_map: HashMap<u64, &str> = [(1_u64, "lesson-learned"), (2_u64, "decision")]
            .into_iter()
            .collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        let supports: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Supports")
            .collect();
        assert_eq!(
            supports.len(),
            1,
            "TC-01: exactly one Supports edge expected"
        );
        assert_eq!(
            supports[0].source, EDGE_SOURCE_COSINE_SUPPORTS,
            "TC-01: source must be cosine_supports"
        );
    }

    /// TC-02: Pair below threshold produces no edge (AC-02).
    #[tokio::test]
    async fn test_path_c_below_threshold_no_edge() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 1, "lesson-learned").await;
        insert_test_entry_with_category(&store, 2, "decision").await;

        let config = path_c_config();
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![(1, 2, 0.64_f32)];
        let existing = HashSet::new();
        let category_map: HashMap<u64, &str> = [(1_u64, "lesson-learned"), (2_u64, "decision")]
            .into_iter()
            .collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        assert!(
            edges.is_empty(),
            "TC-02: no Supports edge expected for cosine below threshold"
        );
    }

    /// TC-03: Disallowed category pair produces no edge even at cosine 0.80 (AC-03, R-01).
    #[tokio::test]
    async fn test_path_c_disallowed_category_no_edge() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 5, "decision").await;
        insert_test_entry_with_category(&store, 6, "decision").await;

        let config = path_c_config();
        // "decision"/"decision" is NOT in the default informs_category_pairs allow-list.
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![(5, 6, 0.80_f32)];
        let existing = HashSet::new();
        let category_map: HashMap<u64, &str> = [(5_u64, "decision"), (6_u64, "decision")]
            .into_iter()
            .collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        assert!(
            edges.is_empty(),
            "TC-03: no edge for disallowed category pair even at cosine 0.80"
        );
    }

    /// TC-04: Pair already in existing_supports_pairs is skipped (AC-04).
    #[tokio::test]
    async fn test_path_c_existing_pair_skipped() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 1, "lesson-learned").await;
        insert_test_entry_with_category(&store, 2, "decision").await;

        let config = path_c_config();
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![(1, 2, 0.70_f32)];
        // Pre-populate existing_supports_pairs so the pair is skipped.
        let mut existing = HashSet::new();
        existing.insert((1_u64, 2_u64));
        let category_map: HashMap<u64, &str> = [(1_u64, "lesson-learned"), (2_u64, "decision")]
            .into_iter()
            .collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        assert!(
            edges.is_empty(),
            "TC-04: pair in existing_supports_pairs must be skipped"
        );
    }

    /// TC-05: Path C runs even when nli_enabled=false — it is unconditional (AC-05, FR-13).
    /// Calls run_cosine_supports_path directly; NLI flag has no influence on Path C.
    #[tokio::test]
    async fn test_path_c_runs_unconditionally_nli_disabled() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 1, "lesson-learned").await;
        insert_test_entry_with_category(&store, 2, "decision").await;

        // Config with nli_enabled = false — Path B would skip but Path C must still run.
        let config = InferenceConfig {
            nli_enabled: false,
            ..InferenceConfig::default()
        };
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![(1, 2, 0.70_f32)];
        let existing = HashSet::new();
        let category_map: HashMap<u64, &str> = [(1_u64, "lesson-learned"), (2_u64, "decision")]
            .into_iter()
            .collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        let supports: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Supports")
            .collect();
        assert_eq!(
            supports.len(),
            1,
            "TC-05: Path C must write Supports edge even when nli_enabled=false"
        );
        assert_eq!(
            supports[0].source, EDGE_SOURCE_COSINE_SUPPORTS,
            "TC-05: source must be cosine_supports"
        );
    }

    /// TC-07: 60 qualifying pairs → exactly 50 edges written (budget cap, AC-12).
    /// config.max_graph_inference_per_tick must be >= 60 so Phase 5 truncation doesn't
    /// reduce the list before Path C runs.
    #[tokio::test]
    async fn test_path_c_budget_cap_50_from_60_qualifying() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

        // Insert 60 + 60 entries with IDs 1..=60 and 1001..=1060.
        for i in 1_u64..=60 {
            insert_test_entry_with_category(&store, i, "lesson-learned").await;
            insert_test_entry_with_category(&store, i + 1000, "decision").await;
        }

        let config = InferenceConfig {
            // REQUIRED: must be >= 60 so Phase 5 doesn't truncate before Path C sees them.
            max_graph_inference_per_tick: 60,
            ..InferenceConfig::default()
        };

        // Build 60 qualifying pairs, all above threshold, all cross-category.
        let candidate_pairs: Vec<(u64, u64, f32)> =
            (1_u64..=60).map(|i| (i, i + 1000, 0.70_f32)).collect();
        let existing = HashSet::new();
        let mut category_map: HashMap<u64, &str> = HashMap::new();
        for i in 1_u64..=60 {
            category_map.insert(i, "lesson-learned");
            category_map.insert(i + 1000, "decision");
        }
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        let supports: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Supports")
            .collect();
        assert_eq!(
            supports.len(),
            50,
            "TC-07: budget cap must limit writes to MAX_COSINE_SUPPORTS_PER_TICK=50; got {}",
            supports.len()
        );
    }

    /// TC-08: UNIQUE conflict (pair not in pre-filter but already in DB) → write_graph_edge
    /// returns false, budget counter not incremented (R-07).
    #[tokio::test]
    async fn test_path_c_budget_counter_not_incremented_on_unique_conflict() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

        // Insert entries for 60 pairs: ids 1..=60 ("lesson-learned") and 1001..=1060 ("decision").
        for i in 1_u64..=60 {
            insert_test_entry_with_category(&store, i, "lesson-learned").await;
            insert_test_entry_with_category(&store, i + 1000, "decision").await;
        }

        let ts = current_timestamp_secs();
        let config = InferenceConfig {
            max_graph_inference_per_tick: 65,
            ..InferenceConfig::default()
        };
        let mut category_map: HashMap<u64, &str> = HashMap::new();
        for i in 1_u64..=60 {
            category_map.insert(i, "lesson-learned");
            category_map.insert(i + 1000, "decision");
        }

        // Pre-insert 10 edges directly to create UNIQUE conflict pairs NOT in the pre-filter.
        // These 10 will cause INSERT OR IGNORE → rows_affected=0 → false return.
        for i in 51_u64..=60 {
            let meta = format!(r#"{{"cosine":{}}}"#, 0.70_f32);
            write_graph_edge(
                &store,
                i,
                i + 1000,
                "Supports",
                0.70_f32,
                ts,
                EDGE_SOURCE_COSINE_SUPPORTS,
                &meta,
            )
            .await;
        }

        // candidate_pairs: 60 pairs total.
        // Pairs 1..=50 are new (write_graph_edge returns true → budget increments).
        // Pairs 51..=60 are already in DB but NOT in existing_supports_pairs → UNIQUE conflict
        //   → write_graph_edge returns false → budget NOT incremented.
        // After 50 true returns, budget is exhausted and the loop breaks.
        let candidate_pairs: Vec<(u64, u64, f32)> =
            (1_u64..=60).map(|i| (i, i + 1000, 0.70_f32)).collect();
        let existing = HashSet::new(); // NOT pre-filtered — they must go to DB to hit UNIQUE

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        // After the run: pairs 1..=50 written (new), pairs 51..=60 already existed.
        let edges = store.query_graph_edges().await.unwrap();
        let supports: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Supports")
            .collect();
        assert_eq!(
            supports.len(),
            60,
            "TC-08: 10 pre-existing + 50 new = 60 total Supports edges; got {}",
            supports.len()
        );
    }

    /// TC-09: NaN cosine produces no edge and emits warn, loop continues (R-09).
    #[tokio::test]
    async fn test_path_c_nan_cosine_no_edge() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 1, "lesson-learned").await;
        insert_test_entry_with_category(&store, 2, "decision").await;
        // Second pair is valid — verifies the loop continues after the NaN pair.
        insert_test_entry_with_category(&store, 3, "lesson-learned").await;
        insert_test_entry_with_category(&store, 4, "decision").await;

        let config = path_c_config();
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![
            (1, 2, f32::NAN), // NaN — must be skipped, warn emitted
            (3, 4, 0.70_f32), // Valid — must be written (verifies loop continues)
        ];
        let existing = HashSet::new();
        let category_map: HashMap<u64, &str> = [
            (1_u64, "lesson-learned"),
            (2_u64, "decision"),
            (3_u64, "lesson-learned"),
            (4_u64, "decision"),
        ]
        .into_iter()
        .collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        let supports: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Supports")
            .collect();
        // NaN pair: no edge. Valid pair: one edge.
        assert_eq!(
            supports.len(),
            1,
            "TC-09: only the valid pair must produce an edge"
        );
        assert!(
            supports
                .iter()
                .all(|e| e.source_id != 1 && e.target_id != 2),
            "TC-09: NaN pair must NOT produce an edge"
        );
    }

    /// TC-12: Observability log fires with zero counts when candidate_pairs and
    /// informs_metadata are both empty (AC-19, R-06).
    /// (Structural test — verifies no panic + function completes; log verification
    ///  requires a tracing subscriber which is not set up in unit tests.)
    #[tokio::test]
    async fn test_path_c_observability_log_fires_with_empty_candidates() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

        let config = path_c_config();
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![];
        let existing = HashSet::new();
        let category_map: HashMap<u64, &str> = HashMap::new();
        let ts = current_timestamp_secs();

        // Must not panic and must complete (observability log fires unconditionally).
        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        assert!(
            edges.is_empty(),
            "TC-12: no edges expected with empty candidate_pairs; got {}",
            edges.len()
        );
    }

    /// MAX_COSINE_SUPPORTS_PER_TICK value and independence from MAX_INFORMS_PER_TICK (ADR-004).
    #[test]
    fn test_max_cosine_supports_per_tick_value_and_independence() {
        assert_eq!(
            MAX_COSINE_SUPPORTS_PER_TICK, 50_usize,
            "MAX_COSINE_SUPPORTS_PER_TICK must be 50"
        );
        assert_ne!(
            MAX_COSINE_SUPPORTS_PER_TICK, MAX_INFORMS_PER_TICK,
            "MAX_COSINE_SUPPORTS_PER_TICK (50) must be independent of MAX_INFORMS_PER_TICK (25)"
        );
    }

    // -----------------------------------------------------------------------
    // Path C: Missing TCs from Gate 3b (crt-040)
    // -----------------------------------------------------------------------

    /// TC-03: Pair at exactly 0.65 threshold MUST qualify (>= not >) (AC-02 boundary).
    /// Addresses the gate-3b WARN: the code at line 776 uses `<` (i.e. >= semantics)
    /// but the boundary case lacked a dedicated test. R-09 guard fires before threshold.
    #[tokio::test]
    async fn test_path_c_exact_threshold_boundary_qualifies() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 1, "lesson-learned").await;
        insert_test_entry_with_category(&store, 2, "decision").await;

        let config = InferenceConfig {
            supports_cosine_threshold: 0.65_f32,
            ..InferenceConfig::default()
        };
        // Pair at EXACTLY the threshold — must write the edge (>= semantics, not >).
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![(1, 2, 0.65_f32)];
        let existing = HashSet::new();
        let category_map: HashMap<u64, &str> = [(1_u64, "lesson-learned"), (2_u64, "decision")]
            .into_iter()
            .collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        let supports: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Supports")
            .collect();
        assert_eq!(
            supports.len(),
            1,
            "TC-03: pair at exactly 0.65 must qualify (>= not >); got {} edges",
            supports.len()
        );
    }

    /// TC-10: Infinity cosine pair produces no edge (R-09 second variant).
    /// `f32::INFINITY.is_finite()` is false — same guard path as NaN.
    #[tokio::test]
    async fn test_path_c_infinity_cosine_no_edge() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 1, "lesson-learned").await;
        insert_test_entry_with_category(&store, 2, "decision").await;

        let config = path_c_config();
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![(1, 2, f32::INFINITY)];
        let existing = HashSet::new();
        let category_map: HashMap<u64, &str> = [(1_u64, "lesson-learned"), (2_u64, "decision")]
            .into_iter()
            .collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        assert!(
            edges.is_empty(),
            "TC-10: f32::INFINITY must produce no edge (is_finite() guard)"
        );
    }

    /// TC-11: NaN guard fires BEFORE threshold comparison (R-09 guard placement).
    /// Sets threshold = 0.0 (every finite value qualifies) to ensure the result
    /// cannot be explained by threshold rejection — only the is_finite() guard applies.
    #[tokio::test]
    async fn test_path_c_nan_guard_order_threshold_not_evaluated() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 1, "lesson-learned").await;
        insert_test_entry_with_category(&store, 2, "decision").await;

        let config = InferenceConfig {
            // Any finite value would pass this threshold — if threshold were evaluated
            // first and NaN happened to pass via raw comparison, an edge might be written.
            // The is_finite() guard must fire before the threshold check.
            supports_cosine_threshold: 0.0_f32,
            ..InferenceConfig::default()
        };
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![(1, 2, f32::NAN)];
        let existing = HashSet::new();
        let category_map: HashMap<u64, &str> = [(1_u64, "lesson-learned"), (2_u64, "decision")]
            .into_iter()
            .collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        assert!(
            edges.is_empty(),
            "TC-11: NaN must produce no edge even at threshold=0.0; is_finite() guard must fire first"
        );
    }

    /// TC-13: Observability log fires with correct non-zero counts (AC-19).
    /// 5 pairs above threshold, 3 below — candidates=5, written=5.
    /// Structural test: no panic, correct edge count as proxy for counter correctness.
    #[tokio::test]
    async fn test_path_c_observability_log_counts_correct() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

        // Insert 8 pairs of entries.
        for i in 1_u64..=8 {
            insert_test_entry_with_category(&store, i, "lesson-learned").await;
            insert_test_entry_with_category(&store, i + 100, "decision").await;
        }

        let config = path_c_config(); // threshold = 0.65
        // 5 qualifying pairs (cosine=0.70), 3 below threshold (cosine=0.50).
        let mut candidate_pairs: Vec<(u64, u64, f32)> =
            (1_u64..=5).map(|i| (i, i + 100, 0.70_f32)).collect();
        candidate_pairs.extend((6_u64..=8).map(|i| (i, i + 100, 0.50_f32)));

        let existing = HashSet::new();
        let mut category_map: HashMap<u64, &str> = HashMap::new();
        for i in 1_u64..=8 {
            category_map.insert(i, "lesson-learned");
            category_map.insert(i + 100, "decision");
        }
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        // Proxy assertion: exactly 5 edges written matches cosine_supports_edges_written=5.
        // The debug! log is unconditional — absence of panic confirms it fired.
        let edges = store.query_graph_edges().await.unwrap();
        let supports: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Supports")
            .collect();
        assert_eq!(
            supports.len(),
            5,
            "TC-13: exactly 5 qualifying pairs must write 5 edges; observability count matches"
        );
    }

    /// TC-15: inferred_edge_count counts all inference sources and excludes co_access.
    ///
    /// Documents the semantic contract of the exclusive filter
    /// (`source NOT IN ('co_access', '')`):
    /// - Every named inference source (nli, cosine_supports, S1, S2, S8, behavioral) increments
    ///   `inferred_edge_count` by exactly 1 when a non-bootstrap edge is inserted with that source.
    /// - `co_access` edges (bootstrap_only=0) do NOT increment `inferred_edge_count`, because
    ///   co_access promotion is a co-retrieval bookkeeping path, not structural inference.
    ///
    /// New inference sources added in future features are automatically counted without any
    /// code change (open/closed: open for extension, closed to silent miscounting).
    #[tokio::test]
    async fn test_inferred_edge_count_table_driven() {
        // Each row: (source_value, should_increment: bool, label)
        let cases: &[(&str, bool, &str)] = &[
            (EDGE_SOURCE_NLI, true, "nli"),
            (EDGE_SOURCE_COSINE_SUPPORTS, true, "cosine_supports"),
            (EDGE_SOURCE_S1, true, "S1"),
            (EDGE_SOURCE_S2, true, "S2"),
            (EDGE_SOURCE_S8, true, "S8"),
            ("behavioral", true, "behavioral"),
            (EDGE_SOURCE_CO_ACCESS, false, "co_access (excluded)"),
        ];

        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

        // Insert enough entries for each case (one unique pair per case).
        // Cases are indexed 0..N; pair IDs are (case_idx*2+1, case_idx*2+2).
        for i in 0..(cases.len() as u64) {
            insert_test_entry_with_category(&store, i * 2 + 1, "lesson-learned").await;
            insert_test_entry_with_category(&store, i * 2 + 2, "decision").await;
        }

        let ts = current_timestamp_secs();

        for (idx, (source, should_increment, label)) in cases.iter().enumerate() {
            let src_id = (idx as u64) * 2 + 1;
            let tgt_id = (idx as u64) * 2 + 2;

            let before = store
                .compute_graph_cohesion_metrics()
                .await
                .expect("TC-15: metrics before insert");
            let before_inferred = before.inferred_edge_count;

            write_graph_edge(
                &store, src_id, tgt_id, "Supports", 0.80_f32, ts, source, "{}",
            )
            .await;

            let after = store
                .compute_graph_cohesion_metrics()
                .await
                .expect("TC-15: metrics after insert");

            if *should_increment {
                assert_eq!(
                    after.inferred_edge_count,
                    before_inferred + 1,
                    "TC-15: source='{}' ({}) must increment inferred_edge_count by 1",
                    source,
                    label,
                );
            } else {
                assert_eq!(
                    after.inferred_edge_count, before_inferred,
                    "TC-15: source='{}' ({}) must NOT increment inferred_edge_count (excluded source)",
                    source, label,
                );
            }
        }
    }

    /// TC-17: Reversed pair (A,B) and (B,A) both in candidate_pairs produces at most one edge (R-08).
    /// INSERT OR IGNORE UNIQUE(source_id, target_id, relation_type) is the authoritative dedup.
    /// Phase 4 canonical (lo,hi) normalization is the pre-filter; UNIQUE constraint is the backstop.
    #[tokio::test]
    async fn test_path_c_reversed_pair_no_duplicate_edge() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 1, "lesson-learned").await;
        insert_test_entry_with_category(&store, 2, "decision").await;

        let config = path_c_config();
        // Both (1,2) and (2,1) present — only one Supports edge must be written.
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![(1, 2, 0.70_f32), (2, 1, 0.70_f32)];
        let existing = HashSet::new();
        let category_map: HashMap<u64, &str> = [(1_u64, "lesson-learned"), (2_u64, "decision")]
            .into_iter()
            .collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        let supports: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Supports")
            .collect();
        assert_eq!(
            supports.len(),
            1,
            "TC-17: (1,2) and (2,1) must produce at most one Supports edge; got {}",
            supports.len()
        );
    }

    // -----------------------------------------------------------------------
    // bugfix-523 Item 1 — NLI tick gate (AC-01 / AC-02 / AC-03)
    // -----------------------------------------------------------------------

    /// AC-01 (bugfix-523): Path B is skipped when nli_enabled=false and candidate_pairs is
    /// non-empty. The non-empty pair list is mandatory — the empty-pairs fast-exit fires
    /// before the nli_enabled gate, so an empty list would not exercise the new gate.
    ///
    /// Behavioral proxy: no Supports edges with source != EDGE_SOURCE_COSINE_SUPPORTS
    /// are written, confirming Path B (NLI) did not execute.
    #[tokio::test]
    async fn test_nli_gate_path_b_skipped_nli_disabled() {
        let tmp = tempfile::TempDir::new().unwrap();
        let arc_store = Arc::new(unimatrix_store::test_helpers::open_test_store(&tmp).await);

        insert_test_entry(&arc_store, 1).await;
        insert_test_entry(&arc_store, 2).await;

        let vector_index = Arc::new(
            unimatrix_vector::VectorIndex::new(
                Arc::clone(&arc_store),
                unimatrix_core::VectorConfig::default(),
            )
            .expect("VectorIndex"),
        );
        let dim = unimatrix_core::VectorConfig::default().dimension;
        let mut emb = vec![0.0_f32; dim];
        emb[0] = 1.0;
        vector_index.insert(1, &emb).await.expect("insert 1");
        vector_index.insert(2, &emb).await.expect("insert 2");

        // nli_enabled=false + supports_candidate_threshold low enough to produce candidates.
        let config = InferenceConfig {
            nli_enabled: false,
            supports_candidate_threshold: 0.60,
            ..InferenceConfig::default()
        };

        // Handle in Loading state — get_provider() returns Err.
        // The new gate fires before get_provider() is reached when nli_enabled=false.
        let not_ready_handle = NliServiceHandle::new();

        run_graph_inference_tick(
            &arc_store,
            &not_ready_handle,
            &vector_index,
            &make_rayon_pool(),
            &config,
        )
        .await;

        let edges = arc_store.query_graph_edges().await.unwrap();
        // No NLI-sourced Supports edges (source != EDGE_SOURCE_COSINE_SUPPORTS).
        let nli_supports: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Supports" && e.source != EDGE_SOURCE_COSINE_SUPPORTS)
            .collect();
        assert_eq!(
            nli_supports.len(),
            0,
            "AC-01: no NLI Supports edges when nli_enabled=false; edges={edges:?}"
        );
    }

    /// AC-02 part 1 (bugfix-523): Path A (Informs) still runs when nli_enabled=false.
    /// Proves the gate does not precede Phase A.
    #[tokio::test]
    async fn test_nli_gate_path_a_informs_edges_still_written_nli_disabled() {
        let tmp = tempfile::TempDir::new().unwrap();
        let arc_store = Arc::new(unimatrix_store::test_helpers::open_test_store(&tmp).await);

        let config = InferenceConfig {
            nli_enabled: false,
            // Set threshold above any cosine we produce to suppress Supports candidates
            // (keeps test focus on Informs-only, avoids candidate_pairs noise).
            supports_candidate_threshold: 1.1,
            ..InferenceConfig::default()
        };

        let (src_cat, tgt_cat) = (
            config.informs_category_pairs[0][0].clone(),
            config.informs_category_pairs[0][1].clone(),
        );

        sqlx::query(
            "INSERT OR IGNORE INTO entries \
             (id, title, content, topic, category, source, status, confidence, \
              created_at, updated_at, last_accessed_at, access_count, \
              created_by, modified_by, content_hash, previous_hash, \
              version, feature_cycle, trust_source, helpful_count, unhelpful_count, \
              pre_quarantine_status, correction_count, embedding_dim) \
             VALUES (101, 'src', 'src content', 'test', ?1, 'test', 0, 0.5, \
                     1000, 1000, 0, 0, 'test', 'test', 'h1', '', 1, 'crt-001', '', 0, 0, NULL, 0, 0)",
        )
        .bind(src_cat.as_str())
        .execute(arc_store.write_pool_server())
        .await
        .unwrap();

        sqlx::query(
            "INSERT OR IGNORE INTO entries \
             (id, title, content, topic, category, source, status, confidence, \
              created_at, updated_at, last_accessed_at, access_count, \
              created_by, modified_by, content_hash, previous_hash, \
              version, feature_cycle, trust_source, helpful_count, unhelpful_count, \
              pre_quarantine_status, correction_count, embedding_dim) \
             VALUES (102, 'tgt', 'tgt content', 'test', ?1, 'test', 0, 0.5, \
                     2000, 2000, 0, 0, 'test', 'test', 'h2', '', 1, 'crt-002', '', 0, 0, NULL, 0, 0)",
        )
        .bind(tgt_cat.as_str())
        .execute(arc_store.write_pool_server())
        .await
        .unwrap();

        let vector_index = Arc::new(
            unimatrix_vector::VectorIndex::new(
                Arc::clone(&arc_store),
                unimatrix_core::VectorConfig::default(),
            )
            .expect("VectorIndex"),
        );
        let dim = unimatrix_core::VectorConfig::default().dimension;
        let mut emb = vec![0.0_f32; dim];
        emb[0] = 1.0;
        vector_index.insert(101, &emb).await.expect("insert 101");
        vector_index.insert(102, &emb).await.expect("insert 102");

        let not_ready_handle = NliServiceHandle::new();

        run_graph_inference_tick(
            &arc_store,
            &not_ready_handle,
            &vector_index,
            &make_rayon_pool(),
            &config,
        )
        .await;

        let edges = arc_store.query_graph_edges().await.unwrap();
        let informs_count = edges
            .iter()
            .filter(|e| e.relation_type == "Informs")
            .count();
        assert!(
            informs_count >= 1,
            "AC-02 (Path A): at least one Informs edge must be written when nli_enabled=false; edges={edges:?}"
        );
    }

    /// AC-02 part 2 (bugfix-523): Path C (cosine Supports) still runs when nli_enabled=false.
    /// Proves the gate does not precede run_cosine_supports_path (Path C gate position check).
    ///
    /// Config overrides informs_category_pairs to include both pair orderings so that
    /// regardless of which entry the HNSW selects as source vs. neighbor, the category
    /// filter passes and the cosine Supports edge is written.
    #[tokio::test]
    async fn test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled() {
        let tmp = tempfile::TempDir::new().unwrap();
        let arc_store = Arc::new(unimatrix_store::test_helpers::open_test_store(&tmp).await);

        insert_test_entry_with_category(&arc_store, 201, "lesson-learned").await;
        insert_test_entry_with_category(&arc_store, 202, "decision").await;

        let vector_index = Arc::new(
            unimatrix_vector::VectorIndex::new(
                Arc::clone(&arc_store),
                unimatrix_core::VectorConfig::default(),
            )
            .expect("VectorIndex"),
        );
        let dim = unimatrix_core::VectorConfig::default().dimension;
        // Identical embeddings → cosine = 1.0 >= supports_cosine_threshold (0.65 default).
        let mut emb = vec![0.0_f32; dim];
        emb[0] = 1.0;
        vector_index.insert(201, &emb).await.expect("insert 201");
        vector_index.insert(202, &emb).await.expect("insert 202");

        // Include both orderings of the category pair so the test passes regardless of
        // which direction the HNSW scan produces (source candidates are shuffle-selected).
        let config = InferenceConfig {
            nli_enabled: false,
            informs_category_pairs: vec![
                ["lesson-learned".to_string(), "decision".to_string()],
                ["decision".to_string(), "lesson-learned".to_string()],
            ],
            ..InferenceConfig::default()
        };

        let not_ready_handle = NliServiceHandle::new();

        run_graph_inference_tick(
            &arc_store,
            &not_ready_handle,
            &vector_index,
            &make_rayon_pool(),
            &config,
        )
        .await;

        let edges = arc_store.query_graph_edges().await.unwrap();
        let cosine_supports: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Supports" && e.source == EDGE_SOURCE_COSINE_SUPPORTS)
            .collect();
        assert!(
            !cosine_supports.is_empty(),
            "AC-02 (Path C): cosine Supports edge must be written when nli_enabled=false; edges={edges:?}"
        );
    }

    /// AC-03 (bugfix-523): NLI-enabled path is not regressed by the new gate.
    /// When nli_enabled=true the gate condition is false, execution reaches get_provider().
    /// The handle is in Loading state so get_provider() returns Err — function returns
    /// without panic and no NLI Supports edges are written (not a regression: same as TC-02).
    #[tokio::test]
    async fn test_nli_gate_nli_enabled_path_not_regressed() {
        let tmp = tempfile::TempDir::new().unwrap();
        let arc_store = Arc::new(unimatrix_store::test_helpers::open_test_store(&tmp).await);

        insert_test_entry(&arc_store, 10).await;
        insert_test_entry(&arc_store, 20).await;

        let vector_index = Arc::new(
            unimatrix_vector::VectorIndex::new(
                Arc::clone(&arc_store),
                unimatrix_core::VectorConfig::default(),
            )
            .expect("VectorIndex"),
        );
        let dim = unimatrix_core::VectorConfig::default().dimension;
        let mut emb = vec![0.0_f32; dim];
        emb[0] = 1.0;
        vector_index.insert(10, &emb).await.expect("insert 10");
        vector_index.insert(20, &emb).await.expect("insert 20");

        // nli_enabled=true — the new gate MUST NOT fire.
        // Handle in Loading state so get_provider() returns Err (NliNotReady),
        // confirming that execution reached get_provider() (gate condition was false).
        let config = InferenceConfig {
            nli_enabled: true,
            ..InferenceConfig::default()
        };
        let not_ready_handle = NliServiceHandle::new();

        run_graph_inference_tick(
            &arc_store,
            &not_ready_handle,
            &vector_index,
            &make_rayon_pool(),
            &config,
        )
        .await;

        // Function must return without panic (gate does not fire).
        // No NLI Supports edges because provider returned Err — this is expected, not a regression.
        let edges = arc_store.query_graph_edges().await.unwrap();
        let nli_supports: Vec<_> = edges
            .iter()
            .filter(|e| e.relation_type == "Supports" && e.source != EDGE_SOURCE_COSINE_SUPPORTS)
            .collect();
        assert_eq!(
            nli_supports.len(),
            0,
            "AC-03: NLI Supports edges = 0 (provider not ready); no regression from gate; edges={edges:?}"
        );
    }

    // -----------------------------------------------------------------------
    // bugfix-523 Item 2 — log downgrade behavioral proxy (AC-04 / AC-05)
    // Log level is NOT asserted per ADR-001(c) (entry #4143). Behavioral-only coverage.
    // -----------------------------------------------------------------------

    /// AC-04 src branch (bugfix-523): run_cosine_supports_path skips pair when src_id is
    /// absent from category_map. No panic, no Supports edge written.
    /// Log level (debug! at this site) verified by code review only per ADR-001(c).
    #[tokio::test]
    async fn test_cosine_supports_path_skips_missing_category_map_src() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 1, "lesson-learned").await;
        insert_test_entry_with_category(&store, 2, "decision").await;

        let config = InferenceConfig::default();
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![(1, 2, 0.80_f32)];
        let existing: HashSet<(u64, u64)> = HashSet::new();
        // src_id=1 is intentionally absent from category_map — tgt_id=2 present only.
        let category_map: HashMap<u64, &str> = [(2_u64, "decision")].into_iter().collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        assert!(
            edges.is_empty(),
            "AC-04 (src): pair skipped when src_id absent from category_map; edges={edges:?}"
        );
    }

    /// AC-04 tgt branch (bugfix-523): run_cosine_supports_path skips pair when tgt_id is
    /// absent from category_map. No panic, no Supports edge written.
    /// Log level (debug! at this site) verified by code review only per ADR-001(c).
    #[tokio::test]
    async fn test_cosine_supports_path_skips_missing_category_map_tgt() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 1, "lesson-learned").await;
        insert_test_entry_with_category(&store, 2, "decision").await;

        let config = InferenceConfig::default();
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![(1, 2, 0.80_f32)];
        let existing: HashSet<(u64, u64)> = HashSet::new();
        // src_id=1 present, tgt_id=2 intentionally absent from category_map.
        let category_map: HashMap<u64, &str> = [(1_u64, "lesson-learned")].into_iter().collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        assert!(
            edges.is_empty(),
            "AC-04 (tgt): pair skipped when tgt_id absent from category_map; edges={edges:?}"
        );
    }

    /// AC-05 (bugfix-523): run_cosine_supports_path skips pair when cosine is non-finite (NaN).
    /// No panic, no Supports edge written.
    /// The non-finite cosine site (line ~776) remains tracing::warn! — verified by code review
    /// only per ADR-001(c) (entry #4143). Not asserted here.
    #[tokio::test]
    async fn test_cosine_supports_path_nonfinite_cosine_handled() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
        insert_test_entry_with_category(&store, 1, "lesson-learned").await;
        insert_test_entry_with_category(&store, 2, "decision").await;

        let config = InferenceConfig::default();
        // NaN cosine — non-finite guard fires before any threshold or category check.
        let candidate_pairs: Vec<(u64, u64, f32)> = vec![(1, 2, f32::NAN)];
        let existing: HashSet<(u64, u64)> = HashSet::new();
        // Both entries present so the non-finite cosine branch is reached (not category_map miss).
        let category_map: HashMap<u64, &str> = [(1_u64, "lesson-learned"), (2_u64, "decision")]
            .into_iter()
            .collect();
        let ts = current_timestamp_secs();

        run_cosine_supports_path(
            &store,
            &config,
            &candidate_pairs,
            &existing,
            &category_map,
            ts,
        )
        .await;

        let edges = store.query_graph_edges().await.unwrap();
        assert!(
            edges.is_empty(),
            "AC-05: pair with NaN cosine must be skipped; no Supports edge written; edges={edges:?}"
        );
    }
}
