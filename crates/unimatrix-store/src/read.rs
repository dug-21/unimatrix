use std::collections::{BTreeMap, HashMap, HashSet};

use sqlx::Row;

use crate::db::SqlxStore;
use crate::error::{Result, StoreError};
use crate::schema::{CoAccessRecord, EntryRecord, QueryFilter, Status, TimeRange};

/// All SELECT columns for the entries table, in DDL order.
/// Used by every query that constructs EntryRecord.
pub const ENTRY_COLUMNS: &str = "id, title, content, topic, category, source, status, confidence, \
     created_at, updated_at, last_accessed_at, access_count, \
     supersedes, superseded_by, correction_count, embedding_dim, \
     created_by, modified_by, content_hash, previous_hash, \
     version, feature_cycle, trust_source, helpful_count, unhelpful_count, \
     pre_quarantine_status";

/// Construct an EntryRecord from a sqlx `SqliteRow`. Tags are set to vec![].
/// Caller MUST use `load_tags_for_entries()` (ADR-006, C-10).
pub fn entry_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<EntryRecord> {
    Ok(EntryRecord {
        id: row
            .try_get::<i64, _>("id")
            .map_err(|e| StoreError::Database(e.into()))? as u64,
        title: row
            .try_get("title")
            .map_err(|e| StoreError::Database(e.into()))?,
        content: row
            .try_get("content")
            .map_err(|e| StoreError::Database(e.into()))?,
        topic: row
            .try_get("topic")
            .map_err(|e| StoreError::Database(e.into()))?,
        category: row
            .try_get("category")
            .map_err(|e| StoreError::Database(e.into()))?,
        tags: vec![],
        source: row
            .try_get("source")
            .map_err(|e| StoreError::Database(e.into()))?,
        status: Status::try_from(
            row.try_get::<i64, _>("status")
                .map_err(|e| StoreError::Database(e.into()))? as u8,
        )
        .unwrap_or(Status::Active),
        confidence: row
            .try_get("confidence")
            .map_err(|e| StoreError::Database(e.into()))?,
        created_at: row
            .try_get::<i64, _>("created_at")
            .map_err(|e| StoreError::Database(e.into()))? as u64,
        updated_at: row
            .try_get::<i64, _>("updated_at")
            .map_err(|e| StoreError::Database(e.into()))? as u64,
        last_accessed_at: row
            .try_get::<i64, _>("last_accessed_at")
            .map_err(|e| StoreError::Database(e.into()))? as u64,
        access_count: row
            .try_get::<i64, _>("access_count")
            .map_err(|e| StoreError::Database(e.into()))? as u32,
        supersedes: row
            .try_get::<Option<i64>, _>("supersedes")
            .map_err(|e| StoreError::Database(e.into()))?
            .map(|v| v as u64),
        superseded_by: row
            .try_get::<Option<i64>, _>("superseded_by")
            .map_err(|e| StoreError::Database(e.into()))?
            .map(|v| v as u64),
        correction_count: row
            .try_get::<i64, _>("correction_count")
            .map_err(|e| StoreError::Database(e.into()))? as u32,
        embedding_dim: row
            .try_get::<i64, _>("embedding_dim")
            .map_err(|e| StoreError::Database(e.into()))? as u16,
        created_by: row
            .try_get("created_by")
            .map_err(|e| StoreError::Database(e.into()))?,
        modified_by: row
            .try_get("modified_by")
            .map_err(|e| StoreError::Database(e.into()))?,
        content_hash: row
            .try_get("content_hash")
            .map_err(|e| StoreError::Database(e.into()))?,
        previous_hash: row
            .try_get("previous_hash")
            .map_err(|e| StoreError::Database(e.into()))?,
        version: row
            .try_get::<i64, _>("version")
            .map_err(|e| StoreError::Database(e.into()))? as u32,
        feature_cycle: row
            .try_get("feature_cycle")
            .map_err(|e| StoreError::Database(e.into()))?,
        trust_source: row
            .try_get("trust_source")
            .map_err(|e| StoreError::Database(e.into()))?,
        helpful_count: row
            .try_get::<i64, _>("helpful_count")
            .map_err(|e| StoreError::Database(e.into()))? as u32,
        unhelpful_count: row
            .try_get::<i64, _>("unhelpful_count")
            .map_err(|e| StoreError::Database(e.into()))? as u32,
        pre_quarantine_status: row
            .try_get::<Option<i64>, _>("pre_quarantine_status")
            .map_err(|e| StoreError::Database(e.into()))?
            .map(|v| v as u8),
    })
}

/// Batch-load tags for multiple entries. Returns map of entry_id -> Vec<tag>.
/// Every code path constructing EntryRecord MUST call this (ADR-006, C-10).
pub async fn load_tags_for_entries(
    pool: &sqlx::sqlite::SqlitePool,
    ids: &[u64],
) -> Result<HashMap<u64, Vec<String>>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    // Build IN clause with positional parameters
    let placeholders: Vec<String> = ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect();
    let sql = format!(
        "SELECT entry_id, tag FROM entry_tags WHERE entry_id IN ({}) ORDER BY entry_id, tag",
        placeholders.join(",")
    );

    let mut query = sqlx::query(&sql);
    for &id in ids {
        query = query.bind(id as i64);
    }

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

    let mut map: HashMap<u64, Vec<String>> = HashMap::new();
    for row in rows {
        let entry_id = row
            .try_get::<i64, _>(0)
            .map_err(|e| StoreError::Database(e.into()))? as u64;
        let tag: String = row.try_get(1).map_err(|e| StoreError::Database(e.into()))?;
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

impl SqlxStore {
    /// Get a single entry by ID.
    pub async fn get(&self, entry_id: u64) -> Result<EntryRecord> {
        let sql = format!("SELECT {} FROM entries WHERE id = ?1", ENTRY_COLUMNS);
        let row = sqlx::query(&sql)
            .bind(entry_id as i64)
            .fetch_optional(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?
            .ok_or(StoreError::EntryNotFound(entry_id))?;

        let mut entry = entry_from_row(&row)?;
        let tag_map = load_tags_for_entries(self.read_pool(), &[entry_id]).await?;
        if let Some(tags) = tag_map.get(&entry_id) {
            entry.tags = tags.clone();
        }
        Ok(entry)
    }

    /// Check if an entry exists without deserializing it.
    pub async fn exists(&self, entry_id: u64) -> Result<bool> {
        let val: Option<i64> = sqlx::query_scalar("SELECT 1 FROM entries WHERE id = ?1 LIMIT 1")
            .bind(entry_id as i64)
            .fetch_optional(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
        Ok(val.is_some())
    }

    /// Query entries by topic.
    pub async fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>> {
        let sql = format!("SELECT {} FROM entries WHERE topic = ?1", ENTRY_COLUMNS);
        let rows = sqlx::query(&sql)
            .bind(topic)
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let mut entries: Vec<EntryRecord> = rows
            .iter()
            .map(entry_from_row)
            .collect::<Result<Vec<_>>>()?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(self.read_pool(), &ids).await?;
        apply_tags(&mut entries, &tag_map);
        Ok(entries)
    }

    /// Query entries by category.
    pub async fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>> {
        let sql = format!("SELECT {} FROM entries WHERE category = ?1", ENTRY_COLUMNS);
        let rows = sqlx::query(&sql)
            .bind(category)
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let mut entries: Vec<EntryRecord> = rows
            .iter()
            .map(entry_from_row)
            .collect::<Result<Vec<_>>>()?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(self.read_pool(), &ids).await?;
        apply_tags(&mut entries, &tag_map);
        Ok(entries)
    }

    /// Query entries matching ALL specified tags (intersection).
    pub async fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>> {
        if tags.is_empty() {
            return Ok(vec![]);
        }

        let placeholders: Vec<String> = tags
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect();
        let tag_count_param = tags.len() + 1;
        let sql = format!(
            "SELECT {} FROM entries WHERE id IN (\
                SELECT entry_id FROM entry_tags \
                WHERE tag IN ({}) \
                GROUP BY entry_id \
                HAVING COUNT(DISTINCT tag) = ?{}\
            )",
            ENTRY_COLUMNS,
            placeholders.join(","),
            tag_count_param
        );

        let mut query = sqlx::query(&sql);
        for tag in tags {
            query = query.bind(tag.clone());
        }
        query = query.bind(tags.len() as i64);

        let rows = query
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let mut entries: Vec<EntryRecord> = rows
            .iter()
            .map(entry_from_row)
            .collect::<Result<Vec<_>>>()?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(self.read_pool(), &ids).await?;
        apply_tags(&mut entries, &tag_map);
        Ok(entries)
    }

    /// Query entries within a time range (inclusive on both ends).
    pub async fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>> {
        if range.start > range.end {
            return Ok(vec![]);
        }
        let sql = format!(
            "SELECT {} FROM entries WHERE created_at BETWEEN ?1 AND ?2",
            ENTRY_COLUMNS
        );
        let rows = sqlx::query(&sql)
            .bind(range.start as i64)
            .bind(range.end as i64)
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let mut entries: Vec<EntryRecord> = rows
            .iter()
            .map(entry_from_row)
            .collect::<Result<Vec<_>>>()?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(self.read_pool(), &ids).await?;
        apply_tags(&mut entries, &tag_map);
        Ok(entries)
    }

    /// Query entries with a given status.
    pub async fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>> {
        let sql = format!("SELECT {} FROM entries WHERE status = ?1", ENTRY_COLUMNS);
        let rows = sqlx::query(&sql)
            .bind(status as u8 as i64)
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let mut entries: Vec<EntryRecord> = rows
            .iter()
            .map(entry_from_row)
            .collect::<Result<Vec<_>>>()?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(self.read_pool(), &ids).await?;
        apply_tags(&mut entries, &tag_map);
        Ok(entries)
    }

    /// Query all entries regardless of status (GH #266).
    pub async fn query_all_entries(&self) -> Result<Vec<EntryRecord>> {
        let sql = format!("SELECT {} FROM entries", ENTRY_COLUMNS);
        let rows = sqlx::query(&sql)
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let mut entries: Vec<EntryRecord> = rows
            .iter()
            .map(entry_from_row)
            .collect::<Result<Vec<_>>>()?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(self.read_pool(), &ids).await?;
        apply_tags(&mut entries, &tag_map);
        Ok(entries)
    }

    /// Combined query with SQL WHERE clause across all specified filters.
    pub async fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>> {
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

        // Build dynamic WHERE clause using positional ?N params
        let mut conditions: Vec<String> = Vec::new();
        // We'll collect bind values as a dynamic list of boxed closures
        // Since sqlx doesn't support late-bound params easily, build params as Vec<String>
        // and bind them in order.
        struct Param {
            kind: ParamKind,
        }
        enum ParamKind {
            Text(String),
            Int(i64),
        }

        let mut params: Vec<Param> = Vec::new();
        let mut param_idx = 1usize;

        if let Some(ref topic) = filter.topic {
            conditions.push(format!("topic = ?{param_idx}"));
            params.push(Param {
                kind: ParamKind::Text(topic.clone()),
            });
            param_idx += 1;
        }
        if let Some(ref category) = filter.category {
            conditions.push(format!("category = ?{param_idx}"));
            params.push(Param {
                kind: ParamKind::Text(category.clone()),
            });
            param_idx += 1;
        }
        if let Some(status) = effective_status {
            conditions.push(format!("status = ?{param_idx}"));
            params.push(Param {
                kind: ParamKind::Int(status as u8 as i64),
            });
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
            params.push(Param {
                kind: ParamKind::Int(range.start as i64),
            });
            params.push(Param {
                kind: ParamKind::Int(range.end as i64),
            });
            param_idx += 2;
        }
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
                params.push(Param {
                    kind: ParamKind::Text(tag.clone()),
                });
            }
            params.push(Param {
                kind: ParamKind::Int(tags.len() as i64),
            });
            let _ = param_idx; // suppress unused warning
        }

        let where_clause = if conditions.is_empty() {
            "WHERE status = 0".to_string()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!("SELECT {} FROM entries {}", ENTRY_COLUMNS, where_clause);
        let mut query = sqlx::query(&sql);
        for p in params {
            match p.kind {
                ParamKind::Text(s) => {
                    query = query.bind(s);
                }
                ParamKind::Int(i) => {
                    query = query.bind(i);
                }
            }
        }

        let rows = query
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let mut entries: Vec<EntryRecord> = rows
            .iter()
            .map(entry_from_row)
            .collect::<Result<Vec<_>>>()?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(self.read_pool(), &ids).await?;
        apply_tags(&mut entries, &tag_map);
        Ok(entries)
    }

    /// Look up the hnsw_data_id for an entry in vector_map.
    pub async fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>> {
        let val: Option<i64> =
            sqlx::query_scalar("SELECT hnsw_data_id FROM vector_map WHERE entry_id = ?1")
                .bind(entry_id as i64)
                .fetch_optional(self.read_pool())
                .await
                .map_err(|e| StoreError::Database(e.into()))?;
        Ok(val.map(|v| v as u64))
    }

    /// Iterate all entries in the vector_map table.
    pub async fn iter_vector_mappings(&self) -> Result<Vec<(u64, u64)>> {
        let rows = sqlx::query("SELECT entry_id, hnsw_data_id FROM vector_map ORDER BY entry_id")
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        rows.iter()
            .map(|row| {
                let eid = row
                    .try_get::<i64, _>(0)
                    .map_err(|e| StoreError::Database(e.into()))? as u64;
                let did = row
                    .try_get::<i64, _>(1)
                    .map_err(|e| StoreError::Database(e.into()))? as u64;
                Ok((eid, did))
            })
            .collect()
    }

    /// Read a named counter value. Returns 0 if the counter does not exist.
    pub async fn read_counter(&self, name: &str) -> Result<u64> {
        crate::counters::read_counter(self.read_pool(), name).await
    }

    /// Get all co-access partners for an entry, filtering by staleness.
    pub async fn get_co_access_partners(
        &self,
        entry_id: u64,
        staleness_cutoff: u64,
    ) -> Result<Vec<(u64, CoAccessRecord)>> {
        let mut partners = Vec::new();

        // Scan 1: pairs where entry_id is entry_id_a
        let rows_a = sqlx::query(
            "SELECT entry_id_b, count, last_updated FROM co_access \
             WHERE entry_id_a = ?1 AND last_updated >= ?2",
        )
        .bind(entry_id as i64)
        .bind(staleness_cutoff as i64)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        for row in rows_a {
            let partner_id = row
                .try_get::<i64, _>(0)
                .map_err(|e| StoreError::Database(e.into()))? as u64;
            if partner_id != entry_id {
                let record = CoAccessRecord {
                    count: row
                        .try_get::<i64, _>(1)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u32,
                    last_updated: row
                        .try_get::<i64, _>(2)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                };
                partners.push((partner_id, record));
            }
        }

        // Scan 2: pairs where entry_id is entry_id_b
        let rows_b = sqlx::query(
            "SELECT entry_id_a, count, last_updated FROM co_access \
             WHERE entry_id_b = ?1 AND last_updated >= ?2",
        )
        .bind(entry_id as i64)
        .bind(staleness_cutoff as i64)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        for row in rows_b {
            let partner_id = row
                .try_get::<i64, _>(0)
                .map_err(|e| StoreError::Database(e.into()))? as u64;
            if partner_id != entry_id {
                let record = CoAccessRecord {
                    count: row
                        .try_get::<i64, _>(1)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u32,
                    last_updated: row
                        .try_get::<i64, _>(2)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                };
                partners.push((partner_id, record));
            }
        }

        Ok(partners)
    }

    /// Get co-access statistics: (total_pairs, active_pairs).
    pub async fn co_access_stats(&self, staleness_cutoff: u64) -> Result<(u64, u64)> {
        let row = sqlx::query(
            "SELECT COUNT(*), \
             SUM(CASE WHEN last_updated >= ?1 THEN 1 ELSE 0 END) \
             FROM co_access",
        )
        .bind(staleness_cutoff as i64)
        .fetch_one(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let total = row
            .try_get::<i64, _>(0)
            .map_err(|e| StoreError::Database(e.into()))? as u64;
        let active = row
            .try_get::<Option<i64>, _>(1)
            .map_err(|e| StoreError::Database(e.into()))?
            .unwrap_or(0) as u64;
        Ok((total, active))
    }

    /// Get top N co-access pairs by count (non-stale only).
    pub async fn top_co_access_pairs(
        &self,
        n: usize,
        staleness_cutoff: u64,
    ) -> Result<Vec<((u64, u64), CoAccessRecord)>> {
        let rows = sqlx::query(
            "SELECT entry_id_a, entry_id_b, count, last_updated FROM co_access \
             WHERE last_updated >= ?1 \
             ORDER BY count DESC \
             LIMIT ?2",
        )
        .bind(staleness_cutoff as i64)
        .bind(n as i64)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        rows.iter()
            .map(|row| {
                let a = row
                    .try_get::<i64, _>(0)
                    .map_err(|e| StoreError::Database(e.into()))? as u64;
                let b = row
                    .try_get::<i64, _>(1)
                    .map_err(|e| StoreError::Database(e.into()))? as u64;
                let record = CoAccessRecord {
                    count: row
                        .try_get::<i64, _>(2)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u32,
                    last_updated: row
                        .try_get::<i64, _>(3)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                };
                Ok(((a, b), record))
            })
            .collect()
    }

    /// Retrieve stored observation metrics for a feature cycle (nxs-009: typed API).
    pub async fn get_metrics(
        &self,
        feature_cycle: &str,
    ) -> Result<Option<crate::metrics::MetricVector>> {
        let parent_row = sqlx::query(
            "SELECT computed_at,
                total_tool_calls, total_duration_secs, session_count,
                search_miss_rate, edit_bloat_total_kb, edit_bloat_ratio,
                permission_friction_events, bash_for_search_count,
                cold_restart_events, coordinator_respawn_count,
                parallel_call_rate, context_load_before_first_write_kb,
                total_context_loaded_kb, post_completion_work_pct,
                follow_up_issues_created, knowledge_entries_stored,
                sleep_workaround_count, agent_hotspot_count,
                friction_hotspot_count, session_hotspot_count, scope_hotspot_count,
                domain_metrics_json
             FROM observation_metrics WHERE feature_cycle = ?1",
        )
        .bind(feature_cycle)
        .fetch_optional(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let Some(row) = parent_row else {
            return Ok(None);
        };

        // Deserialize domain_metrics_json: NULL → empty map (v13 rows, FR-05.4).
        // Malformed JSON → empty map (best-effort degradation, never panic).
        let domain_metrics_json: Option<String> = row
            .try_get(22)
            .map_err(|e| StoreError::Database(e.into()))?;
        let domain_metrics: std::collections::HashMap<String, f64> = match domain_metrics_json {
            None => std::collections::HashMap::new(),
            Some(ref json_str) => {
                serde_json::from_str(json_str).unwrap_or_else(|_| std::collections::HashMap::new())
            }
        };

        let mv = crate::metrics::MetricVector {
            computed_at: row
                .try_get::<i64, _>(0)
                .map_err(|e| StoreError::Database(e.into()))? as u64,
            universal: crate::metrics::UniversalMetrics {
                total_tool_calls: row
                    .try_get::<i64, _>(1)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                total_duration_secs: row
                    .try_get::<i64, _>(2)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                session_count: row
                    .try_get::<i64, _>(3)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                search_miss_rate: row.try_get(4).map_err(|e| StoreError::Database(e.into()))?,
                edit_bloat_total_kb: row.try_get(5).map_err(|e| StoreError::Database(e.into()))?,
                edit_bloat_ratio: row.try_get(6).map_err(|e| StoreError::Database(e.into()))?,
                permission_friction_events: row
                    .try_get::<i64, _>(7)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                bash_for_search_count: row
                    .try_get::<i64, _>(8)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                cold_restart_events: row
                    .try_get::<i64, _>(9)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                coordinator_respawn_count: row
                    .try_get::<i64, _>(10)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                parallel_call_rate: row
                    .try_get(11)
                    .map_err(|e| StoreError::Database(e.into()))?,
                context_load_before_first_write_kb: row
                    .try_get(12)
                    .map_err(|e| StoreError::Database(e.into()))?,
                total_context_loaded_kb: row
                    .try_get(13)
                    .map_err(|e| StoreError::Database(e.into()))?,
                post_completion_work_pct: row
                    .try_get(14)
                    .map_err(|e| StoreError::Database(e.into()))?,
                follow_up_issues_created: row
                    .try_get::<i64, _>(15)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                knowledge_entries_stored: row
                    .try_get::<i64, _>(16)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                sleep_workaround_count: row
                    .try_get::<i64, _>(17)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                agent_hotspot_count: row
                    .try_get::<i64, _>(18)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                friction_hotspot_count: row
                    .try_get::<i64, _>(19)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                session_hotspot_count: row
                    .try_get::<i64, _>(20)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                scope_hotspot_count: row
                    .try_get::<i64, _>(21)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
            },
            phases: BTreeMap::new(),
            domain_metrics,
        };

        // Load phase rows
        let phase_rows = sqlx::query(
            "SELECT phase_name, duration_secs, tool_call_count
             FROM observation_phase_metrics WHERE feature_cycle = ?1",
        )
        .bind(feature_cycle)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let mut phases = BTreeMap::new();
        for row in phase_rows {
            let name: String = row.try_get(0).map_err(|e| StoreError::Database(e.into()))?;
            let phase = crate::metrics::PhaseMetrics {
                duration_secs: row
                    .try_get::<i64, _>(1)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                tool_call_count: row
                    .try_get::<i64, _>(2)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
            };
            phases.insert(name, phase);
        }

        Ok(Some(crate::metrics::MetricVector { phases, ..mv }))
    }

    /// List all stored observation metrics (nxs-009: typed API).
    pub async fn list_all_metrics(&self) -> Result<Vec<(String, crate::metrics::MetricVector)>> {
        let rows = sqlx::query(
            "SELECT feature_cycle, computed_at,
                total_tool_calls, total_duration_secs, session_count,
                search_miss_rate, edit_bloat_total_kb, edit_bloat_ratio,
                permission_friction_events, bash_for_search_count,
                cold_restart_events, coordinator_respawn_count,
                parallel_call_rate, context_load_before_first_write_kb,
                total_context_loaded_kb, post_completion_work_pct,
                follow_up_issues_created, knowledge_entries_stored,
                sleep_workaround_count, agent_hotspot_count,
                friction_hotspot_count, session_hotspot_count, scope_hotspot_count,
                domain_metrics_json
             FROM observation_metrics ORDER BY feature_cycle",
        )
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let mut results: Vec<(String, crate::metrics::MetricVector)> = rows
            .iter()
            .map(|row| -> Result<(String, crate::metrics::MetricVector)> {
                let fc: String = row.try_get(0).map_err(|e| StoreError::Database(e.into()))?;
                // Deserialize domain_metrics_json: NULL → empty map (v13 rows, FR-05.4).
                // Malformed JSON → empty map (best-effort degradation, never panic).
                let domain_metrics_json: Option<String> = row
                    .try_get(23)
                    .map_err(|e| StoreError::Database(e.into()))?;
                let domain_metrics: std::collections::HashMap<String, f64> =
                    match domain_metrics_json {
                        None => std::collections::HashMap::new(),
                        Some(ref json_str) => serde_json::from_str(json_str)
                            .unwrap_or_else(|_| std::collections::HashMap::new()),
                    };
                let mv = crate::metrics::MetricVector {
                    computed_at: row
                        .try_get::<i64, _>(1)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                    universal: crate::metrics::UniversalMetrics {
                        total_tool_calls: row
                            .try_get::<i64, _>(2)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        total_duration_secs: row
                            .try_get::<i64, _>(3)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        session_count: row
                            .try_get::<i64, _>(4)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        search_miss_rate: row
                            .try_get(5)
                            .map_err(|e| StoreError::Database(e.into()))?,
                        edit_bloat_total_kb: row
                            .try_get(6)
                            .map_err(|e| StoreError::Database(e.into()))?,
                        edit_bloat_ratio: row
                            .try_get(7)
                            .map_err(|e| StoreError::Database(e.into()))?,
                        permission_friction_events: row
                            .try_get::<i64, _>(8)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        bash_for_search_count: row
                            .try_get::<i64, _>(9)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        cold_restart_events: row
                            .try_get::<i64, _>(10)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        coordinator_respawn_count: row
                            .try_get::<i64, _>(11)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        parallel_call_rate: row
                            .try_get(12)
                            .map_err(|e| StoreError::Database(e.into()))?,
                        context_load_before_first_write_kb: row
                            .try_get(13)
                            .map_err(|e| StoreError::Database(e.into()))?,
                        total_context_loaded_kb: row
                            .try_get(14)
                            .map_err(|e| StoreError::Database(e.into()))?,
                        post_completion_work_pct: row
                            .try_get(15)
                            .map_err(|e| StoreError::Database(e.into()))?,
                        follow_up_issues_created: row
                            .try_get::<i64, _>(16)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        knowledge_entries_stored: row
                            .try_get::<i64, _>(17)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        sleep_workaround_count: row
                            .try_get::<i64, _>(18)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        agent_hotspot_count: row
                            .try_get::<i64, _>(19)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        friction_hotspot_count: row
                            .try_get::<i64, _>(20)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        session_hotspot_count: row
                            .try_get::<i64, _>(21)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                        scope_hotspot_count: row
                            .try_get::<i64, _>(22)
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u64,
                    },
                    phases: BTreeMap::new(),
                    domain_metrics,
                };
                Ok((fc, mv))
            })
            .collect::<Result<Vec<_>>>()?;

        // Load all phase rows, sorted for single-pass merge
        let phase_rows = sqlx::query(
            "SELECT feature_cycle, phase_name, duration_secs, tool_call_count
             FROM observation_phase_metrics ORDER BY feature_cycle, phase_name",
        )
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let mut result_idx = 0;
        for row in phase_rows {
            let fc: String = row.try_get(0).map_err(|e| StoreError::Database(e.into()))?;
            let phase_name: String = row.try_get(1).map_err(|e| StoreError::Database(e.into()))?;
            let phase = crate::metrics::PhaseMetrics {
                duration_secs: row
                    .try_get::<i64, _>(2)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
                tool_call_count: row
                    .try_get::<i64, _>(3)
                    .map_err(|e| StoreError::Database(e.into()))?
                    as u64,
            };
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
    pub async fn compute_status_aggregates(&self) -> Result<StatusAggregates> {
        let row = sqlx::query(
            "SELECT \
                COALESCE(SUM(CASE WHEN supersedes IS NOT NULL THEN 1 ELSE 0 END), 0), \
                COALESCE(SUM(CASE WHEN superseded_by IS NOT NULL THEN 1 ELSE 0 END), 0), \
                COALESCE(SUM(correction_count), 0), \
                COALESCE(SUM(CASE WHEN created_by = '' OR created_by IS NULL THEN 1 ELSE 0 END), 0) \
            FROM entries",
        )
        .fetch_one(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let supersedes_count = row
            .try_get::<i64, _>(0)
            .map_err(|e| StoreError::Database(e.into()))? as u64;
        let superseded_by_count = row
            .try_get::<i64, _>(1)
            .map_err(|e| StoreError::Database(e.into()))? as u64;
        let total_correction_count =
            row.try_get::<i64, _>(2)
                .map_err(|e| StoreError::Database(e.into()))? as u64;
        let unattributed_count = row
            .try_get::<i64, _>(3)
            .map_err(|e| StoreError::Database(e.into()))? as u64;

        let dist_rows = sqlx::query(
            "SELECT CASE WHEN trust_source = '' OR trust_source IS NULL \
                    THEN '(none)' ELSE trust_source END, \
                    COUNT(*) \
             FROM entries \
             GROUP BY 1",
        )
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let mut trust_source_distribution = BTreeMap::new();
        for row in dist_rows {
            let source: String = row.try_get(0).map_err(|e| StoreError::Database(e.into()))?;
            let count: i64 = row.try_get(1).map_err(|e| StoreError::Database(e.into()))?;
            trust_source_distribution.insert(source, count as u64);
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
    pub async fn count_active_entries_by_category(&self) -> Result<HashMap<String, u64>> {
        let rows = sqlx::query(
            "SELECT category, COUNT(*) FROM entries \
             WHERE status = 0 \
             GROUP BY category",
        )
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        rows.iter()
            .map(|row| {
                let category: String =
                    row.try_get(0).map_err(|e| StoreError::Database(e.into()))?;
                let count =
                    row.try_get::<i64, _>(1)
                        .map_err(|e| StoreError::Database(e.into()))? as u64;
                Ok((category, count))
            })
            .collect()
    }

    /// Load only Active entries with their tags populated.
    pub async fn load_active_entries_with_tags(&self) -> Result<Vec<EntryRecord>> {
        let sql = format!("SELECT {} FROM entries WHERE status = ?1", ENTRY_COLUMNS);
        let rows = sqlx::query(&sql)
            .bind(Status::Active as u8 as i64)
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let mut entries: Vec<EntryRecord> = rows
            .iter()
            .map(entry_from_row)
            .collect::<Result<Vec<_>>>()?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(self.read_pool(), &ids).await?;
        apply_tags(&mut entries, &tag_map);
        Ok(entries)
    }

    /// Load only entries with category="outcome" and their tags populated.
    pub async fn load_outcome_entries_with_tags(&self) -> Result<Vec<EntryRecord>> {
        let sql = format!(
            "SELECT {} FROM entries WHERE category = 'outcome'",
            ENTRY_COLUMNS
        );
        let rows = sqlx::query(&sql)
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        let mut entries: Vec<EntryRecord> = rows
            .iter()
            .map(entry_from_row)
            .collect::<Result<Vec<_>>>()?;

        let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let tag_map = load_tags_for_entries(self.read_pool(), &ids).await?;
        apply_tags(&mut entries, &tag_map);
        Ok(entries)
    }

    /// Compute effectiveness aggregates via SQL joins (crt-018: ADR-001).
    pub async fn compute_effectiveness_aggregates(&self) -> Result<EffectivenessAggregates> {
        // Query 1: Entry injection stats
        let stat_rows = sqlx::query(
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
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let entry_stats: Vec<EntryInjectionStats> = stat_rows
            .iter()
            .map(|row| -> Result<EntryInjectionStats> {
                Ok(EntryInjectionStats {
                    entry_id: row
                        .try_get::<i64, _>(0)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                    injection_count: row
                        .try_get::<i64, _>(1)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u32,
                    success_count: row
                        .try_get::<i64, _>(2)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u32,
                    rework_count: row
                        .try_get::<i64, _>(3)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u32,
                    abandoned_count: row
                        .try_get::<i64, _>(4)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u32,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        // Query 2: Active topics
        let topic_rows = sqlx::query(
            "SELECT DISTINCT feature_cycle FROM sessions \
             WHERE feature_cycle IS NOT NULL AND feature_cycle != ''",
        )
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let mut active_topics = HashSet::new();
        for row in topic_rows {
            let fc: String = row.try_get(0).map_err(|e| StoreError::Database(e.into()))?;
            active_topics.insert(fc);
        }

        // Query 3: Calibration rows
        let cal_rows = sqlx::query(
            "SELECT il.confidence, (s.outcome = 'success') as succeeded \
             FROM injection_log il \
             JOIN sessions s ON il.session_id = s.session_id \
             WHERE s.outcome IS NOT NULL",
        )
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let calibration_rows: Vec<(f64, bool)> = cal_rows
            .iter()
            .map(|row| -> Result<(f64, bool)> {
                let confidence: f64 = row.try_get(0).map_err(|e| StoreError::Database(e.into()))?;
                let succeeded: i64 = row.try_get(1).map_err(|e| StoreError::Database(e.into()))?;
                Ok((confidence, succeeded != 0))
            })
            .collect::<Result<Vec<_>>>()?;

        // Query 4: Data window
        let dw_row = sqlx::query(
            "SELECT COUNT(*), MIN(started_at), MAX(started_at) \
             FROM sessions WHERE outcome IS NOT NULL",
        )
        .fetch_one(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let session_count = dw_row
            .try_get::<i64, _>(0)
            .map_err(|e| StoreError::Database(e.into()))? as u32;
        let earliest_session_at = dw_row
            .try_get::<Option<i64>, _>(1)
            .map_err(|e| StoreError::Database(e.into()))?
            .map(|v| v as u64);
        let latest_session_at = dw_row
            .try_get::<Option<i64>, _>(2)
            .map_err(|e| StoreError::Database(e.into()))?
            .map(|v| v as u64);

        Ok(EffectivenessAggregates {
            entry_stats,
            active_topics,
            calibration_rows,
            session_count,
            earliest_session_at,
            latest_session_at,
        })
    }

    /// Query all rows from the `graph_edges` table (crt-021).
    ///
    /// Used by the background tick to load the full edge set and pass it to
    /// `build_typed_relation_graph`. No ORDER BY — the caller is order-independent.
    /// The `metadata` column is intentionally excluded; it is NULL for all crt-021 rows.
    pub async fn query_graph_edges(&self) -> Result<Vec<GraphEdgeRow>> {
        let rows = sqlx::query(
            "SELECT source_id, target_id, relation_type, weight, created_at, \
                    created_by, source, bootstrap_only \
             FROM graph_edges",
        )
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        rows.into_iter()
            .map(|row| {
                Ok(GraphEdgeRow {
                    source_id: row
                        .try_get::<i64, _>("source_id")
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                    target_id: row
                        .try_get::<i64, _>("target_id")
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                    relation_type: row
                        .try_get("relation_type")
                        .map_err(|e| StoreError::Database(e.into()))?,
                    weight: row
                        .try_get::<f32, _>("weight")
                        .map_err(|e| StoreError::Database(e.into()))?,
                    created_at: row
                        .try_get::<i64, _>("created_at")
                        .map_err(|e| StoreError::Database(e.into()))?,
                    created_by: row
                        .try_get("created_by")
                        .map_err(|e| StoreError::Database(e.into()))?,
                    source: row
                        .try_get("source")
                        .map_err(|e| StoreError::Database(e.into()))?,
                    bootstrap_only: row
                        .try_get::<i64, _>("bootstrap_only")
                        .map_err(|e| StoreError::Database(e.into()))?
                        != 0,
                })
            })
            .collect::<Result<Vec<_>>>()
    }

    /// Fetch all GRAPH_EDGES rows with bootstrap_only=1 AND relation_type='Contradicts'.
    ///
    /// Returns (edge_id, source_id, target_id) for all bootstrap contradiction edges.
    /// Used by the bootstrap NLI promotion task (crt-023).
    pub async fn query_bootstrap_contradicts(&self) -> Result<Vec<(u64, u64, u64)>> {
        let rows = sqlx::query(
            "SELECT id, source_id, target_id FROM graph_edges \
             WHERE bootstrap_only = 1 AND relation_type = 'Contradicts'",
        )
        .fetch_all(self.write_pool_server())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        rows.into_iter()
            .map(|row| {
                let edge_id =
                    row.try_get::<i64, _>("id")
                        .map_err(|e| StoreError::Database(e.into()))? as u64;
                let source_id =
                    row.try_get::<i64, _>("source_id")
                        .map_err(|e| StoreError::Database(e.into()))? as u64;
                let target_id =
                    row.try_get::<i64, _>("target_id")
                        .map_err(|e| StoreError::Database(e.into()))? as u64;
                Ok((edge_id, source_id, target_id))
            })
            .collect::<Result<Vec<_>>>()
    }

    /// Fetch entry content by ID using the write pool.
    ///
    /// Used by the NLI bootstrap promotion path (crt-023) to read entry content
    /// immediately after it has been written — the write pool sees all committed data
    /// while the read pool (opened read-only) may lag behind in WAL mode.
    ///
    /// Returns `StoreError::EntryNotFound` if the entry does not exist.
    pub async fn get_content_via_write_pool(&self, entry_id: u64) -> Result<String> {
        let row: Option<sqlx::sqlite::SqliteRow> =
            sqlx::query("SELECT content FROM entries WHERE id = ?1")
                .bind(entry_id as i64)
                .fetch_optional(self.write_pool_server())
                .await
                .map_err(|e| StoreError::Database(e.into()))?;
        let row = row.ok_or(StoreError::EntryNotFound(entry_id))?;
        row.try_get::<String, _>("content")
            .map_err(|e| StoreError::Database(e.into()))
    }

    /// Fetch all Contradicts edges targeting `entry_id` from `graph_edges`.
    ///
    /// Returns one `ContradictEdgeRow` per matching row. Includes the edge `id` and
    /// `metadata` JSON blob so the caller can inspect NLI-origin scores.
    ///
    /// Used by the background tick NLI auto-quarantine threshold guard (ADR-007 crt-023).
    pub async fn query_contradicts_edges_for_entry(
        &self,
        entry_id: u64,
    ) -> Result<Vec<ContradictEdgeRow>> {
        let rows = sqlx::query(
            "SELECT id, source_id, target_id, source, bootstrap_only, metadata \
             FROM graph_edges \
             WHERE target_id = ?1 AND relation_type = 'Contradicts'",
        )
        .bind(entry_id as i64)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        rows.into_iter()
            .map(|row| {
                let id = row
                    .try_get::<i64, _>("id")
                    .map_err(|e| StoreError::Database(e.into()))? as u64;
                let source_id =
                    row.try_get::<i64, _>("source_id")
                        .map_err(|e| StoreError::Database(e.into()))? as u64;
                let target_id =
                    row.try_get::<i64, _>("target_id")
                        .map_err(|e| StoreError::Database(e.into()))? as u64;
                let source: String = row
                    .try_get("source")
                    .map_err(|e| StoreError::Database(e.into()))?;
                let bootstrap_only: bool = row
                    .try_get::<i64, _>("bootstrap_only")
                    .map_err(|e| StoreError::Database(e.into()))?
                    != 0;
                let metadata: Option<String> = row
                    .try_get("metadata")
                    .map_err(|e| StoreError::Database(e.into()))?;
                Ok(ContradictEdgeRow {
                    id,
                    source_id,
                    target_id,
                    source,
                    bootstrap_only,
                    metadata,
                })
            })
            .collect::<Result<Vec<_>>>()
    }

    /// Load entry metadata for effectiveness classification (crt-018).
    pub async fn load_entry_classification_meta(&self) -> Result<Vec<EntryClassificationMeta>> {
        let rows = sqlx::query(
            "SELECT id, title, \
                    CASE WHEN topic IS NULL OR topic = '' THEN '(unattributed)' ELSE topic END, \
                    COALESCE(trust_source, ''), \
                    helpful_count, unhelpful_count \
             FROM entries \
             WHERE status = 0",
        )
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        rows.iter()
            .map(|row| -> Result<EntryClassificationMeta> {
                Ok(EntryClassificationMeta {
                    entry_id: row
                        .try_get::<i64, _>(0)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                    title: row.try_get(1).map_err(|e| StoreError::Database(e.into()))?,
                    topic: row.try_get(2).map_err(|e| StoreError::Database(e.into()))?,
                    trust_source: row.try_get(3).map_err(|e| StoreError::Database(e.into()))?,
                    helpful_count: row
                        .try_get::<i64, _>(4)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u32,
                    unhelpful_count: row
                        .try_get::<i64, _>(5)
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u32,
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Graph edge types (crt-021)
// ---------------------------------------------------------------------------

/// One row from the `graph_edges` table.
///
/// Used by the background tick to load all edges and pass them to
/// `build_typed_relation_graph`. The `metadata` column is not included —
/// it is NULL for all crt-021 writes and reserved for W3-1 GNN use.
#[derive(Debug, Clone)]
pub struct GraphEdgeRow {
    pub source_id: u64,
    pub target_id: u64,
    pub relation_type: String,
    pub weight: f32,
    pub created_at: i64,
    pub created_by: String,
    pub source: String,
    pub bootstrap_only: bool,
}

/// One `Contradicts` edge row from `graph_edges` for a specific target entry.
///
/// Includes the raw `metadata` JSON blob and `source` field needed by the
/// NLI auto-quarantine threshold guard (ADR-007 crt-023). Only `Contradicts`
/// edges targeting the queried entry_id are returned.
#[derive(Debug, Clone)]
pub struct ContradictEdgeRow {
    /// Row primary key.
    pub id: u64,
    /// Entry ID of the source (the entry that contradicts the target).
    pub source_id: u64,
    /// Entry ID of the target (the entry being evaluated for quarantine).
    pub target_id: u64,
    /// Edge source system: `"nli"` for NLI-written edges, `"manual"` otherwise.
    pub source: String,
    /// True when this edge was written during the bootstrap NLI pass only.
    pub bootstrap_only: bool,
    /// Raw JSON metadata blob, e.g. `{"nli_entailment": 0.1, "nli_contradiction": 0.92}`.
    /// `None` when the column is NULL (pre-crt-023 edges).
    pub metadata: Option<String>,
}

// ---------------------------------------------------------------------------
// Public output types
// ---------------------------------------------------------------------------

/// Aggregated status metrics computed via SQL (crt-013: ADR-004).
#[derive(Debug, Clone)]
pub struct StatusAggregates {
    pub supersedes_count: u64,
    pub superseded_by_count: u64,
    pub total_correction_count: u64,
    pub trust_source_distribution: BTreeMap<String, u64>,
    pub unattributed_count: u64,
}

/// Raw effectiveness data aggregated by SQL (crt-018: ADR-001).
#[derive(Debug, Clone)]
pub struct EffectivenessAggregates {
    pub entry_stats: Vec<EntryInjectionStats>,
    pub active_topics: HashSet<String>,
    pub calibration_rows: Vec<(f64, bool)>,
    pub session_count: u32,
    pub earliest_session_at: Option<u64>,
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

/// Metadata about entries needed for classification.
#[derive(Debug, Clone)]
pub struct EntryClassificationMeta {
    pub entry_id: u64,
    pub title: String,
    pub topic: String,
    pub trust_source: String,
    pub helpful_count: u32,
    pub unhelpful_count: u32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::open_test_store;

    /// Create the graph_edges table for tests that run against a pre-v13 schema.
    async fn create_graph_edges_table(pool: &sqlx::sqlite::SqlitePool) {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS graph_edges (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id      INTEGER NOT NULL,
                target_id      INTEGER NOT NULL,
                relation_type  TEXT    NOT NULL,
                weight         REAL    NOT NULL DEFAULT 1.0,
                created_at     INTEGER NOT NULL,
                created_by     TEXT    NOT NULL DEFAULT '',
                source         TEXT    NOT NULL DEFAULT '',
                bootstrap_only INTEGER NOT NULL DEFAULT 0,
                metadata       TEXT    DEFAULT NULL,
                UNIQUE(source_id, target_id, relation_type)
            )",
        )
        .execute(pool)
        .await
        .expect("create graph_edges table");
    }

    #[tokio::test]
    async fn test_query_graph_edges_returns_rows() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        create_graph_edges_table(&store.write_pool).await;

        // Insert two rows directly.
        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (10, 20, 'Supersedes', 1.0, 1000, 'bootstrap', 'entries.supersedes', 0),
                    (30, 40, 'CoAccess',   0.6, 2000, 'bootstrap', 'co_access',          1)",
        )
        .execute(&store.write_pool)
        .await
        .expect("insert rows");

        let rows = store.query_graph_edges().await.expect("query_graph_edges");
        assert_eq!(rows.len(), 2, "expected 2 GraphEdgeRow entries");

        let sup = rows
            .iter()
            .find(|r| r.relation_type == "Supersedes")
            .expect("Supersedes row");
        assert_eq!(sup.source_id, 10);
        assert_eq!(sup.target_id, 20);
        assert!((sup.weight - 1.0_f32).abs() < f32::EPSILON);
        assert_eq!(sup.created_at, 1000);
        assert_eq!(sup.created_by, "bootstrap");
        assert_eq!(sup.source, "entries.supersedes");
        assert!(!sup.bootstrap_only);

        let ca = rows
            .iter()
            .find(|r| r.relation_type == "CoAccess")
            .expect("CoAccess row");
        assert_eq!(ca.source_id, 30);
        assert_eq!(ca.target_id, 40);
        assert!(ca.bootstrap_only);
    }

    #[tokio::test]
    async fn test_query_graph_edges_returns_empty_on_empty_table() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        create_graph_edges_table(&store.write_pool).await;

        let rows = store.query_graph_edges().await.expect("query_graph_edges");
        assert!(rows.is_empty(), "expected empty vec for empty table");
    }

    #[tokio::test]
    async fn test_query_graph_edges_bootstrap_only_bool_mapping() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = open_test_store(&dir).await;
        create_graph_edges_table(&store.write_pool).await;

        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (1, 2, 'Supersedes', 1.0, 0, 'a', 'b', 0),
                    (3, 4, 'CoAccess',   0.5, 0, 'a', 'b', 1)",
        )
        .execute(&store.write_pool)
        .await
        .expect("insert");

        let rows = store.query_graph_edges().await.expect("query");
        let row_false = rows
            .iter()
            .find(|r| r.source_id == 1)
            .expect("row source_id=1");
        let row_true = rows
            .iter()
            .find(|r| r.source_id == 3)
            .expect("row source_id=3");
        assert!(!row_false.bootstrap_only, "0 must map to false");
        assert!(row_true.bootstrap_only, "1 must map to true");
    }
}
