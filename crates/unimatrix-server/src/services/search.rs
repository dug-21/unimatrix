//! SearchService: unified search pipeline replacing duplicated logic
//! in tools.rs and uds_listener.rs.

use std::collections::HashMap;
use std::sync::Arc;

use unimatrix_core::{
    EmbedService, EntryRecord, QueryFilter, Store, StoreAdapter, VectorAdapter,
};
use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};

use unimatrix_adapt::AdaptationService;

use crate::coaccess::{compute_search_boost, CO_ACCESS_STALENESS_SECONDS};
use crate::confidence::rerank_score;
use crate::embed_handle::EmbedServiceHandle;
use crate::audit::{AuditEvent, Outcome};
use crate::services::gateway::SecurityGateway;
use crate::services::{AuditContext, ServiceError};

/// HNSW search expansion factor.
const EF_SEARCH: usize = 32;

/// Provenance boost for lesson-learned entries (matches existing behavior).
const PROVENANCE_BOOST: f64 = unimatrix_engine::confidence::PROVENANCE_BOOST;

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
}

impl SearchService {
    pub(crate) fn new(
        store: Arc<Store>,
        vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
        entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
        embed_service: Arc<EmbedServiceHandle>,
        adapt_service: Arc<AdaptationService>,
        gateway: Arc<SecurityGateway>,
    ) -> Self {
        SearchService {
            store,
            vector_store,
            entry_store,
            embed_service,
            adapt_service,
            gateway,
        }
    }

    /// Execute the full search pipeline.
    ///
    /// Mirrors the exact behavior of tools.rs context_search (steps 6-12)
    /// and uds_listener.rs handle_context_search.
    pub(crate) async fn search(
        &self,
        params: ServiceSearchParams,
        audit_ctx: &AuditContext,
    ) -> Result<SearchResults, ServiceError> {
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
        let adapted = self.adapt_service.adapt_embedding(&raw_embedding, None, None);
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

        // Step 7: Re-rank: 0.85*sim + 0.15*conf + provenance
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
            let score_a = rerank_score(*sim_a, entry_a.confidence) + prov_a;
            let score_b = rerank_score(*sim_b, entry_b.confidence) + prov_b;
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Step 8: Co-access boost
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
            let result_ids: Vec<u64> =
                results_with_scores.iter().map(|(e, _)| e.id).collect();

            let store = Arc::clone(&self.store);
            let boost_map = tokio::task::spawn_blocking(move || {
                compute_search_boost(&anchor_ids, &result_ids, &store, staleness_cutoff)
            })
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("co-access boost task failed: {e}");
                HashMap::new()
            });

            if !boost_map.is_empty() {
                results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
                    let base_a = rerank_score(*sim_a, entry_a.confidence);
                    let base_b = rerank_score(*sim_b, entry_b.confidence);
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
                    let final_a = base_a + boost_a + prov_a;
                    let final_b = base_b + boost_b + prov_b;
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

        // Step 11: Build ScoredEntry results
        let entries: Vec<ScoredEntry> = results_with_scores
            .iter()
            .map(|(entry, sim)| ScoredEntry {
                entry: entry.clone(),
                final_score: rerank_score(*sim, entry.confidence),
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
