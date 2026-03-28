# col-031: query_log.rs Store Method — Pseudocode

File: `crates/unimatrix-store/src/query_log.rs`
Status: MODIFIED (add `PhaseFreqRow` struct and `query_phase_freq_table` method)

---

## Purpose

Extend `query_log.rs` with the SQL aggregation that feeds `PhaseFreqTable::rebuild`.
All SQL logic lives here — not in the service layer (NFR-01, 500-line constraint on
`phase_freq_table.rs`). The pattern mirrors `scan_query_log_by_session`: use
`sqlx::query(sql_string).bind(...)` with `row.try_get::<T, _>(index)` deserialization.

---

## Prerequisites (Do Not Change)

The existing `row_to_query_log` helper and all existing methods are unchanged.
`PhaseFreqRow` is a new struct. `query_phase_freq_table` is a new method on `SqlxStore`.

---

## New Struct: `PhaseFreqRow`

Add after the existing `QueryLogRecord` struct in the file (after line 33):

```
/// Transient row returned by `query_phase_freq_table`.
///
/// Used only during `PhaseFreqTable::rebuild`; not stored or returned to callers.
///
/// `freq` is i64 because SQLite `COUNT(*)` maps to i64 via sqlx 0.8.
/// Do NOT use u64 — sqlx deserialization will fail silently at runtime (R-13).
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseFreqRow {
    pub phase:    String,
    pub category: String,
    /// entry_id read as i64 from SQL (CAST result), then cast to u64.
    /// The SQL CAST(je.value AS INTEGER) guarantees a non-negative integer value.
    pub entry_id: u64,
    /// COUNT(*) result — always i64 in sqlx 0.8 SQLite mapping.
    pub freq:     i64,
}
```

---

## New Method: `SqlxStore::query_phase_freq_table`

Add to the `impl SqlxStore` block (after `scan_query_log_by_session`):

```
/// Query (phase, category, entry_id, freq) aggregates from query_log within
/// a time window, joined to entries for category lookup.
///
/// # SQL
///
/// The SQL uses CROSS JOIN json_each to expand the JSON array in
/// `result_entry_ids`. CAST(je.value AS INTEGER) is MANDATORY — omitting it
/// causes a text-to-integer JOIN mismatch that returns zero rows silently (R-05).
/// Verified against mcp/knowledge_reuse.rs json_each usage (Unimatrix #3681).
///
/// Results are ordered by (phase, category, freq DESC) — the caller uses this
/// ordering directly for rank-based normalization without re-sorting.
///
/// # Parameters
///
/// `lookback_days` is bound as i64 (sqlx 0.8 INTEGER mapping requirement).
/// Validated to [1, 3650] by InferenceConfig::validate() at startup (R-08).
///
/// # Returns
///
/// Empty Vec when:
///   - No query_log rows have non-null phase within the time window.
///   - All result_entry_ids are null.
///   - The entries table has no rows matching any entry_id in the log.
///
/// Caller (`PhaseFreqTable::rebuild`) treats an empty Vec as use_fallback=true.
pub async fn query_phase_freq_table(
    &self,
    lookback_days: u32,
) -> Result<Vec<PhaseFreqRow>> {
    // The SQL is specified verbatim — do NOT modify the CAST forms or WHERE clause.
    // Any change to CAST(je.value AS INTEGER) risks returning zero rows silently (R-05).
    let sql = "
        SELECT
            q.phase,
            e.category,
            CAST(je.value AS INTEGER)  AS entry_id,
            COUNT(*)                   AS freq
        FROM query_log q
          CROSS JOIN json_each(q.result_entry_ids) AS je
          JOIN entries e ON CAST(je.value AS INTEGER) = e.id
        WHERE q.phase IS NOT NULL
          AND q.result_entry_ids IS NOT NULL
          AND q.ts > strftime('%s', 'now') - ?1 * 86400
        GROUP BY q.phase, e.category, CAST(je.value AS INTEGER)
        ORDER BY q.phase, e.category, freq DESC
    ";

    // Bind lookback_days as i64 (sqlx 0.8 INTEGER mapping — u32 would fail).
    let rows = sqlx::query(sql)
        .bind(lookback_days as i64)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

    // Deserialize using positional index access, matching existing query_log.rs pattern.
    // Column order from SELECT:
    //   0: q.phase       -> String
    //   1: e.category    -> String
    //   2: entry_id      -> i64  (CAST result is INTEGER in SQLite)
    //   3: freq          -> i64  (COUNT(*) is always i64 in sqlx 0.8)
    rows.iter()
        .map(|row| row_to_phase_freq_row(row))
        .collect()
}
```

---

## New Private Helper: `row_to_phase_freq_row`

Add after `row_to_query_log` in the file:

```
/// Deserialize a single SQL row into PhaseFreqRow.
///
/// Column positions must match the SELECT clause in query_phase_freq_table:
///   0: phase    (String)
///   1: category (String)
///   2: entry_id (i64, cast to u64)
///   3: freq     (i64)
///
/// entry_id is read as i64 and cast to u64 because:
///   - SQLite INTEGER is always signed i64 in sqlx 0.8
///   - Entry IDs are non-negative by construction
///   - The CAST(je.value AS INTEGER) SQL expression produces INTEGER affinity
fn row_to_phase_freq_row(row: &sqlx::sqlite::SqliteRow) -> Result<PhaseFreqRow> {
    Ok(PhaseFreqRow {
        phase:    row.try_get::<String, _>(0).map_err(|e| StoreError::Database(e.into()))?,
        category: row.try_get::<String, _>(1).map_err(|e| StoreError::Database(e.into()))?,
        entry_id: row.try_get::<i64, _>(2).map_err(|e| StoreError::Database(e.into()))? as u64,
        freq:     row.try_get::<i64, _>(3).map_err(|e| StoreError::Database(e.into()))?,
    })
}
```

---

## Export

`PhaseFreqRow` must be pub-exported from the crate root so `unimatrix-server` can
import it. Check whether `unimatrix_store/src/lib.rs` already re-exports
query_log items. If not, add:

```
// In crates/unimatrix-store/src/lib.rs:
pub use crate::query_log::PhaseFreqRow;
```

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `fetch_all` returns sqlx error | `map_err` wraps to `StoreError::Database`; propagated to `rebuild` |
| `try_get` fails (wrong type) | `map_err` wraps to `StoreError::Database`; propagated to `rebuild` |
| Zero rows returned | Returns `Ok(Vec::new())`; caller treats as cold-start |
| `result_entry_ids` is NULL | Filtered by `WHERE result_entry_ids IS NOT NULL` |
| `phase` is NULL | Filtered by `WHERE phase IS NOT NULL` |
| entry_id references deleted entry | `JOIN entries e` silently drops orphaned IDs; not an error |

---

## Key Test Scenarios

### AC-08: Integration test — rebuild from seeded data (R-05 guard)

```
// Test setup: use TestDb (existing integration test infrastructure).
// Seed:
//   - entries table: one entry with id=42, category="decision"
//   - query_log table: 10 rows with:
//       phase="delivery", result_entry_ids="[42]", ts = now() (within window)
//
// Call: store.query_phase_freq_table(30)
//
// Assert:
//   - Returns non-empty Vec
//   - Contains row: PhaseFreqRow { phase: "delivery", category: "decision",
//                                  entry_id: 42, freq: 10 }
//   - No row with entry_id=99 (not in entries table)
//
// This test guards R-05: if CAST is missing, entry_id would be 0 or absent.
```

### R-13: freq type must be i64

```
// If PhaseFreqRow.freq were u64, sqlx 0.8 would fail at runtime with a type error.
// AC-08 passing without deserialization errors confirms the i64 type is correct.
```

### Boundary: result_entry_ids IS NULL filtered

```
// Seed a query_log row with result_entry_ids = NULL (phase set, ts in window).
// Call: store.query_phase_freq_table(30)
// Assert: the NULL row does NOT appear in the output Vec.
```

### Boundary: phase IS NULL filtered

```
// Seed a query_log row with phase = NULL (result_entry_ids set, ts in window).
// Call: store.query_phase_freq_table(30)
// Assert: the NULL-phase row does NOT appear in the output Vec.
```

### Boundary: lookback_days = 1 (minimal window)

```
// Seed a row with ts = now() - 2 * 86400 (2 days ago).
// Call: store.query_phase_freq_table(1)
// Assert: that row does NOT appear (outside 1-day window).
```
