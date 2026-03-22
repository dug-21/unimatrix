//! Extended write operations for the sqlx backend.
//!
//! Usage tracking, confidence updates, vector mappings, feature entries,
//! co-access pairs, and observation metrics.

use std::collections::HashSet;

use crate::analytics::AnalyticsWrite;
use crate::db::{SqlxStore, map_pool_timeout};
use crate::error::{PoolKind, Result, StoreError};
use crate::read::{ENTRY_COLUMNS, entry_from_row, load_tags_for_entries};
use crate::schema::EntryRecord;

/// Get the current unix timestamp in seconds.
fn current_unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl SqlxStore {
    /// Record usage for a batch of entries in a single write transaction.
    pub async fn record_usage(
        &self,
        all_ids: &[u64],
        access_ids: &[u64],
        helpful_ids: &[u64],
        unhelpful_ids: &[u64],
        decrement_helpful_ids: &[u64],
        decrement_unhelpful_ids: &[u64],
    ) -> Result<()> {
        self.record_usage_with_confidence(
            all_ids,
            access_ids,
            helpful_ids,
            unhelpful_ids,
            decrement_helpful_ids,
            decrement_unhelpful_ids,
            None::<Box<dyn Fn(&EntryRecord, u64) -> f64 + Send + Sync>>,
        )
        .await
    }

    /// Record usage for a batch of entries with optional inline confidence computation.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub async fn record_usage_with_confidence(
        &self,
        all_ids: &[u64],
        access_ids: &[u64],
        helpful_ids: &[u64],
        unhelpful_ids: &[u64],
        decrement_helpful_ids: &[u64],
        decrement_unhelpful_ids: &[u64],
        confidence_fn: Option<Box<dyn Fn(&EntryRecord, u64) -> f64 + Send + Sync>>,
    ) -> Result<()> {
        if all_ids.is_empty() {
            return Ok(());
        }

        let now = current_unix_timestamp_secs();

        let access_set: HashSet<u64> = access_ids.iter().copied().collect();
        let helpful_set: HashSet<u64> = helpful_ids.iter().copied().collect();
        let unhelpful_set: HashSet<u64> = unhelpful_ids.iter().copied().collect();
        let dec_helpful_set: HashSet<u64> = decrement_helpful_ids.iter().copied().collect();
        let dec_unhelpful_set: HashSet<u64> = decrement_unhelpful_ids.iter().copied().collect();

        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        for &id in all_ids {
            // Check entry exists
            let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM entries WHERE id = ?1")
                .bind(id as i64)
                .fetch_optional(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;

            if exists.is_none() {
                continue;
            }

            // Build dynamic SET clause based on which sets contain this id
            let mut sets: Vec<String> = vec![format!("last_accessed_at = {}", now)];
            if access_set.contains(&id) {
                sets.push("access_count = access_count + 1".to_string());
            }
            if helpful_set.contains(&id) {
                sets.push("helpful_count = helpful_count + 1".to_string());
            }
            if unhelpful_set.contains(&id) {
                sets.push("unhelpful_count = unhelpful_count + 1".to_string());
            }
            if dec_helpful_set.contains(&id) {
                sets.push("helpful_count = MAX(0, helpful_count - 1)".to_string());
            }
            if dec_unhelpful_set.contains(&id) {
                sets.push("unhelpful_count = MAX(0, unhelpful_count - 1)".to_string());
            }

            let sql = format!("UPDATE entries SET {} WHERE id = ?1", sets.join(", "));
            sqlx::query(&sql)
                .bind(id as i64)
                .execute(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;

            // If confidence_fn provided, read back the record and recompute
            if let Some(ref f) = confidence_fn {
                let entry_sql = format!("SELECT {} FROM entries WHERE id = ?1", ENTRY_COLUMNS);
                let row = sqlx::query(&entry_sql)
                    .bind(id as i64)
                    .fetch_optional(&mut *txn)
                    .await
                    .map_err(|e| StoreError::Database(e.into()))?
                    .ok_or(StoreError::EntryNotFound(id))?;

                let mut record = entry_from_row(&row)?;

                // Load tags for the confidence function
                let tag_map = load_tags_for_entries(self.read_pool(), &[id]).await?;
                if let Some(tags) = tag_map.get(&id) {
                    record.tags = tags.clone();
                }

                let new_confidence = f(&record, now);
                sqlx::query("UPDATE entries SET confidence = ?1 WHERE id = ?2")
                    .bind(new_confidence)
                    .bind(id as i64)
                    .execute(&mut *txn)
                    .await
                    .map_err(|e| StoreError::Database(e.into()))?;
            }
        }

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Increment `access_count` by `amount` for each entry ID.
    pub async fn increment_access_counts(&self, ids: &[u64], amount: u32) -> Result<()> {
        if ids.is_empty() || amount == 0 {
            return Ok(());
        }
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        for &id in ids {
            sqlx::query("UPDATE entries SET access_count = access_count + ?1 WHERE id = ?2")
                .bind(amount as i64)
                .bind(id as i64)
                .execute(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;
        }

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Update the confidence score for an entry (integrity write via write_pool).
    pub async fn update_confidence(&self, entry_id: u64, confidence: f64) -> Result<()> {
        let result = sqlx::query("UPDATE entries SET confidence = ?1 WHERE id = ?2")
            .bind(confidence)
            .bind(entry_id as i64)
            .execute(&self.write_pool)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        if result.rows_affected() == 0 {
            return Err(StoreError::EntryNotFound(entry_id));
        }
        Ok(())
    }

    /// Insert or update a vector mapping (integrity write via write_pool).
    pub async fn put_vector_mapping(&self, entry_id: u64, hnsw_data_id: u64) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)")
            .bind(entry_id as i64)
            .bind(hnsw_data_id as i64)
            .execute(&self.write_pool)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Rewrite the entire vector_map table (integrity write via write_pool).
    pub async fn rewrite_vector_map(&self, mappings: &[(u64, u64)]) -> Result<()> {
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        sqlx::query("DELETE FROM vector_map")
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        for &(entry_id, data_id) in mappings {
            sqlx::query("INSERT INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)")
                .bind(entry_id as i64)
                .bind(data_id as i64)
                .execute(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;
        }

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Record feature-entry associations directly into the write pool.
    ///
    /// Writes directly (not via analytics drain) to ensure immediate read
    /// visibility. Callers (e.g. usage recording, tests) query feature_entries
    /// immediately after recording, so eventual-consistency is not acceptable.
    ///
    /// `phase` is the workflow phase active at the moment of the `context_store`
    /// call (ADR-001 crt-025). Pass `None` when no phase signal has been emitted
    /// yet for the session, or when the call site does not have phase context.
    /// Pre-existing rows written before crt-025 have `phase = NULL` (C-05).
    pub async fn record_feature_entries(
        &self,
        feature_cycle: &str,
        entry_ids: &[u64],
        phase: Option<&str>,
    ) -> Result<()> {
        for &entry_id in entry_ids {
            sqlx::query(
                "INSERT OR IGNORE INTO feature_entries (feature_id, entry_id, phase) VALUES (?1, ?2, ?3)",
            )
            .bind(feature_cycle)
            .bind(entry_id as i64)
            .bind(phase)  // Option<&str> — sqlx encodes None as NULL
            .execute(&self.write_pool)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        }
        Ok(())
    }

    /// Record co-access pairs (analytics write via enqueue_analytics).
    pub fn record_co_access_pairs(&self, pairs: &[(u64, u64)]) {
        for &(a, b) in pairs {
            let (min_id, max_id) = crate::schema::co_access_key(a, b);
            if min_id == max_id {
                continue; // Skip self-pairs
            }
            self.enqueue_analytics(AnalyticsWrite::CoAccess {
                id_a: min_id,
                id_b: max_id,
            });
        }
    }

    /// Remove stale co-access pairs (integrity write via write_pool).
    ///
    /// Returns the number of deleted rows.
    pub async fn cleanup_stale_co_access(&self, staleness_cutoff: u64) -> Result<u64> {
        let result = sqlx::query("DELETE FROM co_access WHERE last_updated < ?1")
            .bind(staleness_cutoff as i64)
            .execute(&self.write_pool)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(result.rows_affected())
    }

    /// Update entry status with optional modified_by and pre_quarantine_status fields.
    ///
    /// Used by quarantine / restore / deprecate operations that need to set
    /// additional fields beyond status + updated_at.
    ///
    /// Returns the updated EntryRecord (with tags loaded).
    pub async fn update_entry_status_extended(
        &self,
        entry_id: u64,
        new_status: crate::schema::Status,
        set_modified_by: Option<&str>,
        pre_quarantine_status: Option<u8>,
    ) -> Result<crate::schema::EntryRecord> {
        let now = current_unix_timestamp_secs();

        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| crate::db::map_pool_timeout(e, crate::error::PoolKind::Write))?;

        // Read current status for counter adjustment
        let row: Option<(i64, Option<i64>)> =
            sqlx::query_as("SELECT status, pre_quarantine_status FROM entries WHERE id = ?1")
                .bind(entry_id as i64)
                .fetch_optional(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;

        let (old_status_val, _old_pre_q) = row.ok_or(StoreError::EntryNotFound(entry_id))?;
        let old_status = crate::schema::Status::try_from(old_status_val as u8)
            .unwrap_or(crate::schema::Status::Active);

        // Build the UPDATE depending on which fields need updating
        let pre_q_i64 = pre_quarantine_status.map(|v| v as i64);

        if let Some(modified_by) = set_modified_by {
            sqlx::query(
                "UPDATE entries SET status = ?1, modified_by = ?2, updated_at = ?3, \
                 pre_quarantine_status = ?4 WHERE id = ?5",
            )
            .bind(new_status as u8 as i64)
            .bind(modified_by)
            .bind(now as i64)
            .bind(pre_q_i64)
            .bind(entry_id as i64)
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        } else {
            sqlx::query(
                "UPDATE entries SET status = ?1, updated_at = ?2, \
                 pre_quarantine_status = ?3 WHERE id = ?4",
            )
            .bind(new_status as u8 as i64)
            .bind(now as i64)
            .bind(pre_q_i64)
            .bind(entry_id as i64)
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        }

        // Update counters
        crate::counters::decrement_counter(
            &mut *txn,
            crate::schema::status_counter_key(old_status),
            1,
        )
        .await?;
        crate::counters::increment_counter(
            &mut *txn,
            crate::schema::status_counter_key(new_status),
            1,
        )
        .await?;

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        // Re-read the updated entry with tags
        self.get(entry_id).await
    }

    /// Deprecate an entry and insert a correction in a single write transaction.
    ///
    /// Atomically:
    /// - Updates original entry: status=Deprecated, superseded_by=new_id, correction_count++
    /// - Inserts new correction entry (all standard fields)
    /// - Inserts tags for the correction
    /// - Updates status counters for both entries
    ///
    /// Does NOT insert vector mapping or outcome index — caller handles those.
    /// Does NOT write audit — caller handles that separately.
    ///
    /// Returns (deprecated_original_record, new_correction_record).
    #[allow(clippy::too_many_arguments)]
    pub async fn correct_entry(
        &self,
        original_id: u64,
        correction: crate::schema::NewEntry,
        new_data_id: u64, // HNSW data_id pre-allocated by caller
        embedding_dim: u16,
    ) -> Result<(crate::schema::EntryRecord, crate::schema::EntryRecord)> {
        let now = current_unix_timestamp_secs();
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| crate::db::map_pool_timeout(e, crate::error::PoolKind::Write))?;

        // 1. Read the original entry
        let sql = format!(
            "SELECT {} FROM entries WHERE id = ?1",
            crate::read::ENTRY_COLUMNS
        );
        let original_row = sqlx::query(&sql)
            .bind(original_id as i64)
            .fetch_optional(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?
            .ok_or(StoreError::EntryNotFound(original_id))?;
        let mut original = crate::read::entry_from_row(&original_row)?;

        // Load tags for original
        let tag_map = crate::read::load_tags_for_entries(self.read_pool(), &[original_id]).await?;
        if let Some(tags) = tag_map.get(&original_id) {
            original.tags = tags.clone();
        }

        // Validate status
        if original.status == crate::schema::Status::Deprecated {
            return Err(StoreError::InvalidInput {
                field: "original_id".to_string(),
                reason: "cannot correct a deprecated entry".to_string(),
            });
        }
        if original.status == crate::schema::Status::Quarantined {
            return Err(StoreError::InvalidInput {
                field: "original_id".to_string(),
                reason: "cannot correct quarantined entry; restore first".to_string(),
            });
        }

        // 2. Allocate new entry ID
        let new_id = crate::counters::next_entry_id(&mut *txn).await?;

        // 3. Deprecate original
        let old_status = original.status;
        sqlx::query(
            "UPDATE entries SET status = ?1, superseded_by = ?2, \
             correction_count = correction_count + 1, updated_at = ?3 \
             WHERE id = ?4",
        )
        .bind(crate::schema::Status::Deprecated as u8 as i64)
        .bind(new_id as i64)
        .bind(now as i64)
        .bind(original_id as i64)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        crate::counters::decrement_counter(
            &mut *txn,
            crate::schema::status_counter_key(old_status),
            1,
        )
        .await?;
        crate::counters::increment_counter(
            &mut *txn,
            crate::schema::status_counter_key(crate::schema::Status::Deprecated),
            1,
        )
        .await?;

        // 4. Build new correction record
        let content_hash =
            crate::hash::compute_content_hash(&correction.title, &correction.content);
        let new_rec = crate::schema::EntryRecord {
            id: new_id,
            title: correction.title.clone(),
            content: correction.content.clone(),
            topic: correction.topic.clone(),
            category: correction.category.clone(),
            tags: correction.tags.clone(),
            source: correction.source.clone(),
            status: correction.status,
            confidence: 0.0,
            created_at: now,
            updated_at: now,
            last_accessed_at: 0,
            access_count: 0,
            supersedes: Some(original_id),
            superseded_by: None,
            correction_count: 0,
            embedding_dim,
            created_by: correction.created_by.clone(),
            modified_by: correction.created_by.clone(),
            content_hash: content_hash.clone(),
            previous_hash: String::new(),
            version: 1,
            feature_cycle: correction.feature_cycle.clone(),
            trust_source: correction.trust_source.clone(),
            helpful_count: 0,
            unhelpful_count: 0,
            pre_quarantine_status: None,
        };

        // 5. Insert correction entry
        sqlx::query(
            "INSERT INTO entries (id, title, content, topic, category, source,
                status, confidence, created_at, updated_at, last_accessed_at,
                access_count, supersedes, superseded_by, correction_count,
                embedding_dim, created_by, modified_by, content_hash,
                previous_hash, version, feature_cycle, trust_source,
                helpful_count, unhelpful_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10, ?11,
                ?12, ?13, ?14, ?15,
                ?16, ?17, ?18, ?19,
                ?20, ?21, ?22, ?23,
                ?24, ?25)",
        )
        .bind(new_id as i64)
        .bind(&new_rec.title)
        .bind(&new_rec.content)
        .bind(&new_rec.topic)
        .bind(&new_rec.category)
        .bind(&new_rec.source)
        .bind(new_rec.status as u8 as i64)
        .bind(0.0_f64)
        .bind(now as i64)
        .bind(now as i64)
        .bind(0_i64)
        .bind(0_i64)
        .bind(new_rec.supersedes.map(|v| v as i64))
        .bind(Option::<i64>::None)
        .bind(0_i64)
        .bind(embedding_dim as i64)
        .bind(&new_rec.created_by)
        .bind(&new_rec.modified_by)
        .bind(&content_hash)
        .bind("")
        .bind(1_i64)
        .bind(&new_rec.feature_cycle)
        .bind(&new_rec.trust_source)
        .bind(0_i64)
        .bind(0_i64)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // 6. Insert tags
        for tag in &new_rec.tags {
            sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)")
                .bind(new_id as i64)
                .bind(tag)
                .execute(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;
        }

        // 7. Insert vector mapping
        sqlx::query("INSERT OR REPLACE INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)")
            .bind(new_id as i64)
            .bind(new_data_id as i64)
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        // 8. Status counter for correction
        crate::counters::increment_counter(
            &mut *txn,
            crate::schema::status_counter_key(new_rec.status),
            1,
        )
        .await?;

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        // Update original record for return value
        original.status = crate::schema::Status::Deprecated;
        original.superseded_by = Some(new_id);
        original.correction_count += 1;
        original.updated_at = now;

        Ok((original, new_rec))
    }

    /// Insert into outcome_index if category is "outcome" and feature_cycle is non-empty.
    ///
    /// Idempotent: uses INSERT OR IGNORE.
    pub async fn insert_outcome_index_if_applicable(
        &self,
        entry_id: u64,
        category: &str,
        feature_cycle: &str,
    ) -> Result<()> {
        if category != "outcome" || feature_cycle.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "INSERT OR IGNORE INTO outcome_index (feature_cycle, entry_id) VALUES (?1, ?2)",
        )
        .bind(feature_cycle)
        .bind(entry_id as i64)
        .execute(&self.write_pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Store observation metrics for a feature cycle (nxs-009: typed API).
    ///
    /// Enqueues `ObservationMetric` for the universal row and separate
    /// `ObservationPhaseMetric` events for each phase row (OQ-NEW-01).
    /// Both enqueue calls are fire-and-forget analytics writes.
    ///
    /// Note: because these go through the analytics queue they are NOT
    /// in a single atomic transaction. The drain task commits them
    /// individually per batch. This is consistent with the analytics
    /// routing rule (AC-08) — `observation_metrics` is an analytics table.
    pub fn store_metrics(&self, feature_cycle: &str, mv: &crate::metrics::MetricVector) {
        let u = &mv.universal;

        // Serialize domain_metrics to JSON. NULL when empty (FR-05.3: claude-code sessions).
        // Non-empty maps are stored as a flat JSON object: {"key": value, ...}.
        let domain_metrics_json: Option<String> = if mv.domain_metrics.is_empty() {
            None
        } else {
            match serde_json::to_string(&mv.domain_metrics) {
                Ok(json) => Some(json),
                Err(e) => {
                    tracing::error!(
                        feature_cycle,
                        error = %e,
                        "failed to serialize domain_metrics; storing NULL"
                    );
                    None
                }
            }
        };

        self.enqueue_analytics(AnalyticsWrite::ObservationMetric {
            feature_cycle: feature_cycle.to_owned(),
            computed_at: mv.computed_at as i64,
            total_tool_calls: u.total_tool_calls as i64,
            total_duration_secs: u.total_duration_secs as i64,
            session_count: u.session_count as i64,
            search_miss_rate: u.search_miss_rate,
            edit_bloat_total_kb: u.edit_bloat_total_kb,
            edit_bloat_ratio: u.edit_bloat_ratio,
            permission_friction_events: u.permission_friction_events as i64,
            bash_for_search_count: u.bash_for_search_count as i64,
            cold_restart_events: u.cold_restart_events as i64,
            coordinator_respawn_count: u.coordinator_respawn_count as i64,
            parallel_call_rate: u.parallel_call_rate,
            context_load_before_first_write_kb: u.context_load_before_first_write_kb,
            total_context_loaded_kb: u.total_context_loaded_kb,
            post_completion_work_pct: u.post_completion_work_pct,
            follow_up_issues_created: u.follow_up_issues_created as i64,
            knowledge_entries_stored: u.knowledge_entries_stored as i64,
            sleep_workaround_count: u.sleep_workaround_count as i64,
            agent_hotspot_count: u.agent_hotspot_count as i64,
            friction_hotspot_count: u.friction_hotspot_count as i64,
            session_hotspot_count: u.session_hotspot_count as i64,
            scope_hotspot_count: u.scope_hotspot_count as i64,
            domain_metrics_json,
        });

        // Enqueue delete of all existing phase rows for this feature_cycle BEFORE
        // inserting new phases. The drain task processes events in order within a batch,
        // so the delete executes before the inserts, ensuring stale phases (from a
        // previous store_metrics call) are removed.
        self.enqueue_analytics(AnalyticsWrite::DeleteObservationPhases {
            feature_cycle: feature_cycle.to_owned(),
        });

        for (phase_name, phase) in &mv.phases {
            self.enqueue_analytics(AnalyticsWrite::ObservationPhaseMetric {
                feature_cycle: feature_cycle.to_owned(),
                phase_name: phase_name.clone(),
                duration_secs: phase.duration_secs as i64,
                tool_call_count: phase.tool_call_count as i64,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// crt-025 store-layer tests: record_feature_entries phase parameter (R-14, AC-09)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{TestEntry, open_test_store};
    use sqlx::Row as _;

    /// Compile-time structural test: verify `record_feature_entries` accepts
    /// three arguments (R-14). This compiles only if the new signature is in place.
    #[allow(dead_code)]
    fn _assert_record_feature_entries_three_arg_signature(store: &SqlxStore) {
        // This function is never called; it exists only to enforce the compile-time
        // signature check. If record_feature_entries still has the old two-arg
        // signature, this will not compile.
        let _ = store.record_feature_entries("f", &[], None);
        let _ = store.record_feature_entries("f", &[], Some("scope"));
    }

    /// Integration test: `record_feature_entries` with `phase = Some("scope")` stores
    /// the phase value (AC-09 non-NULL case).
    #[tokio::test]
    async fn test_record_feature_entries_with_phase_some() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let entry_id = store
            .insert(TestEntry::new("crt-025", "decision").build())
            .await
            .unwrap();

        store
            .record_feature_entries("crt-025", &[entry_id], Some("scope"))
            .await
            .expect("record_feature_entries must succeed");

        let row = sqlx::query(
            "SELECT phase FROM feature_entries WHERE feature_id = 'crt-025' AND entry_id = ?1",
        )
        .bind(entry_id as i64)
        .fetch_one(&store.write_pool)
        .await
        .expect("feature_entries row must exist");

        let phase: Option<String> = row.try_get(0).unwrap();
        assert_eq!(
            phase.as_deref(),
            Some("scope"),
            "phase='scope' must be written when Some('scope') is passed (AC-09)"
        );

        store.close().await.unwrap();
    }

    /// Integration test: `record_feature_entries` with `phase = None` stores SQL NULL
    /// (AC-09 NULL case).
    #[tokio::test]
    async fn test_record_feature_entries_with_phase_none() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let entry_id = store
            .insert(TestEntry::new("crt-025", "decision").build())
            .await
            .unwrap();

        store
            .record_feature_entries("crt-025", &[entry_id], None)
            .await
            .expect("record_feature_entries must succeed");

        let row = sqlx::query(
            "SELECT phase FROM feature_entries WHERE feature_id = 'crt-025' AND entry_id = ?1",
        )
        .bind(entry_id as i64)
        .fetch_one(&store.write_pool)
        .await
        .expect("feature_entries row must exist");

        let phase: Option<String> = row.try_get(0).unwrap();
        assert!(
            phase.is_none(),
            "phase must be SQL NULL when None is passed (AC-09)"
        );

        store.close().await.unwrap();
    }

    /// Integration test: two entries recorded in one call both get the same phase.
    #[tokio::test]
    async fn test_record_feature_entries_multiple_entries_same_phase() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = open_test_store(&dir).await;

        let id1 = store
            .insert(TestEntry::new("crt-025", "decision").build())
            .await
            .unwrap();
        let id2 = store
            .insert(TestEntry::new("crt-025", "pattern").build())
            .await
            .unwrap();

        store
            .record_feature_entries("crt-025", &[id1, id2], Some("implementation"))
            .await
            .expect("record_feature_entries must succeed");

        for &entry_id in &[id1, id2] {
            let row = sqlx::query(
                "SELECT phase FROM feature_entries WHERE feature_id = 'crt-025' AND entry_id = ?1",
            )
            .bind(entry_id as i64)
            .fetch_one(&store.write_pool)
            .await
            .expect("feature_entries row must exist");

            let phase: Option<String> = row.try_get(0).unwrap();
            assert_eq!(
                phase.as_deref(),
                Some("implementation"),
                "entry {entry_id} must have phase='implementation'"
            );
        }

        store.close().await.unwrap();
    }
}
