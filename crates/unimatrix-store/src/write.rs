use rusqlite::OptionalExtension;

use crate::error::{Result, StoreError};
use crate::schema::{EntryRecord, NewEntry, Status, status_counter_key};

use crate::db::Store;

/// Get the current unix timestamp in seconds.
fn current_unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Store {
    /// Insert a new entry. Returns the assigned entry_id.
    ///
    /// All columns and counters are updated atomically within a single
    /// transaction. If any step fails, the transaction is rolled back.
    pub fn insert(&self, entry: NewEntry) -> Result<u64> {
        let now = current_unix_timestamp_secs();
        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<u64> {
            // Step 1: Generate ID via counters module
            let id = crate::counters::next_entry_id(&conn)?;

            // Step 2: Compute content hash
            let content_hash = crate::hash::compute_content_hash(&entry.title, &entry.content);

            // Step 3: INSERT into entries with named params (ADR-004)
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
                    ":title": &entry.title,
                    ":content": &entry.content,
                    ":topic": &entry.topic,
                    ":category": &entry.category,
                    ":source": &entry.source,
                    ":status": entry.status as u8 as i64,
                    ":confidence": 0.0_f64,
                    ":created_at": now as i64,
                    ":updated_at": now as i64,
                    ":last_accessed_at": 0_i64,
                    ":access_count": 0_i64,
                    ":supersedes": Option::<i64>::None,
                    ":superseded_by": Option::<i64>::None,
                    ":correction_count": 0_i64,
                    ":embedding_dim": 0_i64,
                    ":created_by": &entry.created_by,
                    ":modified_by": "",
                    ":content_hash": &content_hash,
                    ":previous_hash": "",
                    ":version": 1_i64,
                    ":feature_cycle": &entry.feature_cycle,
                    ":trust_source": &entry.trust_source,
                    ":helpful_count": 0_i64,
                    ":unhelpful_count": 0_i64,
                },
            )
            .map_err(StoreError::Sqlite)?;

            // Step 4: Insert tags into entry_tags
            for tag in &entry.tags {
                conn.execute(
                    "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                    rusqlite::params![id as i64, tag],
                )
                .map_err(StoreError::Sqlite)?;
            }

            // Step 5: Update status counter
            crate::counters::increment_counter(&conn, status_counter_key(entry.status), 1)?;

            Ok(id)
        })();

        match result {
            Ok(id) => {
                conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?;
                Ok(id)
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Update an existing entry. Returns an error if the entry does not exist.
    ///
    /// Takes a full EntryRecord (matching the Store API). The entry.id field
    /// identifies which record to update.
    pub fn update(&self, entry: EntryRecord) -> Result<()> {
        let entry_id = entry.id;
        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<()> {
            // Read old status for counter adjustment (single column, no blob deserialize)
            let old_status: i64 = conn
                .query_row(
                    "SELECT status FROM entries WHERE id = ?1",
                    rusqlite::params![entry_id as i64],
                    |row| row.get(0),
                )
                .optional()
                .map_err(StoreError::Sqlite)?
                .ok_or(StoreError::EntryNotFound(entry_id))?;

            // UPDATE all 24 columns with named params (ADR-004)
            conn.execute(
                "UPDATE entries SET
                    title = :title, content = :content, topic = :topic,
                    category = :category, source = :source, status = :status,
                    confidence = :confidence, created_at = :created_at,
                    updated_at = :updated_at, last_accessed_at = :last_accessed_at,
                    access_count = :access_count, supersedes = :supersedes,
                    superseded_by = :superseded_by, correction_count = :correction_count,
                    embedding_dim = :embedding_dim, created_by = :created_by,
                    modified_by = :modified_by, content_hash = :content_hash,
                    previous_hash = :previous_hash, version = :version,
                    feature_cycle = :feature_cycle, trust_source = :trust_source,
                    helpful_count = :helpful_count, unhelpful_count = :unhelpful_count
                 WHERE id = :id",
                rusqlite::named_params! {
                    ":id": entry.id as i64,
                    ":title": &entry.title,
                    ":content": &entry.content,
                    ":topic": &entry.topic,
                    ":category": &entry.category,
                    ":source": &entry.source,
                    ":status": entry.status as u8 as i64,
                    ":confidence": entry.confidence,
                    ":created_at": entry.created_at as i64,
                    ":updated_at": entry.updated_at as i64,
                    ":last_accessed_at": entry.last_accessed_at as i64,
                    ":access_count": entry.access_count as i64,
                    ":supersedes": entry.supersedes.map(|v| v as i64),
                    ":superseded_by": entry.superseded_by.map(|v| v as i64),
                    ":correction_count": entry.correction_count as i64,
                    ":embedding_dim": entry.embedding_dim as i64,
                    ":created_by": &entry.created_by,
                    ":modified_by": &entry.modified_by,
                    ":content_hash": &entry.content_hash,
                    ":previous_hash": &entry.previous_hash,
                    ":version": entry.version as i64,
                    ":feature_cycle": &entry.feature_cycle,
                    ":trust_source": &entry.trust_source,
                    ":helpful_count": entry.helpful_count as i64,
                    ":unhelpful_count": entry.unhelpful_count as i64,
                },
            )
            .map_err(StoreError::Sqlite)?;

            // Replace tags: delete all, re-insert (ADR-006)
            conn.execute(
                "DELETE FROM entry_tags WHERE entry_id = ?1",
                rusqlite::params![entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            for tag in &entry.tags {
                conn.execute(
                    "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                    rusqlite::params![entry_id as i64, tag],
                )
                .map_err(StoreError::Sqlite)?;
            }

            // Status counter adjustment
            let new_status = entry.status as u8 as i64;
            if new_status != old_status {
                let old = Status::try_from(old_status as u8).unwrap_or(Status::Active);
                crate::counters::decrement_counter(&conn, status_counter_key(old), 1)?;
                crate::counters::increment_counter(&conn, status_counter_key(entry.status), 1)?;
            }

            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Update only the status of an entry.
    pub fn update_status(&self, entry_id: u64, new_status: Status) -> Result<()> {
        let now = current_unix_timestamp_secs();
        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<()> {
            // Read old status (single column, no blob deserialize)
            let old_status_val: i64 = conn
                .query_row(
                    "SELECT status FROM entries WHERE id = ?1",
                    rusqlite::params![entry_id as i64],
                    |row| row.get(0),
                )
                .optional()
                .map_err(StoreError::Sqlite)?
                .ok_or(StoreError::EntryNotFound(entry_id))?;

            // Update status and updated_at directly
            conn.execute(
                "UPDATE entries SET status = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![new_status as u8 as i64, now as i64, entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            // Counter adjustment
            let old_status = Status::try_from(old_status_val as u8).unwrap_or(Status::Active);
            crate::counters::decrement_counter(&conn, status_counter_key(old_status), 1)?;
            crate::counters::increment_counter(&conn, status_counter_key(new_status), 1)?;

            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Delete an entry and all its index references.
    pub fn delete(&self, entry_id: u64) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<()> {
            // Read status for counter adjustment (single column)
            let old_status_val: i64 = conn
                .query_row(
                    "SELECT status FROM entries WHERE id = ?1",
                    rusqlite::params![entry_id as i64],
                    |row| row.get(0),
                )
                .optional()
                .map_err(StoreError::Sqlite)?
                .ok_or(StoreError::EntryNotFound(entry_id))?;

            // Delete from entries (CASCADE deletes entry_tags automatically)
            conn.execute(
                "DELETE FROM entries WHERE id = ?1",
                rusqlite::params![entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            // Delete from vector_map (no FK, manual)
            conn.execute(
                "DELETE FROM vector_map WHERE entry_id = ?1",
                rusqlite::params![entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            // Decrement status counter
            let old_status = Status::try_from(old_status_val as u8).unwrap_or(Status::Active);
            crate::counters::decrement_counter(&conn, status_counter_key(old_status), 1)?;

            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }
}
