use crate::db::{SqlxStore, map_pool_timeout};
use crate::error::{PoolKind, Result, StoreError};
use crate::schema::{EntryRecord, NewEntry, Status, status_counter_key};

/// Get the current unix timestamp in seconds.
fn current_unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl SqlxStore {
    /// Insert a new entry. Returns the assigned entry_id.
    ///
    /// All columns and counters are updated atomically within a single
    /// write_pool transaction.
    pub async fn insert(&self, entry: NewEntry) -> Result<u64> {
        let now = current_unix_timestamp_secs();
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        // Step 1: Generate ID via counters module
        let id = crate::counters::next_entry_id(&mut txn).await?;

        // Step 2: Compute content hash
        let content_hash = crate::hash::compute_content_hash(&entry.title, &entry.content);

        // Step 3: INSERT into entries
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
        .bind(id as i64)
        .bind(&entry.title)
        .bind(&entry.content)
        .bind(&entry.topic)
        .bind(&entry.category)
        .bind(&entry.source)
        .bind(entry.status as u8 as i64)
        .bind(0.0_f64)
        .bind(now as i64)
        .bind(now as i64)
        .bind(0_i64)
        .bind(0_i64)
        .bind(Option::<i64>::None)
        .bind(Option::<i64>::None)
        .bind(0_i64)
        .bind(0_i64)
        .bind(&entry.created_by)
        .bind("")
        .bind(&content_hash)
        .bind("")
        .bind(1_i64)
        .bind(&entry.feature_cycle)
        .bind(&entry.trust_source)
        .bind(0_i64)
        .bind(0_i64)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 4: Insert tags into entry_tags
        for tag in &entry.tags {
            sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)")
                .bind(id as i64)
                .bind(tag)
                .execute(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;
        }

        // Step 5: Update status counter
        crate::counters::increment_counter(&mut txn, status_counter_key(entry.status), 1).await?;

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        Ok(id)
    }

    /// Update an existing entry. Returns an error if the entry does not exist.
    pub async fn update(&self, entry: EntryRecord) -> Result<()> {
        let entry_id = entry.id;
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        // Read old status for counter adjustment
        let old_status_val: Option<i64> =
            sqlx::query_scalar("SELECT status FROM entries WHERE id = ?1")
                .bind(entry_id as i64)
                .fetch_optional(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;

        let old_status_val = old_status_val.ok_or(StoreError::EntryNotFound(entry_id))?;

        // UPDATE all 24 columns
        sqlx::query(
            "UPDATE entries SET
                title = ?1, content = ?2, topic = ?3,
                category = ?4, source = ?5, status = ?6,
                confidence = ?7, created_at = ?8,
                updated_at = ?9, last_accessed_at = ?10,
                access_count = ?11, supersedes = ?12,
                superseded_by = ?13, correction_count = ?14,
                embedding_dim = ?15, created_by = ?16,
                modified_by = ?17, content_hash = ?18,
                previous_hash = ?19, version = ?20,
                feature_cycle = ?21, trust_source = ?22,
                helpful_count = ?23, unhelpful_count = ?24
             WHERE id = ?25",
        )
        .bind(&entry.title)
        .bind(&entry.content)
        .bind(&entry.topic)
        .bind(&entry.category)
        .bind(&entry.source)
        .bind(entry.status as u8 as i64)
        .bind(entry.confidence)
        .bind(entry.created_at as i64)
        .bind(entry.updated_at as i64)
        .bind(entry.last_accessed_at as i64)
        .bind(entry.access_count as i64)
        .bind(entry.supersedes.map(|v| v as i64))
        .bind(entry.superseded_by.map(|v| v as i64))
        .bind(entry.correction_count as i64)
        .bind(entry.embedding_dim as i64)
        .bind(&entry.created_by)
        .bind(&entry.modified_by)
        .bind(&entry.content_hash)
        .bind(&entry.previous_hash)
        .bind(entry.version as i64)
        .bind(&entry.feature_cycle)
        .bind(&entry.trust_source)
        .bind(entry.helpful_count as i64)
        .bind(entry.unhelpful_count as i64)
        .bind(entry_id as i64)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Replace tags: delete all, re-insert (ADR-006)
        sqlx::query("DELETE FROM entry_tags WHERE entry_id = ?1")
            .bind(entry_id as i64)
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        for tag in &entry.tags {
            sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)")
                .bind(entry_id as i64)
                .bind(tag)
                .execute(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;
        }

        // Status counter adjustment
        let new_status_val = entry.status as u8 as i64;
        if new_status_val != old_status_val {
            let old = Status::try_from(old_status_val as u8).unwrap_or(Status::Active);
            crate::counters::decrement_counter(&mut txn, status_counter_key(old), 1).await?;
            crate::counters::increment_counter(&mut txn, status_counter_key(entry.status), 1)
                .await?;
        }

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Update only the status of an entry.
    pub async fn update_status(&self, entry_id: u64, new_status: Status) -> Result<()> {
        let now = current_unix_timestamp_secs();
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        let old_status_val: Option<i64> =
            sqlx::query_scalar("SELECT status FROM entries WHERE id = ?1")
                .bind(entry_id as i64)
                .fetch_optional(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;

        let old_status_val = old_status_val.ok_or(StoreError::EntryNotFound(entry_id))?;

        sqlx::query("UPDATE entries SET status = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(new_status as u8 as i64)
            .bind(now as i64)
            .bind(entry_id as i64)
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let old = Status::try_from(old_status_val as u8).unwrap_or(Status::Active);
        crate::counters::decrement_counter(&mut txn, status_counter_key(old), 1).await?;
        crate::counters::increment_counter(&mut txn, status_counter_key(new_status), 1).await?;

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Delete an entry and all its index references.
    pub async fn delete(&self, entry_id: u64) -> Result<()> {
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        let old_status_val: Option<i64> =
            sqlx::query_scalar("SELECT status FROM entries WHERE id = ?1")
                .bind(entry_id as i64)
                .fetch_optional(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;

        let old_status_val = old_status_val.ok_or(StoreError::EntryNotFound(entry_id))?;

        // Delete from entries (CASCADE deletes entry_tags automatically)
        sqlx::query("DELETE FROM entries WHERE id = ?1")
            .bind(entry_id as i64)
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        // Delete from vector_map (no FK, manual)
        sqlx::query("DELETE FROM vector_map WHERE entry_id = ?1")
            .bind(entry_id as i64)
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let old = Status::try_from(old_status_val as u8).unwrap_or(Status::Active);
        crate::counters::decrement_counter(&mut txn, status_counter_key(old), 1).await?;

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }
}
