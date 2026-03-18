//! StoreService: unified write operations with atomic audit.
//!
//! Replaces inline write logic in tools.rs context_store and context_correct.
//! Uses direct SQL with named params (ADR-004, nxs-008).

use std::sync::Arc;

use unimatrix_adapt::AdaptationService;
use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{
    CoreError, EmbedService, EntryRecord, NewEntry, Store, VectorAdapter, VectorIndex,
};

use crate::infra::audit::{AuditEvent, AuditLog, Outcome};
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::timeout::{MCP_HANDLER_TIMEOUT, spawn_blocking_with_timeout};
use crate::services::gateway::SecurityGateway;
use crate::services::{AuditContext, CallerId, ServiceError};

/// Near-duplicate cosine similarity threshold.
const DUPLICATE_THRESHOLD: f64 = 0.92;

/// HNSW search expansion factor.
const EF_SEARCH: usize = 32;

/// Result of an insert operation.
pub(crate) struct InsertResult {
    pub entry: EntryRecord,
    pub duplicate_of: Option<u64>,
    /// Similarity score when duplicate detected (for response formatting).
    pub duplicate_similarity: Option<f64>,
}

/// Result of a correct operation.
pub(crate) struct CorrectResult {
    pub corrected_entry: EntryRecord,
    pub deprecated_original: EntryRecord,
}

/// Unified write operations service.
#[derive(Clone)]
pub(crate) struct StoreService {
    pub(crate) store: Arc<Store>,
    pub(crate) vector_index: Arc<VectorIndex>,
    #[allow(dead_code)]
    pub(crate) vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    #[allow(dead_code)]
    pub(crate) entry_store: Arc<Store>,
    pub(crate) embed_service: Arc<EmbedServiceHandle>,
    pub(crate) adapt_service: Arc<AdaptationService>,
    pub(crate) gateway: Arc<SecurityGateway>,
    pub(crate) audit: Arc<AuditLog>,
}

impl StoreService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
        entry_store: Arc<Store>,
        embed_service: Arc<EmbedServiceHandle>,
        adapt_service: Arc<AdaptationService>,
        gateway: Arc<SecurityGateway>,
        audit: Arc<AuditLog>,
    ) -> Self {
        StoreService {
            store,
            vector_index,
            vector_store,
            entry_store,
            embed_service,
            adapt_service,
            gateway,
            audit,
        }
    }

    /// Insert a new entry with atomic audit.
    ///
    /// Steps: validate (S2/S1/S3), embed, adapt, duplicate check, atomic insert+audit,
    /// HNSW insert, update adaptation prototypes.
    pub(crate) async fn insert(
        &self,
        entry: NewEntry,
        embedding: Option<Vec<f32>>,
        audit_ctx: &AuditContext,
        caller_id: &CallerId,
    ) -> Result<InsertResult, ServiceError> {
        // Step 0: S2 rate check before any work
        self.gateway.check_write_rate(caller_id)?;

        // Step 1: S1 + S3 validation via gateway
        self.gateway.validate_write(
            &entry.title,
            &entry.content,
            &entry.category,
            &entry.tags,
            audit_ctx,
        )?;

        // Step 2: Generate embedding if not pre-computed
        let (embedding, adapted_for_prototypes) = match embedding {
            Some(e) => (e, None),
            None => {
                let adapter = self
                    .embed_service
                    .get_adapter()
                    .await
                    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;
                let title = entry.title.clone();
                let content = entry.content.clone();
                let raw = spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, {
                    let adapter = Arc::clone(&adapter);
                    move || adapter.embed_entry(&title, &content)
                })
                .await
                .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
                .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;

                let adapted = self.adapt_service.adapt_embedding(
                    &raw,
                    Some(&entry.category),
                    Some(&entry.topic),
                );
                let normalized = unimatrix_embed::l2_normalized(&adapted);
                (normalized, Some(adapted))
            }
        };

        // Step 3: Near-duplicate detection
        let dup_results = self
            .vector_store
            .search(embedding.clone(), 1, EF_SEARCH)
            .await
            .map_err(ServiceError::Core)?;

        if let Some(top) = dup_results.first() {
            if top.similarity >= DUPLICATE_THRESHOLD {
                if let Ok(existing) = self.entry_store.get(top.entry_id).await {
                    // Audit duplicate detection
                    self.gateway.emit_audit(AuditEvent {
                        event_id: 0,
                        timestamp: 0,
                        session_id: audit_ctx.session_id.clone().unwrap_or_default(),
                        agent_id: audit_ctx.caller_id.clone(),
                        operation: "context_store".to_string(),
                        target_ids: vec![existing.id],
                        outcome: Outcome::Success,
                        detail: format!(
                            "near-duplicate detected: entry #{} at {:.2} similarity",
                            existing.id, top.similarity
                        ),
                    });
                    return Ok(InsertResult {
                        entry: existing,
                        duplicate_of: Some(top.entry_id),
                        duplicate_similarity: Some(top.similarity),
                    });
                }
                // Entry was deleted since search; proceed with store
            }
        }

        // Step 4: Atomic insert with audit
        let audit_event = AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: audit_ctx.session_id.clone().unwrap_or_default(),
            agent_id: audit_ctx.caller_id.clone(),
            operation: "context_store".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: format!("stored entry: {}", entry.title),
        };

        let data_id = self.vector_index.allocate_data_id();
        let embedding_dim = embedding.len() as u16;
        let entry_category = entry.category.clone();
        let entry_feature_cycle = entry.feature_cycle.clone();

        // 1. Insert entry (handles tags + counters atomically)
        let entry_id = self
            .store
            .insert(entry)
            .await
            .map_err(|e| ServiceError::Core(CoreError::Store(e)))?;

        // 2. Insert vector mapping
        self.store
            .put_vector_mapping(entry_id, data_id)
            .await
            .map_err(|e| ServiceError::Core(CoreError::Store(e)))?;

        // 3. Outcome index if applicable
        self.store
            .insert_outcome_index_if_applicable(entry_id, &entry_category, &entry_feature_cycle)
            .await
            .map_err(|e| ServiceError::Core(CoreError::Store(e)))?;

        // 4. Read back full record (with tags)
        let record = self
            .store
            .get(entry_id)
            .await
            .map_err(|e| ServiceError::Core(CoreError::Store(e)))?;

        // 5. Audit event — fire-and-forget to avoid blocking the write pool.
        // GH #302: the synchronous log_event() call used block_in_place, which
        // raced with the analytics drain task holding the single write connection,
        // causing a 5s pool-acquire timeout on context_store.
        let audit_event_with_target = AuditEvent {
            target_ids: vec![entry_id],
            ..audit_event
        };
        {
            let audit = Arc::clone(&self.audit);
            tokio::spawn(async move {
                let _ = audit.log_event_async(audit_event_with_target).await;
            });
        }

        // Wrap record with correct embedding_dim
        let record = EntryRecord {
            embedding_dim,
            ..record
        };

        // Step 5: HNSW insert (after transaction commits)
        if !embedding.is_empty() {
            self.vector_index
                .insert_hnsw_only(entry_id, data_id, &embedding)
                .map_err(|e| ServiceError::Core(CoreError::Vector(e)))?;
        }

        // Step 6: Update adaptation prototypes (crt-006)
        if let Some(adapted) = adapted_for_prototypes {
            self.adapt_service.update_prototypes(
                &adapted,
                Some(&record.category),
                Some(&record.topic),
            );
        }

        Ok(InsertResult {
            entry: record,
            duplicate_of: None,
            duplicate_similarity: None,
        })
    }
}
