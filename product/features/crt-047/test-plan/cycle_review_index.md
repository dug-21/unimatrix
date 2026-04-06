# Test Plan: `cycle_review_index.rs`

Component: `crates/unimatrix-store/src/cycle_review_index.rs`
Risk coverage: R-02, R-07, R-09, R-11 (cold-start via window reads), R-12

---

## Changes Under Test

1. Seven new fields added to `CycleReviewRecord`:
   `corrections_total`, `corrections_agent`, `corrections_human`, `corrections_system`,
   `deprecations_total`, `orphan_deprecations`, `first_computed_at` (all `i64`).
2. `store_cycle_review()` converted from plain `INSERT OR REPLACE` to two-step upsert
   preserving `first_computed_at` on overwrite.
3. `get_cycle_review()` updated to select and map all seven new columns.
4. `SUMMARY_SCHEMA_VERSION` bumped from `1` to `2`.
5. New method `get_curation_baseline_window(n: usize) -> Result<Vec<CurationBaselineRow>>`.

---

## Unit Test Expectations

All tests live in `#[cfg(test)] mod tests` in `cycle_review_index.rs`.
Use `open_test_store(&dir)` helper. All tests are `#[tokio::test]`.

### CRS-V24-U-01: `SUMMARY_SCHEMA_VERSION` is `2`

```
test_summary_schema_version_is_two
```

- Assert `SUMMARY_SCHEMA_VERSION == 2u32`.
- This replaces the pre-existing `test_summary_schema_version_is_one` — the old test
  must be renamed or the assertion updated to assert `2`.

### CRS-V24-U-02: `CycleReviewRecord` round-trip includes all seven new fields

```
test_cycle_review_record_v24_round_trip
```

- Arrange: build `CycleReviewRecord` with all fields populated, including all seven new
  i64 fields set to non-zero sentinel values.
- Act: `store.store_cycle_review(&record).await`, then `store.get_cycle_review(&feature_cycle).await`.
- Assert: each of the seven new fields matches the stored value byte-for-byte.
- Note: the existing `test_cycle_review_record_round_trip` uses zero-defaulted new fields
  (pre-v24 shape) and should remain; this test adds coverage for non-zero new fields.

### CRS-V24-U-03: `first_computed_at` preserved on overwrite (AC-R01, R-07)

```
test_store_cycle_review_preserves_first_computed_at_on_overwrite
```

- Arrange: call `store_cycle_review()` with `first_computed_at = 1_700_000_000`.
- Act: call `store_cycle_review()` again with same `feature_cycle`, `first_computed_at = 1_800_000_000`.
- Assert: retrieved row has `first_computed_at == 1_700_000_000` (first write wins).
- Assert: all other fields reflect the second write (overwrite occurred for snapshot columns).
- This is the critical R-07 regression test. A naive `INSERT OR REPLACE` reinserts the row
  and would produce `first_computed_at = 1_800_000_000`.

### CRS-V24-U-04: First write sets `first_computed_at` from caller-supplied timestamp

```
test_store_cycle_review_first_write_sets_first_computed_at
```

- Arrange: no prior row exists for `feature_cycle`.
- Act: `store_cycle_review()` with `first_computed_at = 1_750_000_000`.
- Assert: retrieved `first_computed_at == 1_750_000_000`.

### CRS-V24-U-05: `get_curation_baseline_window` excludes `first_computed_at = 0` (AC-R02)

```
test_get_curation_baseline_window_excludes_zero_first_computed_at
```

- Arrange: insert three rows — two with `first_computed_at = 0` (legacy), one with
  `first_computed_at = 1_700_000_000`.
- Act: `store.get_curation_baseline_window(10).await`.
- Assert: result length is `1`.
- Assert: returned row's `first_computed_at == 1_700_000_000`.

### CRS-V24-U-06: `get_curation_baseline_window` returns rows ordered by `first_computed_at DESC` (AC-R02, AC-R03)

```
test_get_curation_baseline_window_ordered_by_first_computed_at_desc
```

- Arrange: insert rows with `first_computed_at` values `[1_000, 2_000, 3_000]`.
- Act: `store.get_curation_baseline_window(10).await`.
- Assert: result length is 3.
- Assert: `result[0].first_computed_at == 3_000`, `result[1].first_computed_at == 2_000`,
  `result[2].first_computed_at == 1_000`.

### CRS-V24-U-07: `get_curation_baseline_window` caps at `n` (R-11 boundary)

```
test_get_curation_baseline_window_caps_at_n
```

- Arrange: insert 12 rows with distinct `first_computed_at > 0`.
- Act: `store.get_curation_baseline_window(10).await`.
- Assert: result length is `10`.
- Assert: the 10 rows with the largest `first_computed_at` are returned.

### CRS-V24-U-08: `force=true` historical cycle does not appear at top of window (AC-R03)

```
test_force_true_historical_does_not_perturb_baseline_window_order
```

- Arrange: insert "current-cycle" row with `first_computed_at = 2_000`.
- Insert "historical-cycle" row with `first_computed_at = 1_000`.
- Act: simulate `force=true` overwrite on "historical-cycle" — call `store_cycle_review()`
  for "historical-cycle" with updated snapshot values and current time as `computed_at`.
- Assert: `get_curation_baseline_window(2)` returns "current-cycle" first
  (index 0), "historical-cycle" second (index 1).
- Assert: "historical-cycle" `first_computed_at` remains `1_000`.

### CRS-V24-U-09: `corrections_system` survives round-trip (AC-03, R-09)

```
test_corrections_system_round_trips_through_store
```

- Arrange: record with `corrections_system = 7`, all other snapshot fields distinct.
- Act: store and retrieve.
- Assert: `retrieved.corrections_system == 7`.
- This guards against `corrections_system` being silently dropped in the SQL projection.

### CRS-V24-U-10: Empty window returns empty slice (FM-03)

```
test_get_curation_baseline_window_empty_when_no_qualifying_rows
```

- Arrange: fresh DB (no rows), or insert only rows with `first_computed_at = 0`.
- Act: `store.get_curation_baseline_window(10).await`.
- Assert: result is `Ok(vec![])`.
- Assert: no error returned.

---

## Integration Test Expectations

The `get_cycle_review()` → `store_cycle_review()` round-trip is exercised by the
store-level unit tests above. The MCP-visible integration tests live in
`test_lifecycle.py` (see OVERVIEW.md). No additional integration tests are owned by
this component — the store-layer tests are sufficient for the schema-boundary behavior.

---

## Cascade Update Requirements

The `SUMMARY_SCHEMA_VERSION` bump from `1` to `2` cascades to one existing test
assertion in this file:

| Existing test | Required change |
|---------------|-----------------|
| `test_summary_schema_version_is_one` | Rename to `test_summary_schema_version_is_two`; change `assert_eq!(SUMMARY_SCHEMA_VERSION, 1u32, ...)` to `assert_eq!(SUMMARY_SCHEMA_VERSION, 2u32, ...)` |

The existing `test_cycle_review_record_round_trip` and all other `CRS-*` tests
from crt-033 remain valid and must pass after adding the new fields — they use
zero-defaulted values for the seven new columns, which is the correct expected
behavior for pre-v24 records.

---

## Edge Cases

| Edge Case | Test | Expected |
|-----------|------|----------|
| EC-04: Concurrent `force=true` for same cycle | `test_concurrent_force_true_preserves_first_computed_at` | `write_pool_server` serializes; `first_computed_at` matches first write |
| EC-06: Window of only legacy rows | `test_get_curation_baseline_window_empty_when_no_qualifying_rows` | Returns empty slice, no error |
