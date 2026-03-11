use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::OptionalExtension;

use crate::error::{Result, StoreError};
use crate::schema::{CoAccessRecord, EntryRecord, QueryFilter, Status, TimeRange};

use crate::db::Store;

/// All SELECT columns for the entries table, in DDL order.
/// Used by every query that constructs EntryRecord.
pub const ENTRY_COLUMNS: &str = "id, title, content, topic, category, source, status, confidence, \
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
        status: Status::try_from(row.get::<_, i64>("status")? as u8).unwrap_or(Status::Active),
        confidence: row.get("confidence")?,
        created_at: row.get::<_, i64>("created_at")? as u64,
        updated_at: row.get::<_, i64>("updated_at")? as u64,
        last_accessed_at: row.get::<_, i64>("last_accessed_at")? as u64,
        access_count: row.get::<_, i64>("access_count")? as u32,
        supersedes: row.get::<_, Option<i64>>("supersedes")?.map(|v| v as u64),
        superseded_by: row
            .get::<_, Option<i64>>("superseded_by")?
            .map(|v| v as u64),
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
        pre_quarantine_status: row
            .get::<_, Option<i64>>("pre_quarantine_status")?
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
            .query_map(rusqlite::params![status as u8 as i64], entry_from_row)
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
                        (row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64),
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

    /// Retrieve stored observation metrics for a feature cycle (nxs-009: typed API).
    pub fn get_metrics(&self, feature_cycle: &str) -> Result<Option<crate::metrics::MetricVector>> {
        let conn = self.lock_conn();

        // Query parent row
        let parent = conn
            .query_row(
                "SELECT computed_at,
                    total_tool_calls, total_duration_secs, session_count,
                    search_miss_rate, edit_bloat_total_kb, edit_bloat_ratio,
                    permission_friction_events, bash_for_search_count,
                    cold_restart_events, coordinator_respawn_count,
                    parallel_call_rate, context_load_before_first_write_kb,
                    total_context_loaded_kb, post_completion_work_pct,
                    follow_up_issues_created, knowledge_entries_stored,
                    sleep_workaround_count, agent_hotspot_count,
                    friction_hotspot_count, session_hotspot_count, scope_hotspot_count
                 FROM observation_metrics WHERE feature_cycle = ?1",
                rusqlite::params![feature_cycle],
                |row| {
                    Ok(crate::metrics::MetricVector {
                        computed_at: row.get::<_, i64>(0)? as u64,
                        universal: crate::metrics::UniversalMetrics {
                            total_tool_calls: row.get::<_, i64>(1)? as u64,
                            total_duration_secs: row.get::<_, i64>(2)? as u64,
                            session_count: row.get::<_, i64>(3)? as u64,
                            search_miss_rate: row.get(4)?,
                            edit_bloat_total_kb: row.get(5)?,
                            edit_bloat_ratio: row.get(6)?,
                            permission_friction_events: row.get::<_, i64>(7)? as u64,
                            bash_for_search_count: row.get::<_, i64>(8)? as u64,
                            cold_restart_events: row.get::<_, i64>(9)? as u64,
                            coordinator_respawn_count: row.get::<_, i64>(10)? as u64,
                            parallel_call_rate: row.get(11)?,
                            context_load_before_first_write_kb: row.get(12)?,
                            total_context_loaded_kb: row.get(13)?,
                            post_completion_work_pct: row.get(14)?,
                            follow_up_issues_created: row.get::<_, i64>(15)? as u64,
                            knowledge_entries_stored: row.get::<_, i64>(16)? as u64,
                            sleep_workaround_count: row.get::<_, i64>(17)? as u64,
                            agent_hotspot_count: row.get::<_, i64>(18)? as u64,
                            friction_hotspot_count: row.get::<_, i64>(19)? as u64,
                            session_hotspot_count: row.get::<_, i64>(20)? as u64,
                            scope_hotspot_count: row.get::<_, i64>(21)? as u64,
                        },
                        phases: std::collections::BTreeMap::new(),
                    })
                },
            )
            .optional()
            .map_err(StoreError::Sqlite)?;

        let Some(mut mv) = parent else {
            return Ok(None);
        };

        // Query phase rows
        let mut stmt = conn
            .prepare(
                "SELECT phase_name, duration_secs, tool_call_count
                 FROM observation_phase_metrics WHERE feature_cycle = ?1",
            )
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![feature_cycle], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    crate::metrics::PhaseMetrics {
                        duration_secs: row.get::<_, i64>(1)? as u64,
                        tool_call_count: row.get::<_, i64>(2)? as u64,
                    },
                ))
            })
            .map_err(StoreError::Sqlite)?;
        for row in rows {
            let (name, phase) = row.map_err(StoreError::Sqlite)?;
            mv.phases.insert(name, phase);
        }

        Ok(Some(mv))
    }

    /// List all stored observation metrics (nxs-009: typed API).
    ///
    /// Uses two queries with single-pass merge (architecture spec).
    pub fn list_all_metrics(&self) -> Result<Vec<(String, crate::metrics::MetricVector)>> {
        let conn = self.lock_conn();

        // Query 1: all universal metrics
        let mut stmt = conn
            .prepare(
                "SELECT feature_cycle, computed_at,
                    total_tool_calls, total_duration_secs, session_count,
                    search_miss_rate, edit_bloat_total_kb, edit_bloat_ratio,
                    permission_friction_events, bash_for_search_count,
                    cold_restart_events, coordinator_respawn_count,
                    parallel_call_rate, context_load_before_first_write_kb,
                    total_context_loaded_kb, post_completion_work_pct,
                    follow_up_issues_created, knowledge_entries_stored,
                    sleep_workaround_count, agent_hotspot_count,
                    friction_hotspot_count, session_hotspot_count, scope_hotspot_count
                 FROM observation_metrics ORDER BY feature_cycle",
            )
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    crate::metrics::MetricVector {
                        computed_at: row.get::<_, i64>(1)? as u64,
                        universal: crate::metrics::UniversalMetrics {
                            total_tool_calls: row.get::<_, i64>(2)? as u64,
                            total_duration_secs: row.get::<_, i64>(3)? as u64,
                            session_count: row.get::<_, i64>(4)? as u64,
                            search_miss_rate: row.get(5)?,
                            edit_bloat_total_kb: row.get(6)?,
                            edit_bloat_ratio: row.get(7)?,
                            permission_friction_events: row.get::<_, i64>(8)? as u64,
                            bash_for_search_count: row.get::<_, i64>(9)? as u64,
                            cold_restart_events: row.get::<_, i64>(10)? as u64,
                            coordinator_respawn_count: row.get::<_, i64>(11)? as u64,
                            parallel_call_rate: row.get(12)?,
                            context_load_before_first_write_kb: row.get(13)?,
                            total_context_loaded_kb: row.get(14)?,
                            post_completion_work_pct: row.get(15)?,
                            follow_up_issues_created: row.get::<_, i64>(16)? as u64,
                            knowledge_entries_stored: row.get::<_, i64>(17)? as u64,
                            sleep_workaround_count: row.get::<_, i64>(18)? as u64,
                            agent_hotspot_count: row.get::<_, i64>(19)? as u64,
                            friction_hotspot_count: row.get::<_, i64>(20)? as u64,
                            session_hotspot_count: row.get::<_, i64>(21)? as u64,
                            scope_hotspot_count: row.get::<_, i64>(22)? as u64,
                        },
                        phases: std::collections::BTreeMap::new(),
                    },
                ))
            })
            .map_err(StoreError::Sqlite)?;

        let mut results: Vec<(String, crate::metrics::MetricVector)> = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }

        // Query 2: all phase metrics, sorted by feature_cycle for single-pass merge
        let mut stmt = conn
            .prepare(
                "SELECT feature_cycle, phase_name, duration_secs, tool_call_count
                 FROM observation_phase_metrics ORDER BY feature_cycle, phase_name",
            )
            .map_err(StoreError::Sqlite)?;
        let phase_rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    crate::metrics::PhaseMetrics {
                        duration_secs: row.get::<_, i64>(2)? as u64,
                        tool_call_count: row.get::<_, i64>(3)? as u64,
                    },
                ))
            })
            .map_err(StoreError::Sqlite)?;

        // Single-pass merge: both lists ordered by feature_cycle
        let mut result_idx = 0;
        for row in phase_rows {
            let (fc, phase_name, phase) = row.map_err(StoreError::Sqlite)?;
            // Advance result_idx to find matching feature_cycle
            while result_idx < results.len() && results[result_idx].0 < fc {
                result_idx += 1;
            }
            if result_idx < results.len() && results[result_idx].0 == fc {
                results[result_idx].1.phases.insert(phase_name, phase);
            }
        }

        Ok(results)
    }

    /// Compute status aggregates via SQL without deserializing all entries.
    ///
    /// Replaces the full table scan in StatusService (crt-013: ADR-004).
    pub fn compute_status_aggregates(&self) -> Result<StatusAggregates> {
        let conn = self.lock_conn();

        // Query 1: Scalar aggregates (single row)
        let (supersedes_count, superseded_by_count, total_correction_count, unattributed_count) =
            conn.query_row(
                "SELECT \
                    COALESCE(SUM(CASE WHEN supersedes IS NOT NULL THEN 1 ELSE 0 END), 0), \
                    COALESCE(SUM(CASE WHEN superseded_by IS NOT NULL THEN 1 ELSE 0 END), 0), \
                    COALESCE(SUM(correction_count), 0), \
                    COALESCE(SUM(CASE WHEN created_by = '' OR created_by IS NULL THEN 1 ELSE 0 END), 0) \
                FROM entries",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)? as u64,
                        row.get::<_, i64>(1)? as u64,
                        row.get::<_, i64>(2)? as u64,
                        row.get::<_, i64>(3)? as u64,
                    ))
                },
            )
            .map_err(StoreError::Sqlite)?;

        // Query 2: Trust source distribution
        let mut trust_source_distribution = BTreeMap::new();
        let mut stmt = conn
            .prepare(
                "SELECT CASE WHEN trust_source = '' OR trust_source IS NULL \
                        THEN '(none)' ELSE trust_source END, \
                        COUNT(*) \
                 FROM entries \
                 GROUP BY 1",
            )
            .map_err(StoreError::Sqlite)?;

        let rows = stmt
            .query_map([], |row| {
                let source: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((source, count as u64))
            })
            .map_err(StoreError::Sqlite)?;

        for item in rows {
            let (source, count) = item.map_err(StoreError::Sqlite)?;
            trust_source_distribution.insert(source, count);
        }

        Ok(StatusAggregates {
            supersedes_count,
            superseded_by_count,
            total_correction_count,
            trust_source_distribution,
            unattributed_count,
        })
    }

    /// Count active entries grouped by category.
    ///
    /// Returns a map of category name to count. Only entries with `status = 0`
    /// (Active) are counted. Deprecated, Proposed, and Quarantined entries are excluded.
    /// Returns an empty HashMap if no active entries exist.
    pub fn count_active_entries_by_category(&self) -> Result<HashMap<String, u64>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare(
                "SELECT category, COUNT(*) FROM entries \
                 WHERE status = 0 \
                 GROUP BY category",
            )
            .map_err(StoreError::Sqlite)?;

        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })
            .map_err(StoreError::Sqlite)?;

        let mut result: HashMap<String, u64> = HashMap::new();
        for row in rows {
            let (category, count) = row.map_err(StoreError::Sqlite)?;
            result.insert(category, count);
        }

        Ok(result)
    }

    /// Load only Active entries with their tags populated.
    ///
    /// More efficient than loading all entries when only active ones are needed (crt-013).
    pub fn load_active_entries_with_tags(&self) -> Result<Vec<EntryRecord>> {
        let conn = self.lock_conn();

        let mut stmt = conn
            .prepare(&format!(
                "SELECT {} FROM entries WHERE status = ?1",
                ENTRY_COLUMNS
            ))
            .map_err(StoreError::Sqlite)?;

        let mut entries: Vec<EntryRecord> = stmt
            .query_map(
                rusqlite::params![Status::Active as u8 as i64],
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

    /// Load only entries with category="outcome" and their tags populated.
    ///
    /// Used by StatusService for outcome statistics (crt-013).
    pub fn load_outcome_entries_with_tags(&self) -> Result<Vec<EntryRecord>> {
        let conn = self.lock_conn();

        let mut stmt = conn
            .prepare(&format!(
                "SELECT {} FROM entries WHERE category = 'outcome'",
                ENTRY_COLUMNS
            ))
            .map_err(StoreError::Sqlite)?;

        let mut entries: Vec<EntryRecord> = stmt
            .query_map([], entry_from_row)
            .map_err(StoreError::Sqlite)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(StoreError::Sqlite)?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(&conn, &ids)?;
        apply_tags(&mut entries, &tag_map);

        Ok(entries)
    }

    /// Compute effectiveness aggregates via SQL joins (crt-018: ADR-001).
    ///
    /// Executes 4 sequential queries under a single `lock_conn()` to prevent
    /// GC race conditions (R-07). Returns pre-aggregated data for the
    /// effectiveness engine to classify.
    ///
    /// Query 1: Entry injection stats (injection_log JOIN sessions, GROUP BY entry_id)
    /// Query 2: Active topics (DISTINCT feature_cycle from sessions)
    /// Query 3: Calibration rows (per-injection confidence + outcome)
    /// Query 4: Data window (session count, min/max started_at)
    pub fn compute_effectiveness_aggregates(&self) -> Result<EffectivenessAggregates> {
        let conn = self.lock_conn();

        // Query 1: Entry injection stats
        // Deduplicate injection_log per (entry_id, session_id) first to prevent
        // duplicate inflation of outcome counts (R-03). Multiple injection_log rows
        // for the same (entry, session) pair count as one injection.
        let mut stmt = conn
            .prepare(
                "SELECT ds.entry_id, \
                        COUNT(*) as injection_count, \
                        COALESCE(SUM(CASE WHEN s.outcome = 'success' THEN 1 ELSE 0 END), 0), \
                        COALESCE(SUM(CASE WHEN s.outcome = 'rework' THEN 1 ELSE 0 END), 0), \
                        COALESCE(SUM(CASE WHEN s.outcome = 'abandoned' THEN 1 ELSE 0 END), 0) \
                 FROM (SELECT DISTINCT entry_id, session_id FROM injection_log) ds \
                 JOIN sessions s ON ds.session_id = s.session_id \
                 WHERE s.outcome IS NOT NULL \
                 GROUP BY ds.entry_id",
            )
            .map_err(StoreError::Sqlite)?;

        let rows = stmt
            .query_map([], |row| {
                Ok(EntryInjectionStats {
                    entry_id: row.get::<_, i64>(0)? as u64,
                    injection_count: row.get::<_, i64>(1)? as u32,
                    success_count: row.get::<_, i64>(2)? as u32,
                    rework_count: row.get::<_, i64>(3)? as u32,
                    abandoned_count: row.get::<_, i64>(4)? as u32,
                })
            })
            .map_err(StoreError::Sqlite)?;

        let mut entry_stats = Vec::new();
        for row in rows {
            entry_stats.push(row.map_err(StoreError::Sqlite)?);
        }

        // Query 2: Active topics (ADR-002: NULL/empty feature_cycle excluded)
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT feature_cycle FROM sessions \
                 WHERE feature_cycle IS NOT NULL AND feature_cycle != ''",
            )
            .map_err(StoreError::Sqlite)?;

        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(StoreError::Sqlite)?;

        let mut active_topics = HashSet::new();
        for row in rows {
            active_topics.insert(row.map_err(StoreError::Sqlite)?);
        }

        // Query 3: Calibration rows (one per injection_log record with outcome)
        let mut stmt = conn
            .prepare(
                "SELECT il.confidence, (s.outcome = 'success') as succeeded \
                 FROM injection_log il \
                 JOIN sessions s ON il.session_id = s.session_id \
                 WHERE s.outcome IS NOT NULL",
            )
            .map_err(StoreError::Sqlite)?;

        let rows = stmt
            .query_map([], |row| {
                let confidence: f64 = row.get(0)?;
                let succeeded: bool = row.get::<_, i64>(1)? != 0;
                Ok((confidence, succeeded))
            })
            .map_err(StoreError::Sqlite)?;

        let mut calibration_rows = Vec::new();
        for row in rows {
            calibration_rows.push(row.map_err(StoreError::Sqlite)?);
        }

        // Query 4: Data window
        let (session_count, earliest_session_at, latest_session_at) = conn
            .query_row(
                "SELECT COUNT(*), MIN(started_at), MAX(started_at) \
                 FROM sessions WHERE outcome IS NOT NULL",
                [],
                |row| {
                    let count = row.get::<_, i64>(0)? as u32;
                    let earliest = row.get::<_, Option<i64>>(1)?.map(|v| v as u64);
                    let latest = row.get::<_, Option<i64>>(2)?.map(|v| v as u64);
                    Ok((count, earliest, latest))
                },
            )
            .map_err(StoreError::Sqlite)?;

        Ok(EffectivenessAggregates {
            entry_stats,
            active_topics,
            calibration_rows,
            session_count,
            earliest_session_at,
            latest_session_at,
        })
    }

    /// Load entry metadata for effectiveness classification (crt-018).
    ///
    /// Returns metadata for all active entries (status = 0). NULL/empty topic
    /// is mapped to "(unattributed)" in SQL (ADR-002, AC-16).
    pub fn load_entry_classification_meta(&self) -> Result<Vec<EntryClassificationMeta>> {
        let conn = self.lock_conn();

        let mut stmt = conn
            .prepare(
                "SELECT id, title, \
                        CASE WHEN topic IS NULL OR topic = '' THEN '(unattributed)' ELSE topic END, \
                        COALESCE(trust_source, ''), \
                        helpful_count, unhelpful_count \
                 FROM entries \
                 WHERE status = 0",
            )
            .map_err(StoreError::Sqlite)?;

        let rows = stmt
            .query_map([], |row| {
                Ok(EntryClassificationMeta {
                    entry_id: row.get::<_, i64>(0)? as u64,
                    title: row.get::<_, String>(1)?,
                    topic: row.get::<_, String>(2)?,
                    trust_source: row.get::<_, String>(3)?,
                    helpful_count: row.get::<_, i64>(4)? as u32,
                    unhelpful_count: row.get::<_, i64>(5)? as u32,
                })
            })
            .map_err(StoreError::Sqlite)?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(StoreError::Sqlite)?);
        }

        Ok(result)
    }
}

/// Aggregated status metrics computed via SQL (crt-013: ADR-004).
#[derive(Debug, Clone)]
pub struct StatusAggregates {
    /// Number of entries where supersedes IS NOT NULL.
    pub supersedes_count: u64,
    /// Number of entries where superseded_by IS NOT NULL.
    pub superseded_by_count: u64,
    /// Sum of correction_count across all entries.
    pub total_correction_count: u64,
    /// Distribution of trust_source values (empty mapped to "(none)").
    pub trust_source_distribution: BTreeMap<String, u64>,
    /// Number of entries where created_by is empty or NULL.
    pub unattributed_count: u64,
}

/// Raw effectiveness data aggregated by SQL (crt-018: ADR-001).
///
/// Returned by `Store::compute_effectiveness_aggregates()`. The server
/// constructs `DataWindow` from the raw scalar fields to avoid a
/// store -> engine dependency.
#[derive(Debug, Clone)]
pub struct EffectivenessAggregates {
    /// Per-entry injection and outcome stats from injection_log JOIN sessions.
    pub entry_stats: Vec<EntryInjectionStats>,
    /// Topics (feature_cycle values) that have at least one session in the retained window.
    pub active_topics: HashSet<String>,
    /// Per-injection confidence and outcome for calibration bucketing.
    pub calibration_rows: Vec<(f64, bool)>,
    /// Number of sessions with a non-NULL outcome.
    pub session_count: u32,
    /// Earliest `started_at` among sessions with outcomes.
    pub earliest_session_at: Option<u64>,
    /// Latest `started_at` among sessions with outcomes.
    pub latest_session_at: Option<u64>,
}

/// Per-entry aggregated injection + outcome data.
#[derive(Debug, Clone)]
pub struct EntryInjectionStats {
    pub entry_id: u64,
    pub injection_count: u32,
    pub success_count: u32,
    pub rework_count: u32,
    pub abandoned_count: u32,
}

/// Metadata about entries needed for classification (from entries table).
#[derive(Debug, Clone)]
pub struct EntryClassificationMeta {
    pub entry_id: u64,
    pub title: String,
    pub topic: String,
    pub trust_source: String,
    pub helpful_count: u32,
    pub unhelpful_count: u32,
}

#[cfg(test)]
mod tests {
    use crate::injection_log::InjectionLogRecord;
    use crate::schema::Status;
    use crate::sessions::{SessionLifecycleStatus, SessionRecord};
    use crate::test_helpers::{TestDb, TestEntry};

    #[test]
    fn test_count_active_entries_by_category_basic() {
        let db = TestDb::new();
        let store = db.store();

        let id1 = store
            .insert(TestEntry::new("t", "convention").build())
            .unwrap();
        let _id2 = store
            .insert(TestEntry::new("t", "convention").build())
            .unwrap();
        let _id3 = store
            .insert(TestEntry::new("t", "pattern").build())
            .unwrap();

        store.update_status(id1, Status::Deprecated).unwrap();

        let counts = store.count_active_entries_by_category().unwrap();
        assert_eq!(counts.get("convention"), Some(&1));
        assert_eq!(counts.get("pattern"), Some(&1));
    }

    #[test]
    fn test_count_active_entries_by_category_excludes_quarantined() {
        let db = TestDb::new();
        let store = db.store();

        let id1 = store
            .insert(TestEntry::new("t", "decision").build())
            .unwrap();
        let _id2 = store
            .insert(TestEntry::new("t", "decision").build())
            .unwrap();

        store.update_status(id1, Status::Quarantined).unwrap();

        let counts = store.count_active_entries_by_category().unwrap();
        assert_eq!(counts.get("decision"), Some(&1));
    }

    #[test]
    fn test_count_active_entries_by_category_empty_store() {
        let db = TestDb::new();
        let store = db.store();

        let counts = store.count_active_entries_by_category().unwrap();
        assert!(counts.is_empty());
    }

    // -- crt-018: effectiveness-store tests --

    /// Helper to create a session with given parameters.
    fn make_session(
        session_id: &str,
        feature_cycle: Option<&str>,
        outcome: Option<&str>,
        started_at: u64,
    ) -> SessionRecord {
        SessionRecord {
            session_id: session_id.to_string(),
            feature_cycle: feature_cycle.map(|s| s.to_string()),
            agent_role: None,
            started_at,
            ended_at: None,
            status: if outcome.is_some() {
                SessionLifecycleStatus::Completed
            } else {
                SessionLifecycleStatus::Active
            },
            compaction_count: 0,
            outcome: outcome.map(|s| s.to_string()),
            total_injections: 0,
        }
    }

    /// Helper to create injection log records.
    fn make_injections(
        session_id: &str,
        entry_id: u64,
        count: usize,
        confidence: f64,
    ) -> Vec<InjectionLogRecord> {
        (0..count)
            .map(|i| InjectionLogRecord {
                log_id: 0,
                session_id: session_id.to_string(),
                entry_id,
                confidence,
                timestamp: 1000 + i as u64,
            })
            .collect()
    }

    // S-01: COUNT DISTINCT session deduplication (R-03)
    #[test]
    fn test_effectiveness_count_distinct_session_dedup() {
        let db = TestDb::new();
        let store = db.store();

        let eid = store
            .insert(TestEntry::new("auth", "convention").build())
            .unwrap();
        store
            .insert_session(&make_session("s1", Some("crt-018"), Some("success"), 1000))
            .unwrap();

        // 3 injection_log records for same (entry, session) with different confidence
        let records: Vec<InjectionLogRecord> = vec![0.3, 0.5, 0.8]
            .into_iter()
            .enumerate()
            .map(|(i, conf)| InjectionLogRecord {
                log_id: 0,
                session_id: "s1".to_string(),
                entry_id: eid,
                confidence: conf,
                timestamp: 1000 + i as u64,
            })
            .collect();
        store.insert_injection_log_batch(&records).unwrap();

        let agg = store.compute_effectiveness_aggregates().unwrap();
        assert_eq!(agg.entry_stats.len(), 1);
        assert_eq!(agg.entry_stats[0].injection_count, 1); // distinct sessions, not records
        assert_eq!(agg.entry_stats[0].success_count, 1);
    }

    // S-02: Multiple distinct sessions counted correctly (R-03)
    #[test]
    fn test_effectiveness_multiple_distinct_sessions() {
        let db = TestDb::new();
        let store = db.store();

        let eid = store
            .insert(TestEntry::new("auth", "convention").build())
            .unwrap();

        store
            .insert_session(&make_session("s1", Some("crt-018"), Some("success"), 1000))
            .unwrap();
        store
            .insert_session(&make_session("s2", Some("crt-018"), Some("rework"), 2000))
            .unwrap();
        store
            .insert_session(&make_session(
                "s3",
                Some("crt-018"),
                Some("abandoned"),
                3000,
            ))
            .unwrap();

        for sid in &["s1", "s2", "s3"] {
            store
                .insert_injection_log_batch(&make_injections(sid, eid, 1, 0.9))
                .unwrap();
        }

        let agg = store.compute_effectiveness_aggregates().unwrap();
        assert_eq!(agg.entry_stats.len(), 1);
        let stats = &agg.entry_stats[0];
        assert_eq!(stats.injection_count, 3);
        assert_eq!(stats.success_count, 1);
        assert_eq!(stats.rework_count, 1);
        assert_eq!(stats.abandoned_count, 1);
    }

    // S-03: Sessions with NULL outcome excluded (R-03)
    #[test]
    fn test_effectiveness_null_outcome_excluded() {
        let db = TestDb::new();
        let store = db.store();

        let eid = store
            .insert(TestEntry::new("auth", "convention").build())
            .unwrap();

        store
            .insert_session(&make_session("s1", Some("crt-018"), Some("success"), 1000))
            .unwrap();
        // s2 has no outcome (active session)
        store
            .insert_session(&make_session("s2", Some("crt-018"), None, 2000))
            .unwrap();

        store
            .insert_injection_log_batch(&make_injections("s1", eid, 1, 0.9))
            .unwrap();
        store
            .insert_injection_log_batch(&make_injections("s2", eid, 1, 0.8))
            .unwrap();

        let agg = store.compute_effectiveness_aggregates().unwrap();
        assert_eq!(agg.entry_stats.len(), 1);
        assert_eq!(agg.entry_stats[0].injection_count, 1);
        assert_eq!(agg.entry_stats[0].success_count, 1);
    }

    // S-04: Multiple entries with mixed outcomes
    #[test]
    fn test_effectiveness_multiple_entries_mixed_outcomes() {
        let db = TestDb::new();
        let store = db.store();

        let e1 = store
            .insert(TestEntry::new("auth", "convention").build())
            .unwrap();
        let e2 = store
            .insert(TestEntry::new("logging", "pattern").build())
            .unwrap();
        let e3 = store
            .insert(TestEntry::new("api", "decision").build())
            .unwrap();

        store
            .insert_session(&make_session("s1", Some("f1"), Some("success"), 1000))
            .unwrap();
        store
            .insert_session(&make_session("s2", Some("f1"), Some("rework"), 2000))
            .unwrap();
        store
            .insert_session(&make_session("s3", Some("f2"), Some("abandoned"), 3000))
            .unwrap();

        // e1 injected into s1 and s2
        store
            .insert_injection_log_batch(&make_injections("s1", e1, 1, 0.9))
            .unwrap();
        store
            .insert_injection_log_batch(&make_injections("s2", e1, 1, 0.8))
            .unwrap();
        // e2 injected into s2 only
        store
            .insert_injection_log_batch(&make_injections("s2", e2, 1, 0.7))
            .unwrap();
        // e3 injected into s3 only
        store
            .insert_injection_log_batch(&make_injections("s3", e3, 1, 0.6))
            .unwrap();

        let agg = store.compute_effectiveness_aggregates().unwrap();
        assert_eq!(agg.entry_stats.len(), 3);

        let find = |id: u64| agg.entry_stats.iter().find(|s| s.entry_id == id).unwrap();

        let s1 = find(e1);
        assert_eq!(s1.injection_count, 2);
        assert_eq!(s1.success_count, 1);
        assert_eq!(s1.rework_count, 1);

        let s2 = find(e2);
        assert_eq!(s2.injection_count, 1);
        assert_eq!(s2.rework_count, 1);

        let s3 = find(e3);
        assert_eq!(s3.injection_count, 1);
        assert_eq!(s3.abandoned_count, 1);
    }

    // S-05: NULL feature_cycle excluded from active_topics (R-02)
    #[test]
    fn test_effectiveness_null_feature_cycle_excluded() {
        let db = TestDb::new();
        let store = db.store();

        store
            .insert_session(&make_session("s1", Some("crt-018"), Some("success"), 1000))
            .unwrap();
        store
            .insert_session(&make_session("s2", None, Some("success"), 2000))
            .unwrap();

        let agg = store.compute_effectiveness_aggregates().unwrap();
        assert!(agg.active_topics.contains("crt-018"));
        assert_eq!(agg.active_topics.len(), 1);
    }

    // S-06: Empty string feature_cycle excluded (R-02)
    #[test]
    fn test_effectiveness_empty_feature_cycle_excluded() {
        let db = TestDb::new();
        let store = db.store();

        store
            .insert_session(&make_session("s1", Some(""), Some("success"), 1000))
            .unwrap();

        let agg = store.compute_effectiveness_aggregates().unwrap();
        assert!(agg.active_topics.is_empty());
    }

    // S-07: Multiple distinct feature_cycles (R-02)
    #[test]
    fn test_effectiveness_distinct_feature_cycles() {
        let db = TestDb::new();
        let store = db.store();

        store
            .insert_session(&make_session("s1", Some("crt-018"), Some("success"), 1000))
            .unwrap();
        store
            .insert_session(&make_session("s2", Some("crt-018"), Some("rework"), 2000))
            .unwrap();
        store
            .insert_session(&make_session("s3", Some("vnc-001"), Some("success"), 3000))
            .unwrap();

        let agg = store.compute_effectiveness_aggregates().unwrap();
        assert_eq!(agg.active_topics.len(), 2);
        assert!(agg.active_topics.contains("crt-018"));
        assert!(agg.active_topics.contains("vnc-001"));
    }

    // S-08: NULL feature_cycle session still contributes to injection stats (R-02)
    #[test]
    fn test_effectiveness_null_fc_contributes_to_injection_stats() {
        let db = TestDb::new();
        let store = db.store();

        let eid = store
            .insert(TestEntry::new("auth", "convention").build())
            .unwrap();

        // Session with NULL feature_cycle but has outcome
        store
            .insert_session(&make_session("s1", None, Some("success"), 1000))
            .unwrap();
        store
            .insert_injection_log_batch(&make_injections("s1", eid, 1, 0.9))
            .unwrap();

        let agg = store.compute_effectiveness_aggregates().unwrap();
        assert_eq!(agg.entry_stats.len(), 1);
        assert_eq!(agg.entry_stats[0].success_count, 1);
        assert!(agg.active_topics.is_empty());
    }

    // S-09: Calibration rows include all injection records (R-06)
    #[test]
    fn test_effectiveness_calibration_rows_all_records() {
        let db = TestDb::new();
        let store = db.store();

        let eid = store
            .insert(TestEntry::new("auth", "convention").build())
            .unwrap();

        store
            .insert_session(&make_session("s1", Some("crt-018"), Some("success"), 1000))
            .unwrap();

        // 3 injection records with different confidence values
        let records: Vec<InjectionLogRecord> = vec![0.3, 0.5, 0.8]
            .into_iter()
            .enumerate()
            .map(|(i, conf)| InjectionLogRecord {
                log_id: 0,
                session_id: "s1".to_string(),
                entry_id: eid,
                confidence: conf,
                timestamp: 1000 + i as u64,
            })
            .collect();
        store.insert_injection_log_batch(&records).unwrap();

        let agg = store.compute_effectiveness_aggregates().unwrap();
        // Calibration has 3 rows (one per injection), not 1 (distinct session)
        assert_eq!(agg.calibration_rows.len(), 3);
        assert!(agg.calibration_rows.iter().all(|&(_, succeeded)| succeeded));
    }

    // S-10: Data window from sessions with outcomes
    #[test]
    fn test_effectiveness_data_window() {
        let db = TestDb::new();
        let store = db.store();

        store
            .insert_session(&make_session("s1", Some("f1"), Some("success"), 1000))
            .unwrap();
        store
            .insert_session(&make_session("s2", Some("f1"), Some("rework"), 2000))
            .unwrap();
        // s3 has no outcome -- should not be counted
        store
            .insert_session(&make_session("s3", Some("f1"), None, 3000))
            .unwrap();

        let agg = store.compute_effectiveness_aggregates().unwrap();
        assert_eq!(agg.session_count, 2);
        assert_eq!(agg.earliest_session_at, Some(1000));
        assert_eq!(agg.latest_session_at, Some(2000));
    }

    // S-12: Active entries only
    #[test]
    fn test_classification_meta_active_only() {
        let db = TestDb::new();
        let store = db.store();

        let _e1 = store
            .insert(TestEntry::new("auth", "convention").build())
            .unwrap();
        let e2 = store
            .insert(TestEntry::new("logging", "pattern").build())
            .unwrap();

        store.update_status(e2, Status::Deprecated).unwrap();

        let meta = store.load_entry_classification_meta().unwrap();
        assert_eq!(meta.len(), 1);
        assert_eq!(meta[0].entry_id, _e1);
    }

    // S-13: NULL/empty topic mapped to "(unattributed)" (R-02)
    #[test]
    fn test_classification_meta_empty_topic_unattributed() {
        let db = TestDb::new();
        let store = db.store();

        // Insert entry with empty topic
        let eid = store
            .insert(TestEntry::new("", "convention").build())
            .unwrap();

        let meta = store.load_entry_classification_meta().unwrap();
        assert_eq!(meta.len(), 1);
        assert_eq!(meta[0].entry_id, eid);
        assert_eq!(meta[0].topic, "(unattributed)");
    }

    // S-14: Fields correctly populated
    #[test]
    fn test_classification_meta_fields_populated() {
        let db = TestDb::new();
        let store = db.store();

        let eid = store
            .insert(
                TestEntry::new("auth", "convention")
                    .with_title("My Title")
                    .with_trust_source("auto")
                    .build(),
            )
            .unwrap();

        // Set helpful_count and unhelpful_count via direct SQL
        {
            let conn = store.lock_conn();
            conn.execute(
                "UPDATE entries SET helpful_count = 5, unhelpful_count = 2 WHERE id = ?1",
                rusqlite::params![eid as i64],
            )
            .unwrap();
        }

        let meta = store.load_entry_classification_meta().unwrap();
        assert_eq!(meta.len(), 1);
        let m = &meta[0];
        assert_eq!(m.entry_id, eid);
        assert_eq!(m.title, "My Title");
        assert_eq!(m.topic, "auth");
        assert_eq!(m.trust_source, "auto");
        assert_eq!(m.helpful_count, 5);
        assert_eq!(m.unhelpful_count, 2);
    }

    // S-15: Entry with no helpful/unhelpful counts
    #[test]
    fn test_classification_meta_zero_counts() {
        let db = TestDb::new();
        let store = db.store();

        store
            .insert(TestEntry::new("auth", "convention").build())
            .unwrap();

        let meta = store.load_entry_classification_meta().unwrap();
        assert_eq!(meta.len(), 1);
        assert_eq!(meta[0].helpful_count, 0);
        assert_eq!(meta[0].unhelpful_count, 0);
    }

    // S-16: Empty database returns empty aggregates
    #[test]
    fn test_effectiveness_empty_db() {
        let db = TestDb::new();
        let store = db.store();

        let agg = store.compute_effectiveness_aggregates().unwrap();
        assert!(agg.entry_stats.is_empty());
        assert!(agg.active_topics.is_empty());
        assert!(agg.calibration_rows.is_empty());
        assert_eq!(agg.session_count, 0);
        assert_eq!(agg.earliest_session_at, None);
        assert_eq!(agg.latest_session_at, None);
    }

    // S-17: Empty entry_classification_meta on empty DB
    #[test]
    fn test_classification_meta_empty_db() {
        let db = TestDb::new();
        let store = db.store();

        let meta = store.load_entry_classification_meta().unwrap();
        assert!(meta.is_empty());
    }

    // S-18: Performance at scale (R-06)
    #[test]
    fn test_effectiveness_performance_at_scale() {
        let db = TestDb::new();
        let store = db.store();

        // Insert 500 entries
        let entry_ids: Vec<u64> = (0..500)
            .map(|i| {
                store
                    .insert(
                        TestEntry::new(&format!("topic-{}", i % 10), "convention")
                            .with_title(&format!("Entry {i}"))
                            .build(),
                    )
                    .unwrap()
            })
            .collect();

        // Insert 200 sessions with outcomes
        for i in 0..200 {
            let outcome = match i % 3 {
                0 => "success",
                1 => "rework",
                _ => "abandoned",
            };
            store
                .insert_session(&make_session(
                    &format!("s{i}"),
                    Some(&format!("fc-{}", i % 5)),
                    Some(outcome),
                    1000 + i as u64,
                ))
                .unwrap();
        }

        // Insert 10,000 injection_log rows distributed across entries and sessions
        let mut batch = Vec::new();
        for i in 0..10_000 {
            batch.push(InjectionLogRecord {
                log_id: 0,
                session_id: format!("s{}", i % 200),
                entry_id: entry_ids[i % 500],
                confidence: (i % 100) as f64 / 100.0,
                timestamp: 2000 + i as u64,
            });
            // Insert in batches of 500 to avoid enormous single transactions
            if batch.len() == 500 {
                store.insert_injection_log_batch(&batch).unwrap();
                batch.clear();
            }
        }
        if !batch.is_empty() {
            store.insert_injection_log_batch(&batch).unwrap();
        }

        let start = std::time::Instant::now();
        let agg = store.compute_effectiveness_aggregates().unwrap();
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 500,
            "compute_effectiveness_aggregates took {}ms (budget: 500ms)",
            elapsed.as_millis()
        );
        assert!(!agg.entry_stats.is_empty());
        assert!(!agg.calibration_rows.is_empty());
        assert!(agg.session_count > 0);
    }
}
