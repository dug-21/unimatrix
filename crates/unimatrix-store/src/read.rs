use std::collections::HashMap;

use rusqlite::OptionalExtension;

use crate::error::{Result, StoreError};
use crate::schema::{CoAccessRecord, EntryRecord, QueryFilter, Status, TimeRange};

use crate::db::Store;

/// All SELECT columns for the entries table, in DDL order.
/// Used by every query that constructs EntryRecord.
pub const ENTRY_COLUMNS: &str =
    "id, title, content, topic, category, source, status, confidence, \
     created_at, updated_at, last_accessed_at, access_count, \
     supersedes, superseded_by, correction_count, embedding_dim, \
     created_by, modified_by, content_hash, previous_hash, \
     version, feature_cycle, trust_source, helpful_count, unhelpful_count, \
     pre_quarantine_status";

/// Construct EntryRecord from a SQLite row using column-by-name access.
/// Tags are set to vec![] -- caller MUST use load_tags_for_entries() (ADR-006, C-10).
pub fn entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntryRecord> {
    Ok(EntryRecord {
        id: row.get::<_, i64>("id")? as u64,
        title: row.get("title")?,
        content: row.get("content")?,
        topic: row.get("topic")?,
        category: row.get("category")?,
        tags: vec![], // populated by load_tags_for_entries
        source: row.get("source")?,
        status: Status::try_from(row.get::<_, i64>("status")? as u8)
            .unwrap_or(Status::Active),
        confidence: row.get("confidence")?,
        created_at: row.get::<_, i64>("created_at")? as u64,
        updated_at: row.get::<_, i64>("updated_at")? as u64,
        last_accessed_at: row.get::<_, i64>("last_accessed_at")? as u64,
        access_count: row.get::<_, i64>("access_count")? as u32,
        supersedes: row.get::<_, Option<i64>>("supersedes")?.map(|v| v as u64),
        superseded_by: row.get::<_, Option<i64>>("superseded_by")?.map(|v| v as u64),
        correction_count: row.get::<_, i64>("correction_count")? as u32,
        embedding_dim: row.get::<_, i64>("embedding_dim")? as u16,
        created_by: row.get("created_by")?,
        modified_by: row.get("modified_by")?,
        content_hash: row.get("content_hash")?,
        previous_hash: row.get("previous_hash")?,
        version: row.get::<_, i64>("version")? as u32,
        feature_cycle: row.get("feature_cycle")?,
        trust_source: row.get("trust_source")?,
        helpful_count: row.get::<_, i64>("helpful_count")? as u32,
        unhelpful_count: row.get::<_, i64>("unhelpful_count")? as u32,
        pre_quarantine_status: row.get::<_, Option<i64>>("pre_quarantine_status")?
            .map(|v| v as u8),
    })
}

/// Batch-load tags for multiple entries. Returns map of entry_id -> Vec<tag>.
/// Every code path constructing EntryRecord MUST call this (ADR-006, C-10).
pub fn load_tags_for_entries(
    conn: &rusqlite::Connection,
    ids: &[u64],
) -> Result<HashMap<u64, Vec<String>>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
    let sql = format!(
        "SELECT entry_id, tag FROM entry_tags WHERE entry_id IN ({}) ORDER BY entry_id, tag",
        placeholders.join(",")
    );

    let mut stmt = conn.prepare(&sql).map_err(StoreError::Sqlite)?;
    let params: Vec<Box<dyn rusqlite::types::ToSql>> = ids
        .iter()
        .map(|&id| Box::new(id as i64) as Box<dyn rusqlite::types::ToSql>)
        .collect();

    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((row.get::<_, i64>(0)? as u64, row.get::<_, String>(1)?))
        })
        .map_err(StoreError::Sqlite)?;

    let mut map: HashMap<u64, Vec<String>> = HashMap::new();
    for row in rows {
        let (entry_id, tag) = row.map_err(StoreError::Sqlite)?;
        map.entry(entry_id).or_default().push(tag);
    }

    Ok(map)
}

/// Apply tags from the tag map to a Vec of EntryRecords.
pub fn apply_tags(entries: &mut [EntryRecord], tag_map: &HashMap<u64, Vec<String>>) {
    for entry in entries.iter_mut() {
        if let Some(tags) = tag_map.get(&entry.id) {
            entry.tags = tags.clone();
        }
    }
}

impl Store {
    /// Get a single entry by ID.
    pub fn get(&self, entry_id: u64) -> Result<EntryRecord> {
        let conn = self.lock_conn();
        let mut entry: EntryRecord = conn
            .query_row(
                &format!("SELECT {} FROM entries WHERE id = ?1", ENTRY_COLUMNS),
                rusqlite::params![entry_id as i64],
                entry_from_row,
            )
            .optional()
            .map_err(StoreError::Sqlite)?
            .ok_or(StoreError::EntryNotFound(entry_id))?;

        let tag_map = load_tags_for_entries(&conn, &[entry_id])?;
        if let Some(tags) = tag_map.get(&entry_id) {
            entry.tags = tags.clone();
        }
        Ok(entry)
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
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {} FROM entries WHERE topic = ?1",
                ENTRY_COLUMNS
            ))
            .map_err(StoreError::Sqlite)?;

        let mut entries: Vec<EntryRecord> = stmt
            .query_map(rusqlite::params![topic], entry_from_row)
            .map_err(StoreError::Sqlite)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(StoreError::Sqlite)?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(&conn, &ids)?;
        apply_tags(&mut entries, &tag_map);

        Ok(entries)
    }

    /// Query entries by category.
    pub fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {} FROM entries WHERE category = ?1",
                ENTRY_COLUMNS
            ))
            .map_err(StoreError::Sqlite)?;

        let mut entries: Vec<EntryRecord> = stmt
            .query_map(rusqlite::params![category], entry_from_row)
            .map_err(StoreError::Sqlite)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(StoreError::Sqlite)?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(&conn, &ids)?;
        apply_tags(&mut entries, &tag_map);

        Ok(entries)
    }

    /// Query entries matching ALL specified tags (intersection).
    pub fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>> {
        if tags.is_empty() {
            return Ok(vec![]);
        }
        let conn = self.lock_conn();

        // Build tag subquery: AND semantics via GROUP BY HAVING
        let placeholders: Vec<String> = tags.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT {} FROM entries WHERE id IN (\
                SELECT entry_id FROM entry_tags \
                WHERE tag IN ({}) \
                GROUP BY entry_id \
                HAVING COUNT(DISTINCT tag) = ?\
            )",
            ENTRY_COLUMNS,
            placeholders.join(",")
        );

        let mut stmt = conn.prepare(&sql).map_err(StoreError::Sqlite)?;

        // Build params: tag values + tag count
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = tags
            .iter()
            .map(|t| Box::new(t.clone()) as Box<dyn rusqlite::types::ToSql>)
            .collect();
        params.push(Box::new(tags.len() as i64));

        let mut entries: Vec<EntryRecord> = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), entry_from_row)
            .map_err(StoreError::Sqlite)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(StoreError::Sqlite)?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(&conn, &ids)?;
        apply_tags(&mut entries, &tag_map);

        Ok(entries)
    }

    /// Query entries within a time range (inclusive on both ends).
    pub fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>> {
        if range.start > range.end {
            return Ok(vec![]);
        }
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {} FROM entries WHERE created_at BETWEEN ?1 AND ?2",
                ENTRY_COLUMNS
            ))
            .map_err(StoreError::Sqlite)?;

        let mut entries: Vec<EntryRecord> = stmt
            .query_map(
                rusqlite::params![range.start as i64, range.end as i64],
                entry_from_row,
            )
            .map_err(StoreError::Sqlite)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(StoreError::Sqlite)?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(&conn, &ids)?;
        apply_tags(&mut entries, &tag_map);

        Ok(entries)
    }

    /// Query entries with a given status.
    pub fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {} FROM entries WHERE status = ?1",
                ENTRY_COLUMNS
            ))
            .map_err(StoreError::Sqlite)?;

        let mut entries: Vec<EntryRecord> = stmt
            .query_map(
                rusqlite::params![status as u8 as i64],
                entry_from_row,
            )
            .map_err(StoreError::Sqlite)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(StoreError::Sqlite)?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(&conn, &ids)?;
        apply_tags(&mut entries, &tag_map);

        Ok(entries)
    }

    /// Combined query with SQL WHERE clause across all specified filters.
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

        // Build dynamic WHERE clause
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1usize;

        if let Some(ref topic) = filter.topic {
            conditions.push(format!("topic = ?{param_idx}"));
            params.push(Box::new(topic.clone()));
            param_idx += 1;
        }
        if let Some(ref category) = filter.category {
            conditions.push(format!("category = ?{param_idx}"));
            params.push(Box::new(category.clone()));
            param_idx += 1;
        }
        if let Some(status) = effective_status {
            conditions.push(format!("status = ?{param_idx}"));
            params.push(Box::new(status as u8 as i64));
            param_idx += 1;
        }
        if let Some(range) = filter.time_range
            && range.start <= range.end
        {
            conditions.push(format!(
                "created_at >= ?{} AND created_at <= ?{}",
                param_idx,
                param_idx + 1
            ));
            params.push(Box::new(range.start as i64));
            params.push(Box::new(range.end as i64));
            param_idx += 2;
        }

        // Tag subquery (only if tags is Some and non-empty)
        if let Some(ref tags) = filter.tags
            && !tags.is_empty()
        {
            let tag_placeholders: Vec<String> = tags
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", param_idx + i))
                .collect();
            conditions.push(format!(
                "id IN (SELECT entry_id FROM entry_tags WHERE tag IN ({}) \
                 GROUP BY entry_id HAVING COUNT(DISTINCT tag) = ?{})",
                tag_placeholders.join(","),
                param_idx + tags.len()
            ));
            for tag in tags {
                params.push(Box::new(tag.clone()));
            }
            params.push(Box::new(tags.len() as i64));
            param_idx += tags.len() + 1;
        }

        // Suppress unused variable warning
        let _ = param_idx;

        // If no conditions at all (shouldn't happen due to effective_status), default to Active
        let where_clause = if conditions.is_empty() {
            "WHERE status = 0".to_string()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!("SELECT {} FROM entries {}", ENTRY_COLUMNS, where_clause);
        let mut stmt = conn.prepare(&sql).map_err(StoreError::Sqlite)?;

        let mut entries: Vec<EntryRecord> = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), entry_from_row)
            .map_err(StoreError::Sqlite)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(StoreError::Sqlite)?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(&conn, &ids)?;
        apply_tags(&mut entries, &tag_map);

        Ok(entries)
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
        crate::counters::read_counter(&conn, name)
    }

    /// Get all co-access partners for an entry, filtering by staleness.
    pub fn get_co_access_partners(
        &self,
        entry_id: u64,
        staleness_cutoff: u64,
    ) -> Result<Vec<(u64, CoAccessRecord)>> {
        let conn = self.lock_conn();
        let mut partners = Vec::new();

        // Scan 1: pairs where entry_id is entry_id_a
        let mut stmt = conn
            .prepare(
                "SELECT entry_id_b, count, last_updated FROM co_access \
                 WHERE entry_id_a = ?1 AND last_updated >= ?2",
            )
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(
                rusqlite::params![entry_id as i64, staleness_cutoff as i64],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)? as u64,
                        CoAccessRecord {
                            count: row.get::<_, i64>(1)? as u32,
                            last_updated: row.get::<_, i64>(2)? as u64,
                        },
                    ))
                },
            )
            .map_err(StoreError::Sqlite)?;
        for row in rows {
            let (partner_id, record) = row.map_err(StoreError::Sqlite)?;
            if partner_id != entry_id {
                partners.push((partner_id, record));
            }
        }

        // Scan 2: pairs where entry_id is entry_id_b
        let mut stmt = conn
            .prepare(
                "SELECT entry_id_a, count, last_updated FROM co_access \
                 WHERE entry_id_b = ?1 AND last_updated >= ?2",
            )
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(
                rusqlite::params![entry_id as i64, staleness_cutoff as i64],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)? as u64,
                        CoAccessRecord {
                            count: row.get::<_, i64>(1)? as u32,
                            last_updated: row.get::<_, i64>(2)? as u64,
                        },
                    ))
                },
            )
            .map_err(StoreError::Sqlite)?;
        for row in rows {
            let (partner_id, record) = row.map_err(StoreError::Sqlite)?;
            if partner_id != entry_id {
                partners.push((partner_id, record));
            }
        }

        Ok(partners)
    }

    /// Get co-access statistics: (total_pairs, active_pairs).
    pub fn co_access_stats(&self, staleness_cutoff: u64) -> Result<(u64, u64)> {
        let conn = self.lock_conn();
        let (total, active): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), \
                 SUM(CASE WHEN last_updated >= ?1 THEN 1 ELSE 0 END) \
                 FROM co_access",
                rusqlite::params![staleness_cutoff as i64],
                |row| Ok((row.get(0)?, row.get::<_, Option<i64>>(1)?.unwrap_or(0))),
            )
            .map_err(StoreError::Sqlite)?;
        Ok((total as u64, active as u64))
    }

    /// Get top N co-access pairs by count (non-stale only).
    pub fn top_co_access_pairs(
        &self,
        n: usize,
        staleness_cutoff: u64,
    ) -> Result<Vec<((u64, u64), CoAccessRecord)>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare(
                "SELECT entry_id_a, entry_id_b, count, last_updated FROM co_access \
                 WHERE last_updated >= ?1 \
                 ORDER BY count DESC \
                 LIMIT ?2",
            )
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(
                rusqlite::params![staleness_cutoff as i64, n as i64],
                |row| {
                    Ok((
                        (
                            row.get::<_, i64>(0)? as u64,
                            row.get::<_, i64>(1)? as u64,
                        ),
                        CoAccessRecord {
                            count: row.get::<_, i64>(2)? as u32,
                            last_updated: row.get::<_, i64>(3)? as u64,
                        },
                    ))
                },
            )
            .map_err(StoreError::Sqlite)?;

        let mut pairs = Vec::new();
        for row in rows {
            pairs.push(row.map_err(StoreError::Sqlite)?);
        }
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
