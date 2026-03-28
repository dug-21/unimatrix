# Test Plan: query_log_store_method
# `crates/unimatrix-store/src/query_log.rs` — `query_phase_freq_table` method

## Component Responsibilities

Adds `PhaseFreqRow` struct and `SqlxStore::query_phase_freq_table(lookback_days: u32)`
to the existing `query_log.rs`. SQL aggregation lives exclusively here (NFR-01). The
method is called by `PhaseFreqTable::rebuild` and by unit tests directly.

---

## Unit Test Expectations

All tests in `#[cfg(test)] mod tests` inside `query_log.rs` or an adjacent integration
test file. Tests require `TestDb` (in-memory SQLite) from `unimatrix-store/src/db.rs`
or the existing test helpers used by `scan_query_log_by_session` tests.

### AC-08 / SQL Correctness — Primary R-05 and R-13 Coverage

**`test_query_phase_freq_table_returns_correct_entry_id`**  ← R-05 primary, R-13
- Arrange:
  1. Create a `TestDb`.
  2. Insert one entry with `id = 42, category = "decision"` into `entries`.
  3. Insert 10 `query_log` rows with `phase = "delivery"`, `result_entry_ids = "[42]"`,
     `ts` = `now - 1000` (within lookback window of 30 days).
- Act: call `store.query_phase_freq_table(30)`.
- Assert:
  - Result is `Ok(rows)` with `rows.len() == 1`.
  - `rows[0].phase == "delivery"`.
  - `rows[0].category == "decision"`.
  - `rows[0].entry_id == 42u64`.  ← proves `CAST(je.value AS INTEGER)` works.
  - `rows[0].freq == 10i64`.  ← proves `freq` is `i64`, not `u64` (R-13).
- This test is the primary gate for R-05 (`CAST` omission causes zero rows).

**`test_query_phase_freq_table_absent_entry_not_returned`**
- Arrange: seed `query_log` with `result_entry_ids = "[99]"` but entry `99` does not
  exist in `entries`.
- Act: call `query_phase_freq_table(30)`.
- Assert: result is empty (JOIN on `entries.id` drops orphaned IDs — correct behavior).

**`test_query_phase_freq_table_null_phase_rows_excluded`**
- Arrange: seed `query_log` rows with `phase = NULL` AND rows with `phase = "delivery"`.
- Act: call `query_phase_freq_table(30)`.
- Assert: only the `phase = "delivery"` rows contribute to results (null rows filtered).
- This covers the all-null-phase edge case: if all rows have `phase = NULL`, result is
  empty and `use_fallback = true`.

**`test_query_phase_freq_table_null_result_entry_ids_excluded`**
- Arrange: seed one row with `result_entry_ids = NULL`, one row with `result_entry_ids = "[42]"`.
- Act: call `query_phase_freq_table(30)`.
- Assert: only the non-null row is counted.

**`test_query_phase_freq_table_outside_lookback_window_excluded`**
- Arrange: seed rows with `ts` outside the lookback window (`ts = 0`, i.e., Unix epoch 0).
- Act: call `query_phase_freq_table(30)` (30-day window).
- Assert: result is empty (old rows excluded by time filter).
- Also test with `lookback_days = 1`: only rows from the past 24 hours are included.

**`test_query_phase_freq_table_ordered_by_freq_desc`**
- Arrange: seed entry `42` with 10 accesses and entry `43` with 3 accesses, same `(phase, category)`.
- Act: call `query_phase_freq_table(30)`.
- Assert: rows are ordered by `freq DESC` within the same `(phase, category)` group
  — `entry_id=42` (freq=10) appears before `entry_id=43` (freq=3).

**`test_query_phase_freq_table_multiple_phase_category_groups`**
- Arrange: seed rows for `(delivery, decision)` and `(scope, lesson-learned)`.
- Act: call `query_phase_freq_table(30)`.
- Assert: result contains rows from both groups, correctly separated by `(phase, category)`.

**`test_query_phase_freq_table_empty_query_log_returns_empty`**
- Arrange: empty `query_log`.
- Act: call `query_phase_freq_table(30)`.
- Assert: `Ok(vec![])`.

---

## `PhaseFreqRow` Type Assertions

These are enforced at compile time but should be verified at code review:

- `phase: String` — not `Option<String>` (the SQL `WHERE phase IS NOT NULL` guarantees non-null).
- `category: String`.
- `entry_id: u64` — `CAST(je.value AS INTEGER)` → `i64` in sqlx, then `.try_get::<i64, _>(2)` cast to `u64`.
- `freq: i64` — `COUNT(*)` maps to `i64` in sqlx 0.8 (not `u64`).

**`test_phase_freq_row_freq_type_is_i64`**
- If sqlx row deserialization can be tested without a live DB: verify that calling
  `row.try_get::<u64, _>(3)` on a `COUNT(*)` result returns an error.
- In practice: the AC-08 test above suffices — it returns `freq == 10i64` which confirms
  the type is correct.

---

## Integration Surface

`query_phase_freq_table` is called only by `PhaseFreqTable::rebuild`. It does not
appear in the MCP interface. Its correctness is the foundation of the entire feature's
signal quality.

---

## Covered Risks

| Risk | Test |
|------|------|
| R-05 (`CAST` omission — zero rows or wrong entry_id) | `test_query_phase_freq_table_returns_correct_entry_id` |
| R-13 (`freq: u64` vs `i64` deserialization failure) | `test_query_phase_freq_table_returns_correct_entry_id` (asserts `freq == 10i64`) |
