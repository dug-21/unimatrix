//! Extended write operations for the SQLite backend.
//!
//! Usage tracking, confidence updates, vector mappings, feature entries,
//! co-access pairs, and observation metrics.

use std::collections::HashSet;

use rusqlite::OptionalExtension;

use crate::error::{Result, StoreError};
use crate::read::{entry_from_row, load_tags_for_entries, ENTRY_COLUMNS};
use crate::schema::EntryRecord;

use crate::db::Store;

/// Get the current unix timestamp in seconds.
fn current_unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Store {
    /// Record usage for a batch of entries in a single write transaction.
    ///
    /// Delegates to `record_usage_with_confidence` with no confidence function.
    /// API-compatible with the original Store interface.
    pub fn record_usage(
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
            None,
        )
    }

    /// Record usage for a batch of entries with optional inline confidence computation.
    ///
    /// For each entry_id in `all_ids`, updates `last_accessed_at` to `now`.
    /// For each entry_id in `access_ids`, increments `access_count`.
    /// For each entry_id in `helpful_ids`, increments `helpful_count`.
    /// For each entry_id in `unhelpful_ids`, increments `unhelpful_count`.
    /// For each entry_id in `decrement_helpful_ids`, decrements `helpful_count` (saturating).
    /// For each entry_id in `decrement_unhelpful_ids`, decrements `unhelpful_count` (saturating).
    ///
    /// If `confidence_fn` is `Some`, recomputes confidence for each entry
    /// after applying counter updates.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub fn record_usage_with_confidence(
        &self,
        all_ids: &[u64],
        access_ids: &[u64],
        helpful_ids: &[u64],
        unhelpful_ids: &[u64],
        decrement_helpful_ids: &[u64],
        decrement_unhelpful_ids: &[u64],
        confidence_fn: Option<&dyn Fn(&EntryRecord, u64) -> f64>,
    ) -> Result<()> {
        if all_ids.is_empty() {
            return Ok(());
        }

        let now = current_unix_timestamp_secs();
        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let access_set: HashSet<u64> = access_ids.iter().copied().collect();
        let helpful_set: HashSet<u64> = helpful_ids.iter().copied().collect();
        let unhelpful_set: HashSet<u64> = unhelpful_ids.iter().copied().collect();
        let dec_helpful_set: HashSet<u64> = decrement_helpful_ids.iter().copied().collect();
        let dec_unhelpful_set: HashSet<u64> = decrement_unhelpful_ids.iter().copied().collect();

        let result = (|| -> Result<()> {
            for &id in all_ids {
                // Check entry exists
                let exists: bool = conn
                    .query_row(
                        "SELECT 1 FROM entries WHERE id = ?1",
                        rusqlite::params![id as i64],
                        |_| Ok(true),
                    )
                    .optional()
                    .map_err(StoreError::Sqlite)?
                    .unwrap_or(false);

                if !exists {
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
                conn.execute(&sql, rusqlite::params![id as i64])
                    .map_err(StoreError::Sqlite)?;

                // If confidence_fn provided, read back the record and recompute
                if let Some(f) = &confidence_fn {
                    let mut record: EntryRecord = conn
                        .query_row(
                            &format!("SELECT {} FROM entries WHERE id = ?1", ENTRY_COLUMNS),
                            rusqlite::params![id as i64],
                            entry_from_row,
                        )
                        .optional()
                        .map_err(StoreError::Sqlite)?
                        .ok_or(StoreError::EntryNotFound(id))?;

                    // Load tags for the confidence function
                    let tag_map = load_tags_for_entries(&conn, &[id])?;
                    if let Some(tags) = tag_map.get(&id) {
                        record.tags = tags.clone();
                    }

                    let new_confidence = f(&record, now);
                    conn.execute(
                        "UPDATE entries SET confidence = ?1 WHERE id = ?2",
                        rusqlite::params![new_confidence, id as i64],
                    )
                    .map_err(StoreError::Sqlite)?;
                }
            }
            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")
                    .map_err(StoreError::Sqlite)?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Update the confidence score for an entry.
    pub fn update_confidence(&self, entry_id: u64, confidence: f64) -> Result<()> {
        let conn = self.lock_conn();
        let affected = conn
            .execute(
                "UPDATE entries SET confidence = ?1 WHERE id = ?2",
                rusqlite::params![confidence, entry_id as i64],
            )
            .map_err(StoreError::Sqlite)?;
        if affected == 0 {
            return Err(StoreError::EntryNotFound(entry_id));
        }
        Ok(())
    }

    /// Insert or update a vector mapping.
    pub fn put_vector_mapping(&self, entry_id: u64, hnsw_data_id: u64) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT OR REPLACE INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)",
            rusqlite::params![entry_id as i64, hnsw_data_id as i64],
        )
        .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    /// Rewrite the entire vector_map table.
    pub fn rewrite_vector_map(&self, mappings: &[(u64, u64)]) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<()> {
            conn.execute("DELETE FROM vector_map", [])
                .map_err(StoreError::Sqlite)?;
            for &(entry_id, data_id) in mappings {
                conn.execute(
                    "INSERT INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)",
                    rusqlite::params![entry_id as i64, data_id as i64],
                )
                .map_err(StoreError::Sqlite)?;
            }
            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")
                    .map_err(StoreError::Sqlite)?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Record feature-entry associations.
    pub fn record_feature_entries(&self, feature_cycle: &str, entry_ids: &[u64]) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<()> {
            for &entry_id in entry_ids {
                conn.execute(
                    "INSERT OR IGNORE INTO feature_entries (feature_id, entry_id) VALUES (?1, ?2)",
                    rusqlite::params![feature_cycle, entry_id as i64],
                )
                .map_err(StoreError::Sqlite)?;
            }
            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")
                    .map_err(StoreError::Sqlite)?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Record co-access pairs using SQL columns (no blob serialization).
    pub fn record_co_access_pairs(&self, pairs: &[(u64, u64)]) -> Result<()> {
        let now = current_unix_timestamp_secs();
        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<()> {
            for &(a, b) in pairs {
                let (min_id, max_id) = crate::schema::co_access_key(a, b);
                if min_id == max_id {
                    continue; // Skip self-pairs
                }

                let existing: Option<i64> = conn
                    .query_row(
                        "SELECT count FROM co_access WHERE entry_id_a = ?1 AND entry_id_b = ?2",
                        rusqlite::params![min_id as i64, max_id as i64],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(StoreError::Sqlite)?;

                match existing {
                    Some(count) => {
                        conn.execute(
                            "UPDATE co_access SET count = ?1, last_updated = ?2 \
                             WHERE entry_id_a = ?3 AND entry_id_b = ?4",
                            rusqlite::params![
                                count + 1,
                                now as i64,
                                min_id as i64,
                                max_id as i64
                            ],
                        )
                        .map_err(StoreError::Sqlite)?;
                    }
                    None => {
                        conn.execute(
                            "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) \
                             VALUES (?1, ?2, 1, ?3)",
                            rusqlite::params![min_id as i64, max_id as i64, now as i64],
                        )
                        .map_err(StoreError::Sqlite)?;
                    }
                }
            }
            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")
                    .map_err(StoreError::Sqlite)?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Remove stale co-access pairs. Returns the number deleted.
    pub fn cleanup_stale_co_access(&self, staleness_cutoff: u64) -> Result<u64> {
        let conn = self.lock_conn();
        let deleted = conn
            .execute(
                "DELETE FROM co_access WHERE last_updated < ?1",
                rusqlite::params![staleness_cutoff as i64],
            )
            .map_err(StoreError::Sqlite)?;
        Ok(deleted as u64)
    }

    /// Store observation metrics for a feature cycle.
    pub fn store_metrics(&self, feature_cycle: &str, data: &[u8]) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT OR REPLACE INTO observation_metrics (feature_cycle, data) VALUES (?1, ?2)",
            rusqlite::params![feature_cycle, data],
        )
        .map_err(StoreError::Sqlite)?;
        Ok(())
    }
}
