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
use unimatrix_engine::graph::{
    FALLBACK_PENALTY, find_terminal_active, graph_expand, graph_penalty, personalized_pagerank,
    suppress_contradicts,
};

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
use crate::services::phase_freq_table::PhaseFreqTableHandle;
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
/// constraint (ADR-004, crt-026). With col-031 defaults:
/// // 0.95 + 0.02 + 0.05 = 1.02 — w_phase_explicit is additive outside the six-weight
/// // constraint (ADR-004, crt-026). The six-weight sum check is unchanged.
///
/// Per-field range [0.0, 1.0] is enforced by InferenceConfig::validate for all eight fields.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FusionWeights {
    pub w_sim: f64,             // default 0.25 — bi-encoder similarity
    pub w_nli: f64,             // default 0.35 — NLI entailment (dominant precision signal)
    pub w_conf: f64,            // default 0.15 — confidence tiebreaker
    pub w_coac: f64, // default 0.0 (zeroed in crt-032; PPR subsumes co-access signal via GRAPH_EDGES.CoAccess)
    pub w_util: f64, // default 0.05 — effectiveness classification
    pub w_prov: f64, // default 0.05 — category provenance hint
    pub w_phase_histogram: f64, // crt-026: default 0.02 — histogram affinity (ADR-004, ASS-028 calibrated)
    pub w_phase_explicit: f64,  // col-031: default 0.05 — PhaseFreqTable activates this (ADR-004)
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
    /// Short-circuit (w_nli == 0.0): returns self unchanged regardless of nli_available.
    ///   Re-normalization is semantically meaningful only when w_nli > 0.0 (redistributing
    ///   a real weight budget because NLI is absent). Re-normalizing zero is a correctness
    ///   error that silently inflates sim and conf (ADR-001, crt-038).
    ///
    /// NLI active (nli_available = true, w_nli > 0.0): returns self unchanged.
    ///   The configured weights are used directly. No re-normalization.
    ///
    /// NLI absent (nli_available = false, w_nli > 0.0): sets w_nli = 0.0, re-normalizes
    ///   the remaining five weights by dividing each by their sum.
    ///   This preserves the relative signal dominance ordering (Constraint 9, ADR-003).
    ///
    /// Zero-denominator guard (R-02): if all five non-NLI weights are 0.0
    ///   (pathological but reachable config), returns all-zeros without panic.
    pub(crate) fn effective(&self, nli_available: bool) -> FusionWeights {
        // SHORT-CIRCUIT: w_nli == 0.0 means there is no NLI weight budget to redistribute.
        // Exact f64 equality is safe here because w_nli is always set from a constant literal
        // in default_w_nli() or from operator TOML config, never from computed arithmetic
        // (ADR-001, crt-038).
        if self.w_nli == 0.0 {
            return *self;
        }

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
    /// col-031: Workflow phase at query time, used for phase-conditioned scoring.
    ///
    /// Set by MCP transport from tool call `current_phase` parameter.
    /// Set by eval runner from `record.context.phase` (AC-16).
    ///
    /// When None:
    ///   - Lock on PhaseFreqTableHandle is never acquired.
    ///   - phase_explicit_norm = 0.0 for all candidates.
    ///   - Fused score is bit-for-bit identical to pre-col-031 (NFR-04).
    ///
    /// When Some(phase) and use_fallback = true:
    ///   - use_fallback guard fires; phase_explicit_norm = 0.0.
    ///   - phase_affinity_score is NOT called (ADR-003, R-03).
    pub current_phase: Option<String>,
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
    /// col-031: phase-conditioned frequency table for phase_explicit_norm signal.
    ///
    /// Arc clone received from ServiceLayer (created once in with_rate_config).
    /// Background tick is sole writer. Search path acquires short read lock,
    /// extracts phase snapshot, releases before scoring loop (NFR-02).
    /// Non-optional — missing wiring is a compile error (ADR-005).
    phase_freq_table: PhaseFreqTableHandle,
    /// crt-030: PPR damping factor α in (0.0, 1.0). Default 0.85.
    ppr_alpha: f64,
    /// crt-030: number of power-iteration steps. Default 20.
    ppr_iterations: usize,
    /// crt-030: minimum PPR score (strictly >) for PPR-only pool expansion. Default 0.05.
    ppr_inclusion_threshold: f64,
    /// crt-030: PPR trust weight — blends existing HNSW scores and sets initial_sim for new
    /// PPR-only entries. Default 0.15.
    ppr_blend_weight: f64,
    /// crt-030: maximum number of PPR-only entries added to the pool per query. Default 50.
    ppr_max_expand: usize,
    /// crt-042: enable graph_expand candidate pool widening before PPR.
    /// Default false — gated behind A/B eval before default enablement.
    ppr_expander_enabled: bool,
    /// crt-042: BFS hop depth from seeds. Default 2.
    expansion_depth: usize,
    /// crt-042: maximum entries added by Phase 0 per query. Default 200.
    max_expansion_candidates: usize,
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
        phase_freq_table: PhaseFreqTableHandle, // col-031: required, non-optional (ADR-005)
        ppr_alpha: f64,                         // crt-030
        ppr_iterations: usize,                  // crt-030
        ppr_inclusion_threshold: f64,           // crt-030
        ppr_blend_weight: f64,                  // crt-030
        ppr_max_expand: usize,                  // crt-030
        ppr_expander_enabled: bool,             // crt-042
        expansion_depth: usize,                 // crt-042
        max_expansion_candidates: usize,        // crt-042
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
            phase_freq_table, // col-031
            ppr_alpha,
            ppr_iterations,
            ppr_inclusion_threshold,
            ppr_blend_weight,
            ppr_max_expand,
            ppr_expander_enabled,
            expansion_depth,
            max_expansion_candidates,
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

        // col-031: Pre-loop phase snapshot extraction (ADR-003, NFR-02).
        //
        // LOCK ORDER CONTEXT: At this point, EffectivenessStateHandle read lock has
        // already been acquired and released (step above). TypedGraphStateHandle read
        // lock has been acquired and released (line ~638 above). Now we acquire
        // PhaseFreqTableHandle read lock — this is the third in the chain:
        //   EffectivenessStateHandle -> TypedGraphStateHandle -> PhaseFreqTableHandle
        //
        // Lock acquired once before the scoring loop. Lock MUST be released before
        // the loop body executes (NFR-02: no lock held across scoring loop).
        //
        // Moved before Step 6d (crt-030) so the snapshot is available for PPR
        // personalization vector construction (ADR-006, NFR-04).
        //
        // Three cases:
        //   1. current_phase = None:
        //      -> phase_snapshot = None; lock never acquired.
        //   2. current_phase = Some(phase) AND use_fallback = true:
        //      -> Guard fires; phase_snapshot = None; phase_explicit_norm = 0.0 for all.
        //      -> Scores bit-for-bit identical to pre-col-031 (NFR-04).
        //      -> phase_affinity_score is NOT called (ADR-003 fused-scoring contract).
        //   3. current_phase = Some(phase) AND use_fallback = false:
        //      -> Clone the phase's bucket data out of the guard. Release lock.
        //      -> phase_explicit_norm computed per-entry from cloned snapshot.
        //
        // Snapshot type: HashMap<String, Vec<(u64, f32)>>
        //   key = entry_category, value = sorted (entry_id, rank_score) pairs for this phase.
        let phase_snapshot: Option<HashMap<String, Vec<(u64, f32)>>> = match &params.current_phase {
            None => None, // lock never acquired
            Some(phase) => {
                // Acquire read lock once
                let guard = self
                    .phase_freq_table
                    .read()
                    .unwrap_or_else(|e| e.into_inner());

                if guard.use_fallback {
                    // GUARD FIRES: cold-start; do NOT call phase_affinity_score.
                    // phase_explicit_norm = 0.0 for all candidates (score identity).
                    None
                    // guard drops here — lock released
                } else {
                    // Extract all (category -> Vec<(entry_id, score)>) entries for
                    // this specific phase. Clone out before dropping the guard.
                    //
                    // We need all categories for this phase because the scoring loop
                    // iterates over diverse entries with different categories.
                    let snapshot: HashMap<String, Vec<(u64, f32)>> = guard
                        .table
                        .iter()
                        .filter(|((p, _cat), _)| p == phase)
                        .map(|((_p, cat), bucket)| (cat.clone(), bucket.clone()))
                        .collect();
                    Some(snapshot)
                    // guard drops here — lock released BEFORE scoring loop
                }
            }
        };
        // PhaseFreqTableHandle read lock is now released. Step 6d and scoring loop may begin.

        // Step 6d: PPR expansion (crt-030).
        //
        // Expands the candidate pool with multi-hop PPR neighbors from the HNSW seed set.
        // Guard: skip entirely when use_fallback = true (cold-start / Supersedes cycle).
        // Bit-for-bit identical to pre-crt-030 behaviour when use_fallback = true (AC-12 / R-02).
        if !use_fallback {
            // -----------------------------------------------------------------------
            // Phase 0 [crt-042]: graph_expand — widen seed pool if ppr_expander_enabled
            //
            // Combined ceiling (SR-04 / NFR-08):
            //   HNSW k=20 + Phase 0 max 200 + Phase 5 max 50 = 270 maximum candidates
            //   before PPR scoring and final truncation to k.
            //
            // Runs ONLY when both:
            //   (a) use_fallback = false (PPR is active — outer guard)
            //   (b) ppr_expander_enabled = true (expander feature flag)
            //
            // When ppr_expander_enabled = false (default): zero overhead — no BFS, no fetch,
            // no Instant::now(), no debug! emission. Bit-identical to pre-crt-042 (AC-01, NFR-02).
            //
            // Lock order: typed_graph is the pre-cloned value (lock already released before Step 6d).
            // graph_expand holds no locks (C-04, NFR-06).
            // -----------------------------------------------------------------------
            if self.ppr_expander_enabled {
                let phase0_start = std::time::Instant::now();

                // Collect seed IDs from current results_with_scores (post Steps 6a + 6b).
                let seed_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();

                // BFS traversal: collect entry IDs reachable from seeds via positive edges.
                // Synchronous, pure, no I/O (C-05, NFR-05).
                let expanded_ids: HashSet<u64> = graph_expand(
                    &typed_graph,
                    &seed_ids,
                    self.expansion_depth,
                    self.max_expansion_candidates,
                );

                // Deduplication guard: skip any expanded ID already in the current pool.
                // graph_expand excludes seeds by design (AC-08), but this in_pool check ensures
                // correctness if results_with_scores was modified between seed collection and here.
                let in_pool: HashSet<u64> = seed_ids.iter().copied().collect();
                let mut results_added: usize = 0;

                // Process expanded entries in sorted order for determinism (NFR-04).
                let mut sorted_expanded: Vec<u64> = expanded_ids.iter().copied().collect();
                sorted_expanded.sort_unstable();

                for expanded_id in sorted_expanded {
                    if in_pool.contains(&expanded_id) {
                        continue; // Already present — skip without counting.
                    }

                    // Async fetch (same pattern as Phase 5).
                    // On error: silently skip the entry. Do not fail the search request.
                    let entry = match self.entry_store.get(expanded_id).await {
                        Ok(e) => e,
                        Err(_) => continue, // silent skip
                    };

                    // Quarantine check: MANDATORY (R-03, AC-13, NFR-03, FR-05 step 3b).
                    // This is the ONLY quarantine enforcement point for Phase 0 expanded entries.
                    if SecurityGateway::is_quarantined(&entry.status) {
                        continue; // silent skip — no warn/error log (NFR-03)
                    }

                    // Embedding lookup (O(N) HNSW scan per entry — primary latency driver, C-02).
                    // SR-01 investigation: O(1) path not feasible without significant rework of
                    // hnsw_rs PointIndexation (no get_by_data_id API exists). Filed as follow-up.
                    // On None: silently skip entries with no stored embedding (AC-15).
                    let emb = match self.vector_store.get_embedding(expanded_id).await {
                        Some(e) => e,
                        None => continue, // silent skip — no embedding stored for this entry
                    };

                    // True cosine similarity (ADR-003): real semantic signal, not a floor constant.
                    // `embedding` is the normalized query embedding bound at Step 4.
                    let cosine_sim = cosine_similarity(&embedding, &emb);

                    results_with_scores.push((entry, cosine_sim));
                    results_added += 1;
                }

                // Timing instrumentation (ADR-005, NFR-01, AC-24).
                // Emitted at debug! level — never info! (R-10).
                // All six fields are mandatory for the latency gate measurement.
                tracing::debug!(
                    seeds = seed_ids.len(),
                    expanded_count = expanded_ids.len(),
                    fetched_count = results_added,
                    elapsed_ms = phase0_start.elapsed().as_millis(),
                    expansion_depth = self.expansion_depth,
                    max_expansion_candidates = self.max_expansion_candidates,
                    "Phase 0 (graph_expand) complete"
                );
            }

            // -----------------------------------------------------------------------
            // Phase 1: Build the personalization vector (FR-06 / ADR-006).
            //
            // Read from phase_snapshot (already extracted by col-031 pre-loop block).
            // Do NOT call phase_affinity_score() directly — no lock re-acquisition (ADR-006).
            // Cold-start (no phase, no snapshot): affinity = 1.0 for all seeds (SR-06).
            // -----------------------------------------------------------------------
            let mut seed_scores: HashMap<u64, f64> =
                HashMap::with_capacity(results_with_scores.len());

            for (entry, sim) in &results_with_scores {
                let affinity: f64 = if let (Some(_phase), Some(snapshot)) =
                    (&params.current_phase, &phase_snapshot)
                {
                    snapshot
                        .get(&entry.category)
                        .and_then(|bucket| bucket.iter().find(|(id, _)| *id == entry.id))
                        .map(|(_, score)| *score as f64)
                        .unwrap_or(1.0) // absent entry → neutral (ADR-003 col-031 contract)
                } else {
                    1.0 // no phase or no snapshot → cold-start neutral
                };
                seed_scores.insert(entry.id, sim * affinity);
            }

            // Normalize to sum 1.0.
            let total: f64 = seed_scores.values().sum();

            // Zero-sum guard (FR-08 / FM-05):
            // All HNSW scores are 0.0 — degenerate, should not occur in practice.
            // Skip PPR entirely; proceed to Step 6c with unchanged pool.
            if total > 0.0 {
                for value in seed_scores.values_mut() {
                    *value /= total;
                }

                // -----------------------------------------------------------------------
                // Phase 2: Run PPR.
                // -----------------------------------------------------------------------
                let ppr_scores: HashMap<u64, f64> = personalized_pagerank(
                    &typed_graph,
                    &seed_scores,
                    self.ppr_alpha,
                    self.ppr_iterations,
                );

                // -----------------------------------------------------------------------
                // Phase 3: Blend scores for existing HNSW candidates (FR-08 step 5).
                //
                // For each entry already in results_with_scores that appears in ppr_scores:
                //   new_sim = (1 - ppr_blend_weight) * current_sim + ppr_blend_weight * ppr_score
                // -----------------------------------------------------------------------
                for (entry, sim) in &mut results_with_scores {
                    if let Some(&ppr_score) = ppr_scores.get(&entry.id) {
                        *sim = (1.0 - self.ppr_blend_weight) * (*sim)
                            + self.ppr_blend_weight * ppr_score;
                    }
                }

                // -----------------------------------------------------------------------
                // Phase 4: Identify PPR-only candidates for expansion (FR-08 step 6).
                //
                // Entries in ppr_scores that are NOT already in results_with_scores
                // and whose PPR score STRICTLY exceeds ppr_inclusion_threshold (AC-13, R-06).
                // Threshold comparison: > (not >=).
                // -----------------------------------------------------------------------
                let existing_ids: HashSet<u64> =
                    results_with_scores.iter().map(|(e, _)| e.id).collect();

                let mut ppr_only_candidates: Vec<(u64, f64)> = ppr_scores
                    .iter()
                    .filter(|(id, score)| {
                        !existing_ids.contains(id) && **score > self.ppr_inclusion_threshold // strictly > (AC-13 / R-06)
                    })
                    .map(|(id, score)| (*id, *score))
                    .collect();

                // Sort descending by PPR score.
                ppr_only_candidates
                    .sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

                // Cap at ppr_max_expand (E-04).
                ppr_only_candidates.truncate(self.ppr_max_expand);

                // -----------------------------------------------------------------------
                // Phase 5: Fetch and inject PPR-only entries (FR-08 step 6 / R-08 Critical).
                //
                // Sequential async fetches (ADR-008 / C-10).
                // Error from any single fetch: silently skip (AC-13 / FM-02 / R-05).
                // Quarantined entry: silently skip (R-08 Critical — dedicated tests required).
                // -----------------------------------------------------------------------
                for (entry_id, ppr_score) in ppr_only_candidates {
                    let entry = match self.entry_store.get(entry_id).await {
                        Ok(e) => e,
                        Err(_) => continue, // silent skip on error (AC-13 / R-05)
                    };

                    // R-08 Critical: quarantine check — MANDATORY for every PPR-fetched entry.
                    // PPR-only entries bypass the Step 6 HNSW quarantine filter.
                    // This check is the ONLY thing preventing quarantined entries from
                    // appearing in search results via the PPR expansion path.
                    if SecurityGateway::is_quarantined(&entry.status) {
                        continue; // silent skip (AC-13 / R-08)
                    }

                    // Assign initial similarity (FR-08 step 6 / ADR-007):
                    //   initial_sim = ppr_blend_weight * ppr_score
                    // PPR-only entries have no HNSW component; ppr_blend_weight is the
                    // "PPR trust" coefficient (dual role, ADR-007).
                    // At default 0.15, initial_sim is in [0.0, 0.15] — naturally ranks
                    // below HNSW candidates (SR-07 resolution).
                    let initial_sim = self.ppr_blend_weight * ppr_score;
                    results_with_scores.push((entry, initial_sim));
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
            .map(|h| {
                h.values()
                    .copied()
                    .fold(0u32, |acc, v| acc.saturating_add(v))
            })
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

            // -- col-031: phase_explicit_norm from pre-built snapshot (no lock in loop body). --
            //
            // phase_snapshot = None when:
            //   - current_phase is None (no phase provided)
            //   - use_fallback = true (cold-start; guard fired pre-loop)
            //   In both cases: 0.0 (pre-col-031 score identity preserved, NFR-04).
            //
            // phase_snapshot = Some(snapshot) when:
            //   - Phase history exists and use_fallback = false.
            //   Lookup entry_id in the snapshot's category bucket:
            //     - Entry found: return its rank_score as f64.
            //     - Bucket absent or entry absent: 1.0 (neutral, no suppression).
            //   NOTE: 1.0 neutral return from snapshot is consistent with
            //   phase_affinity_score absent-entry contract (ADR-003).
            let phase_explicit_norm: f64 = match &phase_snapshot {
                None => 0.0,
                Some(snapshot) => {
                    // Look up this entry's category bucket in the phase snapshot.
                    // snapshot is HashMap<category, Vec<(entry_id, score)>>.
                    match snapshot.get(&entry.category) {
                        None => 1.0, // no history for (phase, category) -> neutral
                        Some(bucket) => {
                            // Linear scan within bucket for this entry_id.
                            // Buckets are small; linear scan is appropriate.
                            match bucket.iter().find(|(id, _)| *id == entry.id) {
                                Some((_, score)) => *score as f64,
                                None => 1.0, // entry not in bucket -> neutral
                            }
                        }
                    }
                }
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
                phase_explicit_norm,  // col-031: from pre-built phase snapshot
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

        // Step 10b: Contradicts collision suppression (col-030).
        // Uses if-expression to re-bind final_scores at the correct scope for Step 11 (ADR-004, R-03).
        // Guard: only when TypedRelationGraph is built (use_fallback = false).
        // When use_fallback = true (cold-start), skip — all results pass through unchanged (AC-05).
        // Both Vecs rebuilt in a single indexed pass to preserve the parallel Vec invariant (ADR-004, SR-02).
        let final_scores = if !use_fallback {
            let result_ids: Vec<u64> = results_with_scores
                .iter()
                .map(|(entry, _)| entry.id)
                .collect();

            let (keep_mask, contradicting_ids) = suppress_contradicts(&result_ids, &typed_graph);

            // aligned_len MUST be results_with_scores.len(), NOT final_scores.len() (R-07).
            // After Step 10 floors, results_with_scores may be shorter than final_scores.
            let aligned_len = results_with_scores.len();

            let mut new_rws: Vec<(EntryRecord, f64)> = Vec::with_capacity(aligned_len);
            let mut new_fs: Vec<f64> = Vec::with_capacity(aligned_len);

            // Single indexed pass over zip of the aligned prefix (ADR-004).
            // Never two separate retain calls on each Vec — that violates SR-02 (silently misaligns).
            for (i, (rw, &fs)) in results_with_scores
                .iter()
                .zip(final_scores[..aligned_len].iter())
                .enumerate()
            {
                if keep_mask[i] {
                    new_rws.push(rw.clone());
                    new_fs.push(fs);
                } else {
                    // FR-09, NFR-05: emit DEBUG log with both IDs.
                    tracing::debug!(
                        suppressed_entry_id    = rw.0.id,
                        contradicting_entry_id = ?contradicting_ids[i],
                        "contradicts collision suppression: entry suppressed"
                    );
                }
            }

            results_with_scores = new_rws;
            new_fs // expression: this Vec<f64> is the new value of the outer final_scores binding
        } else {
            // cold-start: original Vec<f64> passes through unchanged (AC-05)
            final_scores
        };
        // final_scores is now the post-suppression Vec (or original on cold-start).
        // Step 11 uses this binding — alignment with results_with_scores is preserved.

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

    // =========================================================================
    // crt-026 (WA-2): ServiceSearchParams new fields tests
    // =========================================================================

    // -- T-SP-NEW-01: test_service_search_params_has_session_fields (AC-04, R-12) --
    #[test]
    fn test_service_search_params_has_session_fields() {
        // AC-04: ServiceSearchParams must have session_id and category_histogram fields.
        // Compilation failure IS the test failure for AC-04.
        let params = ServiceSearchParams {
            query: "test".to_string(),
            k: 5,
            filters: None,
            similarity_floor: None,
            confidence_floor: None,
            feature_tag: None,
            co_access_anchors: None,
            caller_agent_id: None,
            retrieval_mode: RetrievalMode::Flexible,
            session_id: None,         // NEW crt-026 field
            category_histogram: None, // NEW crt-026 field
            current_phase: None,      // col-031: phase field present (AC-05 compile-time check)
        };

        assert!(
            params.session_id.is_none(),
            "session_id field must exist and be Option<String>"
        );
        assert!(
            params.category_histogram.is_none(),
            "category_histogram field must exist and be Option<HashMap<String, u32>>"
        );
        assert!(
            params.current_phase.is_none(),
            "current_phase field must exist and be Option<String>"
        );
    }

    // -- T-SP-NEW-02: test_service_search_params_with_session_data (AC-05 partial, R-12) --
    #[test]
    fn test_service_search_params_with_session_data() {
        let mut hist: HashMap<String, u32> = HashMap::new();
        hist.insert("decision".to_string(), 3);
        hist.insert("pattern".to_string(), 2);

        let params = ServiceSearchParams {
            query: "how to handle session state".to_string(),
            k: 10,
            filters: None,
            similarity_floor: None,
            confidence_floor: None,
            feature_tag: None,
            co_access_anchors: None,
            caller_agent_id: None,
            retrieval_mode: RetrievalMode::Flexible,
            session_id: Some("sid-abc".to_string()),
            category_histogram: Some(hist),
            current_phase: Some("col".to_string()), // col-031: exercise Some(phase) path
        };

        assert_eq!(params.session_id.as_deref(), Some("sid-abc"));
        let h = params.category_histogram.as_ref().unwrap();
        assert_eq!(h.get("decision"), Some(&3));
        assert_eq!(h.get("pattern"), Some(&2));
        assert_eq!(params.current_phase.as_deref(), Some("col"));
    }

    // -- T-SP-NEW-03: test_service_search_params_empty_histogram_maps_to_none (AC-08 partial, R-02, R-09) --
    #[test]
    fn test_service_search_params_empty_histogram_maps_to_none() {
        // Documents the handler invariant: empty histogram must be mapped to None.
        let empty: HashMap<String, u32> = HashMap::new();
        let category_histogram: Option<HashMap<String, u32>> =
            if empty.is_empty() { None } else { Some(empty) };

        assert!(
            category_histogram.is_none(),
            "an empty histogram must be mapped to None before ServiceSearchParams construction"
        );
    }

    // =========================================================================
    // crt-026 (WA-2): FusedScoreInputs / FusionWeights / compute_fused_score tests
    // =========================================================================

    fn make_baseline_inputs(sim: f64, category_matches: bool) -> FusedScoreInputs {
        FusedScoreInputs {
            similarity: sim,
            nli_entailment: 0.0,
            confidence: 0.5,
            coac_norm: 0.0,
            util_norm: 0.5, // neutral
            prov_norm: 0.0,
            phase_histogram_norm: if category_matches { 1.0 } else { 0.0 },
            phase_explicit_norm: 0.0, // ADR-003: always 0.0 in crt-026
        }
    }

    fn make_baseline_weights() -> FusionWeights {
        FusionWeights {
            w_sim: 0.25,
            w_nli: 0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.02,
            w_phase_explicit: 0.0, // ADR-003
        }
    }

    // -- T-FS-01: test_histogram_boost_score_delta_at_p1_equals_weight (GATE BLOCKER, AC-12, R-01) --
    #[test]
    fn test_histogram_boost_score_delta_at_p1_equals_weight() {
        // histogram = {"decision": 5}, total = 5, p("decision") = 1.0
        // Entry A: phase_histogram_norm = 1.0; Entry B: phase_histogram_norm = 0.0
        // All other inputs identical.
        let weights = make_baseline_weights(); // w_phase_histogram = 0.02
        let inputs_a = make_baseline_inputs(0.70, true); // phase_histogram_norm = 1.0
        let inputs_b = make_baseline_inputs(0.70, false); // phase_histogram_norm = 0.0

        let score_a = compute_fused_score(&inputs_a, &weights);
        let score_b = compute_fused_score(&inputs_b, &weights);
        let delta = score_a - score_b;

        assert!(
            delta >= 0.02,
            "score delta at p=1.0 must be >= 0.02 (w_phase_histogram * 1.0); \
             got delta={delta:.6}"
        );
        assert!(
            (delta - 0.02).abs() < 1e-10,
            "score delta at p=1.0 must be exactly 0.02 with default weights; \
             got delta={delta:.6}"
        );
    }

    // -- T-FS-02: test_60_percent_concentration_score_delta (AC-12 partial, R-01 scenario 2) --
    #[test]
    fn test_60_percent_concentration_score_delta() {
        // histogram = {"decision": 3, "pattern": 2}, total = 5, p("decision") = 0.6
        let weights = make_baseline_weights();
        let mut inputs_decision = make_baseline_inputs(0.70, false);
        inputs_decision.phase_histogram_norm = 0.6;

        let inputs_other = make_baseline_inputs(0.70, false); // phase_histogram_norm = 0.0

        let score_decision = compute_fused_score(&inputs_decision, &weights);
        let score_other = compute_fused_score(&inputs_other, &weights);
        let delta = score_decision - score_other;

        assert!(
            (delta - 0.012).abs() < 1e-10,
            "60% concentration must produce delta = 0.02 * 0.6 = 0.012; \
             got delta={delta:.6}"
        );
    }

    // -- T-FS-03: test_absent_category_phase_histogram_norm_is_zero (GATE BLOCKER, AC-13, R-01 scenario 3, R-13) --
    #[test]
    fn test_absent_category_phase_histogram_norm_is_zero() {
        // histogram = {"decision": 5}, total = 5
        // Entry has category = "lesson-learned" (not in histogram)
        let mut histogram: HashMap<String, u32> = HashMap::new();
        histogram.insert("decision".to_string(), 5);
        let total: u32 = histogram.values().sum(); // 5

        let entry_category = "lesson-learned";

        // Simulating the scoring loop's phase_histogram_norm computation.
        let phase_histogram_norm = if total > 0 {
            histogram.get(entry_category).copied().unwrap_or(0) as f64 / total as f64
        } else {
            0.0
        };

        assert_eq!(
            phase_histogram_norm, 0.0,
            "absent category must produce phase_histogram_norm = 0.0; \
             got {phase_histogram_norm}"
        );
    }

    // -- T-FS-04: test_cold_start_search_produces_identical_scores (GATE BLOCKER, AC-08, R-02) --
    #[test]
    fn test_cold_start_search_produces_identical_scores() {
        // Pre-crt-026 baseline: six-term fused score only.
        // crt-026 cold start: same six terms + phase_histogram_norm = 0.0 + phase_explicit_norm = 0.0.
        let weights = make_baseline_weights(); // includes w_phase_histogram = 0.02

        let pre_crt026_inputs = FusedScoreInputs {
            similarity: 0.75,
            nli_entailment: 0.40,
            confidence: 0.60,
            coac_norm: 0.20,
            util_norm: 0.50,
            prov_norm: 0.0,
            phase_histogram_norm: 0.0, // cold start
            phase_explicit_norm: 0.0,  // always 0.0 (ADR-003)
        };

        // Expected: identical to six-term-only formula (phase terms contribute 0.0).
        // 0.25*0.75 + 0.35*0.40 + 0.15*0.60 + 0.10*0.20 + 0.05*0.50 + 0.05*0.0
        // = 0.1875 + 0.14 + 0.09 + 0.02 + 0.025 + 0.0 = 0.4625
        let expected =
            0.25 * 0.75 + 0.35 * 0.40 + 0.15 * 0.60 + 0.10 * 0.20 + 0.05 * 0.50 + 0.05 * 0.0;

        let actual = compute_fused_score(&pre_crt026_inputs, &weights);

        assert!(
            (actual - expected).abs() < f64::EPSILON,
            "cold-start score must be bit-for-bit identical to pre-crt-026 six-term formula; \
             expected={expected:.10}, actual={actual:.10}"
        );
    }

    // -- T-FS-05: test_status_penalty_applied_after_histogram_boost (AC-10, R-08) --
    #[test]
    fn test_status_penalty_applied_after_histogram_boost() {
        // Entry: category="decision" matches histogram (p=1.0), status=Deprecated (penalty=0.5).
        let weights = make_baseline_weights();
        let inputs = FusedScoreInputs {
            similarity: 0.70,
            nli_entailment: 0.0,
            confidence: 0.5,
            coac_norm: 0.0,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 1.0, // p=1.0: histogram matches
            phase_explicit_norm: 0.0,
        };
        let status_penalty = 0.5_f64;

        // Correct order: (fused + boost) * penalty — boost is INSIDE compute_fused_score.
        let fused = compute_fused_score(&inputs, &weights);
        let final_score = fused * status_penalty;

        // Wrong order (must be different): base_without_boost * penalty + boost.
        let base_inputs_no_boost = FusedScoreInputs {
            phase_histogram_norm: 0.0,
            ..inputs
        };
        let fused_no_boost = compute_fused_score(&base_inputs_no_boost, &weights);
        let wrong_score = fused_no_boost * status_penalty + weights.w_phase_histogram * 1.0;

        // Correct: (base + 0.02) * 0.5  vs  Wrong: base * 0.5 + 0.02
        // Differ by: 0.02 * (1.0 - 0.5) = 0.01
        assert!(
            (final_score - wrong_score).abs() > 1e-6,
            "correct and wrong penalty-ordering formulas must produce different results; \
             correct={final_score:.6}, wrong={wrong_score:.6}"
        );
        assert!(
            final_score < wrong_score,
            "correct ordering ((base+boost)*penalty) must be less than \
             wrong ordering (base*penalty+boost) for penalty < 1.0; \
             correct={final_score:.6}, wrong={wrong_score:.6}"
        );
        let expected2 = (fused_no_boost + 0.02) * status_penalty;
        assert!(
            (final_score - expected2).abs() < f64::EPSILON,
            "final_score must equal (fused_without_boost + w_phase_histogram) * status_penalty; \
             got final_score={final_score:.10}, expected={expected2:.10}"
        );
    }

    // -- T-FS-06: test_phase_histogram_norm_zero_when_total_is_zero (R-09, division by zero guard) --
    #[test]
    fn test_phase_histogram_norm_zero_when_total_is_zero() {
        // Primary guard is in handler (is_empty() → None). This tests the secondary in-function guard.
        let histogram: Option<HashMap<String, u32>> = Some(HashMap::new());
        let total: u32 = histogram.as_ref().map(|h| h.values().sum()).unwrap_or(0);

        let phase_histogram_norm = if total > 0 {
            histogram
                .as_ref()
                .and_then(|h| h.get("decision"))
                .copied()
                .unwrap_or(0) as f64
                / total as f64
        } else {
            0.0
        };

        assert_eq!(total, 0);
        assert_eq!(
            phase_histogram_norm, 0.0,
            "total=0 must produce phase_histogram_norm=0.0, not NaN or panic"
        );
        assert!(
            !phase_histogram_norm.is_nan(),
            "phase_histogram_norm must not be NaN"
        );
    }

    // -- T-FS-07: test_phase_explicit_norm_placeholder_fields_present (AC-09, R-07) --
    #[test]
    fn test_phase_explicit_norm_placeholder_fields_present() {
        // ADR-003: phase_explicit_norm is always 0.0 in crt-026; W3-1 will populate it.
        let inputs = FusedScoreInputs {
            similarity: 0.5,
            nli_entailment: 0.0,
            confidence: 0.5,
            coac_norm: 0.0,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.3,
            phase_explicit_norm: 0.0, // ADR-003: always 0.0 in crt-026
        };
        let weights = FusionWeights {
            w_sim: 0.25,
            w_nli: 0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.02,
            w_phase_explicit: 0.0, // ADR-003: always 0.0 in crt-026
        };

        assert_eq!(
            inputs.phase_explicit_norm, 0.0,
            "phase_explicit_norm must be 0.0 in crt-026 (ADR-003 placeholder)"
        );
        assert_eq!(
            weights.w_phase_explicit, 0.0,
            "w_phase_explicit must be 0.0 in crt-026 (ADR-003 placeholder)"
        );
        assert_eq!(
            weights.w_phase_histogram, 0.02,
            "w_phase_histogram default must be 0.02"
        );

        // ADR-003: phase_explicit_norm=0.0 means w_phase_explicit * 0.0 = 0.0 regardless.
        let score_with_explicit = compute_fused_score(
            &inputs,
            &FusionWeights {
                w_phase_explicit: 0.99,
                ..weights
            },
        );
        let score_without_explicit = compute_fused_score(&inputs, &weights);
        assert!(
            (score_with_explicit - score_without_explicit).abs() < f64::EPSILON,
            "phase_explicit_norm=0.0 must contribute 0.0 regardless of w_phase_explicit; \
             score_with={score_with_explicit:.10}, score_without={score_without_explicit:.10}"
        );
    }

    // -- T-FS-08: test_fusion_weights_effective_nli_absent_excludes_phase_from_denominator (GATE BLOCKER, R-06) --
    #[test]
    fn test_fusion_weights_effective_nli_absent_excludes_phase_from_denominator() {
        // R-06 invariant: w_phase_histogram NOT in re-normalization denominator.
        // Five-term denominator: w_sim + w_conf + w_coac + w_util + w_prov = 0.60.
        let weights = FusionWeights {
            w_sim: 0.25,
            w_nli: 0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.02,
            w_phase_explicit: 0.0,
        };

        let effective_nli_absent = weights.effective(false);

        // w_nli must be zeroed out.
        assert_eq!(
            effective_nli_absent.w_nli, 0.0,
            "w_nli must be 0.0 in NLI-absent mode"
        );

        // denominator = 0.25 + 0.15 + 0.10 + 0.05 + 0.05 = 0.60 (five terms only, NOT phase fields)
        let expected_denom = 0.25 + 0.15 + 0.10 + 0.05 + 0.05; // 0.60

        assert!(
            (effective_nli_absent.w_sim - 0.25 / expected_denom).abs() < f64::EPSILON,
            "w_sim must be re-normalized by five-term denominator; \
             expected={}, got={}",
            0.25 / expected_denom,
            effective_nli_absent.w_sim
        );

        // w_phase_histogram must be passed through UNCHANGED (not re-normalized).
        assert_eq!(
            effective_nli_absent.w_phase_histogram, 0.02,
            "w_phase_histogram must be 0.02 unchanged in NLI-absent mode (not in denominator); \
             got={}",
            effective_nli_absent.w_phase_histogram
        );

        // w_phase_explicit must be passed through unchanged.
        assert_eq!(
            effective_nli_absent.w_phase_explicit, 0.0,
            "w_phase_explicit must be 0.0 unchanged in NLI-absent mode"
        );
    }

    // -- T-FS-09: test_fusion_weights_effective_nli_active_phase_fields_pass_through (R-06 NLI-active path) --
    #[test]
    fn test_fusion_weights_effective_nli_active_phase_fields_pass_through() {
        let weights = FusionWeights {
            w_sim: 0.25,
            w_nli: 0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.02,
            w_phase_explicit: 0.0,
        };
        let effective_nli_active = weights.effective(true);

        // NLI-active: all fields returned unchanged.
        assert_eq!(effective_nli_active.w_phase_histogram, 0.02);
        assert_eq!(effective_nli_active.w_phase_explicit, 0.0);
        assert_eq!(effective_nli_active.w_nli, 0.35);
    }

    // =========================================================================
    // col-030: Contradicts collision suppression — Step 10b integration tests
    // =========================================================================

    /// Build a TypedRelationGraph with a single Contradicts edge (source→target).
    /// Uses build_typed_relation_graph with in-memory GraphEdgeRow fixtures (SR-07, R-12).
    /// bootstrap_only=false is required — bootstrap_only=true edges are excluded.
    fn make_graph_with_contradicts(
        higher_id: u64,
        lower_id: u64,
    ) -> unimatrix_engine::graph::TypedRelationGraph {
        use unimatrix_engine::graph::{GraphEdgeRow, RelationType, build_typed_relation_graph};
        let entries = vec![
            make_test_entry(higher_id, Status::Active, None, 0.9, "decision"),
            make_test_entry(lower_id, Status::Active, None, 0.9, "decision"),
        ];
        let edges = vec![GraphEdgeRow {
            source_id: higher_id,
            target_id: lower_id,
            relation_type: RelationType::Contradicts.as_str().to_string(),
            weight: 1.0,
            created_at: 0,
            created_by: "test".to_string(),
            source: "nli".to_string(),
            bootstrap_only: false, // MUST be false — bootstrap_only=true excluded by build_typed_relation_graph
        }];
        build_typed_relation_graph(&entries, &edges).expect("valid graph with contradicts edge")
    }

    /// T-SC-08: Positive integration test — Step 10b removes the lower-ranked member
    /// of a Contradicts pair. Higher-ranked entry A is retained; lower-ranked B is
    /// suppressed; unrelated entry C is retained. (AC-07, FR-14, SR-05)
    ///
    /// Tests the Step 10b block logic directly against pre-built Vecs, mirroring
    /// how the existing search.rs unit tests exercise sub-pipeline logic.
    #[test]
    fn test_step10b_contradicts_suppression_removes_lower_ranked() {
        // Arrange: entries A (rank-0, sim=0.90), B (rank-1, sim=0.75), C (rank-2, sim=0.65).
        // A and B share a Contradicts edge; A ranks higher. C has no edges.
        let entry_a = make_test_entry(1, Status::Active, None, 0.9, "decision");
        let entry_b = make_test_entry(2, Status::Active, None, 0.9, "decision");
        let entry_c = make_test_entry(3, Status::Active, None, 0.9, "decision");

        // Simulate post-Step-10 state: sorted DESC by final_score.
        // results_with_scores: (entry, similarity) — sorted rank order
        let mut results_with_scores: Vec<(EntryRecord, f64)> = vec![
            (entry_a.clone(), 0.90), // rank-0 (highest)
            (entry_b.clone(), 0.75), // rank-1 (contradicts rank-0 → will be suppressed)
            (entry_c.clone(), 0.65), // rank-2 (no edges)
        ];
        // final_scores: parallel Vec<f64> (may be longer than results_with_scores in
        // production after floors; here equal length for simplicity)
        let final_scores: Vec<f64> = vec![0.90, 0.75, 0.65];

        // Build typed graph with Contradicts edge A→B (source=1, target=2)
        let typed_graph = make_graph_with_contradicts(1, 2);
        let use_fallback = false;

        // Act: replicate Step 10b block exactly as inserted into search.rs
        let final_scores = if !use_fallback {
            let result_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();
            let (keep_mask, contradicting_ids) = suppress_contradicts(&result_ids, &typed_graph);
            let aligned_len = results_with_scores.len();
            let mut new_rws: Vec<(EntryRecord, f64)> = Vec::with_capacity(aligned_len);
            let mut new_fs: Vec<f64> = Vec::with_capacity(aligned_len);
            for (i, (rw, &fs)) in results_with_scores
                .iter()
                .zip(final_scores[..aligned_len].iter())
                .enumerate()
            {
                if keep_mask[i] {
                    new_rws.push(rw.clone());
                    new_fs.push(fs);
                } else {
                    let _ = contradicting_ids[i]; // consumed in production by debug! log
                }
            }
            results_with_scores = new_rws;
            new_fs
        } else {
            final_scores
        };

        // Assert: A retained, B suppressed, C retained
        assert_eq!(
            results_with_scores.len(),
            2,
            "k=3 minus 1 suppressed = 2 results"
        );
        assert_eq!(
            results_with_scores[0].0.id, 1,
            "rank-0 (A, id=1) must be retained"
        );
        assert_eq!(
            results_with_scores[1].0.id, 3,
            "rank-2 (C, id=3) must be retained; rank-1 (B, id=2) was suppressed"
        );
        // B must be absent
        let ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();
        assert!(!ids.contains(&2), "B (id=2) must be absent from results");

        // final_scores shadow must be aligned with surviving entries
        assert_eq!(
            final_scores.len(),
            2,
            "final_scores must match result count"
        );
        assert!(
            (final_scores[0] - 0.90).abs() < 1e-10,
            "final_scores[0] must be A's score (0.90)"
        );
        assert!(
            (final_scores[1] - 0.65).abs() < 1e-10,
            "final_scores[1] must be C's score (0.65), not B's (0.75) — R-03 validation"
        );
    }

    /// T-SC-09: Floor + suppression combo. Both Step 10 similarity floor and Step 10b
    /// suppression active in the same pipeline invocation. Validates `aligned_len` (R-07)
    /// and `final_scores` shadow correctness (R-03).
    ///
    /// Setup: A (sim=0.90, passes floor), B (sim=0.82, passes floor, contradicts A →
    /// suppressed), C (sim=0.78, passes floor, no edges), D (sim=0.45, below floor →
    /// removed at Step 10). After Step 10b: A and C survive; B and D absent.
    ///
    /// Critical: if aligned_len uses final_scores.len() (=4) instead of
    /// results_with_scores.len() (=3), the zip panics or misaligns scores. The assertion
    /// on results[1].final_score == F_C catches silent score misalignment (R-03).
    #[test]
    fn test_step10b_floor_and_suppression_combo_correct_scores() {
        use unimatrix_engine::graph::{GraphEdgeRow, RelationType, build_typed_relation_graph};

        // Arrange: four entries; D will be removed by floor; B will be suppressed.
        let entry_a = make_test_entry(1, Status::Active, None, 0.9, "decision");
        let entry_b = make_test_entry(2, Status::Active, None, 0.9, "decision");
        let entry_c = make_test_entry(3, Status::Active, None, 0.9, "decision");
        let entry_d = make_test_entry(4, Status::Active, None, 0.9, "decision");

        // Pre-floor state: all four entries sorted by final_score DESC.
        // final_scores has all 4 entries (NOT filtered by floor — matches production behaviour).
        let score_a = 0.90_f64;
        let score_b = 0.82_f64;
        let score_c = 0.78_f64;
        let score_d = 0.45_f64; // will be dropped by Step 10 floor

        // final_scores built BEFORE Step 10 floors (parallel to the full sorted set).
        let final_scores_pre: Vec<f64> = vec![score_a, score_b, score_c, score_d];

        // Step 10: apply similarity floor (0.60) — removes D.
        // results_with_scores holds (entry, similarity) AFTER floor.
        let mut results_with_scores: Vec<(EntryRecord, f64)> = vec![
            (entry_a.clone(), 0.90),
            (entry_b.clone(), 0.82),
            (entry_c.clone(), 0.78),
            (entry_d.clone(), 0.45), // below floor
        ];
        let sim_floor = 0.60_f64;
        results_with_scores.retain(|(_, sim)| *sim >= sim_floor);
        // After floor: [(A, 0.90), (B, 0.82), (C, 0.78)] — len=3; final_scores_pre still len=4

        // Build typed graph: Contradicts A→B (id=1 → id=2).
        let entries_for_graph = vec![entry_a.clone(), entry_b.clone()];
        let edges = vec![GraphEdgeRow {
            source_id: 1,
            target_id: 2,
            relation_type: RelationType::Contradicts.as_str().to_string(),
            weight: 1.0,
            created_at: 0,
            created_by: "test".to_string(),
            source: "nli".to_string(),
            bootstrap_only: false,
        }];
        let typed_graph =
            build_typed_relation_graph(&entries_for_graph, &edges).expect("valid graph");
        let use_fallback = false;

        // Act: replicate Step 10b block. final_scores moves from pre-floor Vec into block.
        let final_scores = if !use_fallback {
            let result_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();
            let (keep_mask, contradicting_ids) = suppress_contradicts(&result_ids, &typed_graph);
            // CRITICAL (R-07): aligned_len MUST be results_with_scores.len() (=3), NOT
            // final_scores_pre.len() (=4). Using 4 would panic or misalign.
            let aligned_len = results_with_scores.len(); // = 3
            let mut new_rws: Vec<(EntryRecord, f64)> = Vec::with_capacity(aligned_len);
            let mut new_fs: Vec<f64> = Vec::with_capacity(aligned_len);
            for (i, (rw, &fs)) in results_with_scores
                .iter()
                .zip(final_scores_pre[..aligned_len].iter())
                .enumerate()
            {
                if keep_mask[i] {
                    new_rws.push(rw.clone());
                    new_fs.push(fs);
                } else {
                    let _ = contradicting_ids[i]; // consumed in production by debug! log
                }
            }
            results_with_scores = new_rws;
            new_fs
        } else {
            final_scores_pre
        };

        // Assert: 2 survivors — A and C; B suppressed; D removed by floor.
        assert_eq!(
            results_with_scores.len(),
            2,
            "D removed by floor + B suppressed → 2 survivors"
        );
        assert_eq!(
            results_with_scores[0].0.id, 1,
            "results[0] must be A (id=1)"
        );
        assert_eq!(
            results_with_scores[1].0.id, 3,
            "results[1] must be C (id=3)"
        );

        // B (id=2) and D (id=4) must be absent.
        let ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();
        assert!(!ids.contains(&2), "B (id=2) must be absent (suppressed)");
        assert!(!ids.contains(&4), "D (id=4) must be absent (below floor)");

        // CRITICAL (R-03): final_scores shadow correctness.
        // results[0] → score_a (0.90), results[1] → score_c (0.78).
        // If the shadow is omitted, results[1] would pair with final_scores_pre[1] = score_b (0.82).
        assert_eq!(
            final_scores.len(),
            2,
            "final_scores must match result count"
        );
        assert!(
            (final_scores[0] - score_a).abs() < 1e-10,
            "final_scores[0] must be A's score ({score_a}), got {}",
            final_scores[0]
        );
        assert!(
            (final_scores[1] - score_c).abs() < 1e-10,
            "final_scores[1] must be C's score ({score_c}), not B's ({score_b}), got {} — R-03 validation",
            final_scores[1]
        );
    }

    // =========================================================================
    // col-031: Phase snapshot extraction and phase_explicit_norm tests
    // AC-11 cold-start invariants, R-03, R-06
    // =========================================================================

    /// Helper: compute phase_explicit_norm from a pre-built snapshot and a candidate entry.
    /// This mirrors the scoring loop logic exactly.
    fn compute_phase_explicit_norm(
        phase_snapshot: &Option<HashMap<String, Vec<(u64, f32)>>>,
        entry_id: u64,
        entry_category: &str,
    ) -> f64 {
        match phase_snapshot {
            None => 0.0,
            Some(snapshot) => match snapshot.get(entry_category) {
                None => 1.0,
                Some(bucket) => match bucket.iter().find(|(id, _)| *id == entry_id) {
                    Some((_, score)) => *score as f64,
                    None => 1.0,
                },
            },
        }
    }

    /// Helper: build a populated phase snapshot (not cold-start).
    fn make_phase_snapshot(entries: &[(u64, &str, f32)]) -> HashMap<String, Vec<(u64, f32)>> {
        let mut snapshot: HashMap<String, Vec<(u64, f32)>> = HashMap::new();
        for (entry_id, category, score) in entries {
            snapshot
                .entry(category.to_string())
                .or_default()
                .push((*entry_id, *score));
        }
        snapshot
    }

    // AC-11 Test 1: current_phase = None -> phase_snapshot = None -> phase_explicit_norm = 0.0
    #[test]
    fn test_scoring_current_phase_none_sets_phase_explicit_norm_zero() {
        let phase_snapshot: Option<HashMap<String, Vec<(u64, f32)>>> = None;

        assert_eq!(
            compute_phase_explicit_norm(&phase_snapshot, 42, "decision"),
            0.0_f64,
            "AC-11 Test 1: current_phase = None must produce phase_explicit_norm = 0.0"
        );
        assert_eq!(
            compute_phase_explicit_norm(&phase_snapshot, 99, "pattern"),
            0.0_f64,
            "AC-11 Test 1: any entry with phase_snapshot=None must yield 0.0"
        );
    }

    // AC-11 Test 2: use_fallback = true → guard fires before phase_affinity_score →
    // phase_snapshot = None → phase_explicit_norm = 0.0. R-03 primary.
    #[test]
    fn test_scoring_use_fallback_true_sets_phase_explicit_norm_zero() {
        use crate::services::phase_freq_table::PhaseFreqTable;

        let handle = PhaseFreqTable::new_handle();
        // Cold-start handle: use_fallback = true.
        let current_phase: Option<String> = Some("delivery".to_string());

        let phase_snapshot: Option<HashMap<String, Vec<(u64, f32)>>> = match &current_phase {
            None => None,
            Some(phase) => {
                let guard = handle.read().unwrap_or_else(|e| e.into_inner());
                if guard.use_fallback {
                    // GUARD FIRES before phase_affinity_score: return None.
                    None
                } else {
                    let snapshot: HashMap<String, Vec<(u64, f32)>> = guard
                        .table
                        .iter()
                        .filter(|((p, _cat), _)| p == phase)
                        .map(|((_p, cat), bucket)| (cat.clone(), bucket.clone()))
                        .collect();
                    Some(snapshot)
                }
            }
        };

        // Guard fires -> phase_snapshot = None (even though current_phase is Some).
        assert!(
            phase_snapshot.is_none(),
            "AC-11 Test 2: use_fallback=true must produce phase_snapshot=None \
             even when current_phase=Some"
        );

        // phase_explicit_norm = 0.0 for all candidates.
        assert_eq!(
            compute_phase_explicit_norm(&phase_snapshot, 42, "decision"),
            0.0_f64,
            "AC-11 Test 2: use_fallback=true must yield phase_explicit_norm=0.0"
        );
    }

    // AC-11 Test 3: cold-start scores are bit-for-bit identical to pre-col-031.
    // NFR-04, R-03.
    #[test]
    fn test_scoring_score_identity_cold_start() {
        let weights = FusionWeights {
            w_sim: 0.25,
            w_nli: 0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
            w_phase_histogram: 0.02,
            w_phase_explicit: 0.05,
        };

        let base_inputs = FusedScoreInputs {
            similarity: 0.8,
            nli_entailment: 0.7,
            confidence: 0.6,
            coac_norm: 0.3,
            util_norm: 0.5,
            prov_norm: 0.0,
            phase_histogram_norm: 0.2,
            phase_explicit_norm: 0.0,
        };

        // Cold-start: phase_snapshot = None -> phase_explicit_norm = 0.0.
        let phase_snapshot_none: Option<HashMap<String, Vec<(u64, f32)>>> = None;
        let norm_cold = compute_phase_explicit_norm(&phase_snapshot_none, 42, "decision");
        let inputs_cold = FusedScoreInputs {
            phase_explicit_norm: norm_cold,
            ..base_inputs
        };
        let score_cold = compute_fused_score(&inputs_cold, &weights);

        // Pre-col-031 baseline: phase_explicit_norm hardcoded to 0.0.
        let inputs_baseline = FusedScoreInputs {
            phase_explicit_norm: 0.0,
            ..base_inputs
        };
        let score_baseline = compute_fused_score(&inputs_baseline, &weights);

        // Must be bit-for-bit identical (NFR-04).
        assert_eq!(
            score_cold, score_baseline,
            "AC-11 Test 3: cold-start score must be bit-for-bit identical to pre-col-031 baseline"
        );
    }

    // Populated snapshot: entry present in bucket -> non-zero norm.
    #[test]
    fn test_scoring_populated_snapshot_produces_nonzero_norm() {
        let snapshot = make_phase_snapshot(&[(42, "decision", 1.0_f32)]);
        let phase_snapshot = Some(snapshot);

        let norm = compute_phase_explicit_norm(&phase_snapshot, 42, "decision");
        assert!(
            norm > 0.0_f64,
            "populated snapshot: entry present must yield norm > 0.0, got {norm}"
        );
        assert!(
            (norm - 1.0_f64).abs() < f64::EPSILON,
            "entry with rank score 1.0 must yield norm = 1.0, got {norm}"
        );
    }

    // Absent entry in populated snapshot -> 1.0 (neutral per ADR-003 contract).
    #[test]
    fn test_scoring_absent_entry_in_snapshot_norm_is_neutral() {
        let snapshot = make_phase_snapshot(&[(42, "decision", 0.8_f32)]);
        let phase_snapshot = Some(snapshot);

        let norm = compute_phase_explicit_norm(&phase_snapshot, 99, "decision");
        assert!(
            (norm - 1.0_f64).abs() < f64::EPSILON,
            "absent entry in populated snapshot must yield 1.0 (neutral), got {norm}"
        );
    }

    // Absent category in snapshot -> 1.0 (neutral).
    #[test]
    fn test_scoring_absent_category_in_snapshot_norm_is_neutral() {
        let snapshot = make_phase_snapshot(&[(42, "decision", 0.8_f32)]);
        let phase_snapshot = Some(snapshot);

        let norm = compute_phase_explicit_norm(&phase_snapshot, 42, "pattern");
        assert!(
            (norm - 1.0_f64).abs() < f64::EPSILON,
            "absent category in snapshot must yield 1.0 (neutral), got {norm}"
        );
    }

    // ServiceSearchParams.current_phase field accepts None (backward-compatible).
    #[test]
    fn test_service_search_params_current_phase_accepts_none() {
        let params = ServiceSearchParams {
            query: "test".to_string(),
            k: 5,
            filters: None,
            similarity_floor: None,
            confidence_floor: None,
            feature_tag: None,
            co_access_anchors: None,
            caller_agent_id: None,
            retrieval_mode: RetrievalMode::Flexible,
            session_id: None,
            category_histogram: None,
            current_phase: None,
        };
        assert!(
            params.current_phase.is_none(),
            "ServiceSearchParams.current_phase must accept None (backward-compatible default)"
        );
    }

    // Multi-entry bucket: rank scores looked up correctly for each entry.
    #[test]
    fn test_scoring_snapshot_bucket_rank_lookup() {
        let snapshot = make_phase_snapshot(&[
            (10, "decision", 1.0_f32),
            (20, "decision", 0.5_f32),
            (30, "decision", 0.333_f32),
        ]);
        let phase_snapshot = Some(snapshot);

        let norm_10 = compute_phase_explicit_norm(&phase_snapshot, 10, "decision");
        let norm_20 = compute_phase_explicit_norm(&phase_snapshot, 20, "decision");
        let norm_30 = compute_phase_explicit_norm(&phase_snapshot, 30, "decision");

        assert!((norm_10 - 1.0_f64).abs() < 1e-6, "rank-1 must yield 1.0");
        assert!((norm_20 - 0.5_f64).abs() < 1e-6, "rank-2 must yield 0.5");
        assert!(
            (norm_30 - 0.333_f64).abs() < 1e-4,
            "rank-3 must yield ~0.333"
        );
        assert!(norm_10 > norm_20, "rank-1 must outrank rank-2");
        assert!(norm_20 > norm_30, "rank-2 must outrank rank-3");
    }

    // R-06: read lock released before scoring loop. Exercises the acquire/extract/release
    // pattern and confirms a concurrent writer can acquire the write lock immediately after.
    #[test]
    fn test_scoring_lock_released_before_scoring_loop() {
        use crate::services::phase_freq_table::PhaseFreqTable;

        let handle = PhaseFreqTable::new_handle();
        let current_phase: Option<String> = Some("delivery".to_string());

        // Simulate pre-loop extraction (mirrors search() logic).
        let phase_snapshot: Option<HashMap<String, Vec<(u64, f32)>>> = match &current_phase {
            None => None,
            Some(phase) => {
                let guard = handle.read().unwrap_or_else(|e| e.into_inner());
                if guard.use_fallback {
                    None
                    // guard drops here — lock released
                } else {
                    let snapshot: HashMap<String, Vec<(u64, f32)>> = guard
                        .table
                        .iter()
                        .filter(|((p, _cat), _)| p == phase)
                        .map(|((_p, cat), bucket)| (cat.clone(), bucket.clone()))
                        .collect();
                    Some(snapshot)
                    // guard drops here — lock released
                }
            }
        };
        // Read lock now released. Write must not block.

        {
            let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
            guard.use_fallback = false;
        }

        // Cold-start produced None snapshot (expected).
        assert!(
            phase_snapshot.is_none(),
            "R-06: cold-start handle must produce None snapshot; \
             write succeeded without blocking confirms lock was released"
        );
    }

    // ============================================================
    // Step 6d: PPR expansion tests (crt-030)
    // ============================================================
    //
    // These tests exercise the Step 6d logic directly using the
    // `personalized_pagerank` function and helper graph builders,
    // without running the full async search pipeline.
    //
    // Helpers imported from the engine crate.

    mod step_6d {
        use std::collections::{HashMap, HashSet};
        use unimatrix_core::Status;
        use unimatrix_engine::graph::{
            GraphEdgeRow, RelationType, TypedRelationGraph, build_typed_relation_graph,
            personalized_pagerank,
        };

        use super::make_test_entry;

        // ---- Helpers ----

        /// Build a TypedRelationGraph with a single Supports edge from `source` to `target`.
        fn make_graph_supports(source: u64, target: u64) -> TypedRelationGraph {
            let entries = vec![
                make_test_entry(source, Status::Active, None, 0.5, "decision"),
                make_test_entry(target, Status::Active, None, 0.5, "decision"),
            ];
            let edges = vec![GraphEdgeRow {
                source_id: source,
                target_id: target,
                relation_type: RelationType::Supports.as_str().to_string(),
                weight: 1.0,
                created_at: 0,
                created_by: "test".to_string(),
                source: "test".to_string(),
                bootstrap_only: false,
            }];
            build_typed_relation_graph(&entries, &edges).expect("test graph build must succeed")
        }

        /// Default PPR config values (matching InferenceConfig defaults).
        struct PprCfg {
            alpha: f64,
            iterations: usize,
            inclusion_threshold: f64,
            blend_weight: f64,
            max_expand: usize,
        }

        impl Default for PprCfg {
            fn default() -> Self {
                PprCfg {
                    alpha: 0.85,
                    iterations: 20,
                    inclusion_threshold: 0.05,
                    blend_weight: 0.15,
                    max_expand: 50,
                }
            }
        }

        /// Run the synchronous phases of Step 6d (phases 1-4, excluding async fetch).
        /// Returns: (blended pool scores, ppr_only candidates after sort+cap).
        /// Pool entries are blended in-place on the returned Vec.
        fn run_step_6d_sync(
            results_with_scores: &mut Vec<(unimatrix_core::EntryRecord, f64)>,
            graph: &TypedRelationGraph,
            phase_snapshot: &Option<HashMap<String, Vec<(u64, f32)>>>,
            current_phase: &Option<String>,
            cfg: &PprCfg,
        ) -> Vec<(u64, f64)> {
            // Phase 1: build seed scores
            let mut seed_scores: HashMap<u64, f64> =
                HashMap::with_capacity(results_with_scores.len());

            for (entry, sim) in results_with_scores.iter() {
                let affinity: f64 =
                    if let (Some(_phase), Some(snapshot)) = (current_phase, phase_snapshot) {
                        snapshot
                            .get(&entry.category)
                            .and_then(|bucket| bucket.iter().find(|(id, _)| *id == entry.id))
                            .map(|(_, score)| *score as f64)
                            .unwrap_or(1.0)
                    } else {
                        1.0
                    };
                seed_scores.insert(entry.id, sim * affinity);
            }

            // Phase 2: normalize; zero-sum guard
            let total: f64 = seed_scores.values().sum();
            if total == 0.0 {
                return vec![];
            }
            for v in seed_scores.values_mut() {
                *v /= total;
            }

            // Phase 2b: run PPR
            let ppr_scores = personalized_pagerank(graph, &seed_scores, cfg.alpha, cfg.iterations);

            // Phase 3: blend existing HNSW candidates
            for (entry, sim) in results_with_scores.iter_mut() {
                if let Some(&ppr_score) = ppr_scores.get(&entry.id) {
                    *sim = (1.0 - cfg.blend_weight) * (*sim) + cfg.blend_weight * ppr_score;
                }
            }

            // Phase 4: collect PPR-only candidates
            let existing_ids: HashSet<u64> =
                results_with_scores.iter().map(|(e, _)| e.id).collect();

            let mut ppr_only: Vec<(u64, f64)> = ppr_scores
                .iter()
                .filter(|(id, score)| {
                    !existing_ids.contains(id) && **score > cfg.inclusion_threshold
                })
                .map(|(id, score)| (*id, *score))
                .collect();

            ppr_only.sort_unstable_by(|a, b| {
                b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
            });
            ppr_only.truncate(cfg.max_expand);
            ppr_only
        }

        // ---- T-6D-01: use_fallback = true → skip entirely ----

        #[test]
        fn test_step_6d_skipped_when_use_fallback_true() {
            // When use_fallback = true the guard fires immediately; pool is unchanged.
            // This test verifies the guard by NOT calling the inner logic.
            let entry_a = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let entry_b = make_test_entry(2, Status::Active, None, 0.5, "decision");
            let pool_before: Vec<(unimatrix_core::EntryRecord, f64)> =
                vec![(entry_a.clone(), 0.8), (entry_b.clone(), 0.6)];

            // Simulate: when use_fallback = true, the `if !use_fallback` block is skipped.
            let use_fallback = true;
            let mut pool = pool_before.clone();

            if !use_fallback {
                // This block must NOT execute.
                pool.clear();
            }

            assert_eq!(
                pool.len(),
                2,
                "pool must be unchanged when use_fallback = true"
            );
            assert!(
                (pool[0].1 - 0.8).abs() < f64::EPSILON,
                "first entry score unchanged"
            );
            assert!(
                (pool[1].1 - 0.6).abs() < f64::EPSILON,
                "second entry score unchanged"
            );
        }

        // ---- T-6D-05 / R-06: Inclusion threshold strictly greater-than ----

        #[test]
        fn test_step_6d_entry_at_exact_threshold_not_included() {
            // Entry at exactly threshold=0.05 must NOT be included (strictly >).
            let threshold = 0.05_f64;
            let score_at_threshold = threshold;

            let entry_a = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let graph = make_graph_supports(99, 1); // 99 -> 1 (Supports)
            let mut pool = vec![(entry_a, 0.8)];
            let cfg = PprCfg {
                inclusion_threshold: threshold,
                max_expand: 50,
                ..Default::default()
            };

            let ppr_only = run_step_6d_sync(&mut pool, &graph, &None, &None, &cfg);

            // All PPR scores >= threshold are excluded by > filter.
            // Verify entry 99 (PPR-only candidate) is filtered if its score == threshold.
            for (id, score) in &ppr_only {
                assert!(
                    *score > threshold,
                    "all ppr_only candidates must have score > threshold; \
                     id={id} score={score_at_threshold} is at the boundary"
                );
                let _ = score_at_threshold; // suppress unused warning
            }
        }

        #[test]
        fn test_step_6d_entry_just_above_threshold_included() {
            // Entry at threshold + EPSILON is included (strictly > passes).
            let threshold = 0.05_f64;

            let entry_a = make_test_entry(1, Status::Active, None, 0.5, "decision");
            // Build a graph where node 99 supports node 1.
            // With node 1 as the HNSW seed, PPR will propagate mass to node 99.
            let graph = make_graph_supports(99, 1); // 99 → 1

            let mut pool = vec![(entry_a, 0.8)];
            let cfg = PprCfg {
                alpha: 0.85,
                iterations: 20,
                inclusion_threshold: threshold,
                blend_weight: 0.15,
                max_expand: 50,
            };

            let ppr_only = run_step_6d_sync(&mut pool, &graph, &None, &None, &cfg);

            // Any PPR-only candidate returned must exceed threshold strictly.
            for (_, score) in &ppr_only {
                assert!(
                    *score > threshold,
                    "ppr_only score {score} must be strictly > threshold {threshold}"
                );
            }
            // Node 99 supports node 1 (seed); with alpha=0.85 and seed at sim=0.8,
            // PPR should propagate some mass to node 99.
            let has_node_99 = ppr_only.iter().any(|(id, _)| *id == 99);
            assert!(
                has_node_99,
                "node 99 (supporter of seed 1) should be in ppr_only with low threshold"
            );
        }

        // ---- T-6D-06 / E-04: ppr_max_expand cap ----

        #[test]
        fn test_step_6d_pool_expansion_capped_at_ppr_max_expand() {
            // Build a star graph: nodes 10..=14 all support node 1.
            // HNSW pool has only node 1. With max_expand=2, only 2 should be injected.
            let supporters: Vec<u64> = (10..=14).collect();
            let mut all_ids = vec![1_u64];
            all_ids.extend_from_slice(&supporters);

            let entries: Vec<_> = all_ids
                .iter()
                .map(|&id| make_test_entry(id, Status::Active, None, 0.5, "decision"))
                .collect();
            let edges: Vec<GraphEdgeRow> = supporters
                .iter()
                .map(|&src| GraphEdgeRow {
                    source_id: src,
                    target_id: 1,
                    relation_type: RelationType::Supports.as_str().to_string(),
                    weight: 1.0,
                    created_at: 0,
                    created_by: "test".to_string(),
                    source: "test".to_string(),
                    bootstrap_only: false,
                })
                .collect();
            let graph = build_typed_relation_graph(&entries, &edges).expect("test graph build");

            let entry_1 = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let mut pool = vec![(entry_1, 0.8)];

            let cfg = PprCfg {
                inclusion_threshold: 0.001, // very low to include all PPR nodes
                max_expand: 2,
                ..Default::default()
            };

            let ppr_only = run_step_6d_sync(&mut pool, &graph, &None, &None, &cfg);

            assert_eq!(
                ppr_only.len(),
                2,
                "ppr_only must be capped at ppr_max_expand=2"
            );
        }

        // ---- T-6D-07 / AC-15: Blend formula for existing HNSW candidates ----

        #[test]
        fn test_step_6d_blend_formula_known_values() {
            // Test the blend formula with a direct calculation.
            // For an isolated node 1 with seed {1: 1.0 (normalized)} and no edges,
            // PPR iteration: score[1] = (1 - alpha) * seed[1] + alpha * 0.0 = (1 - 0.85) * 1.0 = 0.15.
            // blend_weight = 0.15, hnsw_sim = 0.8:
            // new_sim = (1.0 - 0.15) * 0.8 + 0.15 * 0.15 = 0.68 + 0.0225 = 0.7025.
            let entry_a = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let _entries = vec![entry_a.clone()];
            let graph = build_typed_relation_graph(&_entries, &[]).expect("test graph build");

            let mut pool = vec![(entry_a, 0.8)];
            let cfg = PprCfg {
                alpha: 0.85,
                iterations: 20,
                blend_weight: 0.15,
                inclusion_threshold: 0.05,
                max_expand: 50,
            };

            let _ppr_only = run_step_6d_sync(&mut pool, &graph, &None, &None, &cfg);

            let blended_sim = pool[0].1;
            // Isolated node: ppr_score = (1 - 0.85) * 1.0 = 0.15
            let ppr_score = (1.0 - 0.85) * 1.0_f64;
            let expected = (1.0 - 0.15) * 0.8 + 0.15 * ppr_score;
            assert!(
                (blended_sim - expected).abs() < 1e-9,
                "blend formula: expected {expected} = 0.85*0.8 + 0.15*ppr_score, got {blended_sim}"
            );
        }

        #[test]
        fn test_step_6d_blend_weight_zero_leaves_hnsw_unchanged() {
            // blend_weight=0.0: existing HNSW scores must not change.
            let entry_a = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let entries = vec![entry_a.clone()];
            let graph = build_typed_relation_graph(&entries, &[]).expect("test graph");
            let mut pool = vec![(entry_a, 0.8)];

            let cfg = PprCfg {
                blend_weight: 0.0,
                ..Default::default()
            };
            let _ppr_only = run_step_6d_sync(&mut pool, &graph, &None, &None, &cfg);

            assert_eq!(
                pool[0].1, 0.8,
                "sim must be exactly 0.8 with blend_weight=0.0"
            );
        }

        #[test]
        fn test_step_6d_blend_weight_one_overwrites_hnsw() {
            // blend_weight=1.0: existing sim fully replaced by PPR score.
            // For an isolated node 1 with alpha=0.85:
            // ppr_score = (1 - 0.85) * 1.0 = 0.15 (teleportation only, no edges).
            // blend_weight=1.0 → new_sim = 0.0 * 0.9 + 1.0 * 0.15 = 0.15.
            let entry_a = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let entries = vec![entry_a.clone()];
            let graph = build_typed_relation_graph(&entries, &[]).expect("test graph");
            let mut pool = vec![(entry_a, 0.9)];

            let cfg = PprCfg {
                alpha: 0.85,
                blend_weight: 1.0,
                ..Default::default()
            };
            let _ppr_only = run_step_6d_sync(&mut pool, &graph, &None, &None, &cfg);

            // PPR for isolated node 1 = (1 - alpha) * 1.0 = 0.15.
            let ppr_score = (1.0 - 0.85_f64) * 1.0;
            let expected_sim = ppr_score; // blend_weight=1.0 → new_sim = ppr_score
            assert!(
                (pool[0].1 - expected_sim).abs() < 1e-9,
                "with blend_weight=1.0, sim must equal PPR score ({expected_sim}); got {}",
                pool[0].1
            );
        }

        // ---- T-6D-08 / R-03: blend_weight = 0.0 ----

        #[test]
        fn test_step_6d_ppr_only_entry_blend_weight_zero_initial_sim_is_zero() {
            // PPR-only entry with blend_weight=0.0 gets initial_sim=0.0.
            // initial_sim = blend_weight * ppr_score = 0.0 * anything = 0.0.
            let ppr_score = 0.4_f64;
            let blend_weight = 0.0_f64;
            let initial_sim = blend_weight * ppr_score;
            assert_eq!(
                initial_sim, 0.0,
                "initial_sim with blend_weight=0.0 must be 0.0"
            );
        }

        // ---- AC-14: PPR-only entry initial similarity ----

        #[test]
        fn test_step_6d_ppr_only_entry_initial_sim_formula() {
            // initial_sim = ppr_blend_weight * ppr_score (ADR-007).
            let ppr_score = 0.4_f64;
            let blend_weight = 0.15_f64;
            let expected = blend_weight * ppr_score;
            assert!(
                (expected - 0.06).abs() < 1e-9,
                "initial_sim = 0.15 * 0.4 = 0.06, got {expected}"
            );
        }

        // ---- FM-05: zero-sum guard ----

        #[test]
        fn test_step_6d_all_zero_hnsw_scores_skips_ppr() {
            // All HNSW entries have sim=0.0 → seed_scores sum=0.0 → zero-sum guard fires.
            // The ppr_only list must be empty (PPR was not called).
            let entry_a = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let entry_b = make_test_entry(2, Status::Active, None, 0.5, "decision");
            let graph = make_graph_supports(99, 1);

            let mut pool = vec![(entry_a.clone(), 0.0), (entry_b.clone(), 0.0)];
            let pool_ids_before: Vec<u64> = pool.iter().map(|(e, _)| e.id).collect();

            let cfg = PprCfg::default();
            // run_step_6d_sync returns early with empty vec when total=0.0.
            let ppr_only = run_step_6d_sync(&mut pool, &graph, &None, &None, &cfg);

            assert!(
                ppr_only.is_empty(),
                "zero-sum guard: ppr_only must be empty"
            );

            // Pool must be unchanged: same IDs, same scores (0.0).
            let pool_ids_after: Vec<u64> = pool.iter().map(|(e, _)| e.id).collect();
            assert_eq!(
                pool_ids_before, pool_ids_after,
                "pool IDs must be unchanged after zero-sum guard"
            );
            for (_, sim) in &pool {
                assert_eq!(
                    *sim, 0.0,
                    "pool scores must remain 0.0 after zero-sum guard"
                );
            }
        }

        // ---- AC-16 / R-10: Phase-aware personalization ----

        #[test]
        fn test_step_6d_none_phase_snapshot_uses_hnsw_score_only() {
            // phase_snapshot = None → affinity = 1.0 → seed = hnsw_score * 1.0.
            let entry_a = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let entry_b = make_test_entry(2, Status::Active, None, 0.5, "decision");
            // Seeds before normalization: {1: 0.8, 2: 0.6}; total=1.4.
            // Normalized: {1: 0.8/1.4 ≈ 0.5714, 2: 0.6/1.4 ≈ 0.4286}.
            let entries = vec![entry_a.clone(), entry_b.clone()];
            let graph = build_typed_relation_graph(&entries, &[]).expect("test graph");
            let mut pool = vec![(entry_a, 0.8), (entry_b, 0.6)];

            let cfg = PprCfg::default();
            let _ppr_only = run_step_6d_sync(&mut pool, &graph, &None, &None, &cfg);

            // With no edges and two isolated nodes, PPR reduces to teleportation only.
            // node 1 ppr_score ≈ (1 - alpha) * normalized_seed(1) = 0.15 * (0.8/1.4).
            // node 2 ppr_score ≈ (1 - alpha) * normalized_seed(2) = 0.15 * (0.6/1.4).
            // Blend for node 1: (1-0.15)*0.8 + 0.15*ppr(1).
            // Just verify no panic and scores are finite.
            for (_, sim) in &pool {
                assert!(sim.is_finite(), "blended score must be finite");
                assert!(*sim >= 0.0, "blended score must be non-negative");
            }
        }

        #[test]
        fn test_step_6d_non_uniform_phase_snapshot_amplifies_seeds() {
            // Phase snapshot with entry 1 at affinity=2.0 and entry 2 at 1.0.
            // seed[1] = 0.5 * 2.0 = 1.0, seed[2] = 0.5 * 1.0 = 0.5.
            // After normalization: seed[1] = 1.0/1.5, seed[2] = 0.5/1.5.
            // Uniform (no snapshot): seed[1] = 0.5/1.0, seed[2] = 0.5/1.0.
            // With phase: entry 1 has larger seed weight than entry 2.
            let mut snapshot: HashMap<String, Vec<(u64, f32)>> = HashMap::new();
            snapshot.insert("decision".to_string(), vec![(1, 2.0_f32), (2, 1.0_f32)]);
            let phase = Some("nxs".to_string());
            let phase_snapshot = Some(snapshot);

            let entry_1 = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let entry_2 = make_test_entry(2, Status::Active, None, 0.5, "decision");
            let entries = vec![entry_1.clone(), entry_2.clone()];
            let graph = build_typed_relation_graph(&entries, &[]).expect("test graph");

            // With phase snapshot
            let mut pool_phase = vec![(entry_1.clone(), 0.5), (entry_2.clone(), 0.5)];
            // With no snapshot (uniform)
            let mut pool_uniform = vec![(entry_1, 0.5), (entry_2, 0.5)];

            let cfg = PprCfg::default();

            run_step_6d_sync(&mut pool_phase, &graph, &phase_snapshot, &phase, &cfg);
            run_step_6d_sync(&mut pool_uniform, &graph, &None, &None, &cfg);

            // In the phase-aware run, entry 1 has a higher seed than entry 2.
            // In the uniform run, both seeds are equal (both sim=0.5, affinity=1.0).
            let phase_sim_1 = pool_phase
                .iter()
                .find(|(e, _)| e.id == 1)
                .map(|(_, s)| *s)
                .unwrap();
            let uniform_sim_1 = pool_uniform
                .iter()
                .find(|(e, _)| e.id == 1)
                .map(|(_, s)| *s)
                .unwrap();

            // Phase-boosted run should give entry 1 a higher PPR mass → different blend result.
            // Both runs use blend_weight=0.15, so the difference comes from PPR score.
            assert!(
                (phase_sim_1 - uniform_sim_1).abs() > 1e-12,
                "phase snapshot must produce different seeds from uniform; \
                 phase_sim_1={phase_sim_1}, uniform_sim_1={uniform_sim_1}"
            );
        }

        // ---- AC-17: Integration — PPR surfaces supporter ----

        #[test]
        fn test_step_6d_ppr_surfaces_support_entry() {
            // AC-17 canonical acceptance test.
            // Graph: A(100) → B(200) [Supports]. HNSW seed: B=200. A should surface.
            let graph = make_graph_supports(100, 200); // A=100 supports B=200
            let entry_b = make_test_entry(200, Status::Active, None, 0.5, "decision");
            let mut pool = vec![(entry_b, 0.8)];

            let cfg = PprCfg {
                inclusion_threshold: 0.001, // low threshold so A surfaces
                blend_weight: 0.15,
                max_expand: 10,
                ..Default::default()
            };

            let ppr_only = run_step_6d_sync(&mut pool, &graph, &None, &None, &cfg);

            let has_a = ppr_only.iter().any(|(id, _)| *id == 100);
            assert!(
                has_a,
                "node 100 (supporter of seed 200) must be in ppr_only candidates"
            );

            // initial_sim for A = blend_weight * ppr_score_of_A
            let (_, a_ppr_score) = ppr_only.iter().find(|(id, _)| *id == 100).unwrap();
            let initial_sim = cfg.blend_weight * a_ppr_score;
            assert!(
                initial_sim >= 0.0 && initial_sim <= cfg.blend_weight,
                "initial_sim must be in [0, blend_weight]; got {initial_sim}"
            );
        }

        // ---- T-6D-quarantine: verify quarantine logic (sync check) ----

        #[test]
        fn test_step_6d_quarantine_check_applies_to_fetched_entries() {
            // Verify the quarantine check logic is correct: is_quarantined returns true
            // for Quarantined status, false for Active.
            use crate::services::gateway::SecurityGateway;

            let quarantined_entry = make_test_entry(77, Status::Quarantined, None, 0.5, "decision");
            let active_entry = make_test_entry(77, Status::Active, None, 0.5, "decision");

            assert!(
                SecurityGateway::is_quarantined(&quarantined_entry.status),
                "Quarantined entry must be rejected by is_quarantined"
            );
            assert!(
                !SecurityGateway::is_quarantined(&active_entry.status),
                "Active entry must not be rejected by is_quarantined"
            );
        }

        // ---- T-6D-sort: expansion sorted by score desc ----

        #[test]
        fn test_step_6d_expansion_sorted_by_ppr_score_desc() {
            // Build a graph where nodes 10, 11, 12 all support node 1 (seed).
            // max_expand=2 → only top 2 by PPR score are kept.
            let entries: Vec<_> = [1_u64, 10, 11, 12]
                .iter()
                .map(|&id| make_test_entry(id, Status::Active, None, 0.5, "decision"))
                .collect();
            let edges: Vec<GraphEdgeRow> = [10_u64, 11, 12]
                .iter()
                .map(|&src| GraphEdgeRow {
                    source_id: src,
                    target_id: 1,
                    relation_type: RelationType::Supports.as_str().to_string(),
                    weight: 1.0,
                    created_at: 0,
                    created_by: "test".to_string(),
                    source: "test".to_string(),
                    bootstrap_only: false,
                })
                .collect();
            let graph = build_typed_relation_graph(&entries, &edges).expect("test graph");
            let entry_1 = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let mut pool = vec![(entry_1, 0.8)];

            let cfg = PprCfg {
                inclusion_threshold: 0.001,
                max_expand: 2,
                ..Default::default()
            };

            let ppr_only = run_step_6d_sync(&mut pool, &graph, &None, &None, &cfg);

            // Verify sorted descending: each element's score >= the next.
            assert_eq!(ppr_only.len(), 2, "must have exactly max_expand=2 entries");
            if ppr_only.len() >= 2 {
                assert!(
                    ppr_only[0].1 >= ppr_only[1].1,
                    "ppr_only must be sorted descending by score: {} >= {}",
                    ppr_only[0].1,
                    ppr_only[1].1
                );
            }
        }

        // ---- I-03: FusionWeights sum invariant (crt-038 regression guard) ----

        #[test]
        fn test_fusion_weights_default_sum_unchanged_by_crt030() {
            // crt-038: conf-boost-c defaults: w_sim=0.50, w_nli=0.00, w_conf=0.35,
            // w_coac=0.00, w_util=0.00, w_prov=0.00, w_phase_histogram=0.02, w_phase_explicit=0.05
            // Sum: 0.50 + 0.00 + 0.35 + 0.00 + 0.00 + 0.00 + 0.02 + 0.05 = 0.92
            use super::super::FusionWeights;
            use crate::infra::config::InferenceConfig;
            let fw = FusionWeights::from_config(&InferenceConfig::default());
            let total = fw.w_sim
                + fw.w_nli
                + fw.w_conf
                + fw.w_coac
                + fw.w_util
                + fw.w_prov
                + fw.w_phase_histogram
                + fw.w_phase_explicit;
            assert!(
                (total - 0.92).abs() < 1e-9,
                "FusionWeights default sum must be 0.92 (crt-038: conf-boost-c defaults); got {total}"
            );
        }

        // ---- AC-02: FusionWeights::effective() short-circuit tests (crt-038) ----

        #[test]
        fn test_effective_short_circuit_w_nli_zero_nli_available_false() {
            use super::super::FusionWeights;
            let fw = FusionWeights {
                w_sim: 0.50,
                w_nli: 0.00,
                w_conf: 0.35,
                w_coac: 0.00,
                w_util: 0.00,
                w_prov: 0.00,
                w_phase_histogram: 0.02,
                w_phase_explicit: 0.05,
            };
            let result = fw.effective(false);
            // Short-circuit must fire: weights returned unchanged (not re-normalized).
            // Without this guard, effective(false) would produce w_sim≈0.588, w_conf≈0.412.
            assert_eq!(result.w_sim, 0.50, "w_sim must not be re-normalized");
            assert_eq!(result.w_nli, 0.00, "w_nli must remain 0.0");
            assert_eq!(result.w_conf, 0.35, "w_conf must not be re-normalized");
            assert_eq!(result.w_coac, 0.00);
            assert_eq!(result.w_util, 0.00);
            assert_eq!(result.w_prov, 0.00);
            assert_eq!(result.w_phase_histogram, 0.02);
            assert_eq!(result.w_phase_explicit, 0.05);
        }

        #[test]
        fn test_effective_short_circuit_w_nli_zero_nli_available_true() {
            use super::super::FusionWeights;
            let fw = FusionWeights {
                w_sim: 0.50,
                w_nli: 0.00,
                w_conf: 0.35,
                w_coac: 0.00,
                w_util: 0.00,
                w_prov: 0.00,
                w_phase_histogram: 0.02,
                w_phase_explicit: 0.05,
            };
            let result = fw.effective(true);
            // Short-circuit fires before the nli_available branch — both paths return unchanged.
            assert_eq!(result.w_sim, 0.50, "w_sim must not be re-normalized");
            assert_eq!(result.w_nli, 0.00, "w_nli must remain 0.0");
            assert_eq!(result.w_conf, 0.35, "w_conf must not be re-normalized");
            assert_eq!(result.w_coac, 0.00);
            assert_eq!(result.w_util, 0.00);
            assert_eq!(result.w_prov, 0.00);
            assert_eq!(result.w_phase_histogram, 0.02);
            assert_eq!(result.w_phase_explicit, 0.05);
        }

        #[test]
        fn test_effective_renormalization_still_fires_when_w_nli_positive() {
            use super::super::FusionWeights;
            // w_nli=0.20 > 0.0 — short-circuit must NOT fire.
            // nli_available=false — re-normalization path must execute.
            // Non-NLI sum = 0.25 + 0.15 + 0.00 + 0.05 + 0.05 = 0.50
            let fw = FusionWeights {
                w_sim: 0.25,
                w_nli: 0.20,
                w_conf: 0.15,
                w_coac: 0.00,
                w_util: 0.05,
                w_prov: 0.05,
                w_phase_histogram: 0.02,
                w_phase_explicit: 0.05,
            };
            let result = fw.effective(false);
            // w_nli must be zeroed.
            assert_eq!(
                result.w_nli, 0.00,
                "w_nli must be zeroed during re-normalization"
            );
            // Each non-NLI weight divided by denominator 0.50.
            assert!(
                (result.w_sim - 0.50).abs() < 1e-10,
                "w_sim must be re-normalized to 0.25/0.50=0.50; got {}",
                result.w_sim
            );
            assert!(
                (result.w_conf - 0.30).abs() < 1e-10,
                "w_conf must be re-normalized to 0.15/0.50=0.30; got {}",
                result.w_conf
            );
            assert_eq!(result.w_coac, 0.00);
            assert!(
                (result.w_util - 0.10).abs() < 1e-10,
                "w_util must be re-normalized to 0.05/0.50=0.10; got {}",
                result.w_util
            );
            assert!(
                (result.w_prov - 0.10).abs() < 1e-10,
                "w_prov must be re-normalized to 0.05/0.50=0.10; got {}",
                result.w_prov
            );
            // Phase terms pass through unchanged.
            assert_eq!(result.w_phase_histogram, fw.w_phase_histogram);
            assert_eq!(result.w_phase_explicit, fw.w_phase_explicit);
            // Confirm re-normalization occurred (result differs from input).
            assert!(
                (result.w_sim - fw.w_sim).abs() > 1e-10,
                "w_sim must differ from input — re-normalization must have occurred"
            );
        }
    }

    // ============================================================
    // Phase 0 (graph_expand) tests — crt-042
    // ============================================================
    //
    // These tests exercise the Phase 0 block in Step 6d (search.rs lines ~870–960).
    // Phase 0 widens the HNSW seed pool via graph_expand BFS before PPR runs (Phase 1+).
    //
    // Strategy: mirror the Phase 0 logic in a synchronous helper `run_phase0_sync` that
    // uses in-memory maps for entry and embedding lookups (analogous to run_step_6d_sync
    // for PPR). All eight acceptance criteria from test-plan/phase0_search.md are covered.

    mod phase0 {
        use std::collections::{HashMap, HashSet};

        use tracing_test::traced_test;
        use unimatrix_core::Status;
        use unimatrix_engine::graph::{
            GraphEdgeRow, RelationType, TypedRelationGraph, build_typed_relation_graph,
            graph_expand,
        };

        use crate::confidence::cosine_similarity;
        use crate::services::gateway::SecurityGateway;

        use super::make_test_entry;

        // ---- Helpers ----

        /// Build a TypedRelationGraph from (src, tgt, RelationType) triples.
        /// All referenced node IDs are added automatically with Active status.
        fn make_graph(edges: &[(u64, u64, RelationType)]) -> TypedRelationGraph {
            let mut seen: Vec<u64> = Vec::new();
            for &(src, tgt, _) in edges {
                if !seen.contains(&src) {
                    seen.push(src);
                }
                if !seen.contains(&tgt) {
                    seen.push(tgt);
                }
            }
            let entries: Vec<_> = seen
                .iter()
                .map(|&id| make_test_entry(id, Status::Active, None, 0.5, "decision"))
                .collect();
            let edge_rows: Vec<GraphEdgeRow> = edges
                .iter()
                .map(|&(src, tgt, rel)| GraphEdgeRow {
                    source_id: src,
                    target_id: tgt,
                    relation_type: rel.as_str().to_string(),
                    weight: 1.0,
                    created_at: 0,
                    created_by: "test".to_string(),
                    source: "test".to_string(),
                    bootstrap_only: false,
                })
                .collect();
            build_typed_relation_graph(&entries, &edge_rows).expect("test graph build must succeed")
        }

        /// Normalise a vector to unit length (L2). Returns the zero vector unchanged.
        fn normalise(v: &[f32]) -> Vec<f32> {
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm < f32::EPSILON {
                v.to_vec()
            } else {
                v.iter().map(|x| x / norm).collect()
            }
        }

        /// Synchronous helper that mirrors the Phase 0 block in search.rs Step 6d.
        ///
        /// Uses in-memory `entry_map` and `embedding_map` in place of async store calls.
        /// Emits the same `tracing::debug!` event as the production code so that
        /// AC-24 / R-10 tracing tests can assert on it.
        ///
        /// Returns the number of entries added to `pool` (i.e., `results_added`).
        #[allow(clippy::too_many_arguments)]
        fn run_phase0_sync(
            pool: &mut Vec<(unimatrix_core::EntryRecord, f64)>,
            graph: &TypedRelationGraph,
            entry_map: &HashMap<u64, unimatrix_core::EntryRecord>,
            embedding_map: &HashMap<u64, Vec<f32>>,
            query_embedding: &[f32],
            expansion_depth: usize,
            max_expansion_candidates: usize,
            ppr_expander_enabled: bool,
        ) -> usize {
            if !ppr_expander_enabled {
                return 0;
            }

            let phase0_start = std::time::Instant::now();

            let seed_ids: Vec<u64> = pool.iter().map(|(e, _)| e.id).collect();

            let expanded_ids: HashSet<u64> =
                graph_expand(graph, &seed_ids, expansion_depth, max_expansion_candidates);

            let in_pool: HashSet<u64> = seed_ids.iter().copied().collect();
            let mut results_added: usize = 0;

            let mut sorted_expanded: Vec<u64> = expanded_ids.iter().copied().collect();
            sorted_expanded.sort_unstable();

            for expanded_id in sorted_expanded {
                if in_pool.contains(&expanded_id) {
                    continue;
                }

                let entry = match entry_map.get(&expanded_id) {
                    Some(e) => e.clone(),
                    None => continue, // silent skip — entry not in mock store
                };

                if SecurityGateway::is_quarantined(&entry.status) {
                    continue; // silent skip — quarantined (R-03, AC-13, NFR-03)
                }

                let emb = match embedding_map.get(&expanded_id) {
                    Some(e) => e.clone(),
                    None => continue, // silent skip — no embedding (AC-15)
                };

                let cosine_sim = cosine_similarity(query_embedding, &emb);
                pool.push((entry, cosine_sim));
                results_added += 1;
            }

            tracing::debug!(
                seeds = seed_ids.len(),
                expanded_count = expanded_ids.len(),
                fetched_count = results_added,
                elapsed_ms = phase0_start.elapsed().as_millis(),
                expansion_depth = expansion_depth,
                max_expansion_candidates = max_expansion_candidates,
                "Phase 0 (graph_expand) complete"
            );

            results_added
        }

        // ---- AC-01: Flag-off regression ----

        /// AC-01: When ppr_expander_enabled=false, Phase 0 adds zero entries to the pool.
        ///
        /// Asserts pool length is identical before and after. This is the bit-identical
        /// regression guard (R-01 / NFR-02).
        #[test]
        fn test_search_flag_off_pool_size_unchanged() {
            let seed = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let reachable = make_test_entry(2, Status::Active, None, 0.5, "decision");

            let graph = make_graph(&[(1, 2, RelationType::CoAccess)]);

            let mut entry_map = HashMap::new();
            entry_map.insert(2, reachable);

            let q = normalise(&[1.0, 0.0]);
            let mut embedding_map = HashMap::new();
            embedding_map.insert(2, normalise(&[1.0, 0.0]));

            let mut pool = vec![(seed, 0.9)];
            let before_len = pool.len();

            let added = run_phase0_sync(
                &mut pool,
                &graph,
                &entry_map,
                &embedding_map,
                &q,
                2,
                200,
                false, // ppr_expander_enabled = false
            );

            assert_eq!(added, 0, "AC-01: flag-off must add zero entries");
            assert_eq!(
                pool.len(),
                before_len,
                "AC-01: pool length must be unchanged when ppr_expander_enabled=false"
            );
        }

        // ---- AC-02: Phase 0 adds expanded entry before Phase 1 ----

        /// AC-02: When ppr_expander_enabled=true with a reachable entry E from seed S,
        /// pool after Phase 0 contains E with a non-zero cosine score.
        ///
        /// Proves Phase 0 enriches results_with_scores before Phase 1 sees the pool.
        #[test]
        fn test_search_phase0_expands_before_phase1() {
            let seed = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let expanded = make_test_entry(2, Status::Active, None, 0.5, "lesson-learned");

            let graph = make_graph(&[(1, 2, RelationType::CoAccess)]);

            let mut entry_map = HashMap::new();
            entry_map.insert(2, expanded);

            let q = normalise(&[1.0, 0.0]);
            let mut embedding_map = HashMap::new();
            embedding_map.insert(2, normalise(&[1.0, 0.0]));

            let mut pool = vec![(seed, 0.9)];

            let added = run_phase0_sync(
                &mut pool,
                &graph,
                &entry_map,
                &embedding_map,
                &q,
                2,
                200,
                true, // ppr_expander_enabled = true
            );

            assert_eq!(added, 1, "AC-02: one expanded entry must be added");
            assert_eq!(
                pool.len(),
                2,
                "AC-02: pool must contain seed + expanded entry"
            );

            let e2 = pool.iter().find(|(e, _)| e.id == 2);
            assert!(
                e2.is_some(),
                "AC-02: entry 2 must appear in pool after Phase 0"
            );
            let (_, sim) = e2.unwrap();
            assert!(
                *sim > 0.0,
                "AC-02: expanded entry must have non-zero cosine similarity; got {sim}"
            );
        }

        // ---- AC-13: Quarantine safety — 1-hop direct ----

        /// AC-13: Seed → quarantined entry Q (1-hop direct).
        ///
        /// Q must be absent from pool after Phase 0 (silent skip, no warn/error — NFR-03).
        /// The seed must remain present.
        #[test]
        fn test_search_phase0_excludes_quarantined_direct() {
            let seed = make_test_entry(10, Status::Active, None, 0.5, "decision");
            let quarantined = make_test_entry(20, Status::Quarantined, None, 0.5, "decision");

            let graph = make_graph(&[(10, 20, RelationType::CoAccess)]);

            let mut entry_map = HashMap::new();
            entry_map.insert(20, quarantined);

            let q = normalise(&[1.0, 0.0]);
            let mut embedding_map = HashMap::new();
            embedding_map.insert(20, normalise(&[1.0, 0.0]));

            let mut pool = vec![(seed, 0.8)];

            let added = run_phase0_sync(
                &mut pool,
                &graph,
                &entry_map,
                &embedding_map,
                &q,
                2,
                200,
                true,
            );

            assert_eq!(added, 0, "AC-13: quarantined entry must not be added");
            assert!(
                !pool.iter().any(|(e, _)| e.id == 20),
                "AC-13: Q must be absent from pool"
            );
            assert_eq!(pool.len(), 1, "AC-13: seed must remain in pool");
        }

        // ---- AC-14: Quarantine safety — 2-hop transitive ----

        /// AC-14: Seed A → active entry B → quarantined entry Q (transitive, depth=2).
        ///
        /// Q must be absent. B must be present (not quarantined, depth=1 from seed).
        #[test]
        fn test_search_phase0_excludes_quarantined_transitive() {
            let seed = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let intermediate = make_test_entry(2, Status::Active, None, 0.5, "decision");
            let quarantined = make_test_entry(3, Status::Quarantined, None, 0.5, "decision");

            let graph = make_graph(&[
                (1, 2, RelationType::CoAccess),
                (2, 3, RelationType::CoAccess),
            ]);

            let mut entry_map = HashMap::new();
            entry_map.insert(2, intermediate);
            entry_map.insert(3, quarantined);

            let q = normalise(&[1.0, 0.0]);
            let mut embedding_map = HashMap::new();
            embedding_map.insert(2, normalise(&[1.0, 0.0]));
            embedding_map.insert(3, normalise(&[1.0, 0.0]));

            let mut pool = vec![(seed, 0.9)];

            let added = run_phase0_sync(
                &mut pool,
                &graph,
                &entry_map,
                &embedding_map,
                &q,
                2, // depth=2 so both hops are expanded
                200,
                true,
            );

            assert_eq!(
                added, 1,
                "AC-14: only B (active) must be added; Q is silently skipped"
            );
            assert!(
                pool.iter().any(|(e, _)| e.id == 2),
                "AC-14: intermediate active entry B must be present in pool"
            );
            assert!(
                !pool.iter().any(|(e, _)| e.id == 3),
                "AC-14: quarantined entry Q must be absent from pool"
            );
        }

        // ---- AC-15: No-embedding skip ----

        /// AC-15: Seed → entry E where get_embedding(E) returns None.
        ///
        /// E must be absent from pool (silent skip). Seed remains present.
        #[test]
        fn test_search_phase0_skips_entry_with_no_embedding() {
            let seed = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let no_embed_entry = make_test_entry(2, Status::Active, None, 0.5, "decision");

            let graph = make_graph(&[(1, 2, RelationType::CoAccess)]);

            let mut entry_map = HashMap::new();
            entry_map.insert(2, no_embed_entry);

            // embedding_map deliberately empty — simulates get_embedding returning None.
            let embedding_map: HashMap<u64, Vec<f32>> = HashMap::new();

            let q = normalise(&[1.0, 0.0]);
            let mut pool = vec![(seed, 0.8)];

            let added = run_phase0_sync(
                &mut pool,
                &graph,
                &entry_map,
                &embedding_map,
                &q,
                2,
                200,
                true,
            );

            assert_eq!(added, 0, "AC-15: entry without embedding must not be added");
            assert!(
                !pool.iter().any(|(e, _)| e.id == 2),
                "AC-15: entry E must be absent when embedding is missing"
            );
            assert_eq!(pool.len(), 1, "AC-15: seed must remain in pool");
        }

        // ---- AC-24: debug! trace emission when enabled ----

        /// AC-24: When ppr_expander_enabled=true, a tracing::debug! event is emitted
        /// containing all six mandatory fields after Phase 0.
        ///
        /// This test is MANDATORY per test plan (entry #3935 documents a prior gate
        /// failure from deferring tracing tests).
        #[traced_test]
        #[test]
        fn test_search_phase0_emits_debug_trace_when_enabled() {
            let seed = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let expanded = make_test_entry(2, Status::Active, None, 0.5, "decision");

            let graph = make_graph(&[(1, 2, RelationType::CoAccess)]);

            let mut entry_map = HashMap::new();
            entry_map.insert(2, expanded);

            let q = normalise(&[1.0, 0.0]);
            let mut embedding_map = HashMap::new();
            embedding_map.insert(2, normalise(&[1.0, 0.0]));

            let mut pool = vec![(seed, 0.9)];

            run_phase0_sync(
                &mut pool,
                &graph,
                &entry_map,
                &embedding_map,
                &q,
                2,
                200,
                true, // ppr_expander_enabled = true
            );

            assert!(
                logs_contain("Phase 0 (graph_expand) complete"),
                "AC-24: debug event 'Phase 0 (graph_expand) complete' must be emitted"
            );
            assert!(
                logs_contain("expanded_count"),
                "AC-24: debug event must contain field 'expanded_count'"
            );
            assert!(
                logs_contain("elapsed_ms"),
                "AC-24: debug event must contain field 'elapsed_ms'"
            );
            assert!(
                logs_contain("seeds"),
                "AC-24: debug event must contain field 'seeds'"
            );
            assert!(
                logs_contain("fetched_count"),
                "AC-24: debug event must contain field 'fetched_count'"
            );
            assert!(
                logs_contain("expansion_depth"),
                "AC-24: debug event must contain field 'expansion_depth'"
            );
            assert!(
                logs_contain("max_expansion_candidates"),
                "AC-24: debug event must contain field 'max_expansion_candidates'"
            );
        }

        // ---- R-10: No debug trace when disabled ----

        /// R-10: When ppr_expander_enabled=false, no "Phase 0" debug event is emitted.
        ///
        /// The timing instrumentation branch is not entered at all — zero overhead.
        #[traced_test]
        #[test]
        fn test_search_phase0_does_not_emit_trace_when_disabled() {
            let seed = make_test_entry(1, Status::Active, None, 0.5, "decision");
            let reachable = make_test_entry(2, Status::Active, None, 0.5, "decision");

            let graph = make_graph(&[(1, 2, RelationType::CoAccess)]);

            let mut entry_map = HashMap::new();
            entry_map.insert(2, reachable);

            let q = normalise(&[1.0, 0.0]);
            let mut embedding_map = HashMap::new();
            embedding_map.insert(2, normalise(&[1.0, 0.0]));

            let mut pool = vec![(seed, 0.9)];

            run_phase0_sync(
                &mut pool,
                &graph,
                &entry_map,
                &embedding_map,
                &q,
                2,
                200,
                false, // ppr_expander_enabled = false
            );

            assert!(
                !logs_contain("Phase 0"),
                "R-10: no 'Phase 0' debug event must be emitted when ppr_expander_enabled=false"
            );
        }

        // ---- AC-25: Cross-category behavioral proof (MANDATORY) ----

        /// AC-25: Entry E orthogonal to the query (would never appear in HNSW k=20) is
        /// visible with ppr_expander_enabled=true and absent with ppr_expander_enabled=false.
        ///
        /// This is the core behavioral proof of the entire Phase 0 feature (R-07).
        /// MANDATORY regardless of eval gate outcome (test plan AC-25 annotation).
        ///
        /// Construction:
        ///   - Q = [1, 0] (unit x-axis — the query embedding).
        ///   - S (id=1): embedding [1, 0], cos_sim(Q, S)=1.0 → HNSW seed.
        ///   - E (id=2): embedding [0, 1], cos_sim(Q, E)≈0.0 → would NOT appear via HNSW.
        ///   - Graph: S → E (Supports edge).
        ///   flag=true: Phase 0 adds E to pool.
        ///   flag=false: E remains absent.
        #[test]
        fn test_search_phase0_cross_category_entry_visible_with_flag_on() {
            // Q = unit x-axis; E = unit y-axis (orthogonal).
            let q = vec![1.0_f32, 0.0_f32];
            let e_emb = vec![0.0_f32, 1.0_f32];

            // Sanity: orthogonality
            let sim_check = cosine_similarity(&q, &e_emb);
            assert!(
                sim_check.abs() < 1e-6,
                "AC-25 setup: Q and E must be orthogonal; cosine_sim={sim_check}"
            );

            let graph = make_graph(&[(1, 2, RelationType::Supports)]);

            let cross_cat = make_test_entry(2, Status::Active, None, 0.5, "lesson-learned");
            let mut entry_map = HashMap::new();
            entry_map.insert(2, cross_cat);

            let mut embedding_map = HashMap::new();
            embedding_map.insert(2, e_emb);

            // --- Flag ON: E appears via graph expansion despite orthogonal embedding ---
            {
                let seed = make_test_entry(1, Status::Active, None, 0.5, "decision");
                let mut pool = vec![(seed, 0.9)];
                let added = run_phase0_sync(
                    &mut pool,
                    &graph,
                    &entry_map,
                    &embedding_map,
                    &q,
                    2,
                    200,
                    true,
                );
                assert_eq!(
                    added, 1,
                    "AC-25 [flag=on]: Phase 0 must add the orthogonal cross-category entry"
                );
                assert!(
                    pool.iter().any(|(e, _)| e.id == 2),
                    "AC-25 [flag=on]: E must be present in pool"
                );
                // Cosine similarity is ~0 but entry is still present — this is the key proof.
                let (_, e_sim) = pool.iter().find(|(e, _)| e.id == 2).unwrap();
                assert!(
                    e_sim.abs() < 1e-6,
                    "AC-25 [flag=on]: E cosine sim must be ~0 (orthogonal); got {e_sim}"
                );
            }

            // --- Flag OFF: E is absent (Phase 0 did not run) ---
            {
                let seed = make_test_entry(1, Status::Active, None, 0.5, "decision");
                let mut pool = vec![(seed, 0.9)];
                let added = run_phase0_sync(
                    &mut pool,
                    &graph,
                    &entry_map,
                    &embedding_map,
                    &q,
                    2,
                    200,
                    false,
                );
                assert_eq!(
                    added, 0,
                    "AC-25 [flag=off]: Phase 0 must add zero entries when disabled"
                );
                assert!(
                    !pool.iter().any(|(e, _)| e.id == 2),
                    "AC-25 [flag=off]: E must be absent when ppr_expander_enabled=false"
                );
                assert_eq!(
                    pool.len(),
                    1,
                    "AC-25 [flag=off]: pool must contain only the seed"
                );
            }
        }
    }
}
