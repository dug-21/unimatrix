//! StoreService correction operations.
//!
//! Split from store_ops.rs to keep files under 500 lines.
//! Contains the correct() method using direct SQL (ADR-004, nxs-008).

use std::sync::Arc;

use unimatrix_core::{CoreError, EmbedService, EntryRecord, NewEntry};
use unimatrix_store::read::{ENTRY_COLUMNS, entry_from_row, load_tags_for_entries};
use unimatrix_store::rusqlite::{self, OptionalExtension};
use unimatrix_store::{StoreError, compute_content_hash, status_counter_key};

use crate::error::ServerError;
use crate::infra::audit::{AuditEvent, Outcome};
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
                let conn = &*txn.guard;

                // 1. Read original entry via entry_from_row
                let mut original: EntryRecord = conn
                    .query_row(
                        &format!("SELECT {} FROM entries WHERE id = ?1", ENTRY_COLUMNS),
                        rusqlite::params![original_id as i64],
                        entry_from_row,
                    )
                    .optional()
                    .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?
                    .ok_or(ServerError::Core(CoreError::Store(
                        StoreError::EntryNotFound(original_id),
                    )))?;

                // Load tags for original
                let tag_map = load_tags_for_entries(conn, &[original_id])
                    .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
                if let Some(tags) = tag_map.get(&original_id) {
                    original.tags = tags.clone();
                }

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
                let new_id = unimatrix_store::counters::next_entry_id(conn)
                    .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

                // 4. Deprecate original (direct column UPDATE)
                let old_status = original.status;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                conn.execute(
                    "UPDATE entries SET status = ?1, superseded_by = ?2, \
                     correction_count = correction_count + 1, updated_at = ?3 \
                     WHERE id = ?4",
                    rusqlite::params![
                        unimatrix_store::Status::Deprecated as u8 as i64,
                        new_id as i64,
                        now as i64,
                        original_id as i64
                    ],
                )
                .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

                // Update status counters
                unimatrix_store::counters::decrement_counter(
                    conn,
                    status_counter_key(old_status),
                    1,
                )
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
                unimatrix_store::counters::increment_counter(
                    conn,
                    status_counter_key(unimatrix_store::Status::Deprecated),
                    1,
                )
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

                // Update original record for return value
                original.status = unimatrix_store::Status::Deprecated;
                original.superseded_by = Some(new_id);
                original.correction_count += 1;
                original.updated_at = now;

                // 5. Build correction EntryRecord
                let content_hash = compute_content_hash(&corrected.title, &corrected.content);
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
                    pre_quarantine_status: None,
                };

                // 6. INSERT correction with named params (ADR-004)
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
                        ":id": correction.id as i64,
                        ":title": &correction.title,
                        ":content": &correction.content,
                        ":topic": &correction.topic,
                        ":category": &correction.category,
                        ":source": &correction.source,
                        ":status": correction.status as u8 as i64,
                        ":confidence": correction.confidence,
                        ":created_at": correction.created_at as i64,
                        ":updated_at": correction.updated_at as i64,
                        ":last_accessed_at": correction.last_accessed_at as i64,
                        ":access_count": correction.access_count as i64,
                        ":supersedes": correction.supersedes.map(|v| v as i64),
                        ":superseded_by": correction.superseded_by.map(|v| v as i64),
                        ":correction_count": correction.correction_count as i64,
                        ":embedding_dim": correction.embedding_dim as i64,
                        ":created_by": &correction.created_by,
                        ":modified_by": &correction.modified_by,
                        ":content_hash": &correction.content_hash,
                        ":previous_hash": &correction.previous_hash,
                        ":version": correction.version as i64,
                        ":feature_cycle": &correction.feature_cycle,
                        ":trust_source": &correction.trust_source,
                        ":helpful_count": correction.helpful_count as i64,
                        ":unhelpful_count": correction.unhelpful_count as i64,
                    },
                )
                .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

                // 7. Insert tags for correction
                for tag in &correction.tags {
                    conn.execute(
                        "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                        rusqlite::params![new_id as i64, tag],
                    )
                    .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
                }

                // 8. Insert vector mapping
                conn.execute(
                    "INSERT OR REPLACE INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)",
                    rusqlite::params![new_id as i64, data_id as i64],
                )
                .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

                // 9. Status counter for correction
                unimatrix_store::counters::increment_counter(
                    conn,
                    status_counter_key(correction.status),
                    1,
                )
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

                // 10. Write audit event with both IDs
                let audit_with_ids = AuditEvent {
                    target_ids: vec![original_id, new_id],
                    ..audit_event
                };
                audit_log.write_in_txn(&txn, audit_with_ids)?;

                // 11. Commit
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
                ServerError::InvalidInput { reason, .. } => ServiceError::ValidationFailed(reason),
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
