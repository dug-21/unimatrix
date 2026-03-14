//! SearchService: unified search pipeline replacing duplicated logic
//! in tools.rs and uds_listener.rs.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};
use unimatrix_core::{
    EmbedService, EntryRecord, QueryFilter, Status, Store, StoreAdapter, VectorAdapter,
};

use unimatrix_adapt::AdaptationService;

use crate::coaccess::{CO_ACCESS_STALENESS_SECONDS, compute_search_boost};
use crate::confidence::{DEPRECATED_PENALTY, SUPERSEDED_PENALTY, cosine_similarity, rerank_score};
use crate::infra::audit::{AuditEvent, Outcome};
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::services::confidence::ConfidenceStateHandle;
use crate::services::gateway::SecurityGateway;
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
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    gateway: Arc<SecurityGateway>,
    /// crt-019 (ADR-001): adaptive blend weight state shared with StatusService.
    ///
    /// Readers clone `confidence_weight` f64 under a short read lock before
    /// each re-ranking step. The write lock is held only by the maintenance
    /// tick (StatusService) for the brief field-update critical section.
    #[allow(dead_code)]
    confidence_state: ConfidenceStateHandle,
}

impl SearchService {
    pub(crate) fn new(
        store: Arc<Store>,
        vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
        entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
        embed_service: Arc<EmbedServiceHandle>,
        adapt_service: Arc<AdaptationService>,
        gateway: Arc<SecurityGateway>,
        confidence_state: ConfidenceStateHandle,
    ) -> Self {
        SearchService {
            store,
            vector_store,
            entry_store,
            embed_service,
            adapt_service,
            gateway,
            confidence_state,
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

        // Step 3: Embed query via spawn_blocking
        let query = params.query.clone();
        let raw_embedding: Vec<f32> = tokio::task::spawn_blocking({
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
                .map_err(ServiceError::Core)?;
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
                    // Apply penalty markers (actual penalty applied in Step 7)
                    for (entry, _) in &results_with_scores {
                        if entry.superseded_by.is_some() {
                            penalty_map.insert(entry.id, SUPERSEDED_PENALTY);
                        } else if entry.status == Status::Deprecated {
                            penalty_map.insert(entry.id, DEPRECATED_PENALTY);
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
            // Collect successor IDs from results that have superseded_by set
            let successor_ids: Vec<u64> = results_with_scores
                .iter()
                .filter_map(|(entry, _)| entry.superseded_by)
                .collect();

            if !successor_ids.is_empty() {
                let unique_successor_ids: HashSet<u64> = successor_ids.into_iter().collect();
                let existing_ids: HashSet<u64> =
                    results_with_scores.iter().map(|(e, _)| e.id).collect();

                let to_fetch: Vec<u64> = unique_successor_ids
                    .into_iter()
                    .filter(|id| !existing_ids.contains(id))
                    .collect();

                // Batch-fetch and inject successors (FR-2.2)
                for successor_id in to_fetch {
                    let successor = match self.entry_store.get(successor_id).await {
                        Ok(s) => s,
                        Err(_) => continue, // Dangling reference — skip (FR-2.7, AC-07)
                    };

                    // FR-2.3: Only inject if Active, not itself superseded
                    if successor.status != Status::Active {
                        continue;
                    }
                    if successor.superseded_by.is_some() {
                        continue; // Single-hop only (ADR-003, AC-06)
                    }

                    // Compute cosine similarity from stored embedding (ADR-002)
                    if let Some(emb) = self.vector_store.get_embedding(successor_id).await {
                        let sim = cosine_similarity(&embedding, &emb);
                        results_with_scores.push((successor, sim));
                    }
                    // If no embedding: skip injection (R-01 fallback)
                }
            }
        }

        // Step 7: Re-rank with penalty multipliers (crt-010)
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
            let base_a = rerank_score(*sim_a, entry_a.confidence, confidence_weight) + prov_a;
            let base_b = rerank_score(*sim_b, entry_b.confidence, confidence_weight) + prov_b;
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
            let boost_map = tokio::task::spawn_blocking(move || {
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
                    let penalty_a = penalty_map.get(&entry_a.id).copied().unwrap_or(1.0);
                    let penalty_b = penalty_map.get(&entry_b.id).copied().unwrap_or(1.0);
                    let final_a = (base_a + boost_a + prov_a) * penalty_a;
                    let final_b = (base_b + boost_b + prov_b) * penalty_b;
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

        // Step 11: Build ScoredEntry results with penalty-adjusted final_score
        let entries: Vec<ScoredEntry> = results_with_scores
            .iter()
            .map(|(entry, sim)| {
                let penalty = penalty_map.get(&entry.id).copied().unwrap_or(1.0);
                ScoredEntry {
                    entry: entry.clone(),
                    final_score: rerank_score(*sim, entry.confidence, confidence_weight) * penalty,
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
        let active = make_test_entry(1, Status::Active, None, 0.65, "decision");
        let deprecated = make_test_entry(2, Status::Deprecated, None, 0.65, "decision");

        // Deprecated entry has HIGHER raw similarity than active
        let active_sim = 0.80;
        let deprecated_sim = 0.90;

        let active_score = penalized_score(active_sim, active.confidence, 1.0);
        let deprecated_score =
            penalized_score(deprecated_sim, deprecated.confidence, DEPRECATED_PENALTY);

        assert!(
            active_score > deprecated_score,
            "active ({active_score:.4}) should rank above deprecated ({deprecated_score:.4})"
        );
    }

    // -- T-SP-02: Superseded ranks below active in Flexible mode --
    #[test]
    fn superseded_below_active_flexible() {
        let active = make_test_entry(1, Status::Active, None, 0.65, "decision");
        let superseded = make_test_entry(2, Status::Deprecated, Some(1), 0.65, "decision");

        let active_sim = 0.80;
        let superseded_sim = 0.90;

        let active_score = penalized_score(active_sim, active.confidence, 1.0);
        let superseded_score =
            penalized_score(superseded_sim, superseded.confidence, SUPERSEDED_PENALTY);

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

    // -- T-SP-04: Superseded penalty is harsher than deprecated penalty --
    #[test]
    fn superseded_penalty_harsher() {
        assert!(
            SUPERSEDED_PENALTY < DEPRECATED_PENALTY,
            "superseded penalty ({SUPERSEDED_PENALTY}) should be < deprecated penalty ({DEPRECATED_PENALTY})"
        );
    }

    // -- T-SP-05: Deprecated-only query returns results in Flexible mode --
    #[test]
    fn deprecated_only_results_visible_flexible() {
        let deprecated = make_test_entry(1, Status::Deprecated, None, 0.65, "decision");
        let deprecated_sim = 0.85;

        // In flexible mode, deprecated entries are penalized but NOT excluded
        let score = penalized_score(deprecated_sim, deprecated.confidence, DEPRECATED_PENALTY);

        assert!(
            score > 0.0,
            "deprecated entry should have a positive score ({score:.4})"
        );
    }

    // -- T-SP-06: Successor injection ranking --
    #[test]
    fn successor_ranks_above_superseded() {
        let successor = make_test_entry(1, Status::Active, None, 0.7, "decision");
        let superseded = make_test_entry(2, Status::Deprecated, Some(1), 0.65, "decision");

        // Superseded has higher raw similarity (it matched the query better)
        let successor_sim = 0.70;
        let superseded_sim = 0.90;

        let successor_score = penalized_score(successor_sim, successor.confidence, 1.0);
        let superseded_score =
            penalized_score(superseded_sim, superseded.confidence, SUPERSEDED_PENALTY);

        assert!(
            successor_score > superseded_score,
            "successor ({successor_score:.4}) should rank above superseded ({superseded_score:.4})"
        );
    }

    // -- T-SP-07: Penalty does not affect stored confidence formula invariant --
    #[test]
    fn penalty_independent_of_confidence_formula() {
        // Penalties are multiplicative on the FINAL re-ranked score, not on confidence
        let sim = 0.9;
        let conf = 0.8;
        let base = rerank_score(sim, conf, 0.18375);
        let penalized = base * DEPRECATED_PENALTY;

        // The rerank base score is unchanged
        assert_eq!(base, rerank_score(sim, conf, 0.18375));
        // The penalty only affects the final score
        assert!(penalized < base);
        assert!((penalized - base * DEPRECATED_PENALTY).abs() < f64::EPSILON);
    }

    // -- T-SP-08: Equal similarity, penalty determines ranking --
    #[test]
    fn equal_similarity_penalty_determines_rank() {
        let sim = 0.85;
        let conf = 0.65;

        let active_score = penalized_score(sim, conf, 1.0);
        let deprecated_score = penalized_score(sim, conf, DEPRECATED_PENALTY);
        let superseded_score = penalized_score(sim, conf, SUPERSEDED_PENALTY);

        assert!(active_score > deprecated_score);
        assert!(deprecated_score > superseded_score);
    }
}
