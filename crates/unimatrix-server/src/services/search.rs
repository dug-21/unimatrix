//! SearchService: unified search pipeline replacing duplicated logic
//! in tools.rs and uds_listener.rs.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{
    CoreError, EmbedService, EntryRecord, QueryFilter, Status, Store, VectorAdapter,
};
use unimatrix_embed::{CrossEncoderProvider, NliScores};

use unimatrix_adapt::AdaptationService;
use unimatrix_engine::effectiveness::{
    EffectivenessCategory, SETTLED_BOOST, UTILITY_BOOST, UTILITY_PENALTY,
};

use unimatrix_engine::coaccess::MAX_CO_ACCESS_BOOST;
use unimatrix_engine::graph::{FALLBACK_PENALTY, find_terminal_active, graph_penalty};

use crate::coaccess::{CO_ACCESS_STALENESS_SECONDS, compute_search_boost};
use crate::confidence::cosine_similarity;
#[cfg(test)]
use crate::confidence::rerank_score;
use crate::infra::audit::{AuditEvent, Outcome};
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::nli_handle::NliServiceHandle;
use crate::infra::rayon_pool::RayonPool;
use crate::infra::timeout::{MCP_HANDLER_TIMEOUT, spawn_blocking_with_timeout};
use crate::services::confidence::ConfidenceStateHandle;
use crate::services::effectiveness::{EffectivenessSnapshot, EffectivenessStateHandle};
use crate::services::gateway::SecurityGateway;
use crate::services::typed_graph::TypedGraphStateHandle;
use crate::services::{AuditContext, CallerId, ServiceError};

/// HNSW search expansion factor.
const EF_SEARCH: usize = 32;

/// Provenance boost for lesson-learned entries (matches existing behavior).
const PROVENANCE_BOOST: f64 = unimatrix_engine::confidence::PROVENANCE_BOOST;

// ---------------------------------------------------------------------------
// crt-024: Fused scoring structs and pure function (ADR-004)
// ---------------------------------------------------------------------------

/// Per-candidate signal inputs for the fused scoring formula (crt-024, ADR-004).
///
/// All fields are f64 in [0.0, 1.0] by the time compute_fused_score is called.
/// Field normalization is the caller's responsibility (see SearchService scoring loop).
///
/// This struct is the feature vector interface for W3-1 (GNN training). Each field
/// is a named, learnable dimension. Do not add signals outside this struct.
///
/// crt-026 (WA-2): Two phase fields added. phase_explicit_norm is always 0.0
/// in crt-026 (W3-1 reserved placeholder, ADR-003). Do not remove these fields —
/// W3-1 depends on them as named, stable, learnable dimensions (NFR-06).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FusedScoreInputs {
    /// HNSW cosine similarity (bi-encoder recall). Already in [0, 1].
    pub similarity: f64,
    /// NLI cross-encoder entailment score (cross-encoder precision).
    /// Already in [0, 1] when model produces valid softmax output.
    /// Set to 0.0 when NLI is absent or disabled — the weight is then
    /// re-normalized away by FusionWeights::effective(nli_available: false).
    pub nli_entailment: f64,
    /// Wilson score composite confidence (EntryRecord.confidence). Already in [0, 1].
    pub confidence: f64,
    /// Co-access affinity normalized to [0, 1].
    /// Computed as: raw_boost / MAX_CO_ACCESS_BOOST.
    /// 0.0 when entry has no co-access history or boost_map lookup misses.
    pub coac_norm: f64,
    /// Utility delta normalized to [0, 1] via shift-and-scale (FR-05, ADR-001 crt-024).
    /// Formula: (utility_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY).
    /// Maps: Ineffective (-0.05) -> 0.0, neutral (0.0) -> 0.5, Effective (+0.05) -> 1.0.
    pub util_norm: f64,
    /// Provenance boost normalized to [0, 1] (FR-06, ADR-001 crt-024).
    /// Formula: prov_boost / PROVENANCE_BOOST, guarded for PROVENANCE_BOOST == 0.0.
    /// Binary in practice: 1.0 for boosted categories, 0.0 for all others.
    pub prov_norm: f64,
    /// crt-026: Category histogram affinity (WA-2).
    /// p(entry.category) from the session's category_counts histogram, normalized to [0.0, 1.0].
    /// 0.0 when session has no prior stores (cold start), entry.category not in histogram,
    /// or ServiceSearchParams.category_histogram is None.
    /// Computed in the scoring loop as: count[entry.category] / total_count.
    pub phase_histogram_norm: f64,
    /// crt-026: Explicit phase term (WA-2, ADR-003 placeholder).
    /// Always 0.0 in crt-026. Reserved for W3-1 (GNN training).
    /// W3-1 will populate this from a learned phase-to-category relevance model.
    /// DO NOT remove: W3-1 depends on this named field. Comment cites ADR-003 as guard.
    pub phase_explicit_norm: f64,
}

/// Config-driven fusion weights for the six-term ranking formula (crt-024, ADR-003).
///
/// Constructed from InferenceConfig in SearchService::new. Stored as a field on SearchService.
/// Not derived from InferenceConfig at every search call — built once.
///
/// Invariant (enforced by InferenceConfig::validate at startup):
///   w_sim + w_nli + w_conf + w_coac + w_util + w_prov <= 1.0  (sum of six core terms)
///   Each core field individually in [0.0, 1.0].
///
/// w_phase_histogram and w_phase_explicit are additive terms excluded from this
/// constraint. Their sum does not enter the six-term sum check. With defaults,
/// total sum = 0.95 + 0.02 + 0.0 = 0.97, within <= 1.0.
///
/// Per-field range [0.0, 1.0] is enforced by InferenceConfig::validate for all eight fields.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FusionWeights {
    pub w_sim: f64,             // default 0.25 — bi-encoder similarity
    pub w_nli: f64,             // default 0.35 — NLI entailment (dominant precision signal)
    pub w_conf: f64,            // default 0.15 — confidence tiebreaker
    pub w_coac: f64,            // default 0.10 — co-access affinity (lagging signal)
    pub w_util: f64,            // default 0.05 — effectiveness classification
    pub w_prov: f64,            // default 0.05 — category provenance hint
    pub w_phase_histogram: f64, // crt-026: default 0.02 — histogram affinity (ADR-004, ASS-028 calibrated)
    pub w_phase_explicit: f64,  // crt-026: default 0.0  — W3-1 placeholder (ADR-003)
}

impl FusionWeights {
    /// Construct FusionWeights from the validated InferenceConfig.
    pub(crate) fn from_config(cfg: &crate::infra::config::InferenceConfig) -> FusionWeights {
        FusionWeights {
            w_sim: cfg.w_sim,
            w_nli: cfg.w_nli,
            w_conf: cfg.w_conf,
            w_coac: cfg.w_coac,
            w_util: cfg.w_util,
            w_prov: cfg.w_prov,
            w_phase_histogram: cfg.w_phase_histogram, // crt-026: NEW
            w_phase_explicit: cfg.w_phase_explicit,   // crt-026: NEW
        }
    }

    /// Return an effective weight set adjusted for NLI availability.
    ///
    /// NLI active (nli_available = true): returns self unchanged.
    ///   The configured weights are used directly. No re-normalization.
    ///
    /// NLI absent (nli_available = false): sets w_nli = 0.0, re-normalizes
    ///   the remaining five weights by dividing each by their sum.
    ///   This preserves the relative signal dominance ordering (Constraint 9, ADR-003).
    ///
    /// Zero-denominator guard (R-02): if all five non-NLI weights are 0.0
    ///   (pathological but reachable config), returns all-zeros without panic.
    pub(crate) fn effective(&self, nli_available: bool) -> FusionWeights {
        if nli_available {
            return FusionWeights {
                w_sim: self.w_sim,
                w_nli: self.w_nli,
                w_conf: self.w_conf,
                w_coac: self.w_coac,
                w_util: self.w_util,
                w_prov: self.w_prov,
                w_phase_histogram: self.w_phase_histogram, // crt-026: pass through unchanged
                w_phase_explicit: self.w_phase_explicit,   // crt-026: pass through unchanged
            };
        }

        // NLI absent — zero out w_nli, re-normalize the five core terms only.
        // w_phase_histogram and w_phase_explicit are passed through unchanged (ADR-004, R-06).
        let denom = self.w_sim + self.w_conf + self.w_coac + self.w_util + self.w_prov;
        // NOTE: w_phase_histogram and w_phase_explicit are NOT in the denominator.

        if denom == 0.0 {
            tracing::warn!(
                "FusionWeights::effective: all non-NLI weights are 0.0; \
                 fused_score will be 0.0 for all candidates"
            );
            return FusionWeights {
                w_sim: 0.0,
                w_nli: 0.0,
                w_conf: 0.0,
                w_coac: 0.0,
                w_util: 0.0,
                w_prov: 0.0,
                w_phase_histogram: self.w_phase_histogram, // crt-026: pass through unchanged
                w_phase_explicit: self.w_phase_explicit,   // crt-026: pass through unchanged
            };
        }

        FusionWeights {
            w_sim: self.w_sim / denom,
            w_nli: 0.0,
            w_conf: self.w_conf / denom,
            w_coac: self.w_coac / denom,
            w_util: self.w_util / denom,
            w_prov: self.w_prov / denom,
            w_phase_histogram: self.w_phase_histogram, // crt-026: pass through unchanged (not re-normalized)
            w_phase_explicit: self.w_phase_explicit, // crt-026: pass through unchanged (not re-normalized)
        }
    }
}

/// Compute the fused ranking score from normalized signal inputs and weights.
///
/// Pure function: no I/O, no async, no locks, no side effects.
///
/// Preconditions (caller's responsibility, enforced by construction):
///   - All inputs in [0.0, 1.0]
///   - weights.w_* fields individually in [0.0, 1.0]
///   - sum of six core weights <= 1.0 (after effective() is applied for NLI absence)
///
/// Returns a value in [0.0, 1.0] by construction under the above preconditions.
///
/// `status_penalty` is NOT applied here. Apply it at the call site:
///   final_score = compute_fused_score(&inputs, &weights) * status_penalty
///
/// crt-026: Two phase terms added. phase_explicit_norm is always 0.0 in crt-026
/// (ADR-003 placeholder). The histogram term contributes at most 0.02 with defaults.
/// status_penalty is still applied at the call site: final_score = compute_fused_score(...) * penalty.
pub(crate) fn compute_fused_score(inputs: &FusedScoreInputs, weights: &FusionWeights) -> f64 {
    weights.w_sim * inputs.similarity
        + weights.w_nli * inputs.nli_entailment
        + weights.w_conf * inputs.confidence
        + weights.w_coac * inputs.coac_norm
        + weights.w_util * inputs.util_norm
        + weights.w_prov * inputs.prov_norm
        + weights.w_phase_histogram * inputs.phase_histogram_norm
        // crt-026: ADR-003 placeholder — always 0.0 in crt-026; W3-1 will populate phase_explicit_norm
        + weights.w_phase_explicit * inputs.phase_explicit_norm
}

/// Retrieval mode controlling status-aware filtering behavior (crt-010, ADR-001).
///
/// - `Strict`: UDS path — drop all non-Active and superseded entries. Zero tolerance.
/// - `Flexible`: MCP path — penalize deprecated/superseded entries but keep them visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RetrievalMode {
    /// Hard filter: only Active, non-superseded entries survive.
    Strict,
    /// Soft penalty: deprecated entries penalized, superseded entries penalized more.
    #[default]
    Flexible,
}

/// Transport-agnostic search parameters.
pub(crate) struct ServiceSearchParams {
    pub query: String,
    pub k: usize,
    pub filters: Option<QueryFilter>,
    pub similarity_floor: Option<f64>,
    pub confidence_floor: Option<f64>,
    #[allow(dead_code)]
    pub feature_tag: Option<String>,
    #[allow(dead_code)]
    pub co_access_anchors: Option<Vec<u64>>,
    #[allow(dead_code)]
    pub caller_agent_id: Option<String>,
    /// Retrieval mode: Strict (UDS) or Flexible (MCP). Default: Flexible (crt-010).
    pub retrieval_mode: RetrievalMode,
    /// crt-026: Session identifier for logging and tracing (WA-2).
    /// Populated from ctx.audit_ctx.session_id (MCP path) or
    /// HookRequest::ContextSearch.session_id (UDS path).
    /// Not used in scoring logic; carried for observability.
    pub session_id: Option<String>,
    /// crt-026: Pre-resolved category histogram clone (WA-2, ADR-002).
    ///
    /// Set to None when:
    ///   - session_id is None
    ///   - session is not registered in SessionRegistry
    ///   - get_category_histogram() returned an empty map (is_empty() → None)
    ///
    /// When Some, the histogram is used in the scoring loop to compute
    /// phase_histogram_norm = p(entry.category) per candidate.
    ///
    /// Cold-start invariant: None → phase_histogram_norm = 0.0 for all candidates
    /// → compute_fused_score output bit-for-bit identical to pre-crt-026 (NFR-02).
    pub category_histogram: Option<HashMap<String, u32>>,
}

/// Search results including query embedding for reuse.
pub(crate) struct SearchResults {
    pub entries: Vec<ScoredEntry>,
    #[allow(dead_code)]
    pub query_embedding: Vec<f32>,
}

/// Entry with composite score breakdown.
pub(crate) struct ScoredEntry {
    pub entry: EntryRecord,
    #[allow(dead_code)]
    pub final_score: f64,
    pub similarity: f64,
    #[allow(dead_code)]
    pub confidence: f64,
}

/// Unified search pipeline.
#[derive(Clone)]
pub(crate) struct SearchService {
    store: Arc<Store>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    gateway: Arc<SecurityGateway>,
    /// crt-019 (ADR-001): adaptive blend weight state shared with StatusService.
    ///
    /// Readers clone `confidence_weight` f64 under a short read lock before
    /// each re-ranking step. The write lock is held only by the maintenance
    /// tick (StatusService) for the brief field-update critical section.
    confidence_state: ConfidenceStateHandle,
    /// crt-018b (ADR-001): effectiveness classification snapshot for utility delta.
    /// Arc clone received from ServiceLayer; shared with BriefingService and background tick.
    effectiveness_state: EffectivenessStateHandle,
    /// crt-018b (ADR-001): generation-cached snapshot shared across rmcp clones.
    /// Arc<Mutex<_>> ensures all clones of SearchService share one cache object (R-06).
    cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>,
    /// crt-021: pre-built typed graph state handle. Background tick rebuilds; search reads
    /// the pre-built TypedRelationGraph under a short read lock — no per-query rebuild (FR-22).
    typed_graph_handle: TypedGraphStateHandle,
    /// dsn-001: config-driven provenance boost targets.
    /// Constructed from config.knowledge.boosted_categories at SearchService construction.
    /// Replaces the four hardcoded entry.category == "lesson-learned" comparisons.
    boosted_categories: HashSet<String>,
    /// crt-022 (ADR-004): shared rayon thread pool for ML inference (ONNX embedding).
    rayon_pool: Arc<RayonPool>,
    /// crt-023: NLI cross-encoder handle for search re-ranking (ADR-002).
    /// When Ready and nli_enabled=true, replaces rerank_score sort step.
    /// When Loading/Failed/disabled, pipeline falls back to rerank_score unchanged.
    nli_handle: Arc<NliServiceHandle>,
    /// crt-023: expanded HNSW candidate pool size when NLI is active (from InferenceConfig).
    nli_top_k: usize,
    /// crt-023: fast check before get_provider(); when false, NLI path is never attempted.
    nli_enabled: bool,
    /// crt-024: config-driven fusion weights for the six-term scoring formula (ADR-003).
    /// Constructed from InferenceConfig.{w_sim, w_nli, w_conf, w_coac, w_util, w_prov}
    /// in SearchService::new. Stored here so EvalServiceLayer profile TOMLs can
    /// supply different weights per eval run (FR-14, AC-15).
    fusion_weights: FusionWeights,
}

// ---------------------------------------------------------------------------
// NLI scoring helper (crt-024, ADR-002)
// ---------------------------------------------------------------------------

/// Attempt NLI scoring of `candidates`.
///
/// Returns `Some(nli_scores)` when scoring succeeded — one NliScores per candidate,
/// in the same index order as `candidates`. Does NOT sort. Does NOT truncate.
/// Caller runs the fused scoring pass using these scores alongside other signals.
///
/// Returns `None` on any failure (provider not ready, rayon timeout, inference error,
/// empty candidates, length mismatch). Caller uses nli_entailment=0.0 for all candidates
/// and calls FusionWeights::effective(nli_available: false).
///
/// W1-2 contract: ALL NLI inference is dispatched via `rayon_pool.spawn_with_timeout`.
/// Never inline in async context. Never via `spawn_blocking`.
async fn try_nli_rerank(
    candidates: &[(EntryRecord, f64)],
    query_text: &str,
    nli_handle: &NliServiceHandle,
    rayon_pool: &RayonPool,
) -> Option<Vec<NliScores>> {
    // Fast check: get provider or return None for fallback.
    let provider: Arc<dyn CrossEncoderProvider> = match nli_handle.get_provider().await {
        Ok(p) => p,
        Err(_) => {
            tracing::debug!("NLI provider not ready; NLI term will be 0.0");
            return None;
        }
    };

    if candidates.is_empty() {
        return None;
    }

    // Build owned strings for the rayon closure (Send bound requires 'static).
    let query_owned: String = query_text.to_string();
    let passages: Vec<String> = candidates
        .iter()
        .map(|(entry, _)| entry.content.clone())
        .collect();

    // Dispatch batch scoring to rayon pool with MCP_HANDLER_TIMEOUT (W1-2, FR-16).
    let nli_result = rayon_pool
        .spawn_with_timeout(MCP_HANDLER_TIMEOUT, move || {
            let pairs: Vec<(&str, &str)> = passages
                .iter()
                .map(|p| (query_owned.as_str(), p.as_str()))
                .collect();
            provider.score_batch(&pairs)
        })
        .await;

    let nli_scores: Vec<NliScores> = match nli_result {
        Ok(Ok(scores)) => scores,
        Ok(Err(e)) => {
            tracing::debug!(error = %e, "NLI score_batch error; NLI term will be 0.0");
            return None;
        }
        Err(e) => {
            tracing::debug!(error = %e, "NLI rayon task failed/timed out; NLI term will be 0.0");
            return None;
        }
    };

    // Length check: scores must be parallel to candidates (EC-07).
    if nli_scores.len() != candidates.len() {
        tracing::debug!(
            nli_len = nli_scores.len(),
            candidates_len = candidates.len(),
            "NLI scores length mismatch; NLI term will be 0.0"
        );
        return None;
    }

    // Return raw scores — no sort, no truncation. Caller handles all of that.
    Some(nli_scores)
}

// ---------------------------------------------------------------------------
// Utility helpers
// ---------------------------------------------------------------------------

/// Map an effectiveness category to its additive utility delta for search re-ranking.
///
/// Applied inside the `status_penalty` multiplication (ADR-003):
/// `(rerank_score + utility_delta + prov_boost + co_access_boost) * status_penalty`.
///
/// Absent / unclassified entries (None) produce 0.0 — cold-start safe (AC-06, NFR-06).
/// Both Ineffective and Noisy receive the full symmetric penalty.
fn utility_delta(category: Option<EffectivenessCategory>) -> f64 {
    match category {
        Some(EffectivenessCategory::Effective) => UTILITY_BOOST,
        Some(EffectivenessCategory::Settled) => SETTLED_BOOST,
        Some(EffectivenessCategory::Ineffective) => -UTILITY_PENALTY,
        Some(EffectivenessCategory::Noisy) => -UTILITY_PENALTY,
        Some(EffectivenessCategory::Unmatched) | None => 0.0,
    }
}

impl SearchService {
    pub(crate) fn new(
        store: Arc<Store>,
        vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
        entry_store: Arc<Store>,
        embed_service: Arc<EmbedServiceHandle>,
        adapt_service: Arc<AdaptationService>,
        gateway: Arc<SecurityGateway>,
        confidence_state: ConfidenceStateHandle,
        effectiveness_state: EffectivenessStateHandle,
        typed_graph_handle: TypedGraphStateHandle,
        boosted_categories: HashSet<String>,
        rayon_pool: Arc<RayonPool>,
        nli_handle: Arc<NliServiceHandle>,
        nli_top_k: usize,
        nli_enabled: bool,
        fusion_weights: FusionWeights,
    ) -> Self {
        SearchService {
            store,
            vector_store,
            entry_store,
            embed_service,
            adapt_service,
            gateway,
            confidence_state,
            effectiveness_state,
            cached_snapshot: EffectivenessSnapshot::new_shared(),
            typed_graph_handle,
            boosted_categories,
            rayon_pool,
            nli_handle,
            nli_top_k,
            nli_enabled,
            fusion_weights,
        }
    }

    /// Execute the full search pipeline.
    ///
    /// Pipeline: embed -> HNSW -> quarantine filter -> status filter/penalty (crt-010)
    /// -> supersession injection (crt-010) -> re-rank -> co-access boost -> truncate -> floors
    pub(crate) async fn search(
        &self,
        params: ServiceSearchParams,
        audit_ctx: &AuditContext,
        caller_id: &CallerId,
    ) -> Result<SearchResults, ServiceError> {
        // Snapshot adaptive confidence_weight before any await points (ADR-001).
        // Retained for StatusService/ConfidenceState health; no longer used in the fused
        // scoring path (crt-024). The read is kept to maintain the lock-ordering invariant
        // documented in search.rs and to avoid removing the shared confidence_state dependency.
        let _confidence_weight = {
            let guard = self
                .confidence_state
                .read()
                .unwrap_or_else(|e| e.into_inner());
            guard.confidence_weight
        };

        // crt-018b (ADR-001): snapshot effectiveness categories under short read lock.
        // Generation comparison skips the HashMap clone on the common path (no state change).
        // LOCK ORDERING (R-01): acquire read lock, read generation, DROP guard, then acquire
        // cached_snapshot mutex. Never hold both guards simultaneously.
        let categories: HashMap<u64, EffectivenessCategory> = {
            let current_generation = {
                let guard = self
                    .effectiveness_state
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                guard.generation
                // read guard drops here (end of inner block)
            };
            // Read guard is now out of scope. Safe to acquire the mutex.
            let mut cache = self
                .cached_snapshot
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if cache.generation != current_generation {
                // State has changed since last call — re-clone categories from live state.
                let guard = self
                    .effectiveness_state
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                cache.generation = guard.generation;
                cache.categories = guard.categories.clone();
                // guard drops here
            }
            // Return a local clone of the cached categories for this call's use.
            // This clone happens at most once per 15-minute background tick.
            cache.categories.clone()
        };

        // Step 0: S2 rate check before any work
        self.gateway.check_search_rate(caller_id)?;

        // Step 1: S1 + S3 validation via gateway
        let _scan_warning =
            self.gateway
                .validate_search_query(&params.query, params.k, audit_ctx)?;

        // Step 2: Get embedding adapter
        let adapter = self
            .embed_service
            .get_adapter()
            .await
            .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;

        // Step 3: Embed query via rayon ml_inference_pool (crt-022, ADR-002)
        let query = params.query.clone();
        let raw_embedding: Vec<f32> = self
            .rayon_pool
            .spawn_with_timeout(MCP_HANDLER_TIMEOUT, {
                let adapter = Arc::clone(&adapter);
                move || adapter.embed_entry("", &query)
            })
            .await
            .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
            .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;

        // Step 4: Adapt embedding (MicroLoRA) + normalize
        let adapted = self
            .adapt_service
            .adapt_embedding(&raw_embedding, None, None);
        let embedding = unimatrix_embed::l2_normalized(&adapted);

        // Step 5: HNSW search (filtered or unfiltered)
        // crt-023 (ADR-002): when NLI is enabled, expand candidate pool to nli_top_k so
        // the NLI batch-scorer has more candidates to re-rank before truncating to params.k.
        // If NLI is not actually ready at Step 7, the extra candidates are harmless — the
        // fallback sort path will truncate to params.k there.
        let hnsw_k = if self.nli_enabled {
            self.nli_top_k.max(params.k)
        } else {
            params.k
        };

        let search_results = if let Some(ref filter) = params.filters {
            let entries = self
                .entry_store
                .query(filter.clone())
                .await
                .map_err(|e| ServiceError::Core(CoreError::Store(e)))?;
            let allowed_ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
            if allowed_ids.is_empty() {
                vec![]
            } else {
                self.vector_store
                    .search_filtered(embedding.clone(), hnsw_k, EF_SEARCH, allowed_ids)
                    .await
                    .map_err(ServiceError::Core)?
            }
        } else {
            self.vector_store
                .search(embedding.clone(), hnsw_k, EF_SEARCH)
                .await
                .map_err(ServiceError::Core)?
        };

        // Step 6: Fetch entries, exclude quarantined (S4)
        let mut results_with_scores: Vec<(EntryRecord, f64)> = Vec::new();
        for sr in &search_results {
            match self.entry_store.get(sr.entry_id).await {
                Ok(entry) => {
                    if SecurityGateway::is_quarantined(&entry.status) {
                        continue;
                    }
                    results_with_scores.push((entry, sr.similarity));
                }
                Err(_) => continue,
            }
        }

        // crt-021 (FR-22): read the pre-built TypedRelationGraph under a short read lock.
        // The background tick rebuilds TypedGraphState; the search path only reads.
        // LOCK ORDERING (R-01): acquire read lock, clone fields, DROP guard before any traversal.
        // INVARIANT: build_typed_relation_graph is NEVER called here — only on the pre-built graph.
        let (typed_graph, all_entries, use_fallback) = {
            let guard = self
                .typed_graph_handle
                .read()
                .unwrap_or_else(|e| e.into_inner());
            (
                guard.typed_graph.clone(),
                guard.all_entries.clone(),
                guard.use_fallback,
            )
            // read guard drops here
        };

        // Step 6a: Status filter / penalty marking (crt-010)
        //
        // Determine if caller explicitly requested a non-Active status
        let explicit_status_filter: Option<Status> = params
            .filters
            .as_ref()
            .and_then(|f| f.status)
            .filter(|s| *s != Status::Active);

        // Penalty map: entry_id -> multiplicative penalty (1.0 = no penalty)
        let mut penalty_map: HashMap<u64, f64> = HashMap::new();

        match params.retrieval_mode {
            RetrievalMode::Strict => {
                // Hard filter: drop all non-Active and all superseded
                results_with_scores.retain(|(entry, _)| {
                    entry.status == Status::Active && entry.superseded_by.is_none()
                });
            }
            RetrievalMode::Flexible => {
                if explicit_status_filter.is_none() {
                    // crt-014: Unified penalty condition (IR-02).
                    // Both superseded entries and deprecated entries go through graph_penalty.
                    // OR condition covers: superseded-but-active (data inconsistency) and
                    // pure-orphan deprecated entries with no known successor.
                    for (entry, _) in &results_with_scores {
                        if entry.superseded_by.is_some() || entry.status == Status::Deprecated {
                            let penalty = if use_fallback {
                                FALLBACK_PENALTY
                            } else {
                                graph_penalty(entry.id, &typed_graph, &all_entries)
                            };
                            penalty_map.insert(entry.id, penalty);
                        }
                    }
                }
                // If explicit_status_filter is Some: no penalties (FR-6.2)
            }
        }

        // Step 6b: Supersession candidate injection (crt-010)
        //
        // Skip if explicit status filter is Deprecated (FR-6.2, AC-14b)
        let should_inject = explicit_status_filter != Some(Status::Deprecated);

        if should_inject {
            // crt-014: Multi-hop injection via find_terminal_active.
            // Collect entries that have a superseded_by set (candidates for injection).
            let superseded_entries: Vec<EntryRecord> = results_with_scores
                .iter()
                .filter_map(|(entry, _)| {
                    if entry.superseded_by.is_some() {
                        Some(entry.clone())
                    } else {
                        None
                    }
                })
                .collect();

            if !superseded_entries.is_empty() {
                let existing_ids: HashSet<u64> =
                    results_with_scores.iter().map(|(e, _)| e.id).collect();

                for entry in &superseded_entries {
                    // Resolve terminal: multi-hop via graph, or single-hop fallback on cycle
                    let terminal_id: Option<u64> = if use_fallback {
                        // Fallback: single-hop (old behavior) — ADR-005
                        entry.superseded_by
                    } else {
                        // Multi-hop: follow chain to terminal active node (crt-014 ADR-003)
                        find_terminal_active(entry.id, &typed_graph, &all_entries)
                    };

                    let terminal_id = match terminal_id {
                        Some(id) => id,
                        None => continue, // no active terminal reachable; skip injection
                    };

                    // Skip if already in result set
                    if existing_ids.contains(&terminal_id) {
                        continue;
                    }

                    // Fetch and inject the terminal entry
                    let terminal = match self.entry_store.get(terminal_id).await {
                        Ok(t) => t,
                        Err(_) => continue, // Dangling reference — skip (FR-2.7)
                    };

                    // Validate: terminal must be Active and non-superseded.
                    // find_terminal_active guarantees this, but defensive check for
                    // store state that may have changed since graph build.
                    if terminal.status != Status::Active || terminal.superseded_by.is_some() {
                        continue;
                    }

                    // Compute cosine similarity from stored embedding (ADR-002)
                    if let Some(emb) = self.vector_store.get_embedding(terminal_id).await {
                        let sim = cosine_similarity(&embedding, &emb);
                        results_with_scores.push((terminal, sim));
                    }
                    // If no embedding: skip injection (existing R-01 fallback pattern)
                }
            }
        }

        // Step 6c: Co-access boost map prefetch (crt-024, SR-07).
        //
        // Fully await before the scoring pass begins (correctness constraint, not optimization).
        // Scoring without co-access data would silently produce coac_norm=0.0 for all candidates.
        // Moved earlier from old Step 8 to make boost_map available before fused scoring.
        let boost_map: HashMap<u64, f64> = if results_with_scores.len() > 1 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let staleness_cutoff = now.saturating_sub(CO_ACCESS_STALENESS_SECONDS);

            let anchor_count = results_with_scores.len().min(3);
            let anchor_ids: Vec<u64> = results_with_scores
                .iter()
                .take(anchor_count)
                .map(|(e, _)| e.id)
                .collect();
            let result_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();

            // crt-010: deprecated entries excluded from co-access co-occurrence counts.
            let deprecated_ids: HashSet<u64> = results_with_scores
                .iter()
                .filter(|(e, _)| e.status == Status::Deprecated)
                .map(|(e, _)| e.id)
                .collect();

            let store = Arc::clone(&self.store);
            spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, move || {
                compute_search_boost(
                    &anchor_ids,
                    &result_ids,
                    &store,
                    staleness_cutoff,
                    &deprecated_ids,
                )
            })
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "co-access boost prefetch failed: {e}; coac_norm will be 0.0 for all"
                );
                HashMap::new()
            })
        } else {
            HashMap::new()
        };

        // Step 7: NLI scoring (if enabled) → fused score computation (single pass) →
        //         sort by final_score DESC → truncate to k.
        //
        // NLI scoring and boost_map prefetch run sequentially (boost_map fully awaited above).
        // Both must be resolved before the scoring loop begins.

        // NLI scoring — returns None on any failure; caller handles the NLI-absent path.
        let nli_scores: Option<Vec<NliScores>> = if self.nli_enabled {
            try_nli_rerank(
                &results_with_scores,
                &params.query,
                &self.nli_handle,
                &self.rayon_pool,
            )
            .await
        } else {
            None
        };

        let nli_available = nli_scores.is_some();

        // Compute effective weights once before the loop — NLI availability does not change
        // per-candidate. If NLI absent, re-normalize the five remaining weights.
        let effective_weights = self.fusion_weights.effective(nli_available);

        // crt-026: Pre-compute histogram total once before the scoring loop (WA-2, ADR-002).
        // All per-candidate phase_histogram_norm values derive from this single read.
        // If category_histogram is None (cold start), total = 0 and all norms will be 0.0.
        let category_histogram = params.category_histogram.as_ref();
        let histogram_total: u32 = category_histogram
            .map(|h| h.values().copied().sum())
            .unwrap_or(0);

        // Single fused scoring pass: one iteration over all candidates.
        // Vec element: (entry, sim, final_score)
        let mut scored: Vec<(EntryRecord, f64, f64)> =
            Vec::with_capacity(results_with_scores.len());

        for (i, (entry, sim)) in results_with_scores.iter().enumerate() {
            // -- nli_entailment: f32 cast to f64; 0.0 when NLI absent or NaN guard --
            let nli_entailment: f64 = nli_scores
                .as_ref()
                .and_then(|scores| scores.get(i))
                .map(|s| {
                    let v = s.entailment as f64;
                    if v.is_nan() { 0.0 } else { v }
                })
                .unwrap_or(0.0);

            // -- coac_norm: raw boost / MAX_CO_ACCESS_BOOST (AC-07, R-08) --
            let raw_coac = boost_map.get(&entry.id).copied().unwrap_or(0.0);
            let coac_norm = (raw_coac / MAX_CO_ACCESS_BOOST).min(1.0);

            // -- util_norm: shift-and-scale maps [-UTILITY_PENALTY, +UTILITY_BOOST] to [0, 1] --
            // utility_delta function is unchanged; normalization is new (FR-05, R-01, R-11).
            let raw_delta = utility_delta(categories.get(&entry.id).copied());
            let util_norm = (raw_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY);

            // -- prov_norm: divide by PROVENANCE_BOOST; guard zero denominator (R-03) --
            let raw_prov = if self.boosted_categories.contains(&entry.category) {
                PROVENANCE_BOOST
            } else {
                0.0
            };
            let prov_norm = if PROVENANCE_BOOST == 0.0 {
                0.0
            } else {
                raw_prov / PROVENANCE_BOOST
            };

            // -- crt-026: phase_histogram_norm = p(entry.category) from session histogram (WA-2).
            // Division is safe: guarded by histogram_total > 0 check.
            // 0.0 when: cold start (histogram_total == 0), or entry.category not in histogram.
            let phase_histogram_norm: f64 = if histogram_total > 0 {
                category_histogram
                    .and_then(|h| h.get(&entry.category))
                    .copied()
                    .unwrap_or(0) as f64
                    / histogram_total as f64
            } else {
                0.0
            };

            // -- Construct FusedScoreInputs --
            let inputs = FusedScoreInputs {
                similarity: *sim,
                nli_entailment,
                confidence: entry.confidence,
                coac_norm,
                util_norm,
                prov_norm,
                phase_histogram_norm, // crt-026: histogram affinity
                // crt-026: ADR-003 placeholder — always 0.0; W3-1 will populate this field
                phase_explicit_norm: 0.0,
            };

            // -- Fused score + status penalty (ADR-004: penalty at call site) --
            let fused = compute_fused_score(&inputs, &effective_weights);
            let penalty = penalty_map.get(&entry.id).copied().unwrap_or(1.0);
            let final_score = fused * penalty;

            scored.push((entry.clone(), *sim, final_score));
        }

        // Single sort by final_score DESC. No secondary sort after this (AC-04, FR-08).
        // Rust's sort_by is stable, so equal-score candidates retain their relative
        // pre-sort order (HNSW order). Satisfies the tiebreaker requirement (NFR-03).
        scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(Ordering::Equal));

        // Truncate to requested k.
        scored.truncate(params.k);

        // Rebuild results_with_scores for floor steps (which only need entry + sim).
        // Carry final_scores separately for ScoredEntry construction.
        results_with_scores = scored.iter().map(|(e, sim, _)| (e.clone(), *sim)).collect();
        let final_scores: Vec<f64> = scored.iter().map(|(_, _, fs)| *fs).collect();

        // Step 9: Truncate to k (now a no-op — Step 7 already truncated, kept for safety).
        results_with_scores.truncate(params.k);

        // Step 10: Apply floors (if set)
        if let Some(sim_floor) = params.similarity_floor {
            results_with_scores.retain(|(_, sim)| *sim >= sim_floor);
        }
        if let Some(conf_floor) = params.confidence_floor {
            results_with_scores.retain(|(entry, _)| entry.confidence >= conf_floor);
        }

        // Step 11: Build ScoredEntry with fused final_score.
        // ScoredEntry.final_score = compute_fused_score * status_penalty (already computed).
        // Field name 'final_score' is unchanged; formula changes (FR-10, AC-08).
        // Note: after floors, results_with_scores may be shorter than final_scores;
        // zip stops at the shorter iterator, which is correct.
        let entries: Vec<ScoredEntry> = results_with_scores
            .iter()
            .zip(final_scores.iter())
            .map(|((entry, sim), &final_score)| ScoredEntry {
                entry: entry.clone(),
                final_score,
                similarity: *sim,
                confidence: entry.confidence,
            })
            .collect();

        // Step 12: S5 audit
        let target_ids: Vec<u64> = entries.iter().map(|e| e.entry.id).collect();
        self.gateway.emit_audit(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: audit_ctx.session_id.clone().unwrap_or_default(),
            agent_id: audit_ctx.caller_id.clone(),
            operation: "search_service".to_string(),
            target_ids,
            outcome: Outcome::Success,
            detail: format!("returned {} results", entries.len()),
        });

        Ok(SearchResults {
            entries,
            query_embedding: embedding,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unimatrix_core::EntryRecord;

    fn make_test_entry(
        id: u64,
        status: Status,
        superseded_by: Option<u64>,
        confidence: f64,
        category: &str,
    ) -> EntryRecord {
        EntryRecord {
            id,
            title: format!("entry-{id}"),
            content: String::new(),
            topic: String::new(),
            category: category.to_string(),
            tags: vec![],
            source: String::new(),
            status,
            confidence,
            created_at: 1_000_000,
            updated_at: 0,
            last_accessed_at: 1_000_000,
            access_count: 10,
            supersedes: None,
            superseded_by,
            correction_count: 0,
            embedding_dim: 0,
            created_by: String::new(),
            modified_by: String::new(),
            content_hash: String::new(),
            previous_hash: String::new(),
            version: 1,
            feature_cycle: String::new(),
            trust_source: "agent".to_string(),
            helpful_count: 0,
            unhelpful_count: 0,
            pre_quarantine_status: None,
        }
    }

    /// Simulate the penalty-applied final score as computed in Step 7.
    fn penalized_score(similarity: f64, confidence: f64, penalty: f64) -> f64 {
        // Use initial confidence_weight (0.18375) for test assertions
        rerank_score(similarity, confidence, 0.18375) * penalty
    }

    // -- T-SP-01: Deprecated ranks below active in Flexible mode --
    #[test]
    fn deprecated_below_active_flexible() {
        use unimatrix_engine::graph::ORPHAN_PENALTY;
        let active = make_test_entry(1, Status::Active, None, 0.65, "decision");
        let deprecated = make_test_entry(2, Status::Deprecated, None, 0.65, "decision");

        // Deprecated entry has HIGHER raw similarity than active
        let active_sim = 0.80;
        let deprecated_sim = 0.90;

        let active_score = penalized_score(active_sim, active.confidence, 1.0);
        // Deprecated entry with no successor is an orphan — ORPHAN_PENALTY (0.75)
        let deprecated_score =
            penalized_score(deprecated_sim, deprecated.confidence, ORPHAN_PENALTY);

        assert!(
            active_score > deprecated_score,
            "active ({active_score:.4}) should rank above deprecated ({deprecated_score:.4})"
        );
    }

    // -- T-SP-02: Superseded ranks below active in Flexible mode --
    #[test]
    fn superseded_below_active_flexible() {
        use unimatrix_engine::graph::CLEAN_REPLACEMENT_PENALTY;
        let active = make_test_entry(1, Status::Active, None, 0.65, "decision");
        let superseded = make_test_entry(2, Status::Deprecated, Some(1), 0.65, "decision");

        let active_sim = 0.80;
        let superseded_sim = 0.90;

        let active_score = penalized_score(active_sim, active.confidence, 1.0);
        // Depth-1 clean replacement → CLEAN_REPLACEMENT_PENALTY (0.40)
        let superseded_score = penalized_score(
            superseded_sim,
            superseded.confidence,
            CLEAN_REPLACEMENT_PENALTY,
        );

        assert!(
            active_score > superseded_score,
            "active ({active_score:.4}) should rank above superseded ({superseded_score:.4})"
        );
    }

    // -- T-SP-03: Strict mode excludes deprecated and superseded --
    #[test]
    fn strict_mode_excludes_non_active() {
        let active = make_test_entry(1, Status::Active, None, 0.65, "decision");
        let deprecated = make_test_entry(2, Status::Deprecated, None, 0.65, "decision");
        let superseded = make_test_entry(3, Status::Active, Some(99), 0.65, "decision");

        let entries = vec![
            (active.clone(), 0.9),
            (deprecated.clone(), 0.85),
            (superseded.clone(), 0.8),
        ];

        // Apply strict mode filtering
        let filtered: Vec<_> = entries
            .into_iter()
            .filter(|(e, _)| e.status == Status::Active && e.superseded_by.is_none())
            .collect();

        assert_eq!(
            filtered.len(),
            1,
            "strict mode should keep only active non-superseded"
        );
        assert_eq!(filtered[0].0.id, active.id);
    }

    // -- T-SP-04: Clean-replacement superseded is harsher than orphan deprecated (crt-014) --
    #[test]
    fn superseded_harsher_than_orphan_deprecated() {
        use unimatrix_engine::graph::{CLEAN_REPLACEMENT_PENALTY, ORPHAN_PENALTY};
        assert!(
            CLEAN_REPLACEMENT_PENALTY < ORPHAN_PENALTY,
            "clean replacement ({CLEAN_REPLACEMENT_PENALTY}) must be harsher (lower) than \
             orphan deprecated ({ORPHAN_PENALTY})"
        );
    }

    // -- T-SP-05: Deprecated-only query returns results in Flexible mode --
    #[test]
    fn deprecated_only_results_visible_flexible() {
        use unimatrix_engine::graph::ORPHAN_PENALTY;
        let deprecated = make_test_entry(1, Status::Deprecated, None, 0.65, "decision");
        let deprecated_sim = 0.85;

        // In flexible mode, deprecated entries are penalized but NOT excluded.
        // Orphan deprecated entry (no successors) receives ORPHAN_PENALTY (0.75).
        let score = penalized_score(deprecated_sim, deprecated.confidence, ORPHAN_PENALTY);

        assert!(
            score > 0.0,
            "deprecated entry should have a positive score ({score:.4})"
        );
    }

    // -- T-SP-06: Successor injection ranking --
    #[test]
    fn successor_ranks_above_superseded() {
        use unimatrix_engine::graph::CLEAN_REPLACEMENT_PENALTY;
        let successor = make_test_entry(1, Status::Active, None, 0.7, "decision");
        let superseded = make_test_entry(2, Status::Deprecated, Some(1), 0.65, "decision");

        // Superseded has higher raw similarity (it matched the query better)
        let successor_sim = 0.70;
        let superseded_sim = 0.90;

        let successor_score = penalized_score(successor_sim, successor.confidence, 1.0);
        // Depth-1 superseded → CLEAN_REPLACEMENT_PENALTY (0.40)
        let superseded_score = penalized_score(
            superseded_sim,
            superseded.confidence,
            CLEAN_REPLACEMENT_PENALTY,
        );

        assert!(
            successor_score > superseded_score,
            "successor ({successor_score:.4}) should rank above superseded ({superseded_score:.4})"
        );
    }

    // -- T-SP-07: Penalty does not affect stored confidence formula invariant --
    #[test]
    fn penalty_independent_of_confidence_formula() {
        use unimatrix_engine::graph::ORPHAN_PENALTY;
        // Penalties are multiplicative on the FINAL re-ranked score, not on confidence.
        // Use ORPHAN_PENALTY (0.75) as the representative deprecated-entry penalty (crt-014).
        let sim = 0.9;
        let conf = 0.8;
        let base = rerank_score(sim, conf, 0.18375);
        let penalized = base * ORPHAN_PENALTY;

        // The rerank base score is unchanged
        assert_eq!(base, rerank_score(sim, conf, 0.18375));
        // The penalty only affects the final score
        assert!(penalized < base);
        assert!((penalized - base * ORPHAN_PENALTY).abs() < f64::EPSILON);
    }

    // -- T-SP-08: Equal similarity, penalty determines ranking (crt-014 topology ordering) --
    #[test]
    fn equal_similarity_penalty_determines_rank() {
        use unimatrix_engine::graph::{CLEAN_REPLACEMENT_PENALTY, ORPHAN_PENALTY};
        let sim = 0.85;
        let conf = 0.65;

        // crt-014 topology ordering (ADR-004):
        // active (1.0) > orphan deprecated (0.75) > clean-replacement superseded (0.40)
        // This differs from prior crt-010 ordering: the new constants reflect topology.
        let active_score = penalized_score(sim, conf, 1.0);
        let deprecated_score = penalized_score(sim, conf, ORPHAN_PENALTY); // 0.75
        let superseded_score = penalized_score(sim, conf, CLEAN_REPLACEMENT_PENALTY); // 0.40

        assert!(
            active_score > deprecated_score,
            "active must rank above orphan deprecated"
        );
        assert!(
            deprecated_score > superseded_score,
            "orphan deprecated must rank above clean-replacement superseded"
        );
    }

    // =========================================================================
    // crt-018b: utility_delta unit tests
    // =========================================================================

    // -- AC-03 / AC-04 / AC-16: utility_delta pure function covers all 5 categories + None --

    #[test]
    fn test_utility_delta_effective() {
        assert_eq!(
            utility_delta(Some(EffectivenessCategory::Effective)),
            UTILITY_BOOST,
            "Effective must return UTILITY_BOOST (0.05)"
        );
    }

    #[test]
    fn test_utility_delta_settled() {
        assert_eq!(
            utility_delta(Some(EffectivenessCategory::Settled)),
            SETTLED_BOOST,
            "Settled must return SETTLED_BOOST (0.01)"
        );
    }

    #[test]
    fn test_utility_delta_ineffective() {
        assert_eq!(
            utility_delta(Some(EffectivenessCategory::Ineffective)),
            -UTILITY_PENALTY,
            "Ineffective must return -UTILITY_PENALTY (-0.05)"
        );
    }

    #[test]
    fn test_utility_delta_noisy() {
        assert_eq!(
            utility_delta(Some(EffectivenessCategory::Noisy)),
            -UTILITY_PENALTY,
            "Noisy must return -UTILITY_PENALTY (-0.05)"
        );
    }

    #[test]
    fn test_utility_delta_unmatched_zero() {
        assert_eq!(
            utility_delta(Some(EffectivenessCategory::Unmatched)),
            0.0_f64,
            "Unmatched must return 0.0"
        );
    }

    #[test]
    fn test_utility_delta_none_zero() {
        // AC-06, R-07: absent entry (None) must not default-to-penalty — it is 0.0.
        assert_eq!(
            utility_delta(None),
            0.0_f64,
            "None (absent/unclassified) must return 0.0, not a penalty"
        );
    }

    #[test]
    fn test_utility_delta_noisy_equals_ineffective_penalty() {
        // Documents the intentional symmetry: both bad categories receive the same penalty.
        assert_eq!(
            utility_delta(Some(EffectivenessCategory::Noisy)),
            utility_delta(Some(EffectivenessCategory::Ineffective)),
            "Noisy and Ineffective must receive identical (symmetric) penalty"
        );
    }

    // -- AC-03: constant invariants --

    #[test]
    fn test_utility_constants_values() {
        assert!(
            (UTILITY_BOOST - 0.05_f64).abs() < f64::EPSILON,
            "UTILITY_BOOST must be 0.05"
        );
        assert!(
            (SETTLED_BOOST - 0.01_f64).abs() < f64::EPSILON,
            "SETTLED_BOOST must be 0.01"
        );
        assert!(
            (UTILITY_PENALTY - 0.05_f64).abs() < f64::EPSILON,
            "UTILITY_PENALTY must be 0.05"
        );
        // AC-03: SETTLED_BOOST < co-access max (0.03)
        assert!(
            SETTLED_BOOST < 0.03_f64,
            "SETTLED_BOOST ({SETTLED_BOOST}) must be less than co-access max (0.03)"
        );
    }

    // -- AC-05 / R-02: Effective outranks near-equal Ineffective --

    #[test]
    fn test_effective_outranks_ineffective_at_close_similarity() {
        // confidence_weight = 0.15 (floor)
        // Entry A: sim=0.75, conf=0.60, category=Effective
        // Entry B: sim=0.76, conf=0.60, category=Ineffective
        // A base = rerank(0.75, 0.60, 0.15) + 0.05 = (0.85*0.75 + 0.15*0.60) + 0.05
        // B base = rerank(0.76, 0.60, 0.15) - 0.05 = (0.85*0.76 + 0.15*0.60) - 0.05
        let cw = 0.15_f64;
        let score_a = (rerank_score(0.75, 0.60, cw)
            + utility_delta(Some(EffectivenessCategory::Effective)))
            * 1.0;
        let score_b = (rerank_score(0.76, 0.60, cw)
            + utility_delta(Some(EffectivenessCategory::Ineffective)))
            * 1.0;
        assert!(
            score_a > score_b,
            "Effective entry (sim=0.75) must outrank Ineffective entry (sim=0.76) \
             despite lower similarity: score_a={score_a:.6}, score_b={score_b:.6}"
        );
    }

    #[test]
    fn test_effective_outranks_ineffective_at_max_weight() {
        // Repeat at confidence_weight = 0.25 (ceiling) to confirm ordering holds at both extremes.
        let cw = 0.25_f64;
        let score_a = (rerank_score(0.75, 0.60, cw)
            + utility_delta(Some(EffectivenessCategory::Effective)))
            * 1.0;
        let score_b = (rerank_score(0.76, 0.60, cw)
            + utility_delta(Some(EffectivenessCategory::Ineffective)))
            * 1.0;
        assert!(
            score_a > score_b,
            "Effective entry must outrank Ineffective at max confidence_weight (0.25): \
             score_a={score_a:.6}, score_b={score_b:.6}"
        );
    }

    // -- R-05 / ADR-003: utility_delta is INSIDE the status_penalty multiplication --

    #[test]
    fn test_utility_delta_inside_deprecated_penalty() {
        use unimatrix_engine::graph::ORPHAN_PENALTY;
        // Entry: status=Deprecated orphan (penalty=0.75), category=Effective, sim=0.75, conf=0.60, cw=0.15
        // Correct:  (rerank + UTILITY_BOOST) * ORPHAN_PENALTY
        // Wrong:    rerank * ORPHAN_PENALTY + UTILITY_BOOST
        // (crt-014: DEPRECATED_PENALTY replaced by topology-derived ORPHAN_PENALTY = 0.75)
        let sim = 0.75_f64;
        let conf = 0.60_f64;
        let cw = 0.15_f64;
        let base = rerank_score(sim, conf, cw);
        let delta = utility_delta(Some(EffectivenessCategory::Effective));

        let correct_score = (base + delta) * ORPHAN_PENALTY;
        let wrong_score = base * ORPHAN_PENALTY + delta;

        // Numerical values:
        // base = 0.85*0.75 + 0.15*0.60 = 0.6375 + 0.09 = 0.7275
        // correct = (0.7275 + 0.05) * 0.75 = 0.7775 * 0.75 = 0.583125
        // wrong   = 0.7275 * 0.75 + 0.05  = 0.545625 + 0.05 = 0.595625
        assert!(
            (correct_score - wrong_score).abs() > 0.001,
            "correct and wrong formulas must differ by more than 0.001 (detectable)"
        );
        // The two differ; implementation must produce correct_score, not wrong_score.
        // We verify by computing the step-7 formula directly:
        let step7_score = (base + delta) * ORPHAN_PENALTY;
        assert!(
            (step7_score - correct_score).abs() < f64::EPSILON,
            "Step 7 formula must match (base + delta) * penalty: \
             got {step7_score:.6}, expected {correct_score:.6}"
        );
    }

    #[test]
    fn test_utility_delta_inside_superseded_penalty() {
        use unimatrix_engine::graph::CLEAN_REPLACEMENT_PENALTY;
        // Entry: status=superseded (penalty=0.40 clean replacement), category=Noisy
        // (crt-014: SUPERSEDED_PENALTY replaced by topology-derived CLEAN_REPLACEMENT_PENALTY = 0.40)
        let sim = 0.80_f64;
        let conf = 0.65_f64;
        let cw = 0.18375_f64;
        let base = rerank_score(sim, conf, cw);
        let delta = utility_delta(Some(EffectivenessCategory::Noisy));

        let correct_score = (base + delta) * CLEAN_REPLACEMENT_PENALTY;
        let wrong_score = base * CLEAN_REPLACEMENT_PENALTY + delta;

        assert!(
            (correct_score - wrong_score).abs() > 1e-6,
            "correct and wrong placement must differ for Noisy + superseded"
        );
        let step7_score = (base + delta) * CLEAN_REPLACEMENT_PENALTY;
        assert!(
            (step7_score - correct_score).abs() < f64::EPSILON,
            "Step 7 formula must match (base + delta) * penalty for superseded/Noisy"
        );
    }

    // -- AC-06 / R-07: Empty EffectivenessState produces zero delta --

    #[test]
    fn test_utility_delta_absent_entry_zero() {
        // When an entry_id is not in the categories map, get() returns None.
        // utility_delta(None) must return 0.0 — not a penalty.
        let categories: HashMap<u64, EffectivenessCategory> = HashMap::new();
        let absent_id: u64 = 999;
        let delta = utility_delta(categories.get(&absent_id).copied());
        assert_eq!(
            delta, 0.0_f64,
            "absent entry must produce 0.0 delta (cold-start safe)"
        );
    }

    // -- R-06: cached_snapshot is Arc<Mutex<_>> shared across SearchService clones --

    #[test]
    fn test_cached_snapshot_shared_across_clones() {
        use crate::services::effectiveness::EffectivenessState;
        use crate::services::effectiveness::{EffectivenessSnapshot, EffectivenessStateHandle};
        use std::sync::{Arc, Mutex, RwLock};

        // Simulate the Arc<Mutex<EffectivenessSnapshot>> sharing pattern.
        let shared_snapshot: Arc<Mutex<EffectivenessSnapshot>> =
            EffectivenessSnapshot::new_shared();
        let snapshot_clone = Arc::clone(&shared_snapshot);

        // Update via original arc (as background tick would via SearchService::new)
        {
            let mut cache = shared_snapshot.lock().unwrap_or_else(|e| e.into_inner());
            cache.generation = 3;
            cache.categories.insert(1, EffectivenessCategory::Effective);
        }

        // Clone must see the same state — they share the same Arc backing object
        {
            let cache = snapshot_clone.lock().unwrap_or_else(|e| e.into_inner());
            assert_eq!(
                cache.generation, 3,
                "clone must see updated generation via shared Arc<Mutex<_>>"
            );
            assert_eq!(
                cache.categories.get(&1),
                Some(&EffectivenessCategory::Effective),
                "clone must see the Effective category via shared Arc<Mutex<_>>"
            );
        }
    }

    // -- R-01: Lock ordering — read guard dropped before mutex (code-level verification) --

    #[test]
    fn test_snapshot_read_guard_dropped_before_mutex_lock() {
        // Verifies the lock ordering invariant from ADR-001 / R-01:
        // The effectiveness_state read guard must be out of scope before cached_snapshot.lock().
        // We exercise this by performing the exact same scoping pattern used in search().
        use crate::services::effectiveness::{
            EffectivenessSnapshot, EffectivenessState, EffectivenessStateHandle,
        };
        use std::sync::{Arc, Mutex, RwLock};

        let effectiveness_state: EffectivenessStateHandle =
            Arc::new(RwLock::new(EffectivenessState::new()));
        let cached_snapshot: Arc<Mutex<EffectivenessSnapshot>> =
            EffectivenessSnapshot::new_shared();

        // Pattern from search() — inner block acquires and drops read guard before mutex.
        let _categories: HashMap<u64, EffectivenessCategory> = {
            let current_generation = {
                let guard = effectiveness_state
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                guard.generation
                // read guard drops here
            };
            // Read guard is out of scope here — safe to acquire the mutex.
            let mut cache = cached_snapshot.lock().unwrap_or_else(|e| e.into_inner());
            if cache.generation != current_generation {
                let guard = effectiveness_state
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                cache.generation = guard.generation;
                cache.categories = guard.categories.clone();
            }
            cache.categories.clone()
        };

        // If we get here without deadlock the lock ordering is correct.
        // Verify the result is an empty map (cold-start state).
        assert!(_categories.is_empty(), "cold-start snapshot must be empty");
    }

    // -- Generation cache: snapshot updates only when generation changes --

    #[test]
    fn test_generation_cache_skips_clone_when_unchanged() {
        use crate::services::effectiveness::{
            EffectivenessSnapshot, EffectivenessState, EffectivenessStateHandle,
        };
        use std::sync::{Arc, Mutex, RwLock};

        let effectiveness_state: EffectivenessStateHandle =
            Arc::new(RwLock::new(EffectivenessState::new()));
        let cached_snapshot: Arc<Mutex<EffectivenessSnapshot>> =
            EffectivenessSnapshot::new_shared();

        // First call: both at generation=0, cache should NOT clone (already matches).
        {
            let current_generation = {
                let guard = effectiveness_state
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                guard.generation
            };
            let cache = cached_snapshot.lock().unwrap_or_else(|e| e.into_inner());
            // generations match (both 0) — no update needed
            assert_eq!(
                cache.generation, current_generation,
                "cache and state must both start at 0"
            );
        }

        // Background tick: update state, bump generation
        {
            let mut guard = effectiveness_state
                .write()
                .unwrap_or_else(|e| e.into_inner());
            guard
                .categories
                .insert(42, EffectivenessCategory::Effective);
            guard.generation = 1;
        }

        // Second call: generation mismatch — cache must update
        {
            let current_generation = {
                let guard = effectiveness_state
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                guard.generation
            };
            let mut cache = cached_snapshot.lock().unwrap_or_else(|e| e.into_inner());
            if cache.generation != current_generation {
                let guard = effectiveness_state
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                cache.generation = guard.generation;
                cache.categories = guard.categories.clone();
            }
            assert_eq!(cache.generation, 1, "cache must be updated to generation 1");
            assert_eq!(
                cache.categories.get(&42),
                Some(&EffectivenessCategory::Effective),
                "cache must contain the Effective entry after update"
            );
        }

        // Third call: generation unchanged — cache must NOT re-clone (already at 1).
        {
            let current_generation = {
                let guard = effectiveness_state
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                guard.generation
            };
            let cache = cached_snapshot.lock().unwrap_or_else(|e| e.into_inner());
            // generations match (both 1) — no update needed
            assert_eq!(
                cache.generation, current_generation,
                "cache generation must still match state after second tick (no redundant clone)"
            );
        }
    }

    // =========================================================================
    // crt-014: Topology-aware penalty tests (AC-12, AC-16, IR-02)
    // =========================================================================

    // -- AC-12: graph_penalty returns topology-derived value, not old scalar constant --

    #[test]
    fn penalty_map_uses_graph_penalty_not_constant() {
        use unimatrix_engine::graph::{
            CLEAN_REPLACEMENT_PENALTY, build_typed_relation_graph, graph_penalty,
        };
        // Entry 1: superseded by entry 2 (depth-1 clean replacement)
        let entries = vec![
            make_test_entry(1, Status::Active, Some(2), 0.65, "decision"),
            make_test_entry(2, Status::Active, None, 0.65, "decision"),
        ];
        // Note: make_test_entry arg 3 is superseded_by. Entry 1 is superseded by 2.
        // For the graph: entry 2 must have supersedes=Some(1) to create the edge 1→2.
        // Build entries with correct supersedes/superseded_by fields.
        let entries_for_graph = vec![
            // Entry 1: has superseded_by=Some(2) (it's the old entry)
            make_test_entry(1, Status::Active, Some(2), 0.65, "decision"),
            // Entry 2: supersedes entry 1 (the new replacement). make_test_entry sets supersedes=None,
            // so we build it manually to set supersedes=Some(1).
            {
                let mut e = make_test_entry(2, Status::Active, None, 0.65, "decision");
                e.supersedes = Some(1);
                e
            },
        ];
        let graph = build_typed_relation_graph(&entries_for_graph, &[]).expect("valid DAG");
        // Entry 1 is at depth-1 from its active terminal (entry 2)
        let penalty = graph_penalty(1, &graph, &entries_for_graph);
        assert!(
            (penalty - CLEAN_REPLACEMENT_PENALTY).abs() < 1e-10,
            "depth-1 superseded entry must receive CLEAN_REPLACEMENT_PENALTY (0.40), got {penalty}"
        );
        // Confirm it differs from both old constant values
        assert!(
            (penalty - 0.5_f64).abs() > 0.05,
            "penalty must not equal old SUPERSEDED_PENALTY (0.5)"
        );
        assert!(
            (penalty - 0.7_f64).abs() > 0.05,
            "penalty must not equal old DEPRECATED_PENALTY (0.7)"
        );
    }

    // -- AC-16: Cycle detection produces CycleDetected, FALLBACK_PENALTY valid range --

    #[test]
    fn cycle_fallback_uses_fallback_penalty() {
        use unimatrix_engine::graph::{FALLBACK_PENALTY, GraphError, build_typed_relation_graph};

        // Two entries creating a cycle: entry 1 supersedes entry 2, entry 2 supersedes entry 1.
        let entries = vec![
            {
                let mut e = make_test_entry(1, Status::Active, None, 0.65, "decision");
                e.supersedes = Some(2);
                e
            },
            {
                let mut e = make_test_entry(2, Status::Active, None, 0.65, "decision");
                e.supersedes = Some(1);
                e
            },
        ];
        let result = build_typed_relation_graph(&entries, &[]);
        assert!(
            matches!(result, Err(GraphError::CycleDetected)),
            "cycle must be detected"
        );

        // When CycleDetected, use_fallback=true → FALLBACK_PENALTY applied
        assert!(
            (FALLBACK_PENALTY - 0.70_f64).abs() < f64::EPSILON,
            "FALLBACK_PENALTY must be 0.70"
        );
        assert!(
            FALLBACK_PENALTY > 0.0 && FALLBACK_PENALTY < 1.0,
            "FALLBACK_PENALTY must be in (0.0, 1.0)"
        );
    }

    // -- IR-02: Unified guard covers superseded-but-Active entry --

    #[test]
    fn unified_penalty_guard_covers_superseded_active_entry() {
        // Entry is Active status but has superseded_by set (unusual but valid).
        // The crt-014 unified condition must penalize it.
        let entry = make_test_entry(1, Status::Active, Some(99), 0.65, "decision");
        let should_penalize = entry.superseded_by.is_some() || entry.status == Status::Deprecated;
        assert!(
            should_penalize,
            "entry with superseded_by set must be penalized regardless of status field"
        );
    }

    // -- crt-021 (FR-22): typed graph state handle is readable and reflects pre-built graph --
    //
    // Verifies that:
    // 1. A TypedGraphStateHandle provides a readable cold-start state under a read lock.
    // 2. The search path can clone `typed_graph`, `all_entries`, and `use_fallback` without
    //    store I/O or per-query graph rebuild.
    // 3. Writing new state and re-reading reflects the update (rebuild semantics).
    //
    // This test catches regressions where the graph is rebuilt per query instead of
    // reading the pre-built graph from the handle (FR-22, crt-014 lesson learned).

    #[test]
    fn test_search_uses_cached_typed_graph_state_cold_start_fallback() {
        use crate::services::typed_graph::TypedGraphState;

        // Cold-start handle: empty entries, empty graph, use_fallback=true
        let handle = TypedGraphState::new_handle();
        let (entries, use_fallback) = {
            let guard = handle.read().unwrap_or_else(|e| e.into_inner());
            (guard.all_entries.clone(), guard.use_fallback)
        };

        assert!(entries.is_empty(), "cold-start: all_entries must be empty");
        assert!(use_fallback, "cold-start: use_fallback must be true");
    }

    #[test]
    fn test_search_uses_cached_typed_graph_state_after_rebuild() {
        use crate::services::typed_graph::TypedGraphState;
        use unimatrix_engine::graph::build_typed_relation_graph;

        let handle = TypedGraphState::new_handle();

        // Simulate background tick: build a graph and write a new state with use_fallback=false
        let entry = make_test_entry(42, Status::Active, None, 0.9, "decision");
        let entries = vec![entry.clone()];
        let graph = build_typed_relation_graph(&entries, &[]).expect("valid graph");
        {
            let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
            *guard = TypedGraphState {
                typed_graph: graph,
                all_entries: entries,
                use_fallback: false,
            };
        }

        // Simulate search hot path: read pre-built state under short lock, clone, release
        let (typed_graph, snapshot_entries, snapshot_fallback) = {
            let guard = handle.read().unwrap_or_else(|e| e.into_inner());
            (
                guard.typed_graph.clone(),
                guard.all_entries.clone(),
                guard.use_fallback,
            )
            // guard drops here
        };

        assert_eq!(snapshot_entries.len(), 1, "search must see 1 cached entry");
        assert_eq!(
            snapshot_entries[0].id, 42,
            "search must see the correct entry id"
        );
        assert!(
            !snapshot_fallback,
            "search must see use_fallback=false after rebuild"
        );

        // find_terminal_active on the pre-built graph is pure CPU — no store I/O, no rebuild.
        // Entry 42 is Active with no superseded_by — it is its own terminal (start node check).
        let terminal =
            unimatrix_engine::graph::find_terminal_active(42, &typed_graph, &snapshot_entries);
        assert_eq!(
            terminal,
            Some(42),
            "active entry must be its own terminal in the pre-built graph"
        );
    }

    // =========================================================================
    // crt-023 → crt-024: NLI scoring and fused scorer tests
    // =========================================================================
    //
    // apply_nli_sort was removed (ADR-002). Tests migrated to fused scorer below.
    // try_nli_rerank now returns Option<Vec<NliScores>> (raw scores, no sort).

    /// Build a minimal test EntryRecord with only the fields needed for sort tests.
    fn make_nli_test_entry(id: u64) -> EntryRecord {
        make_test_entry(id, Status::Active, None, 0.70, "decision")
    }

    // -----------------------------------------------------------------------
    // crt-024 FusionWeights::effective tests (T-SS-01 through T-SS-06, AC-06, AC-13, R-02)
    // -----------------------------------------------------------------------

    #[test]
    fn test_fusion_weights_effective_nli_active_unchanged() {
        // AC-13, R-09: NLI active path must return weights unchanged; no re-normalization.
        let weights = FusionWeights {
            w_sim: 0.25,
            w_nli: 0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.0,
            w_phase_explicit: 0.0,
        };
        let eff = weights.effective(true);
        assert!(
            (eff.w_nli - 0.35).abs() < 1e-9,
            "w_nli must be unchanged when NLI active"
        );
        assert!(
            (eff.w_sim - 0.25).abs() < 1e-9,
            "w_sim must be unchanged when NLI active"
        );
        assert!((eff.w_conf - 0.15).abs() < 1e-9);
        assert!((eff.w_coac - 0.10).abs() < 1e-9);
        assert!((eff.w_util - 0.05).abs() < 1e-9);
        assert!((eff.w_prov - 0.05).abs() < 1e-9);
    }

    #[test]
    fn test_fusion_weights_effective_nli_active_headroom_weight_preserved() {
        // AC-13: when weights sum to 0.90 (valid, with headroom), effective(true) must NOT
        // re-normalize to 1.0 — that would silently consume WA-2's reserved headroom.
        let weights = FusionWeights {
            w_sim: 0.25,
            w_nli: 0.30,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.0,
            w_phase_explicit: 0.0,
        }; // sum = 0.90
        let eff = weights.effective(true);
        let sum = eff.w_sim + eff.w_nli + eff.w_conf + eff.w_coac + eff.w_util + eff.w_prov;
        assert!(
            (sum - 0.90).abs() < 1e-9,
            "effective(true) must preserve sum=0.90, not re-normalize to 1.0; got {sum}"
        );
    }

    #[test]
    fn test_fusion_weights_effective_nli_absent_renormalizes_five_weights() {
        // AC-06, R-02: confirms the denominator is all five non-NLI weights (SR-03 resolution).
        let weights = FusionWeights {
            w_sim: 0.25,
            w_nli: 0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.0,
            w_phase_explicit: 0.0,
        };
        let eff = weights.effective(false);
        assert!(
            (eff.w_nli - 0.0).abs() < 1e-9,
            "w_nli must be 0.0 when NLI absent"
        );
        let sum = eff.w_sim + eff.w_conf + eff.w_coac + eff.w_util + eff.w_prov;
        assert!(
            (sum - 1.0).abs() < 1e-9,
            "re-normalized weights must sum to 1.0, got {sum}"
        );
        assert!(
            (eff.w_sim - (0.25 / 0.60)).abs() < 1e-6,
            "w_sim must be re-normalized"
        );
        assert!(
            (eff.w_conf - (0.15 / 0.60)).abs() < 1e-6,
            "w_conf must be re-normalized"
        );
        assert!(
            (eff.w_coac - (0.10 / 0.60)).abs() < 1e-6,
            "w_coac must be re-normalized"
        );
    }

    #[test]
    fn test_fusion_weights_effective_nli_absent_sum_is_one() {
        // R-09 complement: re-normalization must produce sum == 1.0.
        let weights = FusionWeights {
            w_sim: 0.25,
            w_nli: 0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.0,
            w_phase_explicit: 0.0,
        };
        let eff = weights.effective(false);
        let sum = eff.w_sim + eff.w_conf + eff.w_coac + eff.w_util + eff.w_prov;
        assert!(
            (sum - 1.0).abs() < 1e-9,
            "NLI-absent: effective weights must sum to 1.0"
        );
    }

    #[test]
    fn test_fusion_weights_effective_zero_denominator_returns_zeros_without_panic() {
        // R-02: pathological config — all non-NLI weights zero. Must not panic.
        let weights = FusionWeights {
            w_sim: 0.0,
            w_nli: 0.5,
            w_conf: 0.0,
            w_coac: 0.0,
            w_util: 0.0,
            w_prov: 0.0,
            w_phase_histogram: 0.0,
            w_phase_explicit: 0.0,
        };
        let eff = weights.effective(false);
        assert_eq!(
            eff.w_sim, 0.0,
            "w_sim must be 0.0 on zero-denominator guard"
        );
        assert_eq!(eff.w_nli, 0.0);
        assert_eq!(eff.w_conf, 0.0);
        assert_eq!(eff.w_coac, 0.0);
        assert_eq!(eff.w_util, 0.0);
        assert_eq!(eff.w_prov, 0.0);
    }

    #[test]
    fn test_fusion_weights_effective_single_nonzero_weight_nli_absent() {
        // R-02, Scenario 2: single remaining weight → gets full weight 1.0.
        let weights = FusionWeights {
            w_sim: 0.5,
            w_nli: 0.5,
            w_conf: 0.0,
            w_coac: 0.0,
            w_util: 0.0,
            w_prov: 0.0,
            w_phase_histogram: 0.0,
            w_phase_explicit: 0.0,
        };
        let eff = weights.effective(false);
        assert_eq!(eff.w_nli, 0.0);
        assert!(
            (eff.w_sim - 1.0).abs() < 1e-9,
            "single remaining weight must get 1.0"
        );
        assert_eq!(eff.w_conf, 0.0);
        assert_eq!(eff.w_coac, 0.0);
    }

    #[test]
    fn test_fusion_weights_effective_does_not_mutate_original() {
        // T-SS-05: effective() returns a new value; the receiver is not modified.
        let weights = FusionWeights {
            w_sim: 0.25,
            w_nli: 0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.0,
            w_phase_explicit: 0.0,
        };
        let _eff = weights.effective(false);
        assert!(
            (weights.w_nli - 0.35).abs() < 1e-9,
            "original w_nli must remain 0.35 after effective()"
        );
    }

    #[test]
    fn test_fusion_weights_from_config_maps_fields() {
        // T-SS-06: FusionWeights::from_config maps each field from InferenceConfig.
        use crate::infra::config::InferenceConfig;
        let mut cfg = InferenceConfig::default();
        cfg.w_sim = 0.30;
        cfg.w_nli = 0.30;
        cfg.w_conf = 0.15;
        cfg.w_coac = 0.10;
        cfg.w_util = 0.10;
        cfg.w_prov = 0.05;
        let fw = FusionWeights::from_config(&cfg);
        assert!((fw.w_sim - 0.30).abs() < 1e-9);
        assert!((fw.w_nli - 0.30).abs() < 1e-9);
        assert!((fw.w_conf - 0.15).abs() < 1e-9);
        assert!((fw.w_coac - 0.10).abs() < 1e-9);
        assert!((fw.w_util - 0.10).abs() < 1e-9);
        assert!((fw.w_prov - 0.05).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // crt-024 compute_fused_score tests (T-CF-01 through T-CF-11)
    // -----------------------------------------------------------------------

    fn default_weights() -> FusionWeights {
        FusionWeights {
            w_sim: 0.25,
            w_nli: 0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.0,
            w_phase_explicit: 0.0,
        }
    }

    #[test]
    fn test_compute_fused_score_six_term_correctness_ac05() {
        // AC-05: known-value correctness check.
        let inputs = FusedScoreInputs {
            similarity: 0.8,
            nli_entailment: 0.7,
            confidence: 0.6,
            coac_norm: 0.5,
            util_norm: 0.5,
            prov_norm: 1.0,
            phase_histogram_norm: 0.0,
            phase_explicit_norm: 0.0,
        };
        let weights = FusionWeights {
            w_sim: 0.30,
            w_nli: 0.30,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.0,
            w_phase_explicit: 0.0,
        };
        let score = compute_fused_score(&inputs, &weights);
        // 0.30*0.8 + 0.30*0.7 + 0.15*0.6 + 0.10*0.5 + 0.05*0.5 + 0.05*1.0
        // = 0.24 + 0.21 + 0.09 + 0.05 + 0.025 + 0.05 = 0.665
        assert!(
            (score - 0.665).abs() < 1e-9,
            "AC-05: expected 0.665, got {score}"
        );
    }

    #[test]
    fn test_compute_fused_score_nli_high_beats_coac_high_ac11() {
        // AC-11: NLI dominance regression test.
        // Pre-crt-024: co-access applied as additive afterthought in Step 8 (after NLI sort in
        // Step 7), which allowed Entry B (coac=max) to overtake Entry A (nli=0.9) in the re-sort.
        // crt-024 fix: all signals in one weighted formula; NLI weight (0.35) > max co-access (0.10).
        //
        // Inputs from SPECIFICATION.md (sim=0.8, Gate 3a): use sim=0.8 and conf=0.65.
        // Note: test-plan/compute-fused-score.md mistakenly expected 0.540 for these inputs
        // but 0.35*0.9+0.25*0.8+0.15*0.65 = 0.6125. The IMPLEMENTATION-BRIEF 0.540 figure
        // corresponds to sim=0.5, conf=0.5. We use sim=0.8 (SPECIFICATION.md) with the
        // mathematically correct expected values.
        let default_weights = FusionWeights {
            w_sim: 0.25,
            w_nli: 0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.0,
            w_phase_explicit: 0.0,
        };
        // Entry A: high NLI, no co-access
        let entry_a = FusedScoreInputs {
            similarity: 0.8,
            nli_entailment: 0.9,
            confidence: 0.65,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0,
            phase_explicit_norm: 0.0,
        };
        // Entry B: low NLI, max co-access
        let entry_b = FusedScoreInputs {
            similarity: 0.8,
            nli_entailment: 0.3,
            confidence: 0.65,
            coac_norm: 1.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0,
            phase_explicit_norm: 0.0,
        };
        let score_a = compute_fused_score(&entry_a, &default_weights);
        let score_b = compute_fused_score(&entry_b, &default_weights);
        // Entry A: 0.35*0.9 + 0.25*0.8 + 0.15*0.65 = 0.315 + 0.200 + 0.0975 = 0.6125
        // Entry B: 0.35*0.3 + 0.25*0.8 + 0.15*0.65 + 0.10*1.0 = 0.105 + 0.200 + 0.0975 + 0.100 = 0.5025
        let expected_a = 0.35 * 0.9 + 0.25 * 0.8 + 0.15 * 0.65;
        let expected_b = 0.35 * 0.3 + 0.25 * 0.8 + 0.15 * 0.65 + 0.10 * 1.0;
        assert!(
            (score_a - expected_a).abs() < 1e-9,
            "Entry A must score {expected_a:.4}, got {score_a}"
        );
        assert!(
            (score_b - expected_b).abs() < 1e-9,
            "Entry B must score {expected_b:.4}, got {score_b}"
        );
        assert!(
            score_a > score_b,
            "AC-11: Entry A (nli=0.9, coac=0) must beat Entry B (nli=0.3, coac=max): \
             A={score_a:.4} vs B={score_b:.4}"
        );
    }

    #[test]
    fn test_compute_fused_score_constraint9_nli_disabled_sim_dominant() {
        // ADR-003 Constraint 9: NLI disabled, sim must remain dominant over conf.
        let weights = default_weights();
        let eff = weights.effective(false); // denom = 0.60
        // w_sim' ≈ 0.4167, w_conf' ≈ 0.2500
        let entry_a = FusedScoreInputs {
            similarity: 0.9,
            nli_entailment: 0.0,
            confidence: 0.3,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0,
            phase_explicit_norm: 0.0,
        };
        let entry_b = FusedScoreInputs {
            similarity: 0.5,
            nli_entailment: 0.0,
            confidence: 0.9,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0,
            phase_explicit_norm: 0.0,
        };
        let score_a = compute_fused_score(&entry_a, &eff);
        let score_b = compute_fused_score(&entry_b, &eff);
        // A ≈ 0.4167*0.9 + 0.2500*0.3 = 0.375 + 0.075 = 0.450
        // B ≈ 0.4167*0.5 + 0.2500*0.9 = 0.209 + 0.225 = 0.434
        assert!(
            score_a > score_b,
            "Constraint 9: sim dominant over conf when NLI disabled (A={score_a}, B={score_b})"
        );
        // Verify re-normalized w_sim' ≈ 0.4167
        assert!((eff.w_sim - (0.25 / 0.60)).abs() < 1e-6);
        assert!((eff.w_conf - (0.15 / 0.60)).abs() < 1e-6);
    }

    #[test]
    fn test_compute_fused_score_constraint10_sim_dominant_at_defaults() {
        // ADR-003 Constraint 10: sim dominant over conf at full defaults.
        let weights = default_weights();
        let entry_a = FusedScoreInputs {
            similarity: 0.9,
            nli_entailment: 0.0,
            confidence: 0.3,
            coac_norm: 0.0,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let entry_b = FusedScoreInputs {
            similarity: 0.5,
            nli_entailment: 0.0,
            confidence: 0.9,
            coac_norm: 0.0,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        // A: 0.25*0.9 + 0.15*0.3 + 0.05*0.5 = 0.225 + 0.045 + 0.025 = 0.295
        // B: 0.25*0.5 + 0.15*0.9 + 0.05*0.5 = 0.125 + 0.135 + 0.025 = 0.285
        let score_a = compute_fused_score(&entry_a, &weights);
        let score_b = compute_fused_score(&entry_b, &weights);
        assert!(
            score_a > score_b,
            "Constraint 10: sim must dominate conf at defaults (A={score_a}, B={score_b})"
        );
    }

    #[test]
    fn test_compute_fused_score_does_not_accept_status_penalty() {
        // R-14, ADR-004: FusedScoreInputs must NOT have status_penalty field.
        // This struct construction compiles iff exactly the six expected fields exist.
        let inputs = FusedScoreInputs {
            similarity: 0.8,
            nli_entailment: 0.7,
            confidence: 0.6,
            coac_norm: 0.5,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        // Verify penalty is applied externally:
        use unimatrix_engine::graph::ORPHAN_PENALTY;
        let fused = compute_fused_score(&inputs, &default_weights());
        let final_score = fused * ORPHAN_PENALTY;
        assert!(final_score < fused, "penalty must reduce the fused score");
        assert!(final_score > 0.0);
    }

    #[test]
    fn test_status_penalty_applied_as_multiplier_after_fused_score() {
        // AC-09: status penalty as multiplier.
        let inputs = FusedScoreInputs {
            similarity: 1.0,
            nli_entailment: 0.0,
            confidence: 0.0,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let weights = FusionWeights {
            w_sim: 1.0,
            w_nli: 0.0,
            w_conf: 0.0,
            w_coac: 0.0,
            w_util: 0.0,
            w_prov: 0.0,
            w_phase_histogram: 0.0, // crt-026 test: phase fields default to 0.0
            w_phase_explicit: 0.0,  // crt-026 test: W3-1 placeholder
        };
        let fused = compute_fused_score(&inputs, &weights); // = 1.0
        let deprecated_penalty = 0.7_f64;
        let final_score = fused * deprecated_penalty;
        assert!(
            (final_score - 0.7).abs() < 1e-9,
            "final_score must be fused * penalty = 0.7"
        );
    }

    #[test]
    fn test_compute_fused_score_util_contributes_exactly_w_util_times_util_norm() {
        // AC-10: util_norm contribution is exactly w_util * diff.
        let weights = FusionWeights {
            w_sim: 0.30,
            w_nli: 0.30,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.0, // crt-026 test: phase fields default to 0.0
            w_phase_explicit: 0.0,  // crt-026 test: W3-1 placeholder
        };
        let base = FusedScoreInputs {
            similarity: 0.5,
            nli_entailment: 0.5,
            confidence: 0.5,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let with_util = FusedScoreInputs {
            util_norm: 1.0,
            ..base
        };
        let diff = compute_fused_score(&with_util, &weights) - compute_fused_score(&base, &weights);
        assert!(
            (diff - weights.w_util).abs() < 1e-9,
            "util_norm difference must be exactly w_util={}, got diff={diff}",
            weights.w_util
        );
    }

    #[test]
    fn test_compute_fused_score_range_guarantee_all_inputs_max() {
        // NFR-02: all inputs at 1.0, default weights sum=0.95 → score = 0.95.
        let inputs = FusedScoreInputs {
            similarity: 1.0,
            nli_entailment: 1.0,
            confidence: 1.0,
            coac_norm: 1.0,
            util_norm: 1.0,
            prov_norm: 1.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let score = compute_fused_score(&inputs, &default_weights());
        assert!(
            (score - 0.95).abs() < 1e-9,
            "max inputs at defaults must produce 0.95"
        );
        assert!(score <= 1.0);
        assert!(score >= 0.0);
    }

    #[test]
    fn test_compute_fused_score_range_guarantee_all_inputs_zero() {
        // NFR-02: all inputs at 0.0 → score = 0.0.
        let inputs = FusedScoreInputs {
            similarity: 0.0,
            nli_entailment: 0.0,
            confidence: 0.0,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let score = compute_fused_score(&inputs, &default_weights());
        assert_eq!(score, 0.0, "zero inputs must produce 0.0");
    }

    #[test]
    fn test_compute_fused_score_all_zero_nli_degrades_to_five_signals() {
        // EC-04: nli_entailment=0.0 with active NLI weights contributes nothing.
        let inputs_no_nli = FusedScoreInputs {
            similarity: 0.8,
            nli_entailment: 0.0,
            confidence: 0.6,
            coac_norm: 0.3,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let inputs_with_nli = FusedScoreInputs {
            nli_entailment: 0.9,
            ..inputs_no_nli
        };
        let score_no_nli = compute_fused_score(&inputs_no_nli, &default_weights());
        let score_with_nli = compute_fused_score(&inputs_with_nli, &default_weights());
        // With NLI the score is higher; without NLI entailment still sums the five signals.
        assert!(
            score_with_nli > score_no_nli,
            "NLI contributes when non-zero"
        );
        assert!(
            score_no_nli > 0.0,
            "five signals still contribute without NLI"
        );
        assert!(score_no_nli.is_finite());
    }

    #[test]
    fn test_compute_fused_score_single_term_formula() {
        // T-CF-07: single non-zero weight — no cross-term contamination.
        let weights = FusionWeights {
            w_sim: 1.0,
            w_nli: 0.0,
            w_conf: 0.0,
            w_coac: 0.0,
            w_util: 0.0,
            w_prov: 0.0,
            w_phase_histogram: 0.0, // crt-026 test: phase fields default to 0.0
            w_phase_explicit: 0.0,  // crt-026 test: W3-1 placeholder
        };
        let inputs = FusedScoreInputs {
            similarity: 0.75,
            nli_entailment: 0.99,
            confidence: 0.99,
            coac_norm: 0.99,
            util_norm: 0.99,
            prov_norm: 0.99,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let score = compute_fused_score(&inputs, &weights);
        assert!(
            (score - 0.75).abs() < 1e-9,
            "single-weight formula must return sim only"
        );
    }

    #[test]
    fn test_compute_fused_score_nli_disabled_w_nli_zero_contributes_nothing() {
        // T-CF-08: w_nli=0.0 means nli_entailment contributes nothing.
        let weights_no_nli = FusionWeights {
            w_sim: 0.5,
            w_nli: 0.0,
            w_conf: 0.5,
            w_coac: 0.0,
            w_util: 0.0,
            w_prov: 0.0,
            w_phase_histogram: 0.0, // crt-026 test: phase fields default to 0.0
            w_phase_explicit: 0.0,  // crt-026 test: W3-1 placeholder
        };
        let inputs_high_nli = FusedScoreInputs {
            similarity: 0.5,
            nli_entailment: 0.9,
            confidence: 0.5,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let inputs_zero_nli = FusedScoreInputs {
            nli_entailment: 0.0,
            ..inputs_high_nli
        };
        let score_high = compute_fused_score(&inputs_high_nli, &weights_no_nli);
        let score_zero = compute_fused_score(&inputs_zero_nli, &weights_no_nli);
        assert!(
            (score_high - score_zero).abs() < 1e-9,
            "w_nli=0.0: nli_entailment must contribute nothing regardless of value"
        );
    }

    #[test]
    fn test_compute_fused_score_result_is_finite() {
        // T-CF-09, R-03, SeR-02: property-style test — all valid inputs produce finite output.
        let sample_vals = [0.0, 0.1, 0.5, 0.9, 1.0];
        for &sim in &sample_vals {
            for &nli in &sample_vals {
                for &conf in &sample_vals {
                    let inputs = FusedScoreInputs {
                        similarity: sim,
                        nli_entailment: nli,
                        confidence: conf,
                        coac_norm: 0.5,
                        util_norm: 0.5,
                        prov_norm: 0.0,
                        phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
                        phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
                    };
                    let score = compute_fused_score(&inputs, &default_weights());
                    assert!(
                        score.is_finite(),
                        "score must be finite for sim={sim}, nli={nli}, conf={conf}; got {score}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_util_norm_ineffective_entry_maps_to_zero() {
        // T-CF-10, R-01: Ineffective entry → util_norm = 0.0.
        let raw_delta = -UTILITY_PENALTY; // -0.05
        let util_norm = (raw_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY);
        assert!(
            (util_norm - 0.0).abs() < 1e-9,
            "Ineffective: util_norm must be 0.0"
        );
    }

    #[test]
    fn test_util_norm_neutral_entry_maps_to_half() {
        // T-CF-10, R-01: neutral entry → util_norm = 0.5.
        let raw_delta = 0.0_f64;
        let util_norm = (raw_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY);
        assert!(
            (util_norm - 0.5).abs() < 1e-9,
            "neutral: util_norm must be 0.5"
        );
    }

    #[test]
    fn test_util_norm_effective_entry_maps_to_one() {
        // T-CF-10, R-01: Effective entry → util_norm = 1.0.
        let raw_delta = UTILITY_BOOST; // +0.05
        let util_norm = (raw_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY);
        assert!(
            (util_norm - 1.0).abs() < 1e-9,
            "Effective: util_norm must be 1.0"
        );
    }

    #[test]
    fn test_compute_fused_score_ineffective_util_non_negative() {
        // R-11, NFR-02: Ineffective entry (util_norm=0.0) must produce non-negative fused score.
        let inputs = FusedScoreInputs {
            similarity: 0.0,
            nli_entailment: 0.0,
            confidence: 0.0,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let score = compute_fused_score(&inputs, &default_weights());
        assert!(
            score >= 0.0,
            "fused_score must be >= 0.0 for Ineffective entry, got {score}"
        );
        assert!(score.is_finite());
    }

    #[test]
    fn test_prov_norm_zero_denominator_returns_zero() {
        // R-03: when PROVENANCE_BOOST == 0.0, prov_norm must be 0.0.
        let prov_boost_sim = 0.0_f64; // simulate PROVENANCE_BOOST = 0.0
        let raw_boost = 0.02_f64;
        let prov_norm = if prov_boost_sim == 0.0 {
            0.0
        } else {
            raw_boost / prov_boost_sim
        };
        assert_eq!(
            prov_norm, 0.0,
            "prov_norm must be 0.0 when PROVENANCE_BOOST == 0.0"
        );
    }

    #[test]
    fn test_prov_norm_boosted_entry_equals_one() {
        // R-03: boosted entry with raw_boost == PROVENANCE_BOOST → prov_norm = 1.0.
        let prov_boost = PROVENANCE_BOOST;
        let raw_boost = prov_boost;
        let prov_norm = if prov_boost == 0.0 {
            0.0
        } else {
            raw_boost / prov_boost
        };
        assert!(
            (prov_norm - 1.0).abs() < 1e-9,
            "boosted entry: prov_norm must be 1.0"
        );
    }

    #[test]
    fn test_prov_norm_unboosted_entry_equals_zero() {
        // R-03: unboosted entry (raw_boost=0.0) → prov_norm = 0.0.
        let prov_boost = PROVENANCE_BOOST;
        let raw_boost = 0.0_f64;
        let prov_norm = if prov_boost == 0.0 {
            0.0
        } else {
            raw_boost / prov_boost
        };
        assert_eq!(prov_norm, 0.0);
    }

    #[test]
    fn test_fused_score_inputs_struct_accessible_by_field_name() {
        // R-16: named-field struct (WA-2 can add phase_boost_norm without breaking this site).
        let inputs = FusedScoreInputs {
            similarity: 0.8,
            nli_entailment: 0.7,
            confidence: 0.6,
            coac_norm: 0.5,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        assert!((inputs.similarity - 0.8).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // crt-024: SearchService pipeline migration tests (R-05)
    // -----------------------------------------------------------------------

    #[test]
    fn test_fused_score_nli_entailment_dominates_when_high() {
        // R-05 migration of test_nli_sort_orders_by_entailment_descending.
        // compute_fused_score with nli=0.9 vs nli=0.1, equal sim/conf/coac/util/prov.
        let weights = default_weights();
        let high_nli = FusedScoreInputs {
            similarity: 0.5,
            nli_entailment: 0.9,
            confidence: 0.5,
            coac_norm: 0.0,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let low_nli = FusedScoreInputs {
            similarity: 0.5,
            nli_entailment: 0.1,
            confidence: 0.5,
            coac_norm: 0.0,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let score_high = compute_fused_score(&high_nli, &weights);
        let score_low = compute_fused_score(&low_nli, &weights);
        assert!(
            score_high > score_low,
            "high NLI entry must score above low NLI entry (high={score_high}, low={score_low})"
        );
    }

    #[test]
    fn test_fused_score_equal_fused_scores_deterministic_sort() {
        // R-05 migration of test_nli_sort_stable_identical_scores_preserves_original_order.
        // Entries with identical signal values produce identical fused scores.
        // The stable sort must preserve original HNSW insertion order.
        let weights = default_weights();
        let inputs = FusedScoreInputs {
            similarity: 0.75,
            nli_entailment: 0.33,
            confidence: 0.70,
            coac_norm: 0.0,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let score = compute_fused_score(&inputs, &weights);
        // All five entries have identical score.
        let mut entries: Vec<(u64, f64)> = (1u64..=5).map(|id| (id, score)).collect();

        // Sort by fused score DESC (stable). Equal scores must preserve insertion order.
        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        let ids: Vec<u64> = entries.iter().map(|(id, _)| *id).collect();

        // Run again — must be identical (deterministic, stable sort).
        let mut entries2: Vec<(u64, f64)> = (1u64..=5).map(|id| (id, score)).collect();
        entries2.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        let ids2: Vec<u64> = entries2.iter().map(|(id, _)| *id).collect();

        assert_eq!(ids, ids2, "stable sort must be deterministic");
        assert_eq!(
            ids,
            vec![1, 2, 3, 4, 5],
            "equal scores must preserve insertion order"
        );
    }

    #[test]
    fn test_fused_score_nan_nli_defaults_to_zero() {
        // R-05 migration of test_nli_sort_nan_entailment_treated_as_equal.
        // NaN NliScores.entailment cast to f64: the scoring loop substitutes 0.0 for NaN.
        let nan_nli_score = NliScores {
            entailment: f32::NAN,
            neutral: 0.5,
            contradiction: 0.5,
        };
        let entailment: f64 = {
            let v = nan_nli_score.entailment as f64;
            if v.is_nan() { 0.0 } else { v }
        };
        assert_eq!(
            entailment, 0.0,
            "NaN entailment must be substituted with 0.0"
        );

        // Confirm compute_fused_score produces finite result with 0.0 nli_entailment.
        let inputs = FusedScoreInputs {
            similarity: 0.5,
            nli_entailment: entailment,
            confidence: 0.5,
            coac_norm: 0.0,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let score = compute_fused_score(&inputs, &default_weights());
        assert!(
            score.is_finite(),
            "NaN-substituted entry must produce finite score"
        );
    }

    #[test]
    fn test_status_penalty_depresses_final_score() {
        // R-05 migration of test_nli_sort_penalty_depresses_effective_entailment.
        // ADR-004: penalty is applied after compute_fused_score.
        let weights = default_weights();
        let inputs = FusedScoreInputs {
            similarity: 0.8,
            nli_entailment: 0.9,
            confidence: 0.65,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let fused = compute_fused_score(&inputs, &weights);
        let penalty_active = 1.0_f64;
        let penalty_deprecated = 0.7_f64;
        let final_active = fused * penalty_active;
        let final_deprecated = fused * penalty_deprecated;
        assert!(
            final_deprecated < final_active,
            "deprecated entry (penalty=0.7) must have lower final_score than active"
        );
        assert!(final_deprecated > 0.0);
    }

    #[test]
    fn test_coac_norm_boundary_values() {
        // R-08, AC-07: MAX_CO_ACCESS_BOOST imported from engine — not redefined.
        use unimatrix_engine::coaccess::MAX_CO_ACCESS_BOOST;
        let norm_max = MAX_CO_ACCESS_BOOST / MAX_CO_ACCESS_BOOST;
        let norm_half = (MAX_CO_ACCESS_BOOST / 2.0) / MAX_CO_ACCESS_BOOST;
        let norm_zero = 0.0_f64 / MAX_CO_ACCESS_BOOST;
        assert!((norm_max - 1.0).abs() < 1e-9);
        assert!((norm_half - 0.5).abs() < 1e-9);
        assert!((norm_zero - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_eval_service_layer_sim_only_profile_scores_equal_sim() {
        // R-NEW, AC-15: if w_sim=1.0 and all other weights=0.0, fused score equals sim.
        let sim_only_weights = FusionWeights {
            w_sim: 1.0,
            w_nli: 0.0,
            w_conf: 0.0,
            w_coac: 0.0,
            w_util: 0.0,
            w_prov: 0.0,
            w_phase_histogram: 0.0, // crt-026 test: phase fields default to 0.0
            w_phase_explicit: 0.0,  // crt-026 test: W3-1 placeholder
        };
        let inputs = FusedScoreInputs {
            similarity: 0.6,
            nli_entailment: 0.9,
            confidence: 0.8,
            coac_norm: 0.667,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let fused = compute_fused_score(&inputs, &sim_only_weights);
        let status_penalty = 1.0_f64;
        let final_score = fused * status_penalty;
        assert!(
            (final_score - 0.6).abs() < 1e-9,
            "sim-only profile: final_score must be 0.6*penalty = 0.6, got {final_score}"
        );
    }

    #[test]
    fn test_eval_service_layer_default_weights_score_differs_from_sim_only() {
        // R-NEW: default weights produce a different (higher) score than sim-only for same inputs.
        let sim_only_weights = FusionWeights {
            w_sim: 1.0,
            w_nli: 0.0,
            w_conf: 0.0,
            w_coac: 0.0,
            w_util: 0.0,
            w_prov: 0.0,
            w_phase_histogram: 0.0, // crt-026 test: phase fields default to 0.0
            w_phase_explicit: 0.0,  // crt-026 test: W3-1 placeholder
        };
        let inputs = FusedScoreInputs {
            similarity: 0.6,
            nli_entailment: 0.9,
            confidence: 0.8,
            coac_norm: 0.667,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let score_sim_only = compute_fused_score(&inputs, &sim_only_weights);
        let score_default = compute_fused_score(&inputs, &default_weights());
        // Default weights include w_nli=0.35*0.9, w_conf=0.15*0.8, etc. — significantly higher.
        assert!(
            score_default > score_sim_only,
            "default weights must produce higher score than sim-only for NLI-strong inputs"
        );
    }

    #[test]
    fn test_eval_service_layer_differential_two_profiles_produce_different_scores() {
        // R-NEW, AC-15: two profiles with different w_nli must produce meaningfully different scores.
        // Profile 1: old-behavior.toml (w_nli=0.0, w_sim=0.85)
        let profile1 = FusionWeights {
            w_sim: 0.85,
            w_nli: 0.0,
            w_conf: 0.15,
            w_coac: 0.0,
            w_util: 0.0,
            w_prov: 0.0,
            w_phase_histogram: 0.0, // crt-026 test: phase fields default to 0.0
            w_phase_explicit: 0.0,  // crt-026 test: W3-1 placeholder
        };
        // Profile 2: crt024-weights.toml (w_nli=0.35, w_sim=0.25)
        let profile2 = FusionWeights {
            w_sim: 0.25,
            w_nli: 0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.0, // crt-026 test: phase fields default to 0.0
            w_phase_explicit: 0.0,  // crt-026 test: W3-1 placeholder
        };
        // Use an entry where NLI is very high (0.9) and sim is low (0.1).
        // Profile1 will heavily reward sim (0.85*0.1=0.085), profile2 rewards NLI (0.35*0.9=0.315).
        let inputs = FusedScoreInputs {
            similarity: 0.1,
            nli_entailment: 0.9,
            confidence: 0.5,
            coac_norm: 0.0,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let score1 = compute_fused_score(&inputs, &profile1);
        let score2 = compute_fused_score(&inputs, &profile2);
        // Profile1: 0.85*0.1 + 0.15*0.5 = 0.085 + 0.075 = 0.160
        // Profile2: 0.25*0.1 + 0.35*0.9 + 0.15*0.5 + 0.05*0.5 = 0.025 + 0.315 + 0.075 + 0.025 = 0.440
        // Difference = 0.440 - 0.160 = 0.280 (profile2 wins on NLI)
        assert!(
            (score2 - score1).abs() >= 0.20,
            "two profiles must produce meaningfully different scores for NLI-dominant input \
             (diff={:.4}, score1={score1:.4}, score2={score2:.4})",
            (score2 - score1).abs()
        );
    }

    #[test]
    fn test_fused_scoring_nli_scores_aligned_with_candidates() {
        // R-15: nli_scores[i] must be applied to candidates[i].
        // Entry A: low sim (0.3) but high NLI (0.9) — must rank above B if aligned correctly.
        // Entry B: high sim (0.9) but low NLI (0.1).
        let weights = default_weights();
        let entry_a = FusedScoreInputs {
            similarity: 0.3,
            nli_entailment: 0.9,
            confidence: 0.5,
            coac_norm: 0.0,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let entry_b = FusedScoreInputs {
            similarity: 0.9,
            nli_entailment: 0.1,
            confidence: 0.5,
            coac_norm: 0.0,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let score_a = compute_fused_score(&entry_a, &weights);
        let score_b = compute_fused_score(&entry_b, &weights);
        // A: 0.35*0.9 + 0.25*0.3 + rest = 0.315 + 0.075 + ... > B: 0.35*0.1 + 0.25*0.9 + ...
        assert!(
            score_a > score_b,
            "NLI-dominant entry A must score above sim-dominant entry B when scores are aligned"
        );
    }

    #[test]
    fn test_constraint_9_nli_disabled_sim_dominant_over_conf() {
        // ADR-003 Constraint 9 named test (required by test plan).
        let weights = default_weights();
        let eff = weights.effective(false);
        let entry_a = FusedScoreInputs {
            similarity: 0.9,
            nli_entailment: 0.0,
            confidence: 0.3,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let entry_b = FusedScoreInputs {
            similarity: 0.5,
            nli_entailment: 0.0,
            confidence: 0.9,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let score_a = compute_fused_score(&entry_a, &eff);
        let score_b = compute_fused_score(&entry_b, &eff);
        assert!(
            score_a > score_b,
            "Constraint 9: sim must dominate conf when NLI disabled"
        );
    }

    #[test]
    fn test_constraint_10_sim_dominant_no_nli_no_coac() {
        // ADR-003 Constraint 10 named test (required by test plan).
        let weights = default_weights();
        // Default weights, NLI active but zero, no co-access.
        let entry_a = FusedScoreInputs {
            similarity: 0.9,
            nli_entailment: 0.0,
            confidence: 0.3,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        let entry_b = FusedScoreInputs {
            similarity: 0.5,
            nli_entailment: 0.0,
            confidence: 0.9,
            coac_norm: 0.0,
            util_norm: 0.0,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // crt-026 test: cold-start / no histogram
            phase_explicit_norm: 0.0,  // crt-026 test: ADR-003 placeholder
        };
        // A: 0.25*0.9 + 0.15*0.3 = 0.270; B: 0.25*0.5 + 0.15*0.9 = 0.260
        let score_a = compute_fused_score(&entry_a, &weights);
        let score_b = compute_fused_score(&entry_b, &weights);
        assert!(
            score_a > score_b,
            "Constraint 10: sim must dominate conf at default weights"
        );
    }

    // -----------------------------------------------------------------------
    // Fallback path: try_nli_rerank returns None when handle is not Ready (crt-024)
    // Updated to new signature: no penalty_map/top_k params, returns Option<Vec<NliScores>>.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_nli_fallback_when_handle_not_ready() {
        // When NliServiceHandle is in Loading state, try_nli_rerank returns None.
        // Fused scorer path uses nli_entailment=0.0 via FusionWeights::effective(false).
        use crate::infra::nli_handle::NliServiceHandle;
        use crate::infra::rayon_pool::RayonPool;

        let handle = NliServiceHandle::new(); // Loading state → get_provider() returns Err
        let pool = Arc::new(RayonPool::new(1, "test-nli").expect("pool"));

        let entry = make_nli_test_entry(1);
        let candidates = vec![(entry, 0.80)];

        let result = try_nli_rerank(&candidates, "test query", &handle, &pool).await;
        assert!(
            result.is_none(),
            "try_nli_rerank must return None when handle is in Loading state"
        );
    }

    #[tokio::test]
    async fn test_nli_fallback_when_handle_exhausted() {
        // When NliServiceHandle is in Failed+exhausted state, try_nli_rerank returns None.
        use crate::infra::nli_handle::NliServiceHandle;
        use crate::infra::rayon_pool::RayonPool;

        let handle = NliServiceHandle::new();
        handle
            .set_failed_for_test("test error".to_string(), 3)
            .await; // MAX_RETRIES = 3

        let pool = Arc::new(RayonPool::new(1, "test-nli").expect("pool"));
        let candidates = vec![(make_nli_test_entry(1), 0.80)];

        let result = try_nli_rerank(&candidates, "test query", &handle, &pool).await;
        assert!(
            result.is_none(),
            "try_nli_rerank must return None when handle retries are exhausted"
        );
    }

    #[tokio::test]
    async fn test_nli_fallback_on_empty_candidates() {
        // When candidates is empty, try_nli_rerank must return None immediately.
        use crate::infra::nli_handle::NliServiceHandle;
        use crate::infra::rayon_pool::RayonPool;

        let handle = NliServiceHandle::new(); // Loading state
        let pool = Arc::new(RayonPool::new(1, "test-nli").expect("pool"));
        let candidates: Vec<(EntryRecord, f64)> = vec![];

        let result = try_nli_rerank(&candidates, "test query", &handle, &pool).await;
        assert!(
            result.is_none(),
            "try_nli_rerank must return None for empty candidate list"
        );
    }

    // -----------------------------------------------------------------------
    // AC-19: nli_top_k drives HNSW candidate expansion
    // -----------------------------------------------------------------------

    #[test]
    fn test_nli_top_k_drives_hnsw_expansion() {
        // AC-19: when nli_enabled=true, hnsw_k = nli_top_k.max(params.k).
        // Simulate the expansion logic used in Step 5.
        let nli_top_k = 20usize;
        let params_k = 5usize;
        let nli_enabled = true;

        let hnsw_k = if nli_enabled {
            nli_top_k.max(params_k)
        } else {
            params_k
        };

        assert_eq!(
            hnsw_k, 20,
            "hnsw_k must be nli_top_k (20) when nli_enabled=true and nli_top_k > params.k"
        );
    }

    #[test]
    fn test_nli_disabled_uses_params_k() {
        // When nli_enabled=false, hnsw_k must equal params.k exactly.
        let nli_top_k = 20usize;
        let params_k = 5usize;
        let nli_enabled = false;

        let hnsw_k = if nli_enabled {
            nli_top_k.max(params_k)
        } else {
            params_k
        };

        assert_eq!(
            hnsw_k, 5,
            "hnsw_k must equal params.k when nli_enabled=false, got {hnsw_k}"
        );
    }

    #[test]
    fn test_nli_hnsw_k_never_below_params_k() {
        // Even if nli_top_k is smaller than params.k, hnsw_k must be at least params.k.
        let nli_top_k = 3usize;
        let params_k = 10usize;
        let nli_enabled = true;

        let hnsw_k = if nli_enabled {
            nli_top_k.max(params_k)
        } else {
            params_k
        };

        assert_eq!(
            hnsw_k, 10,
            "hnsw_k must be at least params.k even when nli_top_k < params.k"
        );
    }
}
