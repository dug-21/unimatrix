# crt-047: Pseudocode — services/curation_health

## Purpose

New module containing all curation health types and pure compute functions.
Extracted from the start rather than reacting to the 500-line cap on `status.rs`
(ADR-005). The async `compute_curation_snapshot()` runs ENTRIES SQL queries.
All other functions are pure (no I/O) and independently testable.

File: `crates/unimatrix-server/src/services/curation_health.rs` (new)
Module declaration: add `pub(crate) mod curation_health;` to `services/mod.rs`

---

## Constants

```
pub const CURATION_SIGMA_THRESHOLD: f64 = 1.5;
    // Both directions flagged: anomalously high or low corrections.
    // Matches unimatrix_observe::baseline ADR-003 value (SCOPE OQ-03 resolved).

pub const CURATION_MIN_HISTORY: usize = 3;
    // Minimum qualifying rows for sigma comparison.
    // "Qualifying" means schema_version >= 2 OR any non-zero snapshot field.
    // (See NFR-01 and compute_curation_baseline for exclusion logic.)

pub const CURATION_MIN_TREND_HISTORY: usize = 6;
    // Minimum qualifying rows for trend direction computation.
    // Trend requires last-5 vs prior-5 comparison, so 6 is the minimum
    // where prior-5 has at least one entry.
```

---

## Types

All types are defined in this module and re-exported from `services/mod.rs`
as needed by the response layer.

```
pub struct CurationSnapshot {
    pub corrections_total: u32,    // = corrections_agent + corrections_human
    pub corrections_agent: u32,    // trust_source = 'agent'
    pub corrections_human: u32,    // trust_source IN ('human', 'privileged')
    pub corrections_system: u32,   // all other trust_source (informational only)
    pub deprecations_total: u32,   // all deprecated in window
    pub orphan_deprecations: u32,  // deprecated AND superseded_by IS NULL in window
}

pub struct CurationBaseline {
    pub corrections_total_mean: f64,
    pub corrections_total_stddev: f64,
    pub orphan_ratio_mean: f64,    // 0.0 when all deprecations_total = 0 (NFR-02)
    pub orphan_ratio_stddev: f64,
    pub history_cycles: usize,     // count of qualifying rows (annotation in output)
}

pub struct CurationBaselineComparison {
    pub corrections_total_sigma: f64,   // signed sigma distance
    pub orphan_ratio_sigma: f64,        // signed sigma distance
    pub history_cycles: usize,
    pub within_normal_range: bool,      // false if either |sigma| > CURATION_SIGMA_THRESHOLD
}

pub enum TrendDirection {
    Increasing,
    Decreasing,
    Stable,
}

pub struct CurationHealthSummary {
    pub correction_rate_mean: f64,
    pub correction_rate_stddev: f64,
    pub agent_pct: f64,        // corrections_agent / corrections_total (%) — 0.0 when total=0
    pub human_pct: f64,        // corrections_human / corrections_total (%) — 0.0 when total=0
    pub orphan_ratio_mean: f64,
    pub orphan_ratio_stddev: f64,
    pub trend: Option<TrendDirection>,
    pub cycles_in_window: usize,
}

pub struct CurationHealthBlock {
    pub snapshot: CurationSnapshot,
    pub baseline: Option<CurationBaselineComparison>,   // None when < MIN_HISTORY qualifying rows
}
```

---

## Functions

### compute_curation_snapshot (async)

Queries ENTRIES to compute curation counts for a single feature cycle.
Uses `read_pool()`. Called before `store_cycle_review()` (read before write).

```
pub async fn compute_curation_snapshot(
    store: &SqlxStore,
    feature_cycle: &str,
    cycle_start_ts: i64,
    review_ts: i64,
) -> Result<CurationSnapshot, ServiceError>

ALGORITHM:
  // Query 1: Count corrections by trust_source bucket for this feature_cycle.
  //
  // Attribution: correcting entry's feature_cycle column (not updated_at).
  // An entry "corrects" when supersedes IS NOT NULL.
  //
  // ADR-002 bucketing:
  //   trust_source = 'agent'                      → corrections_agent
  //   trust_source IN ('human', 'privileged')     → corrections_human
  //   trust_source NOT IN the above               → corrections_system (informational)
  //
  // SQLite FILTER(WHERE ...) on aggregate functions — available since SQLite 3.30.0.
  // Alternative using SUM(CASE WHEN ...) shown in ADR-002 if compatibility needed.
  //
  // No AUDIT_LOG join (ADR-003).
  corrections_row = sqlx::query(
    "SELECT
       COUNT(*) FILTER (WHERE trust_source = 'agent') AS corrections_agent,
       COUNT(*) FILTER (WHERE trust_source IN ('human', 'privileged')) AS corrections_human,
       COUNT(*) FILTER (WHERE trust_source NOT IN ('agent', 'human', 'privileged')) AS corrections_system
     FROM entries
     WHERE feature_cycle = ?1
       AND supersedes IS NOT NULL"
  )
  .bind(feature_cycle)
  .fetch_one(store.read_pool())
  .await
  .map_err(|e| ServiceError::Core(CoreError::Store(StoreError::Database(e.into()))))?

  corrections_agent:  u32 = corrections_row.get::<i64, _>(0).max(0) as u32
  corrections_human:  u32 = corrections_row.get::<i64, _>(1).max(0) as u32
  corrections_system: u32 = corrections_row.get::<i64, _>(2).max(0) as u32
  corrections_total:  u32 = corrections_agent + corrections_human
      // ADR-002: total = agent + human; system excluded from total

  // Query 2: Count all deprecations in the cycle window (orphan + chain).
  //
  // Attribution: entries.updated_at within [cycle_start_ts, review_ts].
  // No AUDIT_LOG join (ADR-003). Both orphan and non-orphan deprecations included.
  // chain-deprecated entries (from context_correct) have superseded_by IS NOT NULL
  // and are counted here but not in orphan_deprecations.
  deprecations_total_row = sqlx::query(
    "SELECT COUNT(*) FROM entries
     WHERE status = 'deprecated'
       AND updated_at >= ?1
       AND updated_at <= ?2"
  )
  .bind(cycle_start_ts)
  .bind(review_ts)
  .fetch_one(store.read_pool())
  .await
  .map_err(|e| ServiceError::Core(...))?

  deprecations_total: u32 = deprecations_total_row.get::<i64, _>(0).max(0) as u32

  // Query 3: Count orphan deprecations in the cycle window.
  //
  // Orphan = status='deprecated' AND superseded_by IS NULL AND updated_at in window.
  // Only context_deprecate produces orphans (write-path analysis, ADR-003).
  // context_correct and lesson-learned always set superseded_by IS NOT NULL → excluded.
  orphan_row = sqlx::query(
    "SELECT COUNT(*) FROM entries
     WHERE status = 'deprecated'
       AND superseded_by IS NULL
       AND updated_at >= ?1
       AND updated_at <= ?2"
  )
  .bind(cycle_start_ts)
  .bind(review_ts)
  .fetch_one(store.read_pool())
  .await
  .map_err(|e| ServiceError::Core(...))?

  orphan_deprecations: u32 = orphan_row.get::<i64, _>(0).max(0) as u32

  // Fallback note: if cycle_start_ts = 0 (no cycle_start event in cycle_events),
  // the window becomes [0, review_ts] — all history is in-window, over-counting orphans.
  // The caller (context_cycle_review) logs a warning when cycle_start_ts = 0.
  // This function does not check for cycle_start_ts = 0; it executes the query as-is.

  Ok(CurationSnapshot {
    corrections_total,
    corrections_agent,
    corrections_human,
    corrections_system,
    deprecations_total,
    orphan_deprecations,
  })

ERROR HANDLING:
  SQL failure → Err(ServiceError::Core(...)).
  Caller (tools.rs Step 8a) treats Err as non-fatal: logs warning, omits curation_health
  from response (or includes raw counts only with an error annotation).
```

### compute_curation_baseline (pure)

Computes rolling mean/stddev for `corrections_total` and `orphan_ratio` over the
provided window rows. Returns `None` when fewer than `CURATION_MIN_HISTORY` qualifying
rows exist.

```
pub fn compute_curation_baseline(
    rows: &[CurationBaselineRow],
    n: usize,  // requested window size (used for documentation only; rows is already sliced)
) -> Option<CurationBaseline>

PRECONDITIONS:
  rows.len() <= n (callers pass the result of get_curation_baseline_window(n) directly)

ALGORITHM:
  // Step 1: Identify qualifying rows.
  // A row is "legacy DEFAULT" (excluded from MIN_HISTORY count) when:
  //   schema_version < 2 AND corrections_total = 0 AND corrections_agent = 0
  //                      AND corrections_human = 0 AND deprecations_total = 0
  //                      AND orphan_deprecations = 0
  // A real zero-correction cycle at schema_version = 2 IS included (all-zero is
  // legitimate data). Only schema_version < 2 with all-zero is excluded.
  //
  // NFR-01: do not mistake migration DEFAULT-0 rows for real zero-correction cycles.
  qualifying: Vec<&CurationBaselineRow> = rows.iter()
    .filter(|r| is_qualifying_row(r))
    .collect()

  if qualifying.len() < CURATION_MIN_HISTORY:
    return None

  // Step 2: Compute orphan_ratio per qualifying row (0.0 when deprecations_total = 0).
  // NFR-02: no NaN. Division guard is mandatory.
  orphan_ratios: Vec<f64> = qualifying.iter().map(|r| {
    if r.deprecations_total == 0:
      0.0
    else:
      r.orphan_deprecations as f64 / r.deprecations_total as f64
  }).collect()

  corrections_values: Vec<f64> = qualifying.iter()
    .map(|r| r.corrections_total as f64)
    .collect()

  // Step 3: Population mean and stddev (matches unimatrix_observe::baseline pattern).
  // Use population stddev (not sample stddev): divide by n, not n-1.
  // Zero stddev (all values identical) is valid — do NOT produce NaN.
  corrections_total_mean = mean(&corrections_values)
  corrections_total_stddev = population_stddev(&corrections_values)

  orphan_ratio_mean = mean(&orphan_ratios)
  orphan_ratio_stddev = population_stddev(&orphan_ratios)

  // Population stddev helper:
  //   variance = sum((x - mean)^2) / n
  //   stddev = variance.sqrt()
  //   When n = 1: variance = 0.0, stddev = 0.0 (not NaN)
  //   When all values equal: variance = 0.0, stddev = 0.0

  Some(CurationBaseline {
    corrections_total_mean,
    corrections_total_stddev,
    orphan_ratio_mean,
    orphan_ratio_stddev,
    history_cycles: qualifying.len(),
  })

// Helper: is_qualifying_row
fn is_qualifying_row(row: &CurationBaselineRow) -> bool {
  if row.schema_version >= 2:
    return true   // Always qualifying: explicitly computed with crt-047 schema
  // schema_version < 2: qualifying only if any snapshot field is non-zero
  // (cannot be a real zero-correction cycle because those are always written at schema_version = 2)
  row.corrections_total != 0
  || row.corrections_agent != 0
  || row.corrections_human != 0
  || row.deprecations_total != 0
  || row.orphan_deprecations != 0
}

ERROR HANDLING:
  Pure function — no I/O, no errors. All arithmetic must be NaN-free.
  Post-condition: if Some(baseline) is returned, all f64 fields are finite (not NaN, not +inf).
```

### compare_to_baseline (pure)

Computes the sigma distance of the current snapshot from the baseline.

```
pub fn compare_to_baseline(
    snapshot: &CurationSnapshot,
    baseline: &CurationBaseline,
    history_count: usize,
) -> CurationBaselineComparison

ALGORITHM:
  // Sigma distance = (observed - mean) / stddev.
  // Zero stddev → stddev is 0.0 → division would produce NaN or +inf.
  // When stddev = 0.0, all baseline values are identical to the mean.
  // Define sigma = 0.0 when stddev = 0.0 (the current value equals the baseline).
  corrections_total_sigma: f64 =
    if baseline.corrections_total_stddev == 0.0:
      0.0
    else:
      (snapshot.corrections_total as f64 - baseline.corrections_total_mean)
        / baseline.corrections_total_stddev

  // Compute current orphan_ratio with same zero-denominator guard as baseline.
  current_orphan_ratio: f64 =
    if snapshot.deprecations_total == 0:
      0.0
    else:
      snapshot.orphan_deprecations as f64 / snapshot.deprecations_total as f64

  orphan_ratio_sigma: f64 =
    if baseline.orphan_ratio_stddev == 0.0:
      0.0
    else:
      (current_orphan_ratio - baseline.orphan_ratio_mean) / baseline.orphan_ratio_stddev

  within_normal_range: bool =
    corrections_total_sigma.abs() <= CURATION_SIGMA_THRESHOLD
    && orphan_ratio_sigma.abs() <= CURATION_SIGMA_THRESHOLD

  CurationBaselineComparison {
    corrections_total_sigma,
    orphan_ratio_sigma,
    history_cycles: history_count,
    within_normal_range,
  }

ERROR HANDLING:
  Pure function. All f64 results must be finite. Post-condition: sigma values are finite.
```

### compute_trend (pure)

Computes trend direction by comparing mean of last-5 rows vs mean of rows 6-10
(positions 5-9 in the slice, 0-indexed). Returns `None` when fewer than
`CURATION_MIN_TREND_HISTORY` qualifying rows exist.

```
pub fn compute_trend(rows: &[CurationBaselineRow]) -> Option<TrendDirection>

// rows is ordered by first_computed_at DESC (newest first).
// "last 5" = rows[0..5] (most recent 5 cycles)
// "prior 5" = rows[5..10] (cycles 6-10 in the window)

ALGORITHM:
  qualifying: Vec<&CurationBaselineRow> = rows.iter()
    .filter(|r| is_qualifying_row(r))
    .collect()

  if qualifying.len() < CURATION_MIN_TREND_HISTORY:
    return None

  // Split into recent (first 5) and prior (next up to 5).
  recent: &[&CurationBaselineRow] = &qualifying[0..5.min(qualifying.len())]
  prior:  &[&CurationBaselineRow] = &qualifying[5..qualifying.len()]

  if prior.is_empty():
    // This branch is unreachable when len >= 6, but guard defensively.
    return None

  recent_mean: f64 = mean(recent.iter().map(|r| r.corrections_total as f64))
  prior_mean:  f64 = mean(prior.iter().map(|r| r.corrections_total as f64))

  delta: f64 = recent_mean - prior_mean

  // Threshold: use stddev of all qualifying values as the noise floor.
  // A delta smaller than the stddev is treated as Stable.
  all_values: Vec<f64> = qualifying.iter().map(|r| r.corrections_total as f64).collect()
  noise_floor: f64 = population_stddev(&all_values)

  direction = match delta:
    d if d > noise_floor   => TrendDirection::Increasing
    d if d < -noise_floor  => TrendDirection::Decreasing
    _                      => TrendDirection::Stable

  Some(direction)

NOTE on noise_floor:
  When all values are identical, noise_floor = 0.0 and delta = 0.0.
  The match arm `d < -0.0` is false for delta = 0.0, so result is Stable. Correct.
  No NaN produced: mean and stddev of identical values are finite.

ERROR HANDLING:
  Pure function. Returns None for insufficient history; Some(direction) otherwise.
```

### compute_curation_summary (pure)

Aggregates the baseline window into a `CurationHealthSummary` for `context_status`.
Returns `None` when the window is empty.

```
pub fn compute_curation_summary(
    rows: &[CurationBaselineRow],
) -> Option<CurationHealthSummary>

// rows is the result of get_curation_baseline_window(CURATION_BASELINE_WINDOW).
// Ordered by first_computed_at DESC (newest first).

ALGORITHM:
  if rows.is_empty():
    return None

  qualifying: Vec<&CurationBaselineRow> = rows.iter()
    .filter(|r| is_qualifying_row(r))
    .collect()

  // cycles_in_window is the count of ALL rows (including legacy), for transparency.
  cycles_in_window: usize = rows.len()

  corrections_values: Vec<f64> = qualifying.iter()
    .map(|r| r.corrections_total as f64)
    .collect()

  correction_rate_mean: f64 = if qualifying.is_empty() { 0.0 } else { mean(&corrections_values) }
  correction_rate_stddev: f64 = if qualifying.is_empty() { 0.0 } else { population_stddev(&corrections_values) }

  // Source breakdown: agent% and human% of total corrections.
  // When corrections_total = 0 across all qualifying rows, both percentages are 0.0.
  total_corrections_sum: f64 = qualifying.iter().map(|r| r.corrections_total as f64).sum()
  total_agent_sum: f64 = qualifying.iter().map(|r| r.corrections_agent as f64).sum()
  total_human_sum: f64 = qualifying.iter().map(|r| r.corrections_human as f64).sum()

  agent_pct: f64 = if total_corrections_sum == 0.0 { 0.0 } else { total_agent_sum / total_corrections_sum * 100.0 }
  human_pct: f64 = if total_corrections_sum == 0.0 { 0.0 } else { total_human_sum / total_corrections_sum * 100.0 }

  // Orphan ratio per qualifying row (0.0 when deprecations_total = 0 — NFR-02).
  orphan_ratios: Vec<f64> = qualifying.iter().map(|r| {
    if r.deprecations_total == 0 { 0.0 }
    else { r.orphan_deprecations as f64 / r.deprecations_total as f64 }
  }).collect()

  orphan_ratio_mean: f64 = if qualifying.is_empty() { 0.0 } else { mean(&orphan_ratios) }
  orphan_ratio_stddev: f64 = if qualifying.is_empty() { 0.0 } else { population_stddev(&orphan_ratios) }

  // Trend: uses ALL rows (including legacy), consistent with compute_trend signature.
  trend: Option<TrendDirection> = compute_trend(rows)

  Some(CurationHealthSummary {
    correction_rate_mean,
    correction_rate_stddev,
    agent_pct,
    human_pct,
    orphan_ratio_mean,
    orphan_ratio_stddev,
    trend,
    cycles_in_window,
  })

ERROR HANDLING:
  Pure function. All f64 results must be finite. Empty rows → None.
```

---

## Shared Helpers (private to module)

```
fn mean(values: &[f64]) -> f64 {
  if values.is_empty(): return 0.0
  values.iter().sum::<f64>() / values.len() as f64
}

fn population_stddev(values: &[f64]) -> f64 {
  if values.len() < 2: return 0.0  // single value or empty: variance = 0, stddev = 0
  let m = mean(values)
  let variance = values.iter().map(|x| (x - m).powi(2)).sum::<f64>() / values.len() as f64
  variance.sqrt()
  // sqrt(0.0) = 0.0, not NaN (Rust f64::sqrt for non-negative returns finite)
}
```

---

## Data Flow

Inputs to `compute_curation_snapshot`:
- `store: &SqlxStore` — for SQL execution via `read_pool()`
- `feature_cycle: &str` — primary key for corrections query
- `cycle_start_ts: i64` — lower bound for deprecation/orphan window
- `review_ts: i64` — upper bound for deprecation/orphan window (usually now())

Outputs:
- `CurationSnapshot` → consumed by `store_cycle_review()` (written to DB)
- `CurationSnapshot` + `CurationBaseline` → `compare_to_baseline()` → `CurationHealthBlock`
- `Vec<CurationBaselineRow>` → `compute_curation_summary()` → `CurationHealthSummary`

---

## Error Handling Summary

| Function | Error Type | Propagation |
|----------|-----------|-------------|
| `compute_curation_snapshot` | `ServiceError` | Caller logs and continues (non-fatal) |
| `compute_curation_baseline` | None (pure) | Returns `Option<CurationBaseline>` |
| `compare_to_baseline` | None (pure) | Always returns comparison |
| `compute_trend` | None (pure) | Returns `Option<TrendDirection>` |
| `compute_curation_summary` | None (pure) | Returns `Option<CurationHealthSummary>` |

---

## Key Test Scenarios

**T-CH-01 (AC-02, AC-03)**: `compute_curation_snapshot` — trust_source bucketing.
- Store entries with trust_source in ('agent', 'human', 'privileged', 'system', 'direct', 'unknown').
- All have `supersedes IS NOT NULL` and `feature_cycle = "test-cycle"`.
- Call `compute_curation_snapshot("test-cycle", 0, now)`.
- Assert: `corrections_agent` = count of 'agent'; `corrections_human` = count of ('human','privileged');
  `corrections_system` = count of rest; `corrections_total = agent + human`.

**T-CH-02 (AC-04, AC-17, AC-18)**: `compute_curation_snapshot` — window filtering.
- Store entries: (a) orphan inside window, (b) orphan outside window (before cycle_start_ts),
  (c) chain-deprecated (superseded_by IS NOT NULL) inside window.
- Assert: `orphan_deprecations` = 1 (only entry a); `deprecations_total` = 2 (entries a + c).

**T-CH-03 (AC-15a)**: `compute_curation_baseline` — empty input returns None.

**T-CH-04 (AC-15b)**: `compute_curation_baseline` — 2 qualifying rows returns None.

**T-CH-05 (AC-15c)**: `compute_curation_baseline` — 3 qualifying rows returns correct mean/stddev.
- Rows: corrections_total in [1, 2, 3]. Mean = 2.0, stddev = population_stddev([1,2,3]).

**T-CH-06 (AC-15d, R-06)**: `compute_curation_baseline` — zero stddev (all identical) handled.
- 3 rows all with corrections_total = 5.
- Assert: stddev = 0.0, not NaN; mean = 5.0.

**T-CH-07 (AC-15e, NFR-02)**: `compute_curation_baseline` — zero deprecations_total.
- Rows with deprecations_total = 0.
- Assert: orphan_ratio_mean = 0.0, not NaN.

**T-CH-08 (AC-15f, R-05)**: `compute_curation_baseline` — legacy DEFAULT-0 rows excluded from count.
- Window: 5 legacy rows (schema_version=1, all zeros) + 3 real rows (schema_version=2).
- Assert: `history_cycles = 3`; baseline computed over real rows only.

**T-CH-09 (AC-07, R-11)**: `compare_to_baseline` — sigma values and `within_normal_range`.
- Baseline with mean=2.0, stddev=1.0. Snapshot with corrections_total=5.0.
- Assert: sigma = 3.0; within_normal_range = false (3.0 > 1.5 threshold).

**T-CH-10 (AC-10)**: `compute_trend` — fewer than 6 qualifying rows returns None.
- 5 rows. Assert: None.

**T-CH-11 (AC-10)**: `compute_trend` — exactly 6 qualifying rows returns direction.
- 6 rows with increasing corrections_total. Assert: Some(Increasing).

**T-CH-12 (R-11)**: Boundary at exactly 3 rows for MIN_HISTORY (sigma present).
- 2 rows → baseline = None.
- 3 rows → baseline = Some(...).

**T-CH-13 (R-06)**: No NaN in any output field (property test or targeted fixture).
- Assert: all f64 fields in CurationBaseline and CurationHealthSummary pass `!x.is_nan()`.
