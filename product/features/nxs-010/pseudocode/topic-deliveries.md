# C3: topic_deliveries Module (topic_deliveries.rs)

## Purpose

Define `TopicDeliveryRecord` struct and implement Store CRUD methods for the `topic_deliveries` table. Consumed by col-017 (upsert), col-020 (read + update counters), and crt-018 (read).

## New File

`crates/unimatrix-store/src/topic_deliveries.rs`

## Struct: TopicDeliveryRecord

```
pub struct TopicDeliveryRecord {
    pub topic: String,
    pub created_at: u64,
    pub completed_at: Option<u64>,
    pub status: String,
    pub github_issue: Option<i64>,
    pub total_sessions: i64,
    pub total_tool_calls: i64,
    pub total_duration_secs: i64,
    pub phases_completed: Option<String>,
}
```

No derives needed beyond Debug, Clone. Not serialized via serde -- only SQL marshalling.

## Store Methods (impl Store)

### upsert_topic_delivery

```
pub fn upsert_topic_delivery(&self, record: &TopicDeliveryRecord) -> Result<()>
```

**Pseudocode**:
```
fn upsert_topic_delivery(self, record):
    conn = self.lock_conn()
    conn.execute(
        "INSERT OR REPLACE INTO topic_deliveries
            (topic, created_at, completed_at, status, github_issue,
             total_sessions, total_tool_calls, total_duration_secs, phases_completed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            record.topic,
            record.created_at as i64,
            record.completed_at.map(|v| v as i64),
            record.status,
            record.github_issue,
            record.total_sessions,
            record.total_tool_calls,
            record.total_duration_secs,
            record.phases_completed,
        ]
    )?;
    Ok(())
```

**Note on INSERT OR REPLACE**: This is a full-row replace. It will overwrite counter values. R-10 documents this as expected behavior. Callers must not call upsert concurrently with update_counters for the same topic.

**Error handling**: Propagates rusqlite errors via `StoreError::Sqlite`.

### get_topic_delivery

```
pub fn get_topic_delivery(&self, topic: &str) -> Result<Option<TopicDeliveryRecord>>
```

**Pseudocode**:
```
fn get_topic_delivery(self, topic):
    conn = self.lock_conn()
    result = conn.query_row(
        "SELECT topic, created_at, completed_at, status, github_issue,
                total_sessions, total_tool_calls, total_duration_secs, phases_completed
         FROM topic_deliveries WHERE topic = ?1",
        params![topic],
        |row| {
            Ok(TopicDeliveryRecord {
                topic: row.get(0)?,
                created_at: row.get::<_, i64>(1)? as u64,
                completed_at: row.get::<_, Option<i64>>(2)?.map(|v| v as u64),
                status: row.get(3)?,
                github_issue: row.get(4)?,
                total_sessions: row.get(5)?,
                total_tool_calls: row.get(6)?,
                total_duration_secs: row.get(7)?,
                phases_completed: row.get(8)?,
            })
        }
    ).optional()?;
    Ok(result)
```

**Error handling**: Uses `.optional()` to convert `QueryReturnedNoRows` into `None`. Other errors propagate.

### update_topic_delivery_counters

```
pub fn update_topic_delivery_counters(
    &self,
    topic: &str,
    sessions_delta: i64,
    tool_calls_delta: i64,
    duration_delta: i64,
) -> Result<()>
```

**Pseudocode**:
```
fn update_topic_delivery_counters(self, topic, sessions_delta, tool_calls_delta, duration_delta):
    conn = self.lock_conn()
    rows_affected = conn.execute(
        "UPDATE topic_deliveries
         SET total_sessions = total_sessions + ?1,
             total_tool_calls = total_tool_calls + ?2,
             total_duration_secs = total_duration_secs + ?3
         WHERE topic = ?4",
        params![sessions_delta, tool_calls_delta, duration_delta, topic]
    )?;

    if rows_affected == 0 {
        return Err(StoreError::Deserialization(
            format!("topic_delivery not found: {}", topic)
        ));
    }
    Ok(())
```

**Critical**: Returns an error when 0 rows are affected (topic does not exist). This prevents silent failures when col-020 tries to update counters for a topic that was never created. (R-07)

**Error handling**: Uses `StoreError::Deserialization` with a descriptive message for the missing-topic case. The existing `StoreError` enum has no generic `NotFound` variant -- only `EntryNotFound(u64)` for entry-specific lookups. Using `Deserialization(String)` is the established pattern for custom error messages in this codebase (see migration error paths). An alternative is to add a new `TopicNotFound(String)` variant to `StoreError`, which would be cleaner but requires touching `error.rs` and its `Display` impl. The implementation agent should decide based on whether downstream consumers need to match on this error type specifically.

### list_topic_deliveries

```
pub fn list_topic_deliveries(&self) -> Result<Vec<TopicDeliveryRecord>>
```

**Pseudocode**:
```
fn list_topic_deliveries(self):
    conn = self.lock_conn()
    stmt = conn.prepare(
        "SELECT topic, created_at, completed_at, status, github_issue,
                total_sessions, total_tool_calls, total_duration_secs, phases_completed
         FROM topic_deliveries
         ORDER BY created_at DESC"
    )?;

    rows = stmt.query_map([], |row| {
        Ok(TopicDeliveryRecord {
            topic: row.get(0)?,
            created_at: row.get::<_, i64>(1)? as u64,
            completed_at: row.get::<_, Option<i64>>(2)?.map(|v| v as u64),
            status: row.get(3)?,
            github_issue: row.get(4)?,
            total_sessions: row.get(5)?,
            total_tool_calls: row.get(6)?,
            total_duration_secs: row.get(7)?,
            phases_completed: row.get(8)?,
        })
    })?;

    collect rows into Vec, propagate errors
    Ok(results)
```

**Error handling**: Propagates row-level and statement-level errors via `StoreError::Sqlite`.

## Row-reading helper

All four methods share the same column-to-struct mapping. Extract a private helper:

```
fn row_to_topic_delivery(row: &Row) -> rusqlite::Result<TopicDeliveryRecord>
```

This keeps `get_topic_delivery` and `list_topic_deliveries` DRY. The helper reads columns by index (0-8) matching the SELECT column order.

## Key Test Scenarios

1. **Upsert + get round-trip**: Insert a TopicDeliveryRecord, read it back, verify all fields match. (AC-07)

2. **Get nonexistent**: Call get_topic_delivery("nonexistent"). Verify returns None. (AC-08)

3. **Upsert replaces**: Insert a record, upsert with changed status and github_issue. Verify updated fields. (AC-07)

4. **Counter update**: Insert record with total_sessions=5, total_tool_calls=10, total_duration_secs=3600. Call update_counters(+3, +5, +1800). Verify 8, 15, 5400. (AC-09)

5. **Counter update on nonexistent topic**: Call update_counters for a topic that does not exist. Verify error returned. (R-07)

6. **Counter update with negative deltas**: Insert record with total_sessions=5. Call update_counters(-2, 0, 0). Verify total_sessions=3. (R-07 scenario 3)

7. **List ordering**: Insert 3 records with different created_at values. Call list. Verify returned in created_at DESC order. (FR-04.5)

8. **List empty table**: Call list on empty table. Verify empty Vec returned.

9. **Replace semantics (R-10)**: Insert with total_sessions=5. Upsert same topic with total_sessions=0. Verify total_sessions is 0 (replaced, not accumulated).
