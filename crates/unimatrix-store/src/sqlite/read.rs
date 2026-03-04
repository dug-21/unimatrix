use std::collections::HashSet;

use rusqlite::OptionalExtension;

use crate::error::{Result, StoreError};
use crate::schema::{
    CoAccessRecord, EntryRecord, QueryFilter, Status, TimeRange, deserialize_co_access,
    deserialize_entry,
};

use super::db::Store;

/// Fetch full EntryRecords for a set of IDs from the entries table.
fn fetch_entries(conn: &rusqlite::Connection, ids: &HashSet<u64>) -> Result<Vec<EntryRecord>> {
    let mut results = Vec::with_capacity(ids.len());
    let mut stmt = conn
        .prepare("SELECT data FROM entries WHERE id = ?1")
        .map_err(StoreError::Sqlite)?;
    for &id in ids {
        if let Some(bytes) = stmt
            .query_row(rusqlite::params![id as i64], |row| {
                row.get::<_, Vec<u8>>(0)
            })
            .optional()
            .map_err(StoreError::Sqlite)?
        {
            results.push(deserialize_entry(&bytes)?);
        }
    }
    Ok(results)
}

/// Collect entry IDs by topic.
fn collect_ids_by_topic(conn: &rusqlite::Connection, topic: &str) -> Result<HashSet<u64>> {
    let mut stmt = conn
        .prepare("SELECT entry_id FROM topic_index WHERE topic = ?1")
        .map_err(StoreError::Sqlite)?;
    let rows = stmt
        .query_map(rusqlite::params![topic], |row| {
            Ok(row.get::<_, i64>(0)? as u64)
        })
        .map_err(StoreError::Sqlite)?;
    let mut ids = HashSet::new();
    for row in rows {
        ids.insert(row.map_err(StoreError::Sqlite)?);
    }
    Ok(ids)
}

/// Collect entry IDs by category.
fn collect_ids_by_category(conn: &rusqlite::Connection, category: &str) -> Result<HashSet<u64>> {
    let mut stmt = conn
        .prepare("SELECT entry_id FROM category_index WHERE category = ?1")
        .map_err(StoreError::Sqlite)?;
    let rows = stmt
        .query_map(rusqlite::params![category], |row| {
            Ok(row.get::<_, i64>(0)? as u64)
        })
        .map_err(StoreError::Sqlite)?;
    let mut ids = HashSet::new();
    for row in rows {
        ids.insert(row.map_err(StoreError::Sqlite)?);
    }
    Ok(ids)
}

/// Collect entry IDs matching ALL tags (intersection).
fn collect_ids_by_tags(conn: &rusqlite::Connection, tags: &[String]) -> Result<HashSet<u64>> {
    let mut result_set: Option<HashSet<u64>> = None;
    let mut stmt = conn
        .prepare("SELECT entry_id FROM tag_index WHERE tag = ?1")
        .map_err(StoreError::Sqlite)?;

    for tag in tags {
        let rows = stmt
            .query_map(rusqlite::params![tag], |row| {
                Ok(row.get::<_, i64>(0)? as u64)
            })
            .map_err(StoreError::Sqlite)?;
        let mut tag_ids = HashSet::new();
        for row in rows {
            tag_ids.insert(row.map_err(StoreError::Sqlite)?);
        }

        result_set = match result_set {
            None => Some(tag_ids),
            Some(existing) => Some(existing.intersection(&tag_ids).copied().collect()),
        };
    }

    Ok(result_set.unwrap_or_default())
}

/// Collect entry IDs within a time range.
fn collect_ids_by_time_range(
    conn: &rusqlite::Connection,
    range: TimeRange,
) -> Result<HashSet<u64>> {
    let mut stmt = conn
        .prepare("SELECT entry_id FROM time_index WHERE timestamp >= ?1 AND timestamp <= ?2")
        .map_err(StoreError::Sqlite)?;
    let rows = stmt
        .query_map(
            rusqlite::params![range.start as i64, range.end as i64],
            |row| Ok(row.get::<_, i64>(0)? as u64),
        )
        .map_err(StoreError::Sqlite)?;
    let mut ids = HashSet::new();
    for row in rows {
        ids.insert(row.map_err(StoreError::Sqlite)?);
    }
    Ok(ids)
}

/// Collect entry IDs with a given status.
fn collect_ids_by_status(conn: &rusqlite::Connection, status: Status) -> Result<HashSet<u64>> {
    let mut stmt = conn
        .prepare("SELECT entry_id FROM status_index WHERE status = ?1")
        .map_err(StoreError::Sqlite)?;
    let rows = stmt
        .query_map(rusqlite::params![status as u8 as i64], |row| {
            Ok(row.get::<_, i64>(0)? as u64)
        })
        .map_err(StoreError::Sqlite)?;
    let mut ids = HashSet::new();
    for row in rows {
        ids.insert(row.map_err(StoreError::Sqlite)?);
    }
    Ok(ids)
}

impl Store {
    /// Get a single entry by ID.
    pub fn get(&self, entry_id: u64) -> Result<EntryRecord> {
        let conn = self.lock_conn();
        let bytes: Vec<u8> = conn
            .query_row(
                "SELECT data FROM entries WHERE id = ?1",
                rusqlite::params![entry_id as i64],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::Sqlite)?
            .ok_or(StoreError::EntryNotFound(entry_id))?;
        deserialize_entry(&bytes)
    }

    /// Check if an entry exists without deserializing it.
    pub fn exists(&self, entry_id: u64) -> Result<bool> {
        let conn = self.lock_conn();
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM entries WHERE id = ?1 LIMIT 1",
                rusqlite::params![entry_id as i64],
                |_| Ok(true),
            )
            .optional()
            .map_err(StoreError::Sqlite)?
            .unwrap_or(false);
        Ok(exists)
    }

    /// Query entries by topic.
    pub fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>> {
        let conn = self.lock_conn();
        let ids = collect_ids_by_topic(&conn, topic)?;
        fetch_entries(&conn, &ids)
    }

    /// Query entries by category.
    pub fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>> {
        let conn = self.lock_conn();
        let ids = collect_ids_by_category(&conn, category)?;
        fetch_entries(&conn, &ids)
    }

    /// Query entries matching ALL specified tags (intersection).
    pub fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>> {
        if tags.is_empty() {
            return Ok(vec![]);
        }
        let conn = self.lock_conn();
        let ids = collect_ids_by_tags(&conn, tags)?;
        fetch_entries(&conn, &ids)
    }

    /// Query entries within a time range (inclusive on both ends).
    pub fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>> {
        if range.start > range.end {
            return Ok(vec![]);
        }
        let conn = self.lock_conn();
        let ids = collect_ids_by_time_range(&conn, range)?;
        fetch_entries(&conn, &ids)
    }

    /// Query entries with a given status.
    pub fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>> {
        let conn = self.lock_conn();
        let ids = collect_ids_by_status(&conn, status)?;
        fetch_entries(&conn, &ids)
    }

    /// Combined query with set intersection across all specified filters.
    pub fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>> {
        let conn = self.lock_conn();

        let is_empty = filter.topic.is_none()
            && filter.category.is_none()
            && filter.tags.is_none()
            && filter.status.is_none()
            && filter.time_range.is_none();

        let effective_status = if is_empty {
            Some(Status::Active)
        } else {
            filter.status
        };

        let mut sets: Vec<HashSet<u64>> = Vec::new();

        if let Some(ref topic) = filter.topic {
            sets.push(collect_ids_by_topic(&conn, topic)?);
        }
        if let Some(ref category) = filter.category {
            sets.push(collect_ids_by_category(&conn, category)?);
        }
        if let Some(ref tags) = filter.tags
            && !tags.is_empty()
        {
            sets.push(collect_ids_by_tags(&conn, tags)?);
        }
        if let Some(status) = effective_status {
            sets.push(collect_ids_by_status(&conn, status)?);
        }
        if let Some(range) = filter.time_range
            && range.start <= range.end
        {
            sets.push(collect_ids_by_time_range(&conn, range)?);
        }

        if sets.is_empty() {
            let ids = collect_ids_by_status(&conn, Status::Active)?;
            return fetch_entries(&conn, &ids);
        }

        let mut result_ids = sets.remove(0);
        for set in sets {
            result_ids = result_ids.intersection(&set).copied().collect();
        }

        fetch_entries(&conn, &result_ids)
    }

    /// Look up the hnsw_data_id for an entry in vector_map.
    pub fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>> {
        let conn = self.lock_conn();
        let val: Option<i64> = conn
            .query_row(
                "SELECT hnsw_data_id FROM vector_map WHERE entry_id = ?1",
                rusqlite::params![entry_id as i64],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::Sqlite)?;
        Ok(val.map(|v| v as u64))
    }

    /// Iterate all entries in the vector_map table.
    pub fn iter_vector_mappings(&self) -> Result<Vec<(u64, u64)>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare("SELECT entry_id, hnsw_data_id FROM vector_map ORDER BY entry_id")
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64))
            })
            .map_err(StoreError::Sqlite)?;
        let mut mappings = Vec::new();
        for row in rows {
            mappings.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(mappings)
    }

    /// Read a named counter value. Returns 0 if the counter does not exist.
    pub fn read_counter(&self, name: &str) -> Result<u64> {
        let conn = self.lock_conn();
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

    /// Get all co-access partners for an entry, filtering by staleness.
    pub fn get_co_access_partners(
        &self,
        entry_id: u64,
        staleness_cutoff: u64,
    ) -> Result<Vec<(u64, CoAccessRecord)>> {
        let conn = self.lock_conn();
        let mut partners = Vec::new();

        // Scan 1: pairs where entry_id is entry_id_a (indexed by PK)
        let mut stmt = conn
            .prepare("SELECT entry_id_b, data FROM co_access WHERE entry_id_a = ?1")
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![entry_id as i64], |row| {
                Ok((row.get::<_, i64>(0)? as u64, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(StoreError::Sqlite)?;
        for row in rows {
            let (partner_id, data) = row.map_err(StoreError::Sqlite)?;
            if partner_id == entry_id {
                continue;
            }
            let record = deserialize_co_access(&data)?;
            if record.last_updated >= staleness_cutoff {
                partners.push((partner_id, record));
            }
        }

        // Scan 2: pairs where entry_id is entry_id_b (uses idx_co_access_b)
        let mut stmt = conn
            .prepare("SELECT entry_id_a, data FROM co_access WHERE entry_id_b = ?1")
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![entry_id as i64], |row| {
                Ok((row.get::<_, i64>(0)? as u64, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(StoreError::Sqlite)?;
        for row in rows {
            let (partner_id, data) = row.map_err(StoreError::Sqlite)?;
            if partner_id == entry_id {
                continue;
            }
            let record = deserialize_co_access(&data)?;
            if record.last_updated >= staleness_cutoff {
                partners.push((partner_id, record));
            }
        }

        Ok(partners)
    }

    /// Get co-access statistics: (total_pairs, active_pairs).
    pub fn co_access_stats(&self, staleness_cutoff: u64) -> Result<(u64, u64)> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare("SELECT data FROM co_access")
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .map_err(StoreError::Sqlite)?;

        let mut total = 0u64;
        let mut active = 0u64;
        for row in rows {
            let data = row.map_err(StoreError::Sqlite)?;
            total += 1;
            let record = deserialize_co_access(&data)?;
            if record.last_updated >= staleness_cutoff {
                active += 1;
            }
        }

        Ok((total, active))
    }

    /// Get top N co-access pairs by count (non-stale only).
    pub fn top_co_access_pairs(
        &self,
        n: usize,
        staleness_cutoff: u64,
    ) -> Result<Vec<((u64, u64), CoAccessRecord)>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare("SELECT entry_id_a, entry_id_b, data FROM co_access")
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)? as u64,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(StoreError::Sqlite)?;

        let mut pairs = Vec::new();
        for row in rows {
            let (a, b, data) = row.map_err(StoreError::Sqlite)?;
            let record = deserialize_co_access(&data)?;
            if record.last_updated >= staleness_cutoff {
                pairs.push(((a, b), record));
            }
        }

        pairs.sort_by(|a, b| b.1.count.cmp(&a.1.count));
        pairs.truncate(n);
        Ok(pairs)
    }

    /// Retrieve stored observation metrics for a feature cycle.
    pub fn get_metrics(&self, feature_cycle: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.lock_conn();
        let val: Option<Vec<u8>> = conn
            .query_row(
                "SELECT data FROM observation_metrics WHERE feature_cycle = ?1",
                rusqlite::params![feature_cycle],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::Sqlite)?;
        Ok(val)
    }

    /// List all stored observation metrics.
    pub fn list_all_metrics(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare("SELECT feature_cycle, data FROM observation_metrics ORDER BY feature_cycle")
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }
}
