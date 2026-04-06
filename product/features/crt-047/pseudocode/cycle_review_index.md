# crt-047: Pseudocode — schema/cycle_review_index

## Purpose

Extend `CycleReviewRecord` with seven new fields persisting curation health metrics
per cycle. Update `store_cycle_review()` to use a two-step upsert that preserves
`first_computed_at` on overwrite (ADR-001). Add `get_curation_baseline_window()`
for baseline window reads ordered by `first_computed_at DESC`. Bump
`SUMMARY_SCHEMA_VERSION` from `1` to `2`.

File: `crates/unimatrix-store/src/cycle_review_index.rs`

---

## Constants

```
// Bump from 1 to 2 (crt-047).
// Triggers stale-record advisory for all historical rows on force=false.
// Existing test test_summary_schema_version_is_one must be updated to assert 2.
pub const SUMMARY_SCHEMA_VERSION: u32 = 2;
```

---

## New/Modified Types

### CycleReviewRecord (modified)

Add seven new `i64` fields at the end of the existing struct. All default to `0`
if not explicitly set, consistent with the DEFAULT 0 in DDL.

```
pub struct CycleReviewRecord {
    // --- existing fields (unchanged) ---
    pub feature_cycle: String,
    pub schema_version: u32,
    pub computed_at: i64,
    pub raw_signals_available: i32,
    pub summary_json: String,

    // --- new fields (crt-047) ---
    pub corrections_total: i64,       // = corrections_agent + corrections_human (stored sum)
    pub corrections_agent: i64,       // trust_source = 'agent'
    pub corrections_human: i64,       // trust_source IN ('human', 'privileged')
    pub corrections_system: i64,      // all other trust_source values (informational)
    pub deprecations_total: i64,      // all deprecated in window
    pub orphan_deprecations: i64,     // deprecated AND superseded_by IS NULL in window
    pub first_computed_at: i64,       // set once on INSERT; preserved on UPDATE (ADR-001)
                                      // pre-crt-047 rows keep DEFAULT 0 after migration;
                                      // DO NOT "fix" this to now() on force=true of historical rows
}
```

### CurationBaselineRow (new)

Slim projection from `cycle_review_index` for baseline computation. Defined here
because it is a store-boundary type produced by `get_curation_baseline_window()`.

```
pub struct CurationBaselineRow {
    pub corrections_total: i64,
    pub corrections_agent: i64,
    pub corrections_human: i64,
    pub deprecations_total: i64,
    pub orphan_deprecations: i64,
    pub schema_version: i64,    // used by compute_curation_baseline to exclude DEFAULT-0 rows
                                // A row is legacy-DEFAULT when schema_version < 2 AND all
                                // snapshot columns equal zero. A real zero-correction cycle
                                // at schema_version = 2 IS included in the baseline.
}
```

---

## Modified Functions

### get_cycle_review (modified)

Extend the SELECT and row mapping to include the seven new columns.

```
pub async fn get_cycle_review(
    &self,
    feature_cycle: &str,
) -> Result<Option<CycleReviewRecord>, StoreError>

ALGORITHM:
  sql = "SELECT feature_cycle, schema_version, computed_at,
                raw_signals_available, summary_json,
                corrections_total, corrections_agent, corrections_human,
                corrections_system, deprecations_total, orphan_deprecations,
                first_computed_at
         FROM cycle_review_index
         WHERE feature_cycle = ?1"

  row = sqlx::query(sql)
          .bind(feature_cycle)
          .fetch_optional(self.read_pool())
          .await
          .map_err(|e| StoreError::Database(e.into()))?

  match row:
    None => Ok(None)
    Some(r) =>
      Ok(Some(CycleReviewRecord {
        feature_cycle:        r.get::<String, _>(0),
        schema_version:       r.get::<i64, _>(1) as u32,
        computed_at:          r.get::<i64, _>(2),
        raw_signals_available: r.get::<i32, _>(3),
        summary_json:         r.get::<String, _>(4),
        corrections_total:    r.get::<i64, _>(5),
        corrections_agent:    r.get::<i64, _>(6),
        corrections_human:    r.get::<i64, _>(7),
        corrections_system:   r.get::<i64, _>(8),
        deprecations_total:   r.get::<i64, _>(9),
        orphan_deprecations:  r.get::<i64, _>(10),
        first_computed_at:    r.get::<i64, _>(11),
      }))

ERROR HANDLING:
  Returns Err only on genuine SQL infrastructure failure.
  Callers treat Err as a cache miss (existing behavior unchanged).
```

### store_cycle_review (modified — two-step upsert, ADR-001)

Replace the existing `INSERT OR REPLACE` with a two-step upsert that preserves
`first_computed_at`. Plain `INSERT OR REPLACE` deletes and reinserts the row,
resetting `first_computed_at` to the current value — negating ADR-001.

```
pub async fn store_cycle_review(
    &self,
    record: &CycleReviewRecord,
) -> Result<(), StoreError>

PRECONDITIONS:
  record.corrections_total == record.corrections_agent + record.corrections_human
    (caller must compute this sum; it is not verified here)

ALGORITHM:
  // 4MB ceiling check (unchanged from existing implementation — NFR-03)
  if record.summary_json.len() > SUMMARY_JSON_MAX_BYTES:
    return Err(StoreError::InvalidInput { field: "summary_json", reason: "..." })

  // Acquire write connection from write_pool_server().
  // MUST NOT be called from spawn_blocking — sqlx async context required (ADR-001).
  conn = self.write_pool_server().acquire().await
           .map_err(|e| StoreError::Database(e.into()))?

  // Step 1: Check whether a row already exists and read existing first_computed_at.
  //
  // This read uses the same write connection to avoid a TOCTOU race with the
  // single-connection write_pool_server serializer. Alternatively, a second read
  // using read_pool() is acceptable because the write_pool_server serializer
  // ensures only one writer can be here at a time.
  //
  // NOTE: The read is intentionally separate from the write; the two-step pattern
  // cannot be collapsed into a single INSERT OR REPLACE without losing first_computed_at.
  existing_first_computed_at: Option<i64> =
    sqlx::query_scalar::<_, i64>(
      "SELECT first_computed_at FROM cycle_review_index WHERE feature_cycle = ?1"
    )
    .bind(&record.feature_cycle)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| StoreError::Database(e.into()))?

  match existing_first_computed_at:
    None =>
      // Step 2a: No existing row — INSERT with first_computed_at from record.
      // record.first_computed_at is set by the caller (context_cycle_review)
      // to cycle_start_ts if available, or to now() as fallback.
      sqlx::query(
        "INSERT INTO cycle_review_index
           (feature_cycle, schema_version, computed_at, raw_signals_available,
            summary_json, corrections_total, corrections_agent, corrections_human,
            corrections_system, deprecations_total, orphan_deprecations,
            first_computed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
      )
      .bind(&record.feature_cycle)
      .bind(record.schema_version as i64)
      .bind(record.computed_at)
      .bind(record.raw_signals_available)
      .bind(&record.summary_json)
      .bind(record.corrections_total)
      .bind(record.corrections_agent)
      .bind(record.corrections_human)
      .bind(record.corrections_system)
      .bind(record.deprecations_total)
      .bind(record.orphan_deprecations)
      .bind(record.first_computed_at)
      .execute(&mut *conn)
      .await
      .map_err(|e| StoreError::Database(e.into()))?

    Some(preserved_first_computed_at) =>
      // Step 2b: Existing row found — UPDATE all mutable columns, preserving first_computed_at.
      // first_computed_at is intentionally excluded from the SET clause (ADR-001).
      //
      // Edge case for pre-crt-047 rows (force=true on historical cycles):
      // preserved_first_computed_at will be 0 (from migration DEFAULT 0).
      // We do NOT overwrite it with a real timestamp. The row remains excluded
      // from get_curation_baseline_window() (WHERE first_computed_at > 0).
      // This is intentional per ADR-001 and the non-goal of no backfilling.
      // DO NOT "fix" this by using record.first_computed_at when preserved is 0.
      sqlx::query(
        "UPDATE cycle_review_index
         SET schema_version        = ?2,
             computed_at           = ?3,
             raw_signals_available = ?4,
             summary_json          = ?5,
             corrections_total     = ?6,
             corrections_agent     = ?7,
             corrections_human     = ?8,
             corrections_system    = ?9,
             deprecations_total    = ?10,
             orphan_deprecations   = ?11
         WHERE feature_cycle = ?1"
      )
      -- Note: first_computed_at is NOT in the SET clause (ADR-001).
      .bind(&record.feature_cycle)
      .bind(record.schema_version as i64)
      .bind(record.computed_at)
      .bind(record.raw_signals_available)
      .bind(&record.summary_json)
      .bind(record.corrections_total)
      .bind(record.corrections_agent)
      .bind(record.corrections_human)
      .bind(record.corrections_system)
      .bind(record.deprecations_total)
      .bind(record.orphan_deprecations)
      .execute(&mut *conn)
      .await
      .map_err(|e| StoreError::Database(e.into()))?

  Ok(())

ERROR HANDLING:
  Err on SQL failure only. Callers treat Err as non-fatal and log a warning
  (existing behavior in tools.rs Step 8a pattern).
```

---

## New Functions

### get_curation_baseline_window

```
/// Read the last n rows from cycle_review_index ordered by first_computed_at DESC.
/// Excludes rows where first_computed_at = 0 (legacy pre-v24 rows with no
/// temporal anchor — migration DEFAULT 0).
///
/// Uses read_pool() — read-only query (pool discipline per architecture).
/// Returns an empty Vec when no qualifying rows exist. Never returns Err
/// for an empty result — only for SQL infrastructure failures.
pub async fn get_curation_baseline_window(
    &self,
    n: usize,
) -> Result<Vec<CurationBaselineRow>, StoreError>

ALGORITHM:
  sql = "SELECT corrections_total, corrections_agent, corrections_human,
                deprecations_total, orphan_deprecations, schema_version
         FROM cycle_review_index
         WHERE first_computed_at > 0
         ORDER BY first_computed_at DESC
         LIMIT ?1"

  rows = sqlx::query(sql)
           .bind(n as i64)
           .fetch_all(self.read_pool())
           .await
           .map_err(|e| StoreError::Database(e.into()))?

  result = rows.into_iter().map(|r| CurationBaselineRow {
    corrections_total:   r.get::<i64, _>(0),
    corrections_agent:   r.get::<i64, _>(1),
    corrections_human:   r.get::<i64, _>(2),
    deprecations_total:  r.get::<i64, _>(3),
    orphan_deprecations: r.get::<i64, _>(4),
    schema_version:      r.get::<i64, _>(5),
  }).collect::<Vec<_>>()

  Ok(result)

ERROR HANDLING:
  Empty result is Ok(vec![]), not Err.
  SQL failure returns Err(StoreError::Database(...)).
  Callers use .unwrap_or_default() to degrade gracefully.
```

---

## Initialization Sequence

None — all methods operate on the existing `SqlxStore` instance. The store is
initialized by `SqlxStore::open()` which runs `migrate_if_needed()` (adding the
seven columns) before pool construction.

---

## Data Flow

Inputs to `store_cycle_review`:
- `record.feature_cycle` — primary key
- `record.corrections_total` — computed sum: `corrections_agent + corrections_human`
- `record.corrections_agent`, `corrections_human`, `corrections_system` — from `CurationSnapshot`
- `record.deprecations_total`, `record.orphan_deprecations` — from `CurationSnapshot`
- `record.first_computed_at` — caller sets to `cycle_start_ts` (first insert) or ignored on update

Outputs of `get_curation_baseline_window`:
- `Vec<CurationBaselineRow>` — consumed by `compute_curation_baseline()` and `compute_curation_summary()`

---

## Error Handling

| Situation | Behavior |
|-----------|----------|
| `summary_json` exceeds 4MB | `Err(StoreError::InvalidInput)` — unchanged |
| SQL failure in read-check step | `Err(StoreError::Database)` — caller logs and continues |
| SQL failure in INSERT step | `Err(StoreError::Database)` — caller logs and continues |
| SQL failure in UPDATE step | `Err(StoreError::Database)` — caller logs and continues |
| `get_curation_baseline_window` SQL failure | `Err(StoreError::Database)` — callers use `unwrap_or_default()` |

---

## Key Test Scenarios

**T-CRI-01 (AC-R01)**: `first_computed_at` preserved on force=true (two-step upsert round-trip).
- Store a record; record the `first_computed_at` value.
- Store again for the same `feature_cycle` (simulating force=true).
- Assert `first_computed_at` is unchanged after second store.

**T-CRI-02 (AC-01, AC-14)**: All seven new columns present with DEFAULT 0 on migrated rows.
- Open a synthetic v23 database through `Store::open()`.
- Query `pragma_table_info('cycle_review_index')`.
- Assert all seven columns present; existing rows have value 0.

**T-CRI-03 (AC-05)**: Snapshot fields round-trip through store/retrieve.
- Store a `CycleReviewRecord` with non-zero snapshot fields.
- Retrieve and assert all seven fields match.

**T-CRI-04 (FR-10)**: `get_curation_baseline_window` excludes `first_computed_at = 0` rows.
- Insert rows with `first_computed_at = 0` and rows with `first_computed_at > 0`.
- Call `get_curation_baseline_window(10)`.
- Assert only rows with `first_computed_at > 0` are returned.

**T-CRI-05 (AC-11)**: `SUMMARY_SCHEMA_VERSION` equals 2.
- Assert constant value is `2u32`.
- Update test `test_summary_schema_version_is_one` to `test_summary_schema_version_is_two`.

**T-CRI-06 (R-07, EC-04)**: Concurrent force=true — `first_computed_at` always preserved.
- Insert a row with known `first_computed_at`.
- Issue two concurrent `store_cycle_review` calls for the same cycle.
- Assert `first_computed_at` is unchanged after both complete.

**T-CRI-07 (AC-13)**: `get_curation_baseline_window` ordered correctly.
- Insert three rows with distinct `first_computed_at` values.
- Assert returned order is newest-first (DESC).
- Assert LIMIT is respected.
