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

    /// Add a single tag to an entry's `entry_tags` lane (vnc-045, ADR-001).
    ///
    /// Writes `entry_tags` DIRECTLY via a single-row INSERT — never touches any
    /// `entries` column, never re-hashes, never calls `update()` (which rewrites the
    /// content hash) or `context_correct`. The entry's learning vector, hash chain,
    /// edge set, and `id` are all invariant (R-01).
    ///
    /// Idempotent: re-adding an existing `(entry_id, tag)` is a no-op via
    /// `ON CONFLICT DO NOTHING`, not a primary-key error. `tag` is bound as a
    /// parameter and stored literally — no string interpolation (R-08).
    ///
    /// If `entry_id` was cascade-deleted between the caller's read and this call,
    /// the INSERT fails the `entry_tags` FK constraint and surfaces as
    /// `StoreError::Database` — a clean error with no partial write.
    pub async fn add_tag(&self, entry_id: u64, tag: &str) -> Result<()> {
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        sqlx::query(
            "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)
             ON CONFLICT(entry_id, tag) DO NOTHING",
        )
        .bind(entry_id as i64)
        .bind(tag)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Remove a single tag from an entry's `entry_tags` lane (vnc-045, ADR-001).
    ///
    /// Single-row `DELETE` scoped to the exact `(entry_id, tag)` — NOT the delete-all
    /// used by `update()`. Removing a tag that is absent is a no-op (zero rows
    /// affected), not an error. Touches no `entries` column (R-01). `tag` is bound —
    /// matched literally, no interpolation (R-08).
    pub async fn remove_tag(&self, entry_id: u64, tag: &str) -> Result<()> {
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        sqlx::query("DELETE FROM entry_tags WHERE entry_id = ?1 AND tag = ?2")
            .bind(entry_id as i64)
            .bind(tag)
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Atomically replace the single `namespace:*` tag on an entry (vnc-045, ADR-004).
    ///
    /// Runs a namespace-scoped `DELETE` of any prior `namespace:*` tag followed by an
    /// `INSERT` of `new_tag` in ONE SQLite transaction. If the INSERT fails, the `?`
    /// early-return drops the transaction WITHOUT commit → the whole transaction rolls
    /// back → the prior value survives; there is NEVER an observable zero-`namespace:*`
    /// window (R-02, the core atomicity guarantee).
    ///
    /// Returns the evicted prior tag value (for the audit `prior_value`), or `None`
    /// when the namespace held nothing.
    ///
    /// The DELETE is scoped by `LIKE '<namespace>:%' ESCAPE '\'` where the namespace
    /// prefix is passed through [`like_escape`] so SQL `LIKE` metacharacters (`%`, `_`)
    /// are matched literally and cannot over-match sibling tags under a different prefix
    /// (R-08). All statements use bound parameters — no string interpolation.
    ///
    /// An empty `namespace` (the degenerate colon-less case, normally routed to
    /// [`add_tag`](Self::add_tag) by the service) performs a pure insert with no prior
    /// removed — the primitive stays safe even if reached, never issuing an
    /// unscoped/over-broad DELETE.
    pub async fn replace_tag(
        &self,
        entry_id: u64,
        namespace: &str,
        new_tag: &str,
    ) -> Result<Option<String>> {
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        // Steps A + B run only for a real namespace. Colon-less / empty namespace
        // degrades to a pure insert (no prior evicted, no over-broad DELETE).
        let prior: Option<String> = if namespace.is_empty() {
            None
        } else {
            let like_pattern = format!("{}:%", like_escape(namespace));

            // Step A: read the evicted prior (audit prior_value) inside the SAME txn.
            let prior: Option<String> = sqlx::query_scalar(
                "SELECT tag FROM entry_tags
                 WHERE entry_id = ?1 AND tag LIKE ?2 ESCAPE '\\'
                 LIMIT 1",
            )
            .bind(entry_id as i64)
            .bind(&like_pattern)
            .fetch_optional(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

            // Step B: namespace-scoped DELETE (NOT delete-all) — same escaped pattern.
            sqlx::query("DELETE FROM entry_tags WHERE entry_id = ?1 AND tag LIKE ?2 ESCAPE '\\'")
                .bind(entry_id as i64)
                .bind(&like_pattern)
                .execute(&mut *txn)
                .await
                .map_err(|e| StoreError::Database(e.into()))?;

            prior
        };

        // Step C: INSERT the new tag. Shares the namespace, so B already removed any
        // pre-existing copy; the ON CONFLICT guard keeps it idempotent regardless.
        sqlx::query(
            "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)
             ON CONFLICT(entry_id, tag) DO NOTHING",
        )
        .bind(entry_id as i64)
        .bind(new_tag)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step D: ONE commit. If A/B/C errored, `?` returned early → txn dropped →
        // rollback: the prior value survives (R-02).
        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        Ok(prior)
    }
}

/// Escape SQL `LIKE` metacharacters (`%`, `_`) and the escape char (`\`) in a derived
/// namespace prefix so that, under `ESCAPE '\'`, the pattern matches only literal
/// `namespace:` rows and never over-matches siblings (vnc-045 R-08).
///
/// Order matters: the escape character itself is escaped first.
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
#[path = "write_tag_tests.rs"]
mod write_tag_tests;
