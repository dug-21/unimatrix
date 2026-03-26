# col-028: Component 4 — Schema Migration v16→v17 (Atomic Change Unit)

**Files** (must all be modified atomically — C-09, SR-01):
- `crates/unimatrix-store/src/migration.rs`
- `crates/unimatrix-store/src/analytics.rs`
- `crates/unimatrix-store/src/query_log.rs`

**Compile-fix cascades** (no semantic change):
- `crates/unimatrix-server/src/uds/listener.rs` (line 1324)
- `crates/unimatrix-server/src/eval/scenarios/tests.rs` (insert_query_log_row helper)
- `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` (make_query_log struct literal)

**SR-02 cascade** (schema version constant updates):
- `crates/unimatrix-store/tests/migration_v15_to_v16.rs`
- `crates/unimatrix-server/src/server.rs` (lines 2059, 2084)

## Why Atomic (C-09)

The `analytics.rs` INSERT uses positional `?1`..`?N` binding. The SELECT statements in
`query_log.rs` use positional column indices in `row_to_query_log`. If any one of the
four sites (INSERT, scan_query_log_by_sessions SELECT, scan_query_log_by_session SELECT,
row_to_query_log) is updated without the others, the result is silent runtime data
corruption — not a compile error. AC-17 is the runtime guard: if any site diverges,
the round-trip write+read test fails.

## migration.rs Changes

### 1. Bump CURRENT_SCHEMA_VERSION

```
// Before:
/// Current schema version. Incremented from 15 to 16 by col-025 ...
pub const CURRENT_SCHEMA_VERSION: u64 = 16;

// After:
/// Current schema version. Incremented from 16 to 17 by col-028 (query_log.phase).
pub const CURRENT_SCHEMA_VERSION: u64 = 17;
```

### 2. Add v16→v17 migration branch in run_main_migrations

Insert after the `if current_version < 16 { ... }` block. Exact SQL and structure
are load-bearing (FR-11, SPECIFICATION.md Exact Signatures):

```
// v16 → v17: query_log.phase column (col-028)
IF current_version < 17:
    // C-02: pragma_table_info pre-check mandatory (SQLite does not support
    // ALTER TABLE ADD COLUMN IF NOT EXISTS).
    let has_phase_column: bool =
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('query_log') WHERE name = 'phase'"
        )
        .fetch_one(&mut **txn)
        .await
        .map(|count| count > 0)
        .unwrap_or(false)

    IF NOT has_phase_column:
        sqlx::query("ALTER TABLE query_log ADD COLUMN phase TEXT")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    // CREATE INDEX IF NOT EXISTS: idempotent even without the pragma pre-check
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_query_log_phase ON query_log (phase)"
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    sqlx::query("UPDATE counters SET value = 17 WHERE name = 'schema_version'")
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?
```

Pre-check pattern: identical to v13→v14 and v15→v16 branches in this file (C-02
established pattern).

## analytics.rs Changes

### AnalyticsWrite::QueryLog variant — add phase field

```
QueryLog {
    session_id: String,
    query_text: String,
    ts: i64,
    result_count: i64,
    result_entry_ids: Option<String>,
    similarity_scores: Option<String>,
    retrieval_mode: Option<String>,
    source: String,
    phase: Option<String>,   // NEW — col-028
}
```

### SQL INSERT — add phase as column 9 (?9)

The match arm for `AnalyticsWrite::QueryLog` in the drain task:

```
// Before:
"INSERT INTO query_log
    (session_id, query_text, ts, result_count,
     result_entry_ids, similarity_scores, retrieval_mode, source)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
+ eight .bind(...) calls

// After:
"INSERT INTO query_log
    (session_id, query_text, ts, result_count,
     result_entry_ids, similarity_scores, retrieval_mode, source, phase)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
+ eight existing .bind(...) calls UNCHANGED
+ .bind(phase)   // ninth bind — appended last (C-05)
```

The match arm destructuring must include `phase`:
```
AnalyticsWrite::QueryLog {
    session_id,
    query_text,
    ts,
    result_count,
    result_entry_ids,
    similarity_scores,
    retrieval_mode,
    source,
    phase,           // NEW
} => { ... }
```

## query_log.rs Changes

All changes in this file are part of the same atomic unit. Do NOT split across commits.

### QueryLogRecord struct — add phase field

```rust
pub struct QueryLogRecord {
    pub query_id: i64,
    pub session_id: String,
    pub query_text: String,
    pub ts: u64,
    pub result_count: i64,
    pub result_entry_ids: String,
    pub similarity_scores: String,
    pub retrieval_mode: String,
    pub source: String,
    pub phase: Option<String>,  // col-028: workflow phase at query time; None for UDS rows
}
```

### QueryLogRecord::new — add phase parameter (final position)

```
FUNCTION new(
    session_id: String,
    query_text: String,
    entry_ids: &[u64],
    similarity_scores: &[f64],
    retrieval_mode: &str,
    source: &str,
    phase: Option<String>,   // NEW — col-028; final parameter
) -> Self:
    let now = SystemTime::now() ...

    QueryLogRecord {
        query_id: 0,
        session_id,
        query_text,
        ts: now,
        result_count: entry_ids.len() as i64,
        result_entry_ids: serde_json::to_string(entry_ids).unwrap_or_default(),
        similarity_scores: serde_json::to_string(similarity_scores).unwrap_or_default(),
        retrieval_mode: retrieval_mode.to_string(),
        source: source.to_string(),
        phase,   // direct assignment; no computation
    }
```

### insert_query_log — pass phase through to variant

```
FUNCTION insert_query_log(&self, record: &QueryLogRecord):
    self.enqueue_analytics(AnalyticsWrite::QueryLog {
        session_id: record.session_id.clone(),
        query_text: record.query_text.clone(),
        ts: record.ts as i64,
        result_count: record.result_count,
        result_entry_ids: ...,
        similarity_scores: ...,
        retrieval_mode: ...,
        source: record.source.clone(),
        phase: record.phase.clone(),   // NEW — col-028
    })
```

### scan_query_log_by_sessions SELECT — add phase as 10th column

```sql
-- Before (column indices 0..8):
SELECT query_id, session_id, query_text, ts, result_count,
       result_entry_ids, similarity_scores, retrieval_mode, source
FROM query_log
WHERE session_id IN ({placeholders})
ORDER BY ts ASC

-- After (column indices 0..9):
SELECT query_id, session_id, query_text, ts, result_count,
       result_entry_ids, similarity_scores, retrieval_mode, source, phase
FROM query_log
WHERE session_id IN ({placeholders})
ORDER BY ts ASC
```

### scan_query_log_by_session SELECT — add phase as 10th column

```sql
-- Before:
SELECT query_id, session_id, query_text, ts, result_count,
       result_entry_ids, similarity_scores, retrieval_mode, source
FROM query_log
WHERE session_id = ?1
ORDER BY ts ASC

-- After:
SELECT query_id, session_id, query_text, ts, result_count,
       result_entry_ids, similarity_scores, retrieval_mode, source, phase
FROM query_log
WHERE session_id = ?1
ORDER BY ts ASC
```

### row_to_query_log — read index 9 as Option<String>

```
FUNCTION row_to_query_log(row) -> Result<QueryLogRecord>:
    Ok(QueryLogRecord {
        query_id: row.try_get(0)?,           // unchanged
        session_id: row.try_get(1)?,         // unchanged
        query_text: row.try_get(2)?,         // unchanged
        ts: row.try_get::<i64, _>(3)? as u64, // unchanged
        result_count: row.try_get(4)?,       // unchanged
        result_entry_ids: row.try_get::<Option<String>, _>(5)?.unwrap_or_default(), // unchanged
        similarity_scores: row.try_get::<Option<String>, _>(6)?.unwrap_or_default(), // unchanged
        retrieval_mode: row.try_get::<Option<String>, _>(7)?.unwrap_or_default(),  // unchanged
        source: row.try_get(8)?,             // unchanged; NOTE: source is at index 8, NOT 9
        phase: row.try_get::<Option<String>, _>(9)   // NEW — col-028
                  .map_err(|e| StoreError::Database(e.into()))?,
    })
```

FM-04 guard note: `source` is at index 8 and `phase` is at index 9. If `source` were
mistakenly read at index 9 (the pre-existing off-by-one direction), the type mismatch
would not always be caught at compile time — AC-17 round-trip test is the guard.

## New Test File: migration_v16_to_v17.rs

Create `crates/unimatrix-store/tests/migration_v16_to_v17.rs` following the pattern
of `migration_v15_to_v16.rs`.

Six required test functions (AC-19):

**T-V17-01: fresh_db_initialises_at_v17**
```
SETUP: open_test_store (fresh database)
ASSERT: schema_version counter = 17
ASSERT: query_log table has a 'phase' column
  (query pragma_table_info('query_log'), filter name='phase', assert count > 0)
```

**T-V17-02: v16_to_v17_migration_adds_phase_column**
```
SETUP: open a v16 fixture database (database created at schema_version=16 without
       the phase column). Pattern: use the test helper that creates a pre-migration
       DB, same as migration_v15_to_v16.rs uses for its v15 fixture.
RUN: call migrate_if_needed (opens the DB, runs migrations)
ASSERT: query_log table has a 'phase' column
ASSERT: schema_version counter = 17
```

**T-V17-03: idx_query_log_phase_index_exists**
```
SETUP: open_test_store (fresh database)
ASSERT: idx_query_log_phase index exists
  (query sqlite_master WHERE type='index' AND name='idx_query_log_phase',
   assert count > 0)
```

**T-V17-04: migration_is_idempotent**
```
SETUP: open_test_store (already at v17)
RUN: call migrate_if_needed again on the same database path
ASSERT: no error returned
ASSERT: schema_version counter still = 17
ASSERT: only one 'phase' column in pragma_table_info (no duplicate)
```

**T-V17-05: pre_existing_rows_read_back_with_phase_none**
```
SETUP: create a v16 database, insert a query_log row WITHOUT phase column
RUN: migrate to v17
RUN: scan_query_log_by_session for the test session
ASSERT: the row's phase field = None (not a panic, not an error)
```

**T-V17-06: schema_version_is_17_after_migration**
```
SETUP: v16 fixture database
RUN: migrate_if_needed
ASSERT: SELECT value FROM counters WHERE name='schema_version' = 17
```

## SR-02 Cascade: Existing Test File Updates

### migration_v15_to_v16.rs

All `assert_eq!(..., 16)` become `assert_eq!(..., 17)`.
Function `test_current_schema_version_is_16` renamed to `test_current_schema_version_is_17`.
Inline comments referencing "version 16" updated to "version 17".

Pattern: this same cascade happens every time `CURRENT_SCHEMA_VERSION` is bumped. See
AC-22: before gate, run `grep -r 'schema_version.*== 16' crates/` — must return zero.

### migration_v14_to_v15.rs

The `>= 15` style assertions already tolerate version bumps (pattern #2933). Confirm no
`== 16` assertions are present. Update any inline comments referencing "bumped to 16".

## Error Handling

### migration.rs errors
- `pragma_table_info` query: `.unwrap_or(false)` — if the query fails unexpectedly,
  the migration proceeds as if the column is absent and attempts the ALTER TABLE. If the
  column exists and ALTER TABLE is attempted, SQLite returns an error that maps to
  `StoreError::Migration`. The `unwrap_or(false)` path is therefore a safe degradation
  for the idempotency case — but in practice should never fail on a valid SQLite database.
- `ALTER TABLE`, `CREATE INDEX`, `UPDATE counters`: all propagate via `?` as
  `StoreError::Migration { source: Box::new(e) }`.

### analytics.rs errors
- The drain task is fire-and-forget. INSERT errors are logged via `tracing::warn!` but
  not propagated. The `phase` bind follows the same error contract as the existing eight
  binds — no new error conditions.

### query_log.rs errors
- `row.try_get::<Option<String>, _>(9)`: if index 9 does not exist (e.g., old SELECT
  without the phase column), this returns a sqlx error that maps to
  `StoreError::Database`. This is the AC-17 guard: if the SELECT column list is missing
  `phase`, the read-back fails loudly.

## Key Test Scenarios

**AC-13** — `CURRENT_SCHEMA_VERSION == 17`.
  - Unit test (or inline assertion in migration.rs `#[cfg(test)]`):
    `assert_eq!(CURRENT_SCHEMA_VERSION, 17)`.

**AC-14** — Fresh DB at v17 has `phase` column and `idx_query_log_phase`.
  - T-V17-01, T-V17-03.

**AC-15** — Migration is idempotent.
  - T-V17-04.

**AC-17 (SR-01 guard — critical)** — Round-trip write+read:
  1. Write `QueryLogRecord::new(..., phase: Some("design".to_string()))`.
  2. Call `store.insert_query_log(&record)`.
  3. Flush the analytics drain (real drain, not a mock — pattern #3004).
  4. Call `store.scan_query_log_by_session(session_id)`.
  5. Assert `records[0].phase == Some("design")`.
  - If analytics INSERT is missing `?9`, phase is NULL; read-back returns None.
  - If SELECT is missing `phase`, read fails with column-index error.
  - If `row_to_query_log` reads index 8 (source), read returns `Some("mcp")`.
  All three failure modes are caught by this one test.

**AC-18** — Pre-existing rows read back with `phase = None`.
  - T-V17-05.

**AC-19** — All six T-V17-* tests pass.

**EC-06** — Phase string with unusual characters round-trips cleanly.
  - Write with `phase: Some("design/v2".to_string())`.
  - Read back. Assert `Some("design/v2")`.
  - Verifies SQLx parameterized binding handles non-trivial strings.

## Out of Scope

- Backfill of historical `query_log` rows (pre-existing rows get `phase = NULL`).
- Any consumer of `query_log.phase` (ass-032 Loop 2 is a separate feature).
- Changes to any other table in the schema.
- UDS phase semantics (uds/listener.rs:1324 passes `None` — compile fix only).
