# Component: empirical-prior-computation

**File**: `crates/unimatrix-server/src/services/status.rs`

## Purpose

Add Step 2b to `run_maintenance`: after the confidence refresh loop completes,
compute alpha0 and beta0 from the voted-entry population using method-of-moments,
compute `observed_spread` from the full active-entry confidence distribution, and
atomically update `ConfidenceState`. A new helper function
`compute_empirical_prior` encapsulates the estimation logic.

The prior computation runs once per maintenance tick. Between ticks, callers
receive the cached values from the most recent tick. Staleness window = one
refresh cycle (acceptable per SPEC).

## New Constant

```
// In services/status.rs or re-exported from engine/confidence.rs
pub const MINIMUM_VOTED_POPULATION: usize = 10
```

This is the authoritative threshold (ADR-002). SPEC originally stated >= 5, but
ADR-002 raised it to 10 for population stability. The implementation uses 10.

## New Function: compute_empirical_prior

**Location**: `services/status.rs` (private) or `services/confidence.rs` (if
shared with ConfidenceService) — prefer `status.rs` since it is only called
from `run_maintenance`.

```
fn compute_empirical_prior(voted_entries: &[(u32, u32)]) -> (f64, f64):
    // Input: (helpful_count, unhelpful_count) pairs for entries with >= 1 vote
    // Returns: (alpha0, beta0)
    // Falls back to cold-start if voted_entries.len() < MINIMUM_VOTED_POPULATION

    if voted_entries.len() < MINIMUM_VOTED_POPULATION:
        return (3.0, 3.0)  // COLD_START_ALPHA, COLD_START_BETA

    // Compute per-entry helpfulness rate (cast u32 to f64 before division)
    let rates: Vec<f64> = voted_entries.iter().map(|(h, u)| {
        let h_f = *h as f64
        let u_f = *u as f64
        let total = h_f + u_f
        h_f / total  // total >= 1 guaranteed by caller filter
    }).collect()

    let n = rates.len() as f64

    // Population mean
    let p_bar: f64 = rates.iter().sum::<f64>() / n

    // Sample variance (unbiased, Bessel's correction: divide by n-1)
    // With n >= 10, n-1 >= 9 so no division-by-zero
    let variance: f64 = {
        let sum_sq_dev: f64 = rates.iter()
            .map(|r| (r - p_bar).powi(2))
            .sum()
        sum_sq_dev / (n - 1.0)
    }

    // Handle zero-variance degeneracy (R-12):
    // When all entries have identical rate, sigma^2 = 0 -> division by zero.
    // The clamp [0.5, 50.0] handles infinity and NaN post-computation.
    if variance <= 0.0:
        // All entries have same rate: cannot estimate variance, use cold-start
        return (3.0, 3.0)

    // Method of moments:
    // concentration = p_bar * (1 - p_bar) / variance - 1
    let concentration = p_bar * (1.0 - p_bar) / variance - 1.0

    let alpha0_raw = p_bar * concentration
    let beta0_raw  = (1.0 - p_bar) * concentration

    // Clamp to [0.5, 50.0] (SPEC FR-09, R-12, SEC-01)
    // This prevents NaN/infinity from degenerate estimates.
    let alpha0 = alpha0_raw.clamp(0.5, 50.0)
    let beta0  = beta0_raw.clamp(0.5, 50.0)

    (alpha0, beta0)
```

Note on the variance guard: checking `variance <= 0.0` before the division
handles both the exact-zero case (all rates identical) and underflow. The
clamp at [0.5, 50.0] is a second defense for near-zero variance that produces
very large concentration values.

The SPEC states clamp to `[0.5, 20.0]`. The IMPLEMENTATION-BRIEF states
`[0.5, 50.0]`. Use `[0.5, 50.0]` — the brief and architecture are authoritative.

## New Function: compute_observed_spread

```
fn compute_observed_spread(confidences: &[f64]) -> f64:
    // Returns p95 - p5 spread of the confidence distribution
    // Returns 0.0 for empty input (EC-01)

    if confidences.is_empty():
        return 0.0

    // Sort a cloned vec to find percentiles
    let mut sorted = confidences.to_vec()
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))

    let n = sorted.len()

    // Percentile index computation (nearest-rank method):
    // p5  index = max(0, ceil(0.05 * n) - 1)
    // p95 index = min(n-1, ceil(0.95 * n) - 1)
    let p5_idx  = ((0.05 * n as f64).ceil() as usize).saturating_sub(1)
    let p95_idx = (((0.95 * n as f64).ceil() as usize).saturating_sub(1)).min(n - 1)

    let p5  = sorted[p5_idx]
    let p95 = sorted[p95_idx]

    (p95 - p5).max(0.0)  // guard against floating-point negative diff
```

For very small populations (n < 10), the percentile spread will still be
computed but may be noisy. That is acceptable — `observed_spread` adapts the
blend weight, not the prior. Only the prior has the 10-entry threshold.

## Modified run_maintenance: Step 2b Addition

Step 2b is inserted in `run_maintenance` immediately after the existing Step 2
(confidence refresh loop) and before Step 3 (graph compaction). The entire
Step 2b runs in a `spawn_blocking` to avoid blocking the async executor on
potentially large SQL queries.

```
// Step 2b: Empirical prior + spread computation (NEW)
{
    let store_for_prior = Arc::clone(&self.store)
    let confidence_state = Arc::clone(&self.confidence_state_handle)

    let prior_result = tokio::task::spawn_blocking(move || -> (f64, f64, f64, f64) {
        let conn = store_for_prior.lock_conn()

        // Load voted entries: active entries with helpful_count + unhelpful_count >= 1
        let voted_pairs: Vec<(u32, u32)> = {
            let mut stmt = conn.prepare(
                "SELECT helpful_count, unhelpful_count
                 FROM entries
                 WHERE status = 'active'
                   AND (helpful_count + unhelpful_count) >= 1"
            ).unwrap_or_else(|_| /* handle error */ return vec![])
            stmt.query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?))
            }).unwrap_or_else(...)
             .filter_map(|r| r.ok())
             .collect()
        }

        // Load all active confidence values for spread computation
        let all_confidences: Vec<f64> = {
            let mut stmt = conn.prepare(
                "SELECT confidence FROM entries WHERE status = 'active'"
            ).unwrap_or_else(...)
            stmt.query_map([], |row| row.get::<_, f64>(0))
                .unwrap_or_else(...)
                .filter_map(|r| r.ok())
                .collect()
        }

        let (alpha0, beta0) = compute_empirical_prior(&voted_pairs)
        let observed_spread = compute_observed_spread(&all_confidences)
        let confidence_weight = adaptive_confidence_weight(observed_spread)

        (alpha0, beta0, observed_spread, confidence_weight)
    }).await

    match prior_result:
        Ok((alpha0, beta0, observed_spread, confidence_weight)) =>
            // Atomic write of all four values (ADR-002)
            let mut guard = confidence_state
                .write()
                .unwrap_or_else(|e| e.into_inner())
            guard.alpha0            = alpha0
            guard.beta0             = beta0
            guard.observed_spread   = observed_spread
            guard.confidence_weight = confidence_weight
            tracing::debug!(
                "confidence state updated: alpha0={alpha0:.3}, beta0={beta0:.3}, \
                 spread={observed_spread:.4}, weight={confidence_weight:.4}"
            )
        Err(e) =>
            tracing::warn!("prior computation task failed: {e}")
            // ConfidenceState retains previous tick values — graceful degradation (FM-01)
}
```

## Modified StatusService Constructor

```
// Updated signature:
fn StatusService::new(
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    confidence_state: ConfidenceStateHandle,   // NEW
) -> Self:
    StatusService {
        store,
        vector_index,
        embed_service,
        adapt_service,
        confidence_state,   // NEW field
    }
```

## Data Flow

```
run_maintenance() called by background tick
  |
  +--> Step 2 (confidence refresh, see confidence-refresh-batch.md)
  |      reads snapshot of (alpha0, beta0) from ConfidenceState
  |
  +--> Step 2b (this component)
         spawn_blocking:
           SQL: load (helpful_count, unhelpful_count) for active voted entries
           SQL: load confidence values for all active entries
           compute_empirical_prior(voted_pairs) -> (alpha0, beta0)
           compute_observed_spread(all_confidences) -> observed_spread
           adaptive_confidence_weight(observed_spread) -> confidence_weight
         write lock: ConfidenceState <- {alpha0, beta0, observed_spread, confidence_weight}
```

The SQL queries in Step 2b run in a single `spawn_blocking` acquiring a single
`lock_conn()`. This is one additional `lock_conn()` per tick, consistent with
the existing step 2 pattern.

## Error Handling

- SQL query failure: logged at warn level; Step 2b result is `None`; `ConfidenceState`
  retains previous values (graceful degradation — FM-01).
- `spawn_blocking` join failure (`JoinError`): logged at warn level; same fallback.
- Zero-variance degeneracy: handled inside `compute_empirical_prior` by returning
  cold-start defaults — no panic, no NaN (R-12).
- Empty active population: `compute_observed_spread` returns 0.0, giving
  `confidence_weight = 0.15` (floor) — correct behavior per EC-01.

## Key Test Scenarios

```
// compute_empirical_prior cold-start threshold (R-05):
prior_cold_start_at_nine_entries:
    let pairs = vec![(5, 5); 9]  // 9 entries
    let (a0, b0) = compute_empirical_prior(&pairs)
    assert_eq!(a0, 3.0)
    assert_eq!(b0, 3.0)

prior_empirical_at_ten_entries:
    let pairs = vec![(8, 2); 10]  // 10 entries, p=0.8
    let (a0, b0) = compute_empirical_prior(&pairs)
    // variance is 0 for identical pairs -> falls back to cold-start
    assert_eq!(a0, 3.0)
    assert_eq!(b0, 3.0)

// Mix of rates to get non-zero variance:
prior_empirical_mixed_rates:
    let pairs = vec![
        (10, 0), (8, 2), (6, 4), (4, 6), (2, 8),  // rates: 1.0, 0.8, 0.6, 0.4, 0.2
        (9, 1), (7, 3), (5, 5), (3, 7), (1, 9),    // rates: 0.9, 0.7, 0.5, 0.3, 0.1
    ]
    let (a0, b0) = compute_empirical_prior(&pairs)
    // p_bar = 0.5, variance > 0, should get finite clamped values
    assert!(a0 >= 0.5 && a0 <= 50.0)
    assert!(b0 >= 0.5 && b0 <= 50.0)

// Zero variance degeneracy (R-12):
prior_zero_variance_all_identical:
    let pairs = vec![(10, 0); 10]  // all p=1.0, variance=0
    let (a0, b0) = compute_empirical_prior(&pairs)
    assert_eq!(a0, 3.0)   // falls back to cold-start on zero variance
    assert_eq!(b0, 3.0)

prior_zero_variance_all_unhelpful:
    let pairs = vec![(0, 10); 10]  // all p=0.0, variance=0
    let (a0, b0) = compute_empirical_prior(&pairs)
    assert_eq!(a0, 3.0)
    assert_eq!(b0, 3.0)

// compute_observed_spread:
spread_empty_population:
    assert_eq!(compute_observed_spread(&[]), 0.0)

spread_single_value:
    assert_eq!(compute_observed_spread(&[0.5]), 0.0)  // p95 = p5 = 0.5

spread_known_distribution:
    // 20 uniform values 0.0..=0.19 step 0.01
    let confs: Vec<f64> = (0..20).map(|i| i as f64 * 0.01).collect()
    let spread = compute_observed_spread(&confs)
    // p5 ~= 0.0, p95 ~= 0.19
    assert!(spread > 0.15)

// Clamp verification:
prior_clamp_prevents_extreme_values:
    // Force a scenario that would produce very large alpha0 without clamping:
    // near-zero variance with high mean
    let pairs: Vec<(u32, u32)> = (0..10).map(|_| (999, 1u32)).collect()  // p ~= 0.999
    let (a0, _b0) = compute_empirical_prior(&pairs)
    assert!(a0 <= 50.0, "alpha0 must be clamped to 50.0")
```
