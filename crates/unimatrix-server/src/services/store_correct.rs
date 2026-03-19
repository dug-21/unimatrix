//! StoreService correction operations.
//!
//! Split from store_ops.rs to keep files under 500 lines.
//! Contains the correct() method delegating to store.correct_entry() (nxs-011).

use std::sync::Arc;

use unimatrix_core::{CoreError, EmbedService, NewEntry};
use unimatrix_store::StoreError;

use crate::infra::audit::{AuditEvent, Outcome};
use crate::infra::timeout::MCP_HANDLER_TIMEOUT;
use crate::services::{AuditContext, CallerId, ServiceError};

use super::store_ops::{CorrectResult, StoreService};

impl StoreService {
    /// Correct an existing entry: deprecate original, create correction,
    /// both in a single write transaction with audit.
    pub(crate) async fn correct(
        &self,
        original_id: u64,
        corrected: NewEntry,
        _reason: Option<String>,
        audit_ctx: &AuditContext,
        caller_id: &CallerId,
    ) -> Result<CorrectResult, ServiceError> {
        // Step 0: S2 rate check before any work
        self.gateway.check_write_rate(caller_id)?;

        // Step 1: S1 + S3 validation on corrected content
        self.gateway.validate_write(
            &corrected.title,
            &corrected.content,
            &corrected.category,
            &corrected.tags,
            audit_ctx,
        )?;

        // Step 2: Generate embedding for corrected entry
        let adapter = self
            .embed_service
            .get_adapter()
            .await
            .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;
        let title = corrected.title.clone();
        let content = corrected.content.clone();
        let category = corrected.category.clone();
        let topic = corrected.topic.clone();
        let raw = self
            .rayon_pool
            .spawn_with_timeout(MCP_HANDLER_TIMEOUT, {
                let adapter = Arc::clone(&adapter);
                move || adapter.embed_entry(&title, &content)
            })
            .await
            .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
            .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;

        let adapted = self
            .adapt_service
            .adapt_embedding(&raw, Some(&category), Some(&topic));
        let embedding = unimatrix_embed::l2_normalized(&adapted);

        // Step 3: Delegate atomic correct+audit to store.correct_entry()
        let data_id = self.vector_index.allocate_data_id();
        let embedding_dim = embedding.len() as u16;

        let (deprecated_original, new_correction) = self
            .store
            .correct_entry(original_id, corrected, data_id, embedding_dim)
            .await
            .map_err(|e| match e {
                StoreError::EntryNotFound(id) => {
                    ServiceError::Core(CoreError::Store(StoreError::EntryNotFound(id)))
                }
                StoreError::InvalidInput { field, reason } => {
                    let _ = field;
                    ServiceError::ValidationFailed(reason)
                }
                other => ServiceError::Core(CoreError::Store(other)),
            })?;

        // Audit event (after transaction commits) — fire-and-forget.
        // GH #302: same write-pool starvation fix as store_ops.rs insert().
        let audit_event_with_ids = AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: audit_ctx.session_id.clone().unwrap_or_default(),
            agent_id: audit_ctx.caller_id.clone(),
            operation: "context_correct".to_string(),
            target_ids: vec![original_id, new_correction.id],
            outcome: Outcome::Success,
            detail: format!("corrected entry #{original_id}"),
        };
        {
            let audit = Arc::clone(&self.audit);
            tokio::spawn(async move {
                let _ = audit.log_event_async(audit_event_with_ids).await;
            });
        }

        // Step 4: HNSW insert for correction (after commit)
        if !embedding.is_empty() {
            self.vector_index
                .insert_hnsw_only(new_correction.id, data_id, &embedding)
                .map_err(|e| ServiceError::Core(CoreError::Vector(e)))?;
        }

        // Step 5: Update adaptation prototypes (crt-006)
        self.adapt_service.update_prototypes(
            &adapted,
            Some(&new_correction.category),
            Some(&new_correction.topic),
        );

        Ok(CorrectResult {
            corrected_entry: new_correction,
            deprecated_original,
        })
    }
}
