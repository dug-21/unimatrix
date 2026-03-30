# Pseudocode: cycle_review_index.rs

## Purpose

New module owning all read/write operations for `cycle_review_index` table.
Implemented as `impl SqlxStore` methods, following the pattern of
`observations.rs`, `sessions.rs`, and `query_log.rs`.

Separated from `read.rs`/`write.rs` because `CYCLE_REVIEW_INDEX` is a keyed
archive (memoization store), not entry CRUD or analytics. This also prevents
`write.rs` from growing beyond the 500-line guideline (C-10, NFR-08).

## Location

`crates/unimatrix-store/src/cycle_review_index.rs`

Must be declared as `pub mod cycle_review_index;` in `lib.rs` and its public
symbols re-exported alongside other public store types.

## Structs

### CycleReviewRecord

```
#[derive(Debug, Clone)]
pub struct CycleReviewRecord {
    pub feature_cycle:         String  -- PK; matches cycle_events.cycle_id
    pub schema_version:        u32     -- SUMMARY_SCHEMA_VERSION at compute time
    pub computed_at:           i64     -- unix timestamp seconds
    pub raw_signals_available: i32     -- sqlx INTEGER: 1=live signals, 0=purged
    pub summary_json:          String  -- full RetrospectiveReport JSON
}
```

`raw_signals_available` is `i32` (not `bool`) to match sqlx's SQLite INTEGER
binding. Consumers that need bool semantics use `record.raw_signals_available != 0`.
The `store_cycle_review` call site always passes `1i32` (first-call path) since
signals are live; `0i32` is only set on a hypothetical explicit update (GH #409
scope, not crt-033 scope). The `pending_cycle_reviews` response note and the
purged-signals handler path read `raw_signals_available` from the returned record
as-is and report it to callers.

## Constants

```
/// Unified schema version covering both RetrospectiveReport serialization format
/// and hotspot detection rule logic. Defined here only — no import from
/// unimatrix-observe, no definition in tools.rs (C-04, FR-12, ADR-002).
///
/// Bump policy:
///   - Bump when any field on RetrospectiveReport or a nested type changes JSON
///     round-trip fidelity (add, remove, rename).
///   - Bump when any hotspot detection rule in unimatrix-observe changes logic.
///   - Do NOT bump for threshold-only changes that leave stored results valid.
pub const SUMMARY_SCHEMA_VERSION: u32 = 1;

/// 4MB ceiling for stored summary_json (NFR-03).
const SUMMARY_JSON_MAX_BYTES: usize = 4 * 1024 * 1024;
```

## Functions

### get_cycle_review

```
/// Look up a stored cycle review by feature_cycle.
///
/// Uses read_pool() — read-only query, no write contention (entry #3619).
/// Returns None if no row exists for the given feature_cycle.
/// Returns Err only on genuine SQL infrastructure failure.
///
/// On Err at the call site: treat as a cache miss (fall through to full
/// pipeline computation). Do NOT abort the handler on a read failure.
pub async fn get_cycle_review(
    &self,
    feature_cycle: &str,
) -> Result<Option<CycleReviewRecord>>

BODY:
    query:
        SELECT feature_cycle, schema_version, computed_at,
               raw_signals_available, summary_json
        FROM cycle_review_index
        WHERE feature_cycle = ?1

    execute against self.read_pool()
    .fetch_optional(...)
    .await
    .map_err(|e| StoreError::Database(e.into()))?

    match row:
        None  → return Ok(None)
        Some(row) →
            return Ok(Some(CycleReviewRecord {
                feature_cycle:         row.get::<String, _>(0),
                schema_version:        row.get::<i64, _>(1) as u32,
                computed_at:           row.get::<i64, _>(2),
                raw_signals_available: row.get::<i32, _>(3),
                summary_json:          row.get::<String, _>(4),
            }))
```

### store_cycle_review

```
/// Write or overwrite a cycle review record.
///
/// Uses write_pool_server() directly in the caller's async context.
/// MUST NOT be called from spawn_blocking — sqlx async queries require an
/// async context; block_in_place risks pool starvation (ADR-001, #2266, #2249).
///
/// Uses INSERT OR REPLACE to support both first-call writes and force=true
/// overwrites (FR-03, FR-04).
///
/// Enforces the 4MB ceiling on summary_json before any DB call (NFR-03).
/// Returns Err (not panic) when the ceiling is exceeded.
pub async fn store_cycle_review(
    &self,
    record: &CycleReviewRecord,
) -> Result<()>

BODY:
    // 4MB ceiling check (NFR-03). Return Err, not panic.
    if record.summary_json.len() > SUMMARY_JSON_MAX_BYTES:
        return Err(StoreError::InvalidInput {
            field: "summary_json".to_string(),
            reason: format!(
                "summary_json exceeds 4MB ceiling ({} bytes)",
                record.summary_json.len()
            ),
        })

    // Acquire write connection from write_pool_server().
    // This is a direct pool acquire — not spawn_blocking, not block_in_place.
    // The handler's async context drives the await (ADR-001).
    let mut conn = self
        .write_pool_server()
        .acquire()
        .await
        .map_err(|e| StoreError::Database(e.into()))?

    sqlx::query(
        "INSERT OR REPLACE INTO cycle_review_index
             (feature_cycle, schema_version, computed_at,
              raw_signals_available, summary_json)
         VALUES (?1, ?2, ?3, ?4, ?5)"
    )
    .bind(&record.feature_cycle)
    .bind(record.schema_version as i64)
    .bind(record.computed_at)
    .bind(record.raw_signals_available)
    .bind(&record.summary_json)
    .execute(&mut *conn)
    .await
    .map_err(|e| StoreError::Database(e.into()))?

    Ok(())
```

### pending_cycle_reviews

```
/// Return cycle IDs that have a cycle_start event in the K-window but no
/// stored review in cycle_review_index.
///
/// Uses read_pool() — read-only set-difference query (ADR-004, entry #3619).
/// Pre-cycle_events cycles (no cycle_events rows) are excluded by definition.
/// The SELECT DISTINCT prevents duplicates when multiple cycle_start events
/// exist for the same cycle_id (RISK-TEST-STRATEGY edge case).
///
/// k_window_cutoff: unix timestamp seconds = now - PENDING_REVIEWS_K_WINDOW_SECS.
/// Cycles with cycle_start.timestamp < k_window_cutoff are excluded.
pub async fn pending_cycle_reviews(
    &self,
    k_window_cutoff: i64,
) -> Result<Vec<String>>

BODY:
    query:
        SELECT DISTINCT ce.cycle_id
        FROM cycle_events ce
        WHERE ce.event_type = 'cycle_start'
          AND ce.timestamp >= ?1
          AND ce.cycle_id NOT IN (SELECT feature_cycle FROM cycle_review_index)
        ORDER BY ce.cycle_id

    execute against self.read_pool()
    .bind(k_window_cutoff)
    .fetch_all(...)
    .await
    .map_err(|e| StoreError::Database(e.into()))?

    map rows:
        for each row: row.get::<String, _>(0)
        collect into Vec<String>

    return Ok(cycle_ids)
```

## lib.rs Registration

Add to `crates/unimatrix-store/src/lib.rs`:

```
pub mod cycle_review_index;
pub use cycle_review_index::{CycleReviewRecord, SUMMARY_SCHEMA_VERSION};
```

## Error Handling

| Scenario | Response |
|----------|----------|
| `get_cycle_review` SQL error | `Err(StoreError::Database(...))` — caller treats as miss |
| `store_cycle_review` pool acquire timeout | `Err(StoreError::PoolTimeout{...})` via From<sqlx::Error> |
| `store_cycle_review` SQL error | `Err(StoreError::Database(...))` — propagated as tool error |
| `store_cycle_review` 4MB exceeded | `Err(StoreError::InvalidInput{...})` — return Err, not panic |
| `pending_cycle_reviews` SQL error | `Err(StoreError::Database(...))` — caller uses empty vec |

## Key Test Scenarios

1. `get_cycle_review` returns `None` for an unknown feature_cycle.
2. `get_cycle_review` returns `Some(record)` after a `store_cycle_review` call.
3. `store_cycle_review` with `INSERT OR REPLACE` overwrites an existing row
   (verify `computed_at` changes, `schema_version` updates).
4. `store_cycle_review` with `summary_json.len() == SUMMARY_JSON_MAX_BYTES`
   returns `Ok(())`.
5. `store_cycle_review` with `summary_json.len() == SUMMARY_JSON_MAX_BYTES + 1`
   returns `Err(StoreError::InvalidInput{...})`, no panic.
6. `pending_cycle_reviews` returns only cycles with `cycle_start` events in
   K-window that have no `cycle_review_index` row.
7. `pending_cycle_reviews` excludes cycles outside the K-window cutoff.
8. `pending_cycle_reviews` excludes cycles that DO have a `cycle_review_index` row.
9. `pending_cycle_reviews` with `SELECT DISTINCT` correctly de-duplicates cycles
   with multiple `cycle_start` events.
10. `raw_signals_available` round-trip: store `i32 = 1`, fetch, confirm value is 1
    (not true or other type — confirms sqlx INTEGER→i32 binding is consistent).
11. `store_cycle_review` is not called inside `spawn_blocking` — static code review
    and grep check (R-09).
