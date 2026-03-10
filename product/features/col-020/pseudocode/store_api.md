# C4: Store API Extensions (unimatrix-store)

## Purpose

Add batch query methods for multi-session data loading and an absolute counter setter for idempotent topic_deliveries updates.

## New Method: scan_query_log_by_sessions (query_log.rs)

```
pub fn scan_query_log_by_sessions(&self, session_ids: &[&str]) -> Result<Vec<QueryLogRecord>>
```

### Algorithm

```
function scan_query_log_by_sessions(session_ids):
    if session_ids is empty:
        return Ok(empty vec)

    let conn = self.lock_conn()
    let mut all_results: Vec<QueryLogRecord> = empty

    // Chunk into batches of 50 to avoid large IN clauses (R-11)
    for chunk in session_ids.chunks(50):
        // Build parameterized IN clause
        let placeholders = chunk.iter().enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .join(",")

        let sql = format!(
            "SELECT query_id, session_id, query_text, ts, result_count,
                    result_entry_ids, similarity_scores, retrieval_mode, source
             FROM query_log
             WHERE session_id IN ({placeholders})
             ORDER BY ts ASC"
        )

        let stmt = conn.prepare(&sql)?
        let params: Vec<Box<dyn ToSql>> = chunk.iter()
            .map(|id| Box::new(id.to_string()) as Box<dyn ToSql>)
            .collect()

        let rows = stmt.query_map(params_from_iter(params.iter()), row_to_query_log)?
        for row in rows:
            all_results.push(row?)

    return Ok(all_results)
```

## New Method: scan_injection_log_by_sessions (injection_log.rs)

```
pub fn scan_injection_log_by_sessions(&self, session_ids: &[&str]) -> Result<Vec<InjectionLogRecord>>
```

### Algorithm

```
function scan_injection_log_by_sessions(session_ids):
    if session_ids is empty:
        return Ok(empty vec)

    let conn = self.lock_conn()
    let mut all_results: Vec<InjectionLogRecord> = empty

    // Chunk into batches of 50
    for chunk in session_ids.chunks(50):
        let placeholders = chunk.iter().enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .join(",")

        let sql = format!(
            "SELECT log_id, session_id, entry_id, confidence, timestamp
             FROM injection_log
             WHERE session_id IN ({placeholders})
             ORDER BY log_id"
        )

        let stmt = conn.prepare(&sql)?
        let params: Vec<Box<dyn ToSql>> = chunk.iter()
            .map(|id| Box::new(id.to_string()) as Box<dyn ToSql>)
            .collect()

        let rows = stmt.query_map(params_from_iter(params.iter()), |row| {
            Ok(InjectionLogRecord {
                log_id: row.get::<_, i64>("log_id")? as u64,
                session_id: row.get("session_id")?,
                entry_id: row.get::<_, i64>("entry_id")? as u64,
                confidence: row.get("confidence")?,
                timestamp: row.get::<_, i64>("timestamp")? as u64,
            })
        })?

        for row in rows:
            all_results.push(row?)

    return Ok(all_results)
```

## New Method: count_active_entries_by_category (read.rs)

```
pub fn count_active_entries_by_category(&self) -> Result<HashMap<String, u64>>
```

### Algorithm

```
function count_active_entries_by_category():
    let conn = self.lock_conn()

    let sql = "SELECT category, COUNT(*) FROM entries
               WHERE status = 0
               GROUP BY category"
    // status = 0 is Status::Active

    let stmt = conn.prepare(sql)?
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
    })?

    let mut result: HashMap<String, u64> = empty
    for row in rows:
        let (category, count) = row?
        result.insert(category, count)

    return Ok(result)
```

## New Method: set_topic_delivery_counters (topic_deliveries.rs)

```
pub fn set_topic_delivery_counters(
    &self,
    topic: &str,
    total_sessions: i64,
    total_tool_calls: i64,
    total_duration_secs: i64,
) -> Result<()>
```

### Algorithm (ADR-002: absolute-set, not additive)

```
function set_topic_delivery_counters(topic, total_sessions, total_tool_calls, total_duration_secs):
    let conn = self.lock_conn()

    let rows_affected = conn.execute(
        "UPDATE topic_deliveries
         SET total_sessions = ?1,
             total_tool_calls = ?2,
             total_duration_secs = ?3
         WHERE topic = ?4",
        params![total_sessions, total_tool_calls, total_duration_secs, topic]
    )?

    if rows_affected == 0:
        return Err(StoreError::Deserialization(
            format!("topic_delivery not found: {topic}")
        ))

    return Ok(())
```

Note: The handler ensures the topic_deliveries record exists (via `upsert_topic_delivery`) before calling this. The error path handles the case where the record was deleted between upsert and set.

## Error Handling

- All methods return `Result<T>` with `StoreError::Sqlite` for database errors.
- Empty session_ids input: return Ok(empty vec) immediately, no SQL executed.
- set_topic_delivery_counters with nonexistent topic: return error (same pattern as existing `update_topic_delivery_counters`).
- SQL parameter binding uses `rusqlite::params_from_iter` for dynamic IN clauses (same pattern as `load_tags_for_entries` in read.rs).

## Key Test Scenarios

### scan_query_log_by_sessions
1. **Multiple sessions**: Insert records for sessions A, B, C. Query with [A, B]. Verify only A and B records returned.
2. **Empty session list (R-11)**: Returns empty vec, no SQL error.
3. **Nonexistent sessions**: Returns empty vec.
4. **Single session**: Behaves identically to existing `scan_query_log_by_session`.
5. **Ordering**: Results ordered by ts ascending within each batch.

### scan_injection_log_by_sessions
1. **Multiple sessions**: Insert injection records for sessions A, B. Query with [A, B]. Verify both returned.
2. **Empty session list**: Returns empty vec.
3. **Nonexistent sessions**: Returns empty vec.

### count_active_entries_by_category
1. **Mixed statuses**: Active entries counted; Deprecated/Quarantined excluded.
2. **Multiple categories**: Returns correct count per category.
3. **Empty database**: Returns empty HashMap.

### set_topic_delivery_counters
1. **Idempotent (R-05, AC-12)**: Set counters to (5, 100, 3600). Read back. Set again to same values. Read back. Values unchanged.
2. **Overwrite (R-05)**: Set counters to (5, 100, 3600), then set to (3, 50, 1800). Second values are what is read back.
3. **Nonexistent topic**: Returns error (same as update_topic_delivery_counters).
4. **Does not touch non-counter fields**: Set counters, verify status/github_issue/phases_completed unchanged.
