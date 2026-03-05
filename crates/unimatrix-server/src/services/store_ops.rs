//! StoreService: unified write operations with atomic audit.
//!
//! Replaces inline write logic in tools.rs context_store and context_correct.
//! Uses direct SQL with named params (ADR-004, nxs-008).

use std::sync::Arc;

use unimatrix_core::{
    CoreError, EmbedService, EntryRecord, NewEntry, Store, StoreAdapter, VectorAdapter,
    VectorIndex,
};
use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};
use unimatrix_store::rusqlite;
use unimatrix_store::{
    compute_content_hash, status_counter_key, StoreError,
};

use unimatrix_adapt::AdaptationService;

use crate::infra::audit::{AuditEvent, AuditLog, Outcome};
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::error::ServerError;
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
    pub(crate) entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
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
        entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
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
                let raw = tokio::task::spawn_blocking({
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

        let store = Arc::clone(&self.store);
        let audit_log = Arc::clone(&self.audit);

        let (entry_id, record) = tokio::task::spawn_blocking(move || -> Result<(u64, EntryRecord), ServerError> {
            let txn = store
                .begin_write()
                .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
            let conn = &*txn.guard;

            // 1. Allocate entry ID via counters module
            let id = unimatrix_store::counters::next_entry_id(conn)
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

            let content_hash = compute_content_hash(&entry.title, &entry.content);

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let record = EntryRecord {
                id,
                title: entry.title,
                content: entry.content,
                topic: entry.topic,
                category: entry.category,
                tags: entry.tags,
                source: entry.source,
                status: entry.status,
                confidence: 0.0,
                created_at: now,
                updated_at: now,
                last_accessed_at: 0,
                access_count: 0,
                supersedes: None,
                superseded_by: None,
                correction_count: 0,
                embedding_dim,
                created_by: entry.created_by.clone(),
                modified_by: entry.created_by,
                content_hash,
                previous_hash: String::new(),
                version: 1,
                feature_cycle: entry.feature_cycle,
                trust_source: entry.trust_source,
                helpful_count: 0,
                unhelpful_count: 0,
            };

            // 2. INSERT into entries with named params (ADR-004)
            conn.execute(
                "INSERT INTO entries (id, title, content, topic, category, source,
                    status, confidence, created_at, updated_at, last_accessed_at,
                    access_count, supersedes, superseded_by, correction_count,
                    embedding_dim, created_by, modified_by, content_hash,
                    previous_hash, version, feature_cycle, trust_source,
                    helpful_count, unhelpful_count)
                 VALUES (:id, :title, :content, :topic, :category, :source,
                    :status, :confidence, :created_at, :updated_at, :last_accessed_at,
                    :access_count, :supersedes, :superseded_by, :correction_count,
                    :embedding_dim, :created_by, :modified_by, :content_hash,
                    :previous_hash, :version, :feature_cycle, :trust_source,
                    :helpful_count, :unhelpful_count)",
                rusqlite::named_params! {
                    ":id": id as i64,
                    ":title": &record.title,
                    ":content": &record.content,
                    ":topic": &record.topic,
                    ":category": &record.category,
                    ":source": &record.source,
                    ":status": record.status as u8 as i64,
                    ":confidence": record.confidence,
                    ":created_at": record.created_at as i64,
                    ":updated_at": record.updated_at as i64,
                    ":last_accessed_at": record.last_accessed_at as i64,
                    ":access_count": record.access_count as i64,
                    ":supersedes": record.supersedes.map(|v| v as i64),
                    ":superseded_by": record.superseded_by.map(|v| v as i64),
                    ":correction_count": record.correction_count as i64,
                    ":embedding_dim": record.embedding_dim as i64,
                    ":created_by": &record.created_by,
                    ":modified_by": &record.modified_by,
                    ":content_hash": &record.content_hash,
                    ":previous_hash": &record.previous_hash,
                    ":version": record.version as i64,
                    ":feature_cycle": &record.feature_cycle,
                    ":trust_source": &record.trust_source,
                    ":helpful_count": record.helpful_count as i64,
                    ":unhelpful_count": record.unhelpful_count as i64,
                },
            ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

            // 3. Insert tags into entry_tags
            for tag in &record.tags {
                conn.execute(
                    "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                    rusqlite::params![id as i64, tag],
                ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
            }

            // 4. Insert vector mapping
            conn.execute(
                "INSERT OR REPLACE INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)",
                rusqlite::params![id as i64, data_id as i64],
            ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

            // 5. Outcome index (if applicable)
            if record.category == "outcome" && !record.feature_cycle.is_empty() {
                conn.execute(
                    "INSERT OR IGNORE INTO outcome_index (feature_cycle, entry_id) VALUES (?1, ?2)",
                    rusqlite::params![&record.feature_cycle, id as i64],
                ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
            }

            // 6. Status counter
            unimatrix_store::counters::increment_counter(conn, status_counter_key(record.status), 1)
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

            // 7. Audit event
            let audit_event_with_target = AuditEvent {
                target_ids: vec![id],
                ..audit_event
            };
            audit_log.write_in_txn(&txn, audit_event_with_target)?;

            // 8. Commit
            txn.commit()
                .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
            Ok((id, record))
        })
        .await
        .map_err(|e| ServiceError::Core(CoreError::JoinError(e.to_string())))?
        .map_err(|e| -> ServiceError {
            let server_err: ServerError = e;
            match server_err {
                ServerError::Core(ce) => ServiceError::Core(ce),
                other => ServiceError::EmbeddingFailed(format!("{other}")),
            }
        })?;

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
