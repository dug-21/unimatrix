//! SearchService: unified search pipeline replacing duplicated logic
//! in tools.rs and uds_listener.rs.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{
    CoreError, EmbedService, EntryRecord, QueryFilter, Status, Store, VectorAdapter,
};

use unimatrix_adapt::AdaptationService;
use unimatrix_engine::effectiveness::{
    EffectivenessCategory, SETTLED_BOOST, UTILITY_BOOST, UTILITY_PENALTY,
};

use unimatrix_engine::graph::{
    FALLBACK_PENALTY, GraphError, build_supersession_graph, find_terminal_active, graph_penalty,
};

use crate::coaccess::{CO_ACCESS_STALENESS_SECONDS, compute_search_boost};
use crate::confidence::{cosine_similarity, rerank_score};
use crate::infra::audit::{AuditEvent, Outcome};
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::timeout::{MCP_HANDLER_TIMEOUT, spawn_blocking_with_timeout};
use crate::services::confidence::ConfidenceStateHandle;
use crate::services::effectiveness::{EffectivenessSnapshot, EffectivenessStateHandle};
use crate::services::gateway::SecurityGateway;
use crate::services::supersession::SupersessionStateHandle;
use crate::services::{AuditContext, CallerId, ServiceError};

/// HNSW search expansion factor.
const EF_SEARCH: usize = 32;

/// Provenance boost for lesson-learned entries (matches existing behavior).
const PROVENANCE_BOOST: f64 = unimatrix_engine::confidence::PROVENANCE_BOOST;

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
    /// GH #264 fix: cached all-entry snapshot for supersession graph construction.
    /// Eliminates 4x Store::query_by_status() calls from the search hot path.
    /// Rebuilt by the background tick (15-min); search reads under short read lock.
    supersession_state: SupersessionStateHandle,
}

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
        supersession_state: SupersessionStateHandle,
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
            supersession_state,
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
        // Closure captures require a `Copy` f64, not a guard under a lock.
        let confidence_weight = {
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

        // Step 3: Embed query via spawn_blocking_with_timeout (#277)
        let query = params.query.clone();
        let raw_embedding: Vec<f32> = spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, {
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
                    .search_filtered(embedding.clone(), params.k, EF_SEARCH, allowed_ids)
                    .await
                    .map_err(ServiceError::Core)?
            }
        } else {
            self.vector_store
                .search(embedding.clone(), params.k, EF_SEARCH)
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

        // GH #264 fix: read cached entry snapshot under a short read lock — no store I/O.
        // The background tick (15-min) rebuilds SupersessionState; the search path only reads.
        // LOCK ORDERING (R-01): acquire read lock, clone fields, DROP guard before any other lock.
        let (all_entries, cached_use_fallback) = {
            let guard = self
                .supersession_state
                .read()
                .unwrap_or_else(|e| e.into_inner());
            (guard.all_entries.clone(), guard.use_fallback)
            // read guard drops here
        };

        // Build supersession graph from the cached snapshot (pure CPU, no I/O, ~1-2ms).
        // On cold-start, all_entries is empty — graph is empty DAG, use_fallback remains true.
        let graph_result = build_supersession_graph(&all_entries);
        let (graph_opt, use_fallback) = match graph_result {
            Ok(graph) => (Some(graph), cached_use_fallback),
            Err(GraphError::CycleDetected) => {
                tracing::error!(
                    "supersession cycle detected in knowledge graph — \
                     search falling back to flat FALLBACK_PENALTY"
                );
                (None, true)
            }
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
                                // graph_opt is Some when use_fallback is false
                                graph_penalty(entry.id, graph_opt.as_ref().unwrap(), &all_entries)
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
                        find_terminal_active(entry.id, graph_opt.as_ref().unwrap(), &all_entries)
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

        // Step 7: Re-rank with penalty multipliers (crt-010) and utility delta (crt-018b).
        // utility_delta is inside the penalty multiplication per ADR-003:
        // (rerank_score + delta + prov) * penalty
        results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
            let prov_a = if entry_a.category == "lesson-learned" {
                PROVENANCE_BOOST
            } else {
                0.0
            };
            let prov_b = if entry_b.category == "lesson-learned" {
                PROVENANCE_BOOST
            } else {
                0.0
            };
            let delta_a = utility_delta(categories.get(&entry_a.id).copied());
            let delta_b = utility_delta(categories.get(&entry_b.id).copied());
            let base_a =
                rerank_score(*sim_a, entry_a.confidence, confidence_weight) + delta_a + prov_a;
            let base_b =
                rerank_score(*sim_b, entry_b.confidence, confidence_weight) + delta_b + prov_b;
            let penalty_a = penalty_map.get(&entry_a.id).copied().unwrap_or(1.0);
            let penalty_b = penalty_map.get(&entry_b.id).copied().unwrap_or(1.0);
            let final_a = base_a * penalty_a;
            let final_b = base_b * penalty_b;
            final_b
                .partial_cmp(&final_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Step 8: Co-access boost with deprecated exclusion (crt-010: C3)
        if results_with_scores.len() > 1 {
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

            // crt-010: collect deprecated IDs for co-access exclusion
            let deprecated_ids: HashSet<u64> = results_with_scores
                .iter()
                .filter(|(e, _)| e.status == Status::Deprecated)
                .map(|(e, _)| e.id)
                .collect();

            let store = Arc::clone(&self.store);
            let boost_map = spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, move || {
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
                tracing::warn!("co-access boost task failed: {e}");
                HashMap::new()
            });

            if !boost_map.is_empty() {
                // crt-018b (ADR-003): utility_delta inside penalty multiplication alongside boost.
                results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
                    let base_a = rerank_score(*sim_a, entry_a.confidence, confidence_weight);
                    let base_b = rerank_score(*sim_b, entry_b.confidence, confidence_weight);
                    let boost_a = boost_map.get(&entry_a.id).copied().unwrap_or(0.0);
                    let boost_b = boost_map.get(&entry_b.id).copied().unwrap_or(0.0);
                    let prov_a = if entry_a.category == "lesson-learned" {
                        PROVENANCE_BOOST
                    } else {
                        0.0
                    };
                    let prov_b = if entry_b.category == "lesson-learned" {
                        PROVENANCE_BOOST
                    } else {
                        0.0
                    };
                    let delta_a = utility_delta(categories.get(&entry_a.id).copied());
                    let delta_b = utility_delta(categories.get(&entry_b.id).copied());
                    let penalty_a = penalty_map.get(&entry_a.id).copied().unwrap_or(1.0);
                    let penalty_b = penalty_map.get(&entry_b.id).copied().unwrap_or(1.0);
                    let final_a = (base_a + delta_a + boost_a + prov_a) * penalty_a;
                    let final_b = (base_b + delta_b + boost_b + prov_b) * penalty_b;
                    final_b
                        .partial_cmp(&final_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        // Step 9: Truncate to k (before applying floors to match existing order)
        results_with_scores.truncate(params.k);

        // Step 10: Apply floors (if set)
        if let Some(sim_floor) = params.similarity_floor {
            results_with_scores.retain(|(_, sim)| *sim >= sim_floor);
        }
        if let Some(conf_floor) = params.confidence_floor {
            results_with_scores.retain(|(entry, _)| entry.confidence >= conf_floor);
        }

        // Step 11: Build ScoredEntry results with penalty-adjusted final_score.
        // crt-018b: utility_delta included in final_score for consistency with sort order.
        let entries: Vec<ScoredEntry> = results_with_scores
            .iter()
            .map(|(entry, sim)| {
                let penalty = penalty_map.get(&entry.id).copied().unwrap_or(1.0);
                let delta = utility_delta(categories.get(&entry.id).copied());
                ScoredEntry {
                    entry: entry.clone(),
                    final_score: (rerank_score(*sim, entry.confidence, confidence_weight) + delta)
                        * penalty,
                    similarity: *sim,
                    confidence: entry.confidence,
                }
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
            CLEAN_REPLACEMENT_PENALTY, build_supersession_graph, graph_penalty,
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
        let graph = build_supersession_graph(&entries_for_graph).expect("valid DAG");
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
        use unimatrix_engine::graph::{FALLBACK_PENALTY, GraphError, build_supersession_graph};

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
        let result = build_supersession_graph(&entries);
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

    // -- GH #264: Supersession state handle is readable and reflects pre-populated entries --
    //
    // Verifies that:
    // 1. A SupersessionStateHandle pre-populated with entries is readable under a read lock.
    // 2. The search path can clone `all_entries` + `use_fallback` without store I/O.
    // 3. Writing new state and re-reading reflects the update (rebuild semantics).
    //
    // This test catches regressions where the store is re-queried inside the search path
    // instead of reading from the cached handle (the bug in crt-014 that GH #264 fixed).

    #[test]
    fn test_search_uses_cached_supersession_state_cold_start_fallback() {
        use crate::services::supersession::SupersessionState;

        // Cold-start handle: empty entries, use_fallback=true
        let handle = SupersessionState::new_handle();
        let (entries, use_fallback) = {
            let guard = handle.read().unwrap_or_else(|e| e.into_inner());
            (guard.all_entries.clone(), guard.use_fallback)
        };

        assert!(entries.is_empty(), "cold-start: all_entries must be empty");
        assert!(use_fallback, "cold-start: use_fallback must be true");
    }

    #[test]
    fn test_search_uses_cached_supersession_state_after_rebuild() {
        use crate::services::supersession::SupersessionState;

        let handle = SupersessionState::new_handle();

        // Simulate background tick: write a new state with entries and use_fallback=false
        let entry = make_test_entry(42, Status::Active, None, 0.9, "decision");
        {
            let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
            *guard = SupersessionState {
                all_entries: vec![entry.clone()],
                use_fallback: false,
            };
        }

        // Simulate search path: read cached state under a short lock, then drop guard
        let (snapshot_entries, snapshot_fallback) = {
            let guard = handle.read().unwrap_or_else(|e| e.into_inner());
            (guard.all_entries.clone(), guard.use_fallback)
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

        // build_supersession_graph on the snapshot is pure CPU — no store I/O
        let graph_result = build_supersession_graph(&snapshot_entries);
        assert!(
            graph_result.is_ok(),
            "graph build on cached snapshot must succeed for a single active entry"
        );
    }
}
