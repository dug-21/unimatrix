# Component: read-paths (Wave 1)

## File: `crates/unimatrix-store/src/read.rs`

**Action**: REWRITE
**Risk**: HIGH (RISK-03 CRITICAL)
**ADR**: N/A (query semantics preservation)

## Purpose

Replace HashSet intersection query pattern with SQL WHERE clause builder. Eliminate N+1 fetch pattern. Use `entry_from_row()` + `load_tags_for_entries()` for all EntryRecord construction.

## Remove: collect_ids_* Functions

Delete all 5 `collect_ids_by_*` functions and `fetch_entries`. These are replaced by the SQL WHERE builder.

## Store::get Rewrite

```rust
pub fn get(&self, entry_id: u64) -> Result<EntryRecord> {
    let conn = self.lock_conn();
    let mut entry: EntryRecord = conn.query_row(
        &format!("SELECT {} FROM entries WHERE id = ?1", ENTRY_COLUMNS),
        rusqlite::params![entry_id as i64],
        entry_from_row,
    ).optional().map_err(StoreError::Sqlite)?
     .ok_or(StoreError::EntryNotFound(entry_id))?;

    // Load tags (C-10: mandatory)
    let tag_map = load_tags_for_entries(&conn, &[entry_id])?;
    if let Some(tags) = tag_map.get(&entry_id) {
        entry.tags = tags.clone();
    }
    Ok(entry)
}
```

## Store::exists (unchanged)

Keep as-is (no deserialization needed).

## Store::query_by_topic Rewrite

```rust
pub fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>> {
    let conn = self.lock_conn();
    let mut stmt = conn.prepare(
        &format!("SELECT {} FROM entries WHERE topic = ?1", ENTRY_COLUMNS)
    ).map_err(StoreError::Sqlite)?;

    let mut entries: Vec<EntryRecord> = stmt.query_map(
        rusqlite::params![topic],
        entry_from_row,
    ).map_err(StoreError::Sqlite)?
     .collect::<rusqlite::Result<Vec<_>>>()
     .map_err(StoreError::Sqlite)?;

    let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
    let tag_map = load_tags_for_entries(&conn, &ids)?;
    apply_tags(&mut entries, &tag_map);

    Ok(entries)
}
```

## Store::query_by_category, query_by_status (same pattern)

Replace index table scan with `WHERE category = ?1` or `WHERE status = ?1`.

## Store::query_by_tags Rewrite

```rust
pub fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>> {
    if tags.is_empty() {
        return Ok(vec![]);
    }
    let conn = self.lock_conn();

    // Build tag subquery: AND semantics via GROUP BY HAVING
    let placeholders: Vec<String> = tags.iter().map(|_| "?".to_string()).collect();
    let sql = format!(
        "SELECT {} FROM entries WHERE id IN (
            SELECT entry_id FROM entry_tags
            WHERE tag IN ({})
            GROUP BY entry_id
            HAVING COUNT(DISTINCT tag) = ?
        )",
        ENTRY_COLUMNS,
        placeholders.join(",")
    );

    let mut stmt = conn.prepare(&sql).map_err(StoreError::Sqlite)?;

    // Build params: tag values + tag count
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = tags.iter()
        .map(|t| Box::new(t.clone()) as Box<dyn rusqlite::types::ToSql>)
        .collect();
    params.push(Box::new(tags.len() as i64));

    let mut entries: Vec<EntryRecord> = stmt.query_map(
        rusqlite::params_from_iter(params.iter()),
        entry_from_row,
    ).map_err(StoreError::Sqlite)?
     .collect::<rusqlite::Result<Vec<_>>>()
     .map_err(StoreError::Sqlite)?;

    let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
    let tag_map = load_tags_for_entries(&conn, &ids)?;
    apply_tags(&mut entries, &tag_map);

    Ok(entries)
}
```

## Store::query_by_time_range Rewrite

```rust
pub fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>> {
    if range.start > range.end {
        return Ok(vec![]);  // Preserve semantic: invalid range -> empty
    }
    let conn = self.lock_conn();
    let mut stmt = conn.prepare(
        &format!("SELECT {} FROM entries WHERE created_at BETWEEN ?1 AND ?2", ENTRY_COLUMNS)
    ).map_err(StoreError::Sqlite)?;

    let mut entries: Vec<EntryRecord> = stmt.query_map(
        rusqlite::params![range.start as i64, range.end as i64],
        entry_from_row,
    ).map_err(StoreError::Sqlite)?
     .collect::<rusqlite::Result<Vec<_>>>()
     .map_err(StoreError::Sqlite)?;

    let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
    let tag_map = load_tags_for_entries(&conn, &ids)?;
    apply_tags(&mut entries, &tag_map);

    Ok(entries)
}
```

## Store::query Rewrite (Combined Filter)

```rust
pub fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>> {
    let conn = self.lock_conn();

    // Determine if all filters empty -> default to Active
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
    let mut param_idx = 1;

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
    if let Some(range) = filter.time_range {
        if range.start <= range.end {
            conditions.push(format!("created_at >= ?{param_idx} AND created_at <= ?{}", param_idx + 1));
            params.push(Box::new(range.start as i64));
            params.push(Box::new(range.end as i64));
            param_idx += 2;
        }
    }

    // Tag subquery (only if tags is Some and non-empty)
    if let Some(ref tags) = filter.tags {
        if !tags.is_empty() {
            let tag_placeholders: Vec<String> = tags.iter().enumerate()
                .map(|(i, _)| format!("?{}", param_idx + i))
                .collect();
            conditions.push(format!(
                "id IN (SELECT entry_id FROM entry_tags WHERE tag IN ({}) GROUP BY entry_id HAVING COUNT(DISTINCT tag) = ?{})",
                tag_placeholders.join(","),
                param_idx + tags.len()
            ));
            for tag in tags {
                params.push(Box::new(tag.clone()));
            }
            params.push(Box::new(tags.len() as i64));
            param_idx += tags.len() + 1;
        }
    }

    // If no conditions at all (shouldn't happen due to effective_status), default to Active
    let where_clause = if conditions.is_empty() {
        "WHERE status = 0".to_string()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!("SELECT {} FROM entries {}", ENTRY_COLUMNS, where_clause);
    let mut stmt = conn.prepare(&sql).map_err(StoreError::Sqlite)?;

    let mut entries: Vec<EntryRecord> = stmt.query_map(
        rusqlite::params_from_iter(params.iter()),
        entry_from_row,
    ).map_err(StoreError::Sqlite)?
     .collect::<rusqlite::Result<Vec<_>>>()
     .map_err(StoreError::Sqlite)?;

    let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
    let tag_map = load_tags_for_entries(&conn, &ids)?;
    apply_tags(&mut entries, &tag_map);

    Ok(entries)
}
```

## Co-Access Reads Rewrite (Wave 2)

### get_co_access_partners

```rust
// Replace blob deserialize with SQL column read
let mut stmt = conn.prepare(
    "SELECT entry_id_b, count, last_updated FROM co_access
     WHERE entry_id_a = ?1 AND last_updated >= ?2"
)?;
// + reverse query on entry_id_b
```

### co_access_stats

```rust
// Direct SQL aggregation
let (total, active): (i64, i64) = conn.query_row(
    "SELECT COUNT(*), SUM(CASE WHEN last_updated >= ?1 THEN 1 ELSE 0 END) FROM co_access",
    rusqlite::params![staleness_cutoff as i64],
    |row| Ok((row.get(0)?, row.get(1)?)),
)?;
```

### top_co_access_pairs

```rust
// SQL ORDER BY + LIMIT
let mut stmt = conn.prepare(
    "SELECT entry_id_a, entry_id_b, count, last_updated FROM co_access
     WHERE last_updated >= ?1
     ORDER BY count DESC
     LIMIT ?2"
)?;
```

## Query Semantics Preservation Checklist

| Semantic | Pre-normalization | Post-normalization | Verified |
|----------|------------------|-------------------|----------|
| Tag AND | HashSet intersection | HAVING COUNT(DISTINCT tag) = N | RT-18,19 |
| Empty filter -> Active | Explicit check | Same check, effective_status | RT-20 |
| Empty tags skip | `&& !tags.is_empty()` | Same guard | RT-21 |
| Invalid time range -> empty | `range.start > range.end` guard | Same guard | RT-22 |
| Multi-filter AND | HashSet intersection | SQL WHERE AND | RT-24 |
| 0-tag entries in non-tag queries | Not excluded (separate fetch) | Not excluded (no JOIN on entry_tags) | RT-26 |
