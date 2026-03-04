//! StoreService correction operations.
//!
//! Split from store_ops.rs to keep files under 500 lines.
//! Contains the correct() method and its transaction helpers.

use std::sync::Arc;

use unimatrix_core::{
    CoreError, EmbedService, EntryRecord, NewEntry,
};
use redb::ReadableTable;
use unimatrix_store::{
    CATEGORY_INDEX, COUNTERS, ENTRIES, STATUS_INDEX, TAG_INDEX, TIME_INDEX,
    TOPIC_INDEX, VECTOR_MAP,
    compute_content_hash, deserialize_entry, increment_counter, next_entry_id,
    serialize_entry, status_counter_key, StoreError,
};

use crate::infra::audit::{AuditEvent, Outcome};
use crate::error::ServerError;
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
        let raw = tokio::task::spawn_blocking({
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

        // Step 3: Atomic correct with audit
        let audit_event = AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: audit_ctx.session_id.clone().unwrap_or_default(),
            agent_id: audit_ctx.caller_id.clone(),
            operation: "context_correct".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: format!("corrected entry #{original_id}"),
        };

        let data_id = self.vector_index.allocate_data_id();
        let embedding_dim = embedding.len() as u16;

        let store = Arc::clone(&self.store);
        let audit_log = Arc::clone(&self.audit);

        let (deprecated_original, new_correction) = tokio::task::spawn_blocking(
            move || -> Result<(EntryRecord, EntryRecord), ServerError> {
                let txn = store
                    .begin_write()
                    .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;

                // 1. Read and validate original entry
                let original_bytes = {
                    let table = txn
                        .open_table(ENTRIES)
                        .map_err(|e| ServerError::Core(CoreError::Store(StoreError::from(e))))?;
                    let guard = table
                        .get(original_id)
                        .map_err(|e| ServerError::Core(CoreError::Store(StoreError::from(e))))?
                        .ok_or(ServerError::Core(CoreError::Store(
                            StoreError::EntryNotFound(original_id),
                        )))?;
                    guard.value().to_vec()
                };
                let mut original = deserialize_entry(&original_bytes)
                    .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

                // 2. Verify original is not already deprecated or quarantined
                if original.status == unimatrix_store::Status::Deprecated {
                    return Err(ServerError::InvalidInput {
                        field: "original_id".to_string(),
                        reason: "cannot correct a deprecated entry".to_string(),
                    });
                }
                if original.status == unimatrix_store::Status::Quarantined {
                    return Err(ServerError::InvalidInput {
                        field: "original_id".to_string(),
                        reason: "cannot correct quarantined entry; restore first".to_string(),
                    });
                }

                // 3. Generate new entry ID
                let new_id = next_entry_id(&txn)
                    .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

                // 4. Deprecate original
                let old_status = original.status;
                original.status = unimatrix_store::Status::Deprecated;
                original.superseded_by = Some(new_id);
                original.correction_count += 1;
                original.updated_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let original_bytes = serialize_entry(&original)
                    .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
                {
                    let mut table = txn
                        .open_table(ENTRIES)
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                    table
                        .insert(original_id, original_bytes.as_slice())
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                }
                {
                    let mut table = txn
                        .open_table(STATUS_INDEX)
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                    table
                        .remove((old_status as u8, original_id))
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                    table
                        .insert(
                            (unimatrix_store::Status::Deprecated as u8, original_id),
                            (),
                        )
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                }
                decrement_counter(&txn, status_counter_key(old_status), 1)
                    .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
                increment_counter(
                    &txn,
                    status_counter_key(unimatrix_store::Status::Deprecated),
                    1,
                )
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

                // 5. Build correction EntryRecord
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let content_hash =
                    compute_content_hash(&corrected.title, &corrected.content);
                let correction = EntryRecord {
                    id: new_id,
                    title: corrected.title,
                    content: corrected.content,
                    topic: corrected.topic,
                    category: corrected.category,
                    tags: corrected.tags,
                    source: corrected.source,
                    status: corrected.status,
                    confidence: 0.0,
                    created_at: now,
                    updated_at: now,
                    last_accessed_at: 0,
                    access_count: 0,
                    supersedes: Some(original_id),
                    superseded_by: None,
                    correction_count: 0,
                    embedding_dim,
                    created_by: corrected.created_by.clone(),
                    modified_by: corrected.created_by,
                    content_hash,
                    previous_hash: String::new(),
                    version: 1,
                    feature_cycle: corrected.feature_cycle,
                    trust_source: corrected.trust_source,
                    helpful_count: 0,
                    unhelpful_count: 0,
                };

                // 6. Write correction to all indexes
                let correction_bytes = serialize_entry(&correction)
                    .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
                {
                    let mut table = txn
                        .open_table(ENTRIES)
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                    table
                        .insert(new_id, correction_bytes.as_slice())
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                }
                {
                    let mut table = txn
                        .open_table(TOPIC_INDEX)
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                    table
                        .insert((correction.topic.as_str(), new_id), ())
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                }
                {
                    let mut table = txn
                        .open_table(CATEGORY_INDEX)
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                    table
                        .insert((correction.category.as_str(), new_id), ())
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                }
                {
                    let mut table = txn
                        .open_multimap_table(TAG_INDEX)
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                    for tag in &correction.tags {
                        table
                            .insert(tag.as_str(), new_id)
                            .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                    }
                }
                {
                    let mut table = txn
                        .open_table(TIME_INDEX)
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                    table
                        .insert((correction.created_at, new_id), ())
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                }
                {
                    let mut table = txn
                        .open_table(STATUS_INDEX)
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                    table
                        .insert((correction.status as u8, new_id), ())
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                }
                increment_counter(&txn, status_counter_key(correction.status), 1)
                    .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

                {
                    let mut table = txn
                        .open_table(VECTOR_MAP)
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                    table
                        .insert(new_id, data_id)
                        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                }

                // 7. Write audit event with both IDs
                let audit_with_ids = AuditEvent {
                    target_ids: vec![original_id, new_id],
                    ..audit_event
                };
                audit_log.write_in_txn(&txn, audit_with_ids)?;

                // 8. Commit
                txn.commit()
                    .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;

                Ok((original, correction))
            },
        )
        .await
        .map_err(|e| ServiceError::Core(CoreError::JoinError(e.to_string())))?
        .map_err(|e| -> ServiceError {
            match e {
                ServerError::Core(ce) => ServiceError::Core(ce),
                ServerError::InvalidInput { reason, .. } => {
                    ServiceError::ValidationFailed(reason)
                }
                other => ServiceError::EmbeddingFailed(format!("{other}")),
            }
        })?;

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

/// Decrement a counter value, saturating at 0.
fn decrement_counter(
    txn: &redb::WriteTransaction,
    key: &str,
    amount: u64,
) -> Result<(), StoreError> {
    let mut table = txn.open_table(COUNTERS)
        .map_err(StoreError::from)?;
    let current = match table.get(key)
        .map_err(StoreError::from)?
    {
        Some(guard) => guard.value(),
        None => 0,
    };
    table.insert(key, current.saturating_sub(amount))
        .map_err(StoreError::from)?;
    Ok(())
}
