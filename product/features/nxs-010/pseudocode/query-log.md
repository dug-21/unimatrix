# C4: query_log Module (query_log.rs)

## Purpose

Define `QueryLogRecord` struct, Store methods for insert and scan, and a shared constructor function ensuring field parity between UDS and MCP paths (FR-08.1).

## New File

`crates/unimatrix-store/src/query_log.rs`

## Struct: QueryLogRecord

```
pub struct QueryLogRecord {
    pub query_id: i64,            // 0 on insert; AUTOINCREMENT allocates
    pub session_id: String,       // TEXT NOT NULL
    pub query_text: String,       // TEXT NOT NULL
    pub ts: u64,                  // unix timestamp
    pub result_count: i64,        // INTEGER NOT NULL
    pub result_entry_ids: String, // JSON array of u64
    pub similarity_scores: String,// JSON array of f64
    pub retrieval_mode: String,   // "strict" or "flexible"
    pub source: String,           // "uds" or "mcp"
}
```

**Note on result_count type**: The SPECIFICATION FR-05.1 says `result_count: i64`. The IMPLEMENTATION-BRIEF data structure says `result_count: u32`. Use `i64` to match the specification and SQLite INTEGER type directly, avoiding unnecessary casts.

## Shared Constructor: QueryLogRecord::new (FR-08.1)

```
impl QueryLogRecord {
    pub fn new(
        session_id: String,
        query_text: String,
        entry_ids: &[u64],
        similarity_scores: &[f64],
        retrieval_mode: &str,
        source: &str,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        QueryLogRecord {
            query_id: 0,  // ignored on insert; AUTOINCREMENT allocates
            session_id,
            query_text,
            ts: now,
            result_count: entry_ids.len() as i64,
            result_entry_ids: serde_json::to_string(entry_ids).unwrap_or_default(),
            similarity_scores: serde_json::to_string(similarity_scores).unwrap_or_default(),
            retrieval_mode: retrieval_mode.to_string(),
            source: source.to_string(),
        }
    }
}
```

**Why a constructor**: Both UDS and MCP paths must produce identical field populations. A shared constructor eliminates field divergence risk (R-05, SR-07). Both callers pass in the same shape of data (entry IDs, scores, mode, source) and the constructor handles timestamp, JSON serialization, and result_count derivation.

**Error handling for JSON serialization**: `serde_json::to_string` on `Vec<u64>` and `Vec<f64>` cannot fail for normal numeric values. `unwrap_or_default()` produces `""` only if serde_json itself is broken. This matches the pattern documented in NFR-06.

## Store Methods (impl Store)

### insert_query_log

```
pub fn insert_query_log(&self, record: &QueryLogRecord) -> Result<()>
```

**Pseudocode**:
```
fn insert_query_log(self, record):
    conn = self.lock_conn()
    conn.execute(
        "INSERT INTO query_log
            (session_id, query_text, ts, result_count,
             result_entry_ids, similarity_scores, retrieval_mode, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            record.session_id,
            record.query_text,
            record.ts as i64,
            record.result_count,
            record.result_entry_ids,
            record.similarity_scores,
            record.retrieval_mode,
            record.source,
        ]
    )?;
    Ok(())
```

**Note**: `query_id` is omitted from the INSERT column list. SQLite AUTOINCREMENT allocates it. The `query_id` field in the record struct is ignored. (ADR-001)

**Error handling**: Propagates via `StoreError::Sqlite`. Callers (fire-and-forget paths) catch this error, log a warning, and discard it. (ADR-002)

### scan_query_log_by_session

```
pub fn scan_query_log_by_session(&self, session_id: &str) -> Result<Vec<QueryLogRecord>>
```

**Pseudocode**:
```
fn scan_query_log_by_session(self, session_id):
    conn = self.lock_conn()
    stmt = conn.prepare(
        "SELECT query_id, session_id, query_text, ts, result_count,
                result_entry_ids, similarity_scores, retrieval_mode, source
         FROM query_log
         WHERE session_id = ?1
         ORDER BY ts ASC"
    )?;

    rows = stmt.query_map(params![session_id], |row| {
        Ok(QueryLogRecord {
            query_id: row.get(0)?,
            session_id: row.get(1)?,
            query_text: row.get(2)?,
            ts: row.get::<_, i64>(3)? as u64,
            result_count: row.get(4)?,
            result_entry_ids: row.get(5)?,
            similarity_scores: row.get(6)?,
            retrieval_mode: row.get(7)?,
            source: row.get(8)?,
        })
    })?;

    collect rows into Vec, propagate errors
    Ok(results)
```

**Ordering**: `ORDER BY ts ASC` per FR-05.3. Uses the `idx_query_log_ts` index for efficient ordering within a session (after `idx_query_log_session` filters rows).

**Error handling**: Propagates row-level and statement-level errors.

## Row-reading helper

Extract a private helper for the column-to-struct mapping:

```
fn row_to_query_log(row: &Row) -> rusqlite::Result<QueryLogRecord>
```

Used by `scan_query_log_by_session`. Only one method reads rows currently, but this keeps the pattern consistent with `topic_deliveries.rs`.

## Key Test Scenarios

1. **Insert + read round-trip**: Insert a QueryLogRecord via `insert_query_log` with query_id=0. Read back via `scan_query_log_by_session`. Verify query_id > 0 (auto-allocated). Verify all other fields match. (AC-10)

2. **AUTOINCREMENT monotonic**: Insert 3 rows. Verify each gets a unique query_id, each greater than the last. (R-03)

3. **Scan by session ordering**: Insert 3 rows for the same session_id with ts values 300, 100, 200. Scan. Verify returned in order 100, 200, 300. (AC-11)

4. **Scan for nonexistent session**: Call scan with session_id that has no rows. Verify empty Vec returned. (R-12)

5. **Scan cross-session isolation**: Insert rows for session-A and session-B. Scan for session-A. Verify only session-A rows returned. (R-12)

6. **JSON round-trip -- empty results**: Insert with entry_ids=[] and scores=[]. Read back, deserialize result_entry_ids as Vec<u64>, verify empty. (R-06, AC-14)

7. **JSON round-trip -- multi-element**: Insert with entry_ids=[1,2,3] and scores=[0.9,0.8,0.7]. Read back, deserialize both. Verify lengths match result_count. (AC-15)

8. **JSON round-trip -- edge values**: Insert with scores containing 0.0 and 1.0. Verify preserved after round-trip. (R-06)

9. **Shared constructor field parity**: Create a QueryLogRecord via `QueryLogRecord::new(...)`. Verify result_count equals entry_ids.len(). Verify ts is recent (within last 5 seconds). Verify result_entry_ids and similarity_scores are valid JSON arrays. (FR-08.1, R-05)
