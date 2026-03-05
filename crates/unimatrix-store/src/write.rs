use std::collections::HashSet;

use rusqlite::OptionalExtension;

use crate::error::{Result, StoreError};
use crate::schema::{
    EntryRecord, NewEntry, Status, deserialize_entry,
    serialize_entry, status_counter_key,
};

use crate::db::Store;

/// Get the current unix timestamp in seconds.
fn current_unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Read a counter value within a connection (not locked by caller).
fn read_counter(conn: &rusqlite::Connection, name: &str) -> Result<u64> {
    let val: Option<i64> = conn
        .query_row(
            "SELECT value FROM counters WHERE name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        )
        .optional()
        .map_err(StoreError::Sqlite)?;
    Ok(val.unwrap_or(0) as u64)
}

/// Set a counter value within a connection.
fn set_counter(conn: &rusqlite::Connection, name: &str, value: u64) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO counters (name, value) VALUES (?1, ?2)",
        rusqlite::params![name, value as i64],
    )
    .map_err(StoreError::Sqlite)?;
    Ok(())
}

/// Increment a counter by delta.
fn increment_counter(conn: &rusqlite::Connection, name: &str, delta: u64) -> Result<()> {
    let current = read_counter(conn, name)?;
    set_counter(conn, name, current + delta)
}

/// Decrement a counter by delta (saturating).
fn decrement_counter(conn: &rusqlite::Connection, name: &str, delta: u64) -> Result<()> {
    let current = read_counter(conn, name)?;
    set_counter(conn, name, current.saturating_sub(delta))
}

impl Store {
    /// Insert a new entry. Returns the assigned entry_id.
    ///
    /// All index tables and counters are updated atomically within a single
    /// transaction. If any step fails, the transaction is rolled back.
    pub fn insert(&self, entry: NewEntry) -> Result<u64> {
        let now = current_unix_timestamp_secs();
        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<u64> {
            // Step 1: Generate ID
            let id = read_counter(&conn, "next_entry_id")?;
            set_counter(&conn, "next_entry_id", id + 1)?;

            // Step 2: Compute content hash
            let content_hash = crate::hash::compute_content_hash(&entry.title, &entry.content);

            // Step 3: Build EntryRecord
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
                embedding_dim: 0,
                created_by: entry.created_by,
                modified_by: String::new(),
                content_hash,
                previous_hash: String::new(),
                version: 1,
                feature_cycle: entry.feature_cycle,
                trust_source: entry.trust_source,
                helpful_count: 0,
                unhelpful_count: 0,
            };

            // Step 4: Serialize and insert
            let bytes = serialize_entry(&record)?;
            conn.execute(
                "INSERT INTO entries (id, data) VALUES (?1, ?2)",
                rusqlite::params![id as i64, bytes],
            )
            .map_err(StoreError::Sqlite)?;

            // Step 5: Insert into all index tables
            conn.execute(
                "INSERT INTO topic_index (topic, entry_id) VALUES (?1, ?2)",
                rusqlite::params![&record.topic, id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            conn.execute(
                "INSERT INTO category_index (category, entry_id) VALUES (?1, ?2)",
                rusqlite::params![&record.category, id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            for tag in &record.tags {
                conn.execute(
                    "INSERT INTO tag_index (tag, entry_id) VALUES (?1, ?2)",
                    rusqlite::params![tag, id as i64],
                )
                .map_err(StoreError::Sqlite)?;
            }

            conn.execute(
                "INSERT INTO time_index (timestamp, entry_id) VALUES (?1, ?2)",
                rusqlite::params![record.created_at as i64, id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            conn.execute(
                "INSERT INTO status_index (status, entry_id) VALUES (?1, ?2)",
                rusqlite::params![record.status as u8 as i64, id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            // Step 6: Update status counter
            increment_counter(&conn, status_counter_key(record.status), 1)?;

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
            // Read existing entry for index diffing
            let old_bytes: Vec<u8> = conn
                .query_row(
                    "SELECT data FROM entries WHERE id = ?1",
                    rusqlite::params![entry_id as i64],
                    |row| row.get(0),
                )
                .optional()
                .map_err(StoreError::Sqlite)?
                .ok_or(StoreError::EntryNotFound(entry_id))?;
            let old = deserialize_entry(&old_bytes)?;

            // Use the provided entry as the updated record
            let updated = entry;

            let bytes = serialize_entry(&updated)?;
            conn.execute(
                "UPDATE entries SET data = ?1 WHERE id = ?2",
                rusqlite::params![bytes, entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            // Update indexes: topic
            if updated.topic != old.topic {
                conn.execute(
                    "DELETE FROM topic_index WHERE topic = ?1 AND entry_id = ?2",
                    rusqlite::params![&old.topic, entry_id as i64],
                )
                .map_err(StoreError::Sqlite)?;
                conn.execute(
                    "INSERT INTO topic_index (topic, entry_id) VALUES (?1, ?2)",
                    rusqlite::params![&updated.topic, entry_id as i64],
                )
                .map_err(StoreError::Sqlite)?;
            }

            // Category
            if updated.category != old.category {
                conn.execute(
                    "DELETE FROM category_index WHERE category = ?1 AND entry_id = ?2",
                    rusqlite::params![&old.category, entry_id as i64],
                )
                .map_err(StoreError::Sqlite)?;
                conn.execute(
                    "INSERT INTO category_index (category, entry_id) VALUES (?1, ?2)",
                    rusqlite::params![&updated.category, entry_id as i64],
                )
                .map_err(StoreError::Sqlite)?;
            }

            // Tags: diff old vs new
            let old_tags: HashSet<&String> = old.tags.iter().collect();
            let new_tags: HashSet<&String> = updated.tags.iter().collect();
            for removed in old_tags.difference(&new_tags) {
                conn.execute(
                    "DELETE FROM tag_index WHERE tag = ?1 AND entry_id = ?2",
                    rusqlite::params![removed.as_str(), entry_id as i64],
                )
                .map_err(StoreError::Sqlite)?;
            }
            for added in new_tags.difference(&old_tags) {
                conn.execute(
                    "INSERT INTO tag_index (tag, entry_id) VALUES (?1, ?2)",
                    rusqlite::params![added.as_str(), entry_id as i64],
                )
                .map_err(StoreError::Sqlite)?;
            }

            // Time index: remove old, insert new (updated_at changed)
            conn.execute(
                "DELETE FROM time_index WHERE timestamp = ?1 AND entry_id = ?2",
                rusqlite::params![old.created_at as i64, entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;
            conn.execute(
                "INSERT OR REPLACE INTO time_index (timestamp, entry_id) VALUES (?1, ?2)",
                rusqlite::params![updated.updated_at as i64, entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            // Status
            if updated.status != old.status {
                conn.execute(
                    "DELETE FROM status_index WHERE status = ?1 AND entry_id = ?2",
                    rusqlite::params![old.status as u8 as i64, entry_id as i64],
                )
                .map_err(StoreError::Sqlite)?;
                conn.execute(
                    "INSERT INTO status_index (status, entry_id) VALUES (?1, ?2)",
                    rusqlite::params![updated.status as u8 as i64, entry_id as i64],
                )
                .map_err(StoreError::Sqlite)?;
                decrement_counter(&conn, status_counter_key(old.status), 1)?;
                increment_counter(&conn, status_counter_key(updated.status), 1)?;
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
            let old_bytes: Vec<u8> = conn
                .query_row(
                    "SELECT data FROM entries WHERE id = ?1",
                    rusqlite::params![entry_id as i64],
                    |row| row.get(0),
                )
                .optional()
                .map_err(StoreError::Sqlite)?
                .ok_or(StoreError::EntryNotFound(entry_id))?;
            let mut record = deserialize_entry(&old_bytes)?;
            let old_status = record.status;

            record.status = new_status;
            record.updated_at = now;

            let bytes = serialize_entry(&record)?;
            conn.execute(
                "UPDATE entries SET data = ?1 WHERE id = ?2",
                rusqlite::params![bytes, entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            // Update status index
            conn.execute(
                "DELETE FROM status_index WHERE status = ?1 AND entry_id = ?2",
                rusqlite::params![old_status as u8 as i64, entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;
            conn.execute(
                "INSERT INTO status_index (status, entry_id) VALUES (?1, ?2)",
                rusqlite::params![new_status as u8 as i64, entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            // Update counters
            decrement_counter(&conn, status_counter_key(old_status), 1)?;
            increment_counter(&conn, status_counter_key(new_status), 1)?;

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
            let old_bytes: Vec<u8> = conn
                .query_row(
                    "SELECT data FROM entries WHERE id = ?1",
                    rusqlite::params![entry_id as i64],
                    |row| row.get(0),
                )
                .optional()
                .map_err(StoreError::Sqlite)?
                .ok_or(StoreError::EntryNotFound(entry_id))?;
            let record = deserialize_entry(&old_bytes)?;

            // Delete from entries
            conn.execute(
                "DELETE FROM entries WHERE id = ?1",
                rusqlite::params![entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            // Delete from all indexes
            conn.execute(
                "DELETE FROM topic_index WHERE topic = ?1 AND entry_id = ?2",
                rusqlite::params![&record.topic, entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;
            conn.execute(
                "DELETE FROM category_index WHERE category = ?1 AND entry_id = ?2",
                rusqlite::params![&record.category, entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;
            for tag in &record.tags {
                conn.execute(
                    "DELETE FROM tag_index WHERE tag = ?1 AND entry_id = ?2",
                    rusqlite::params![tag, entry_id as i64],
                )
                .map_err(StoreError::Sqlite)?;
            }
            conn.execute(
                "DELETE FROM time_index WHERE timestamp = ?1 AND entry_id = ?2",
                rusqlite::params![record.created_at as i64, entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;
            conn.execute(
                "DELETE FROM status_index WHERE status = ?1 AND entry_id = ?2",
                rusqlite::params![record.status as u8 as i64, entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;
            conn.execute(
                "DELETE FROM vector_map WHERE entry_id = ?1",
                rusqlite::params![entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            // Decrement status counter
            decrement_counter(&conn, status_counter_key(record.status), 1)?;

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
