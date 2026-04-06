# Test Plan: `services/curation_health.rs`

Component: `crates/unimatrix-server/src/services/curation_health.rs` (new file)
Risk coverage: R-01, R-04, R-05, R-06, R-11 (cold-start boundaries), SEC-01

---

## What Is Under Test

All pure functions and the async `compute_curation_snapshot()`:

| Function | Type | Risk |
|----------|------|------|
| `compute_curation_snapshot()` | Async, queries ENTRIES | R-01, R-04, AC-02, AC-03, AC-04, AC-17, AC-18 |
| `compute_curation_baseline()` | Pure | R-05, R-06, R-11, AC-15 (a-f) |
| `compare_to_baseline()` | Pure | R-11, AC-07 |
| `compute_trend()` | Pure | R-11, AC-10 |
| `compute_curation_summary()` | Pure | AC-09, AC-10 |

Constants: `CURATION_SIGMA_THRESHOLD = 1.5`, `CURATION_MIN_HISTORY = 3`,
`CURATION_MIN_TREND_HISTORY = 6`.

Tests live in `#[cfg(test)] mod tests` in `curation_health.rs`.
`compute_curation_snapshot()` tests use `open_test_store(&dir)` and are `#[tokio::test]`.
Pure function tests are plain `#[test]`.

---

## `compute_curation_snapshot()` — Unit Tests

### CH-U-01: Corrections counted by `feature_cycle` join key (AC-02, R-01)

```
test_compute_snapshot_corrections_use_feature_cycle_not_audit_log
```

- Arrange: insert two entries with `supersedes IS NOT NULL` and `feature_cycle = 'crt-047-test'`;
  insert one entry with `supersedes IS NOT NULL` and `feature_cycle = 'other-cycle'`.
- Act: `compute_curation_snapshot(store, "crt-047-test", cycle_start_ts, review_ts).await`.
- Assert: `snapshot.corrections_total == 2` (only crt-047-test entries counted).

### CH-U-02: Trust source bucketing — all six values (AC-03, R-04)

```
test_trust_source_bucketing_all_values
```

- Arrange: seed entries with `supersedes IS NOT NULL` and `feature_cycle = 'bucket-test'`:
  - `trust_source = 'agent'` (2 entries)
  - `trust_source = 'human'` (1 entry)
  - `trust_source = 'privileged'` (1 entry — counts as human)
  - `trust_source = 'system'` (1 entry)
  - `trust_source = 'direct'` (1 entry)
  - `trust_source = 'unknown-future'` (1 entry — fail-safe bucket)
- Act: `compute_curation_snapshot(store, "bucket-test", 0, i64::MAX).await`.
- Assert: `snapshot.corrections_agent == 2`.
- Assert: `snapshot.corrections_human == 2` (human + privileged).
- Assert: `snapshot.corrections_system == 3` (system + direct + unknown-future).
- Assert: `snapshot.corrections_total == 4` (agent + human only, NOT system).
- Assert: `snapshot.corrections_total != 7` (total is NOT all entries with supersedes).

### CH-U-03: Orphan deprecations — ENTRIES-only, `superseded_by IS NULL` (AC-04, R-01)

```
test_orphan_deprecations_entries_only_no_audit_log
```

- Arrange:
  - Entry A: `status='deprecated'`, `superseded_by=99` (chain-deprecated), `updated_at` in window.
  - Entry B: `status='deprecated'`, `superseded_by=NULL` (orphan), `updated_at` in window.
  - Entry C: `status='deprecated'`, `superseded_by=NULL` (orphan), `updated_at` OUTSIDE window.
- Act: `compute_curation_snapshot(store, "orphan-test", cycle_start, review_ts).await`.
- Assert: `snapshot.orphan_deprecations == 1` (only Entry B).
- Assert: `snapshot.deprecations_total == 2` (both A and B, since both `updated_at` in window).
- Entry C is excluded because its `updated_at` falls outside `[cycle_start, review_ts]`.

### CH-U-04: Chain deprecations excluded from orphan count (AC-04)

```
test_chain_deprecations_not_counted_as_orphans
```

- Arrange: entry deprecated via `context_correct` chain — `status='deprecated'`,
  `superseded_by = <new_entry_id>`, `updated_at` in cycle window.
- Act: snapshot for the cycle.
- Assert: `snapshot.orphan_deprecations == 0`.
- Assert: `snapshot.deprecations_total == 1` (it is a deprecation, just non-orphan).

### CH-U-05: Out-of-window orphan excluded (AC-18, R-14)

```
test_orphan_outside_cycle_window_not_counted
```

- Arrange: entry with `status='deprecated'`, `superseded_by=NULL`,
  `updated_at = cycle_start_ts - 1` (one second before cycle start).
- Act: `compute_curation_snapshot(store, "window-test", cycle_start_ts, review_ts).await`.
- Assert: `snapshot.orphan_deprecations == 0`.

### CH-U-06: `deprecations_total` is cycle-window only, not lifetime (AC-17)

```
test_deprecations_total_cycle_window_only
```

- Arrange: three deprecated entries — one `updated_at` before window, one within window,
  one after window.
- Assert: `snapshot.deprecations_total == 1`.

### CH-U-07: Missing `cycle_start` event — fallback to 0 does not panic (EC-02)

```
test_snapshot_fallback_when_no_cycle_start_event
```

- Arrange: call `compute_curation_snapshot()` with `cycle_start_ts = 0`.
- Assert: returns `Ok(snapshot)` with no panic.
- Assert: any entries with `updated_at > 0` in the DB may be counted (over-count is
  documented; the test verifies no panic and returns non-NaN values).

---

## `compute_curation_baseline()` — Unit Tests (AC-15, R-05, R-06, R-11)

All tests use fabricated `CurationBaselineRow` slices — no DB access required.

### CH-U-08: Empty input returns `None` (AC-15a)

```
test_baseline_empty_input_returns_none
```

- Assert: `compute_curation_baseline(&[], 10).is_none()`.

### CH-U-09: 2 real rows returns `None` (AC-15b)

```
test_baseline_two_rows_below_min_history_returns_none
```

- Arrange: 2 rows with `schema_version = 2`, non-zero fields.
- Assert: `compute_curation_baseline(&rows, 10).is_none()`.

### CH-U-10: 3 real rows returns `Some` with correct mean/stddev (AC-15c)

```
test_baseline_three_rows_returns_correct_mean_stddev
```

- Arrange: 3 rows with `corrections_total = [2, 4, 6]`,
  `deprecations_total = [1, 1, 1]`, `orphan_deprecations = [1, 1, 1]`.
- Act: `compute_curation_baseline(&rows, 10)`.
- Assert: `baseline.corrections_total_mean ≈ 4.0` (within f64 epsilon).
- Assert: population stddev is `sqrt(((2-4)^2 + (4-4)^2 + (6-4)^2) / 3) ≈ 1.633`.
- Assert: `!baseline.corrections_total_stddev.is_nan()`.
- Assert: `!baseline.orphan_ratio_mean.is_nan()`.

### CH-U-11: Zero stddev handled without NaN (AC-15d)

```
test_baseline_zero_stddev_not_nan
```

- Arrange: 3 rows all with `corrections_total = 5`, `orphan_deprecations = 0`,
  `deprecations_total = 1`.
- Assert: `baseline.corrections_total_stddev == 0.0` (or very close).
- Assert: `!baseline.corrections_total_stddev.is_nan()`.

### CH-U-12: Zero `deprecations_total` produces `orphan_ratio = 0.0` (AC-15e, R-06)

```
test_baseline_zero_deprecations_produces_zero_ratio
```

- Arrange: 3 rows all with `orphan_deprecations = 5`, `deprecations_total = 0`.
- Assert: `!baseline.orphan_ratio_mean.is_nan()`.
- Assert: `baseline.orphan_ratio_mean == 0.0`.
- Assert: `baseline.orphan_ratio_stddev == 0.0`.

### CH-U-13: Mixed zero/non-zero `deprecations_total` in window (R-06)

```
test_baseline_mixed_zero_nonzero_deprecations_finite
```

- Arrange: 5 rows — two with `deprecations_total=0`, three with `deprecations_total > 0`.
- Assert: `!baseline.orphan_ratio_mean.is_nan()`.
- Assert: `!baseline.orphan_ratio_stddev.is_nan()`.
- Assert: both are finite `f64` values.

### CH-U-14: Legacy DEFAULT-0 rows excluded from `MIN_HISTORY` count (AC-15f, R-05)

```
test_baseline_excludes_legacy_zero_rows_from_min_history
```

- Arrange: 5 rows with `schema_version = 1` and all snapshot fields `= 0` (legacy rows);
  plus 2 rows with `schema_version = 2` and non-zero fields.
- Assert: `compute_curation_baseline(&all_seven_rows, 10).is_none()`.
  (Only 2 qualifying rows — below `MIN_HISTORY = 3`.)
- `history_cycles` annotation must reflect 2, not 7.

### CH-U-15: Genuine zero-correction cycle IS included (R-05)

```
test_baseline_genuine_zero_cycle_counts_toward_min_history
```

- Arrange: 3 rows all with `schema_version = 2` and all snapshot fields `= 0`
  (real cycles that happened to have zero corrections).
- Assert: `compute_curation_baseline(&rows, 10).is_some()`.
- Assert: `baseline.history_cycles == 3`.

---

## `compare_to_baseline()` — Unit Tests

### CH-U-16: σ computed correctly for known values

```
test_compare_to_baseline_sigma_calculation
```

- Arrange: `CurationBaseline { corrections_total_mean: 4.0, corrections_total_stddev: 2.0, ... }`.
- Snapshot: `corrections_total = 8`.
- Assert: `comparison.corrections_total_sigma ≈ (8.0 - 4.0) / 2.0 = 2.0`.
- Assert: `comparison.within_normal_range == false` (2.0 > `CURATION_SIGMA_THRESHOLD = 1.5`).

### CH-U-17: `within_normal_range = true` when both σ ≤ 1.5

```
test_compare_to_baseline_within_normal_range
```

- Arrange: baseline with mean=4.0, stddev=2.0; snapshot corrections_total=5.
- Assert: `corrections_total_sigma ≈ 0.5`.
- Assert: `within_normal_range == true` (both σ within 1.5).

---

## `compute_trend()` — Unit Tests (R-11, AC-10)

### CH-U-18: Fewer than 6 rows returns `None` (AC-10 boundary)

```
test_trend_fewer_than_six_rows_returns_none
```

- Arrange: 5 rows.
- Assert: `compute_trend(&rows).is_none()`.

### CH-U-19: Exactly 6 rows returns `Some` (AC-10 boundary, inclusive)

```
test_trend_exactly_six_rows_returns_some
```

- Arrange: 6 rows — last 5 have mean 10, prior 1 has value 5.
- Assert: `compute_trend(&rows).is_some()`.

### CH-U-20: Increasing trend detected

```
test_trend_increasing
```

- Arrange: 10 rows — `corrections_total` values `[1,1,1,1,1, 5,5,5,5,5]`
  (ordered most-recent-first; last 5 have higher mean than prior 5).
- Assert: `compute_trend(&rows) == Some(TrendDirection::Increasing)`.

### CH-U-21: Decreasing trend detected

```
test_trend_decreasing
```

- Arrange: 10 rows — `corrections_total` values `[5,5,5,5,5, 1,1,1,1,1]`.
- Assert: `compute_trend(&rows) == Some(TrendDirection::Decreasing)`.

### CH-U-22: Stable trend when means are equal

```
test_trend_stable_when_means_equal
```

- Arrange: 10 rows all with `corrections_total = 3`.
- Assert: `compute_trend(&rows) == Some(TrendDirection::Stable)`.

---

## Cold-Start Boundary Test Suite (AC-R05, R-11)

These directly verify the `MIN_HISTORY = 3` and `MIN_TREND_HISTORY = 6` constants.

| Test | Input rows (all schema_version=2) | Expected |
|------|-----------------------------------|----------|
| `test_baseline_boundary_2_rows` | 2 | `None` (below threshold) |
| `test_baseline_boundary_3_rows` | 3 | `Some`, `history_cycles = 3` |
| `test_baseline_boundary_5_rows` | 5 | `Some` baseline, `compute_trend(5) = None` |
| `test_baseline_boundary_6_rows` | 6 | `Some` baseline, `compute_trend(6) = Some(...)` |
| `test_baseline_boundary_10_rows` | 10 | `Some`, `history_cycles = 10` |

---

## Constant Assertion (AC-16)

### CH-U-23: `CURATION_SIGMA_THRESHOLD` is `1.5`

```
test_curation_sigma_threshold_constant
```

```rust
#[test]
fn test_curation_sigma_threshold_constant() {
    assert!((CURATION_SIGMA_THRESHOLD - 1.5f64).abs() < f64::EPSILON);
}
```

**Also verified by grep** (AC-16): `grep -n '1\.5' crates/unimatrix-server/src/services/curation_health.rs`
must return only the constant definition line. Zero matches inside comparison logic.

---

## Security (SEC-01)

All SQL queries in `compute_curation_snapshot()` must use parameterized binds.
Verification via grep:

```
AC-13 grep: grep -n 'format!.*feature_cycle\|concat.*feature_cycle' \
    crates/unimatrix-server/src/services/curation_health.rs
```

Must return zero matches. No string interpolation of `feature_cycle` or any other
user-derived value in SQL statements.
