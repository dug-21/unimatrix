//! Curation health metrics computation for crt-047.
//!
//! Provides the async `compute_curation_snapshot()` function and all pure
//! compute functions (baseline, sigma comparison, trend, summary).
//!
//! Curation types (`CurationSnapshot`, `CurationBaseline`, `CurationBaselineComparison`,
//! `CurationHealthSummary`, `CurationHealthBlock`, `TrendDirection`) are defined in
//! `unimatrix_observe::types` and re-exported here for service-layer convenience.
//! This avoids duplication and keeps the observe crate as the canonical serialization
//! boundary (ADR-005, crt-047).
//!
//! ## Design decisions
//! - `compute_curation_snapshot()` is the only async function; all others are pure.
//! - SQL uses parameterized binds only — no string interpolation of user input (SEC-01).
//! - Population stddev (divide by n, not n-1) — matches `unimatrix_observe::baseline`.
//! - Zero-stddev and zero-denominator guards prevent NaN propagation (NFR-02).
//! - `compute_curation_snapshot()` delegates to `store.get_curation_snapshot()` which
//!   uses `read_pool()` — correct pool selection for a read-only workload (fixes GH #535).

use unimatrix_core::CoreError;
use unimatrix_observe::{
    CurationBaselineComparison, CurationHealthSummary, CurationSnapshot, TrendDirection,
};
use unimatrix_store::SqlxStore;
use unimatrix_store::cycle_review_index::CurationBaselineRow;

use crate::services::ServiceError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Sigma threshold for flagging anomalous curation activity (both directions).
///
/// Matches `unimatrix_observe::baseline` ADR-003 value (SCOPE OQ-03 resolved).
pub const CURATION_SIGMA_THRESHOLD: f64 = 1.5;

/// Minimum qualifying rows for sigma comparison.
///
/// "Qualifying" means `schema_version >= 2` OR any non-zero snapshot field.
pub const CURATION_MIN_HISTORY: usize = 3;

/// Minimum qualifying rows for trend direction computation.
///
/// Trend requires last-5 vs prior-5 comparison; 6 is the minimum
/// where prior-5 has at least one entry.
pub const CURATION_MIN_TREND_HISTORY: usize = 6;

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Population mean. Returns `0.0` for an empty slice.
fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Population stddev (divide by n). Returns `0.0` for fewer than 2 values.
///
/// `sqrt(0.0) = 0.0` in Rust — not NaN. All-identical values produce `0.0` correctly.
fn population_stddev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let m = mean(values);
    let variance = values.iter().map(|x| (x - m).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

/// Returns `true` when the row contributes real curation data.
///
/// A row is a "legacy DEFAULT" (excluded from MIN_HISTORY count) when `schema_version < 2`
/// AND all snapshot columns are zero. A real zero-correction cycle written at
/// `schema_version = 2` IS included — it is a genuine measured zero, not a migration
/// artefact.
fn is_qualifying_row(row: &CurationBaselineRow) -> bool {
    if row.schema_version >= 2 {
        return true;
    }
    // schema_version < 2: qualifying only if any snapshot field is non-zero.
    row.corrections_total != 0
        || row.corrections_agent != 0
        || row.corrections_human != 0
        || row.deprecations_total != 0
        || row.orphan_deprecations != 0
}

// ---------------------------------------------------------------------------
// Baseline struct (private — only used within this module for intermediate computation)
// ---------------------------------------------------------------------------

/// Rolling mean/stddev aggregate over the baseline window.
///
/// Intermediate type; not serialized. All f64 fields are finite — never NaN.
pub struct CurationBaseline {
    pub corrections_total_mean: f64,
    pub corrections_total_stddev: f64,
    /// Mean orphan ratio: `orphan_deprecations / deprecations_total`; `0.0` when denom=0.
    pub orphan_ratio_mean: f64,
    pub orphan_ratio_stddev: f64,
    /// Number of rows that contributed to this baseline (annotation in output).
    pub history_cycles: usize,
}

// ---------------------------------------------------------------------------
// async: compute_curation_snapshot
// ---------------------------------------------------------------------------

/// Query ENTRIES to compute curation counts for a single feature cycle.
///
/// Delegates to `store.get_curation_snapshot()` which issues three read-only
/// SELECT queries via `read_pool()` — correct pool selection for a read-only
/// workload (GH #535).
///
/// This function MUST be called before `store_cycle_review()` acquires the write
/// connection, so the read completes before the write begins.
///
/// # Errors
/// SQL failure propagates as `ServiceError::Core(CoreError::Store(...))`.
/// The caller (`context_cycle_review` step 8a) treats errors as non-fatal,
/// logging a warning and omitting `curation_health` from the response.
pub async fn compute_curation_snapshot(
    store: &SqlxStore,
    feature_cycle: &str,
    cycle_start_ts: i64,
    review_ts: i64,
) -> Result<CurationSnapshot, ServiceError> {
    let row = store
        .get_curation_snapshot(feature_cycle, cycle_start_ts, review_ts)
        .await
        .map_err(|e| ServiceError::Core(CoreError::Store(e)))?;

    let corrections_agent: u32 = row.corrections_agent.max(0) as u32;
    let corrections_human: u32 = row.corrections_human.max(0) as u32;
    let corrections_system: u32 = row.corrections_system.max(0) as u32;
    // ADR-002: total = agent + human; system is informational and excluded.
    let corrections_total: u32 = corrections_agent + corrections_human;
    let deprecations_total: u32 = row.deprecations_total.max(0) as u32;
    let orphan_deprecations: u32 = row.orphan_deprecations.max(0) as u32;

    Ok(CurationSnapshot {
        corrections_total,
        corrections_agent,
        corrections_human,
        corrections_system,
        deprecations_total,
        orphan_deprecations,
    })
}

// ---------------------------------------------------------------------------
// pure: compute_curation_baseline
// ---------------------------------------------------------------------------

/// Compute rolling mean/stddev for `corrections_total` and `orphan_ratio`.
///
/// Returns `None` when fewer than `CURATION_MIN_HISTORY` qualifying rows exist.
/// `_n` is the requested window size (documentation only; `rows` is already sliced).
///
/// # Qualifying rows
/// A row qualifies when `schema_version >= 2` OR any snapshot field is non-zero.
/// This excludes migrated DEFAULT-0 rows (schema_version < 2, all zeros) while
/// including genuine zero-correction cycles (schema_version = 2, all zeros).
pub fn compute_curation_baseline(
    rows: &[CurationBaselineRow],
    _n: usize,
) -> Option<CurationBaseline> {
    let qualifying: Vec<&CurationBaselineRow> =
        rows.iter().filter(|r| is_qualifying_row(r)).collect();

    if qualifying.len() < CURATION_MIN_HISTORY {
        return None;
    }

    // Orphan ratio per qualifying row (0.0 when deprecations_total = 0 — NFR-02).
    let orphan_ratios: Vec<f64> = qualifying
        .iter()
        .map(|r| {
            if r.deprecations_total == 0 {
                0.0
            } else {
                r.orphan_deprecations as f64 / r.deprecations_total as f64
            }
        })
        .collect();

    let corrections_values: Vec<f64> = qualifying
        .iter()
        .map(|r| r.corrections_total as f64)
        .collect();

    let corrections_total_mean = mean(&corrections_values);
    let corrections_total_stddev = population_stddev(&corrections_values);
    let orphan_ratio_mean = mean(&orphan_ratios);
    let orphan_ratio_stddev = population_stddev(&orphan_ratios);

    Some(CurationBaseline {
        corrections_total_mean,
        corrections_total_stddev,
        orphan_ratio_mean,
        orphan_ratio_stddev,
        history_cycles: qualifying.len(),
    })
}

// ---------------------------------------------------------------------------
// pure: compare_to_baseline
// ---------------------------------------------------------------------------

/// Compute the σ distance of the current snapshot from the rolling baseline.
///
/// Zero stddev produces `sigma = 0.0` (no anomaly — all baseline values are identical).
/// This prevents NaN/+inf from division by zero.
pub fn compare_to_baseline(
    snapshot: &CurationSnapshot,
    baseline: &CurationBaseline,
    history_count: usize,
) -> CurationBaselineComparison {
    // Sigma distance = (observed - mean) / stddev.
    // Zero stddev → define sigma = 0.0.
    let corrections_total_sigma = if baseline.corrections_total_stddev == 0.0 {
        0.0
    } else {
        (snapshot.corrections_total as f64 - baseline.corrections_total_mean)
            / baseline.corrections_total_stddev
    };

    // Current orphan ratio with same zero-denominator guard as the baseline.
    let current_orphan_ratio = if snapshot.deprecations_total == 0 {
        0.0
    } else {
        snapshot.orphan_deprecations as f64 / snapshot.deprecations_total as f64
    };

    let orphan_ratio_sigma = if baseline.orphan_ratio_stddev == 0.0 {
        0.0
    } else {
        (current_orphan_ratio - baseline.orphan_ratio_mean) / baseline.orphan_ratio_stddev
    };

    let within_normal_range = corrections_total_sigma.abs() <= CURATION_SIGMA_THRESHOLD
        && orphan_ratio_sigma.abs() <= CURATION_SIGMA_THRESHOLD;

    CurationBaselineComparison {
        corrections_total_sigma,
        orphan_ratio_sigma,
        history_cycles: history_count,
        within_normal_range,
    }
}

// ---------------------------------------------------------------------------
// pure: compute_trend
// ---------------------------------------------------------------------------

/// Compute trend direction by comparing mean of most-recent 5 rows vs prior 5 rows.
///
/// `rows` is ordered by `first_computed_at DESC` (newest first).
/// Returns `None` when fewer than `CURATION_MIN_TREND_HISTORY` qualifying rows exist.
///
/// The noise floor is the population stddev of all qualifying values. A delta smaller
/// than the noise floor is treated as `Stable`, avoiding false positives on noisy data.
pub fn compute_trend(rows: &[CurationBaselineRow]) -> Option<TrendDirection> {
    let qualifying: Vec<&CurationBaselineRow> =
        rows.iter().filter(|r| is_qualifying_row(r)).collect();

    if qualifying.len() < CURATION_MIN_TREND_HISTORY {
        return None;
    }

    // "last 5" = qualifying[0..5] (most recent 5 cycles, newest first).
    // "prior 5" = qualifying[5..] (cycles 6+ in the window).
    let split = 5.min(qualifying.len());
    let recent = &qualifying[..split];
    let prior = &qualifying[split..];

    if prior.is_empty() {
        // Unreachable when len >= 6, but guard defensively.
        return None;
    }

    let recent_mean = mean(
        &recent
            .iter()
            .map(|r| r.corrections_total as f64)
            .collect::<Vec<_>>(),
    );
    let prior_mean = mean(
        &prior
            .iter()
            .map(|r| r.corrections_total as f64)
            .collect::<Vec<_>>(),
    );

    let delta = recent_mean - prior_mean;

    // Noise floor: population stddev of all qualifying values.
    // When all values are identical: noise_floor = 0.0, delta = 0.0 → Stable (correct).
    let all_values: Vec<f64> = qualifying
        .iter()
        .map(|r| r.corrections_total as f64)
        .collect();
    let noise_floor = population_stddev(&all_values);

    let direction = if delta > noise_floor {
        TrendDirection::Increasing
    } else if delta < -noise_floor {
        TrendDirection::Decreasing
    } else {
        TrendDirection::Stable
    };

    Some(direction)
}

// ---------------------------------------------------------------------------
// pure: compute_curation_summary
// ---------------------------------------------------------------------------

/// Aggregate the baseline window into a `CurationHealthSummary` for `context_status`.
///
/// Returns `None` when `rows` is empty.
/// `cycles_in_window` reflects ALL rows (including legacy DEFAULT-0), for transparency.
pub fn compute_curation_summary(rows: &[CurationBaselineRow]) -> Option<CurationHealthSummary> {
    if rows.is_empty() {
        return None;
    }

    let qualifying: Vec<&CurationBaselineRow> =
        rows.iter().filter(|r| is_qualifying_row(r)).collect();

    // cycles_in_window = all rows (including legacy), for transparency in output.
    let cycles_in_window = rows.len();

    let corrections_values: Vec<f64> = qualifying
        .iter()
        .map(|r| r.corrections_total as f64)
        .collect();

    let correction_rate_mean = if qualifying.is_empty() {
        0.0
    } else {
        mean(&corrections_values)
    };
    let correction_rate_stddev = if qualifying.is_empty() {
        0.0
    } else {
        population_stddev(&corrections_values)
    };

    // Source breakdown: agent% and human% of total corrections across qualifying rows.
    let total_corrections_sum: f64 = qualifying.iter().map(|r| r.corrections_total as f64).sum();
    let total_agent_sum: f64 = qualifying.iter().map(|r| r.corrections_agent as f64).sum();
    let total_human_sum: f64 = qualifying.iter().map(|r| r.corrections_human as f64).sum();

    let agent_pct = if total_corrections_sum == 0.0 {
        0.0
    } else {
        total_agent_sum / total_corrections_sum * 100.0
    };
    let human_pct = if total_corrections_sum == 0.0 {
        0.0
    } else {
        total_human_sum / total_corrections_sum * 100.0
    };

    // Orphan ratio per qualifying row (0.0 when deprecations_total = 0 — NFR-02).
    let orphan_ratios: Vec<f64> = qualifying
        .iter()
        .map(|r| {
            if r.deprecations_total == 0 {
                0.0
            } else {
                r.orphan_deprecations as f64 / r.deprecations_total as f64
            }
        })
        .collect();

    let orphan_ratio_mean = if qualifying.is_empty() {
        0.0
    } else {
        mean(&orphan_ratios)
    };
    let orphan_ratio_stddev = if qualifying.is_empty() {
        0.0
    } else {
        population_stddev(&orphan_ratios)
    };

    // Trend uses ALL rows (including legacy), consistent with compute_trend signature.
    let trend = compute_trend(rows);

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
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: make a CurationBaselineRow at schema_version=2 with given corrections_total.
    fn row_v2(corrections_total: i64) -> CurationBaselineRow {
        CurationBaselineRow {
            corrections_total,
            corrections_agent: corrections_total,
            corrections_human: 0,
            deprecations_total: 0,
            orphan_deprecations: 0,
            schema_version: 2,
        }
    }

    // Helper: make a legacy DEFAULT-0 row (schema_version=1, all zeros).
    fn row_legacy_zero() -> CurationBaselineRow {
        CurationBaselineRow {
            corrections_total: 0,
            corrections_agent: 0,
            corrections_human: 0,
            deprecations_total: 0,
            orphan_deprecations: 0,
            schema_version: 1,
        }
    }

    // -----------------------------------------------------------------------
    // CH-U-23: Constant assertions (AC-16)
    // -----------------------------------------------------------------------

    #[test]
    fn test_curation_sigma_threshold_constant() {
        assert!(
            (CURATION_SIGMA_THRESHOLD - 1.5_f64).abs() < f64::EPSILON,
            "CURATION_SIGMA_THRESHOLD must be 1.5"
        );
    }

    #[test]
    fn test_curation_min_history_constant() {
        assert_eq!(CURATION_MIN_HISTORY, 3);
    }

    #[test]
    fn test_curation_min_trend_history_constant() {
        assert_eq!(CURATION_MIN_TREND_HISTORY, 6);
    }

    // -----------------------------------------------------------------------
    // Private helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_mean_empty_returns_zero() {
        assert_eq!(mean(&[]), 0.0);
    }

    #[test]
    fn test_mean_single_value() {
        assert!((mean(&[5.0]) - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_mean_multiple_values() {
        assert!((mean(&[1.0, 2.0, 3.0]) - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_population_stddev_empty_returns_zero() {
        assert_eq!(population_stddev(&[]), 0.0);
    }

    #[test]
    fn test_population_stddev_single_value_returns_zero() {
        assert_eq!(population_stddev(&[42.0]), 0.0);
    }

    #[test]
    fn test_population_stddev_identical_values_returns_zero() {
        assert_eq!(population_stddev(&[3.0, 3.0, 3.0]), 0.0);
    }

    #[test]
    fn test_population_stddev_known_values() {
        // [1, 2, 3]: mean=2, variance=((1-2)^2+(2-2)^2+(3-2)^2)/3 = 2/3
        let stddev = population_stddev(&[1.0, 2.0, 3.0]);
        let expected = (2.0_f64 / 3.0).sqrt();
        assert!(
            (stddev - expected).abs() < 1e-10,
            "stddev={stddev} expected={expected}"
        );
        assert!(!stddev.is_nan());
    }

    // -----------------------------------------------------------------------
    // CH-U-08: compute_curation_baseline — empty input returns None (AC-15a)
    // -----------------------------------------------------------------------

    #[test]
    fn test_baseline_empty_input_returns_none() {
        assert!(compute_curation_baseline(&[], 10).is_none());
    }

    // -----------------------------------------------------------------------
    // CH-U-09: 2 real rows returns None (AC-15b)
    // -----------------------------------------------------------------------

    #[test]
    fn test_baseline_two_rows_below_min_history_returns_none() {
        let rows = vec![row_v2(5), row_v2(10)];
        assert!(compute_curation_baseline(&rows, 10).is_none());
    }

    // -----------------------------------------------------------------------
    // CH-U-10: 3 real rows returns Some with correct mean/stddev (AC-15c)
    // -----------------------------------------------------------------------

    #[test]
    fn test_baseline_three_rows_returns_correct_mean_stddev() {
        let rows = vec![
            CurationBaselineRow {
                corrections_total: 2,
                corrections_agent: 2,
                corrections_human: 0,
                deprecations_total: 1,
                orphan_deprecations: 1,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 4,
                corrections_agent: 4,
                corrections_human: 0,
                deprecations_total: 1,
                orphan_deprecations: 1,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 6,
                corrections_agent: 6,
                corrections_human: 0,
                deprecations_total: 1,
                orphan_deprecations: 1,
                schema_version: 2,
            },
        ];

        let baseline = compute_curation_baseline(&rows, 10).expect("should return Some");
        assert!(
            (baseline.corrections_total_mean - 4.0).abs() < f64::EPSILON,
            "mean must be 4.0, got {}",
            baseline.corrections_total_mean
        );
        // population stddev([2,4,6]): variance=((2-4)^2+(4-4)^2+(6-4)^2)/3 = 8/3
        let expected_stddev = (8.0_f64 / 3.0).sqrt();
        assert!(
            (baseline.corrections_total_stddev - expected_stddev).abs() < 1e-10,
            "stddev must be ≈{expected_stddev}, got {}",
            baseline.corrections_total_stddev
        );
        assert!(!baseline.corrections_total_stddev.is_nan());
        assert!(!baseline.orphan_ratio_mean.is_nan());
        assert_eq!(baseline.history_cycles, 3);
    }

    // -----------------------------------------------------------------------
    // CH-U-11: Zero stddev handled without NaN (AC-15d)
    // -----------------------------------------------------------------------

    #[test]
    fn test_baseline_zero_stddev_not_nan() {
        let rows = vec![
            CurationBaselineRow {
                corrections_total: 5,
                corrections_agent: 5,
                corrections_human: 0,
                deprecations_total: 1,
                orphan_deprecations: 0,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 5,
                corrections_agent: 5,
                corrections_human: 0,
                deprecations_total: 1,
                orphan_deprecations: 0,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 5,
                corrections_agent: 5,
                corrections_human: 0,
                deprecations_total: 1,
                orphan_deprecations: 0,
                schema_version: 2,
            },
        ];

        let baseline = compute_curation_baseline(&rows, 10).expect("should return Some");
        assert_eq!(baseline.corrections_total_stddev, 0.0);
        assert!(!baseline.corrections_total_stddev.is_nan());
        assert!((baseline.corrections_total_mean - 5.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // CH-U-12: Zero deprecations_total produces orphan_ratio = 0.0 (AC-15e, R-06)
    // -----------------------------------------------------------------------

    #[test]
    fn test_baseline_zero_deprecations_produces_zero_ratio() {
        let rows = vec![
            CurationBaselineRow {
                corrections_total: 1,
                corrections_agent: 1,
                corrections_human: 0,
                deprecations_total: 0,
                orphan_deprecations: 5,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 1,
                corrections_agent: 1,
                corrections_human: 0,
                deprecations_total: 0,
                orphan_deprecations: 5,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 1,
                corrections_agent: 1,
                corrections_human: 0,
                deprecations_total: 0,
                orphan_deprecations: 5,
                schema_version: 2,
            },
        ];

        let baseline = compute_curation_baseline(&rows, 10).expect("should return Some");
        assert!(!baseline.orphan_ratio_mean.is_nan());
        assert_eq!(baseline.orphan_ratio_mean, 0.0);
        assert_eq!(baseline.orphan_ratio_stddev, 0.0);
    }

    // -----------------------------------------------------------------------
    // CH-U-13: Mixed zero/non-zero deprecations_total produces finite values (R-06)
    // -----------------------------------------------------------------------

    #[test]
    fn test_baseline_mixed_zero_nonzero_deprecations_finite() {
        let rows = vec![
            CurationBaselineRow {
                corrections_total: 2,
                corrections_agent: 2,
                corrections_human: 0,
                deprecations_total: 0,
                orphan_deprecations: 0,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 2,
                corrections_agent: 2,
                corrections_human: 0,
                deprecations_total: 0,
                orphan_deprecations: 0,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 3,
                corrections_agent: 3,
                corrections_human: 0,
                deprecations_total: 2,
                orphan_deprecations: 1,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 3,
                corrections_agent: 3,
                corrections_human: 0,
                deprecations_total: 4,
                orphan_deprecations: 2,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 3,
                corrections_agent: 3,
                corrections_human: 0,
                deprecations_total: 6,
                orphan_deprecations: 3,
                schema_version: 2,
            },
        ];

        let baseline = compute_curation_baseline(&rows, 10).expect("should return Some");
        assert!(
            !baseline.orphan_ratio_mean.is_nan(),
            "orphan_ratio_mean must not be NaN"
        );
        assert!(
            !baseline.orphan_ratio_stddev.is_nan(),
            "orphan_ratio_stddev must not be NaN"
        );
        assert!(baseline.orphan_ratio_mean.is_finite());
        assert!(baseline.orphan_ratio_stddev.is_finite());
    }

    // -----------------------------------------------------------------------
    // CH-U-14: Legacy DEFAULT-0 rows excluded from MIN_HISTORY count (AC-15f, R-05)
    // -----------------------------------------------------------------------

    #[test]
    fn test_baseline_excludes_legacy_zero_rows_from_min_history() {
        let mut rows: Vec<CurationBaselineRow> = (0..5).map(|_| row_legacy_zero()).collect();
        rows.push(CurationBaselineRow {
            corrections_total: 3,
            corrections_agent: 3,
            corrections_human: 0,
            deprecations_total: 1,
            orphan_deprecations: 0,
            schema_version: 2,
        });
        rows.push(CurationBaselineRow {
            corrections_total: 5,
            corrections_agent: 5,
            corrections_human: 0,
            deprecations_total: 2,
            orphan_deprecations: 1,
            schema_version: 2,
        });

        // 5 legacy + 2 real = only 2 qualifying → below MIN_HISTORY (3).
        let result = compute_curation_baseline(&rows, 10);
        assert!(
            result.is_none(),
            "2 qualifying rows must return None (below MIN_HISTORY=3)"
        );
    }

    // -----------------------------------------------------------------------
    // CH-U-15: Genuine zero-correction cycle IS included (R-05)
    // -----------------------------------------------------------------------

    #[test]
    fn test_baseline_genuine_zero_cycle_counts_toward_min_history() {
        let rows: Vec<CurationBaselineRow> = (0..3)
            .map(|_| CurationBaselineRow {
                corrections_total: 0,
                corrections_agent: 0,
                corrections_human: 0,
                deprecations_total: 0,
                orphan_deprecations: 0,
                schema_version: 2, // Real cycle at schema_version=2
            })
            .collect();

        let result = compute_curation_baseline(&rows, 10);
        assert!(
            result.is_some(),
            "genuine zero-correction cycles must qualify"
        );
        let baseline = result.unwrap();
        assert_eq!(baseline.history_cycles, 3);
    }

    // -----------------------------------------------------------------------
    // Cold-start boundary tests (AC-R05, R-11)
    // -----------------------------------------------------------------------

    #[test]
    fn test_baseline_boundary_2_rows() {
        let rows = vec![row_v2(1), row_v2(2)];
        assert!(
            compute_curation_baseline(&rows, 10).is_none(),
            "2 rows → None"
        );
    }

    #[test]
    fn test_baseline_boundary_3_rows() {
        let rows = vec![row_v2(1), row_v2(2), row_v2(3)];
        let result = compute_curation_baseline(&rows, 10);
        assert!(result.is_some(), "3 rows → Some");
        assert_eq!(result.unwrap().history_cycles, 3);
    }

    #[test]
    fn test_baseline_boundary_5_rows() {
        let rows: Vec<CurationBaselineRow> = (1..=5).map(|i| row_v2(i as i64)).collect();
        // Baseline: Some (5 >= MIN_HISTORY=3).
        assert!(compute_curation_baseline(&rows, 10).is_some());
        // Trend: None (5 < MIN_TREND_HISTORY=6).
        assert!(compute_trend(&rows).is_none());
    }

    #[test]
    fn test_baseline_boundary_6_rows() {
        let rows: Vec<CurationBaselineRow> = (1..=6).map(|i| row_v2(i as i64)).collect();
        // Baseline: Some.
        assert!(compute_curation_baseline(&rows, 10).is_some());
        // Trend: Some (6 >= MIN_TREND_HISTORY=6).
        assert!(compute_trend(&rows).is_some());
    }

    #[test]
    fn test_baseline_boundary_10_rows() {
        let rows: Vec<CurationBaselineRow> = (1..=10).map(|i| row_v2(i as i64)).collect();
        let result = compute_curation_baseline(&rows, 10);
        assert!(result.is_some());
        assert_eq!(result.unwrap().history_cycles, 10);
    }

    // -----------------------------------------------------------------------
    // CH-U-16: compare_to_baseline — sigma calculation (AC-07)
    // -----------------------------------------------------------------------

    #[test]
    fn test_compare_to_baseline_sigma_calculation() {
        let baseline = CurationBaseline {
            corrections_total_mean: 4.0,
            corrections_total_stddev: 2.0,
            orphan_ratio_mean: 0.0,
            orphan_ratio_stddev: 0.0,
            history_cycles: 5,
        };
        let snapshot = CurationSnapshot {
            corrections_total: 8,
            corrections_agent: 8,
            corrections_human: 0,
            corrections_system: 0,
            deprecations_total: 0,
            orphan_deprecations: 0,
        };

        let comparison = compare_to_baseline(&snapshot, &baseline, 5);
        // sigma = (8 - 4) / 2 = 2.0
        assert!(
            (comparison.corrections_total_sigma - 2.0).abs() < f64::EPSILON,
            "sigma must be 2.0, got {}",
            comparison.corrections_total_sigma
        );
        assert!(
            !comparison.within_normal_range,
            "2.0 > 1.5 threshold → not within normal range"
        );
    }

    // -----------------------------------------------------------------------
    // CH-U-17: within_normal_range = true when both sigma <= 1.5
    // -----------------------------------------------------------------------

    #[test]
    fn test_compare_to_baseline_within_normal_range() {
        let baseline = CurationBaseline {
            corrections_total_mean: 4.0,
            corrections_total_stddev: 2.0,
            orphan_ratio_mean: 0.0,
            orphan_ratio_stddev: 0.0,
            history_cycles: 5,
        };
        let snapshot = CurationSnapshot {
            corrections_total: 5,
            corrections_agent: 5,
            corrections_human: 0,
            corrections_system: 0,
            deprecations_total: 0,
            orphan_deprecations: 0,
        };

        let comparison = compare_to_baseline(&snapshot, &baseline, 5);
        // sigma = (5 - 4) / 2 = 0.5
        assert!(
            (comparison.corrections_total_sigma - 0.5).abs() < f64::EPSILON,
            "sigma must be ≈0.5, got {}",
            comparison.corrections_total_sigma
        );
        assert!(
            comparison.within_normal_range,
            "0.5 <= 1.5 → within normal range"
        );
    }

    #[test]
    fn test_compare_to_baseline_zero_stddev_produces_zero_sigma() {
        let baseline = CurationBaseline {
            corrections_total_mean: 5.0,
            corrections_total_stddev: 0.0,
            orphan_ratio_mean: 0.0,
            orphan_ratio_stddev: 0.0,
            history_cycles: 3,
        };
        let snapshot = CurationSnapshot {
            corrections_total: 10,
            corrections_agent: 10,
            corrections_human: 0,
            corrections_system: 0,
            deprecations_total: 0,
            orphan_deprecations: 0,
        };

        let comparison = compare_to_baseline(&snapshot, &baseline, 3);
        assert_eq!(
            comparison.corrections_total_sigma, 0.0,
            "zero stddev must produce sigma=0.0, not NaN"
        );
        assert!(!comparison.corrections_total_sigma.is_nan());
        assert!(comparison.within_normal_range);
    }

    // -----------------------------------------------------------------------
    // CH-U-18: compute_trend — fewer than 6 rows returns None (AC-10 boundary)
    // -----------------------------------------------------------------------

    #[test]
    fn test_trend_fewer_than_six_rows_returns_none() {
        let rows: Vec<CurationBaselineRow> = (0..5).map(|_| row_v2(3)).collect();
        assert!(compute_trend(&rows).is_none());
    }

    // -----------------------------------------------------------------------
    // CH-U-19: Exactly 6 rows returns Some (AC-10 boundary, inclusive)
    // -----------------------------------------------------------------------

    #[test]
    fn test_trend_exactly_six_rows_returns_some() {
        let rows: Vec<CurationBaselineRow> = (0..6).map(|_| row_v2(3)).collect();
        assert!(compute_trend(&rows).is_some());
    }

    // -----------------------------------------------------------------------
    // CH-U-20: Increasing trend detected
    // -----------------------------------------------------------------------

    #[test]
    fn test_trend_increasing() {
        // rows ordered newest-first: recent[0..5] have high corrections, prior[5..10] have low.
        let mut rows: Vec<CurationBaselineRow> = (0..5).map(|_| row_v2(10)).collect();
        rows.extend((0..5).map(|_| row_v2(1)));

        let trend = compute_trend(&rows);
        assert!(
            matches!(trend, Some(TrendDirection::Increasing)),
            "expected Increasing, got {trend:?}"
        );
    }

    // -----------------------------------------------------------------------
    // CH-U-21: Decreasing trend detected
    // -----------------------------------------------------------------------

    #[test]
    fn test_trend_decreasing() {
        // recent[0..5] = 1, prior[5..10] = 10
        let mut rows: Vec<CurationBaselineRow> = (0..5).map(|_| row_v2(1)).collect();
        rows.extend((0..5).map(|_| row_v2(10)));

        let trend = compute_trend(&rows);
        assert!(
            matches!(trend, Some(TrendDirection::Decreasing)),
            "expected Decreasing, got {trend:?}"
        );
    }

    // -----------------------------------------------------------------------
    // CH-U-22: Stable trend when means are equal
    // -----------------------------------------------------------------------

    #[test]
    fn test_trend_stable_when_means_equal() {
        let rows: Vec<CurationBaselineRow> = (0..10).map(|_| row_v2(3)).collect();
        let trend = compute_trend(&rows);
        assert!(
            matches!(trend, Some(TrendDirection::Stable)),
            "expected Stable, got {trend:?}"
        );
    }

    // -----------------------------------------------------------------------
    // compute_curation_summary — pure function tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_summary_empty_rows_returns_none() {
        assert!(compute_curation_summary(&[]).is_none());
    }

    #[test]
    fn test_summary_single_row_returns_some() {
        let rows = vec![CurationBaselineRow {
            corrections_total: 4,
            corrections_agent: 3,
            corrections_human: 1,
            deprecations_total: 2,
            orphan_deprecations: 1,
            schema_version: 2,
        }];
        let summary = compute_curation_summary(&rows).expect("should return Some");
        assert_eq!(summary.cycles_in_window, 1);
        assert!((summary.correction_rate_mean - 4.0).abs() < f64::EPSILON);
        // agent_pct = 3/4 * 100 = 75%
        assert!((summary.agent_pct - 75.0).abs() < f64::EPSILON);
        // human_pct = 1/4 * 100 = 25%
        assert!((summary.human_pct - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_summary_zero_corrections_pcts_are_zero() {
        let rows = vec![
            CurationBaselineRow {
                corrections_total: 0,
                corrections_agent: 0,
                corrections_human: 0,
                deprecations_total: 0,
                orphan_deprecations: 0,
                schema_version: 2,
            };
            3
        ];
        let summary = compute_curation_summary(&rows).expect("should return Some");
        assert_eq!(summary.agent_pct, 0.0, "zero corrections → agent_pct = 0.0");
        assert_eq!(summary.human_pct, 0.0, "zero corrections → human_pct = 0.0");
        assert!(!summary.agent_pct.is_nan());
        assert!(!summary.human_pct.is_nan());
    }

    #[test]
    fn test_summary_includes_trend_when_enough_rows() {
        let rows: Vec<CurationBaselineRow> = (0..6).map(|_| row_v2(3)).collect();
        let summary = compute_curation_summary(&rows).expect("should return Some");
        assert!(summary.trend.is_some(), "6 rows → trend must be Some");
        assert_eq!(summary.cycles_in_window, 6);
    }

    #[test]
    fn test_summary_no_trend_when_fewer_than_six_rows() {
        let rows: Vec<CurationBaselineRow> = (0..5).map(|_| row_v2(3)).collect();
        let summary = compute_curation_summary(&rows).expect("should return Some");
        assert!(summary.trend.is_none(), "5 rows → trend must be None");
    }

    #[test]
    fn test_summary_nan_free_with_all_zero_deprecations() {
        let rows: Vec<CurationBaselineRow> = (0..3)
            .map(|_| CurationBaselineRow {
                corrections_total: 2,
                corrections_agent: 2,
                corrections_human: 0,
                deprecations_total: 0,
                orphan_deprecations: 0,
                schema_version: 2,
            })
            .collect();
        let summary = compute_curation_summary(&rows).expect("should return Some");
        assert!(!summary.orphan_ratio_mean.is_nan());
        assert!(!summary.orphan_ratio_stddev.is_nan());
        assert!(!summary.correction_rate_mean.is_nan());
        assert!(!summary.correction_rate_stddev.is_nan());
    }

    // -----------------------------------------------------------------------
    // Async tests: compute_curation_snapshot
    // -----------------------------------------------------------------------

    #[cfg(feature = "test-support")]
    mod snapshot_tests {
        use super::*;
        use std::time::{SystemTime, UNIX_EPOCH};
        use unimatrix_store::test_helpers::open_test_store;
        use unimatrix_store::{NewEntry, Status};

        fn now_ts() -> i64 {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
        }

        /// Insert a minimal entry and optionally set supersedes/superseded_by.
        async fn insert_entry(
            store: &SqlxStore,
            feature_cycle: &str,
            trust_source: &str,
            supersedes: Option<u64>,
            status: Status,
            superseded_by: Option<u64>,
        ) -> u64 {
            let id = store
                .insert(NewEntry {
                    title: format!("test-{trust_source}-{feature_cycle}"),
                    content: "content".to_string(),
                    topic: "test".to_string(),
                    category: "convention".to_string(),
                    tags: vec![],
                    source: "test".to_string(),
                    status,
                    created_by: "test".to_string(),
                    feature_cycle: feature_cycle.to_string(),
                    trust_source: trust_source.to_string(),
                })
                .await
                .expect("insert entry");

            if supersedes.is_some() || superseded_by.is_some() {
                let pool = store.write_pool_server();
                sqlx::query("UPDATE entries SET supersedes = ?1, superseded_by = ?2 WHERE id = ?3")
                    .bind(supersedes.map(|v| v as i64))
                    .bind(superseded_by.map(|v| v as i64))
                    .bind(id as i64)
                    .execute(pool)
                    .await
                    .expect("update supersedes/superseded_by");
            }

            id
        }

        // CH-U-01: Corrections counted by feature_cycle join key (AC-02, R-01)
        #[tokio::test]
        async fn test_compute_snapshot_corrections_use_feature_cycle_not_audit_log() {
            let dir = tempfile::tempdir().expect("tempdir");
            let store = open_test_store(&dir).await;
            let now = now_ts();

            // Two entries in target cycle with supersedes set.
            insert_entry(
                &store,
                "crt-047-test",
                "agent",
                Some(99),
                Status::Active,
                None,
            )
            .await;
            insert_entry(
                &store,
                "crt-047-test",
                "agent",
                Some(98),
                Status::Active,
                None,
            )
            .await;
            // One entry in different cycle — must NOT be counted.
            insert_entry(
                &store,
                "other-cycle",
                "agent",
                Some(97),
                Status::Active,
                None,
            )
            .await;

            let snapshot = compute_curation_snapshot(&store, "crt-047-test", 0, now + 10)
                .await
                .expect("snapshot must succeed");

            assert_eq!(
                snapshot.corrections_total, 2,
                "only crt-047-test entries with supersedes must be counted"
            );
        }

        // CH-U-02: Trust source bucketing — all six values (AC-03, R-04)
        #[tokio::test]
        async fn test_trust_source_bucketing_all_values() {
            let dir = tempfile::tempdir().expect("tempdir");
            let store = open_test_store(&dir).await;
            let now = now_ts();

            // agent × 2
            insert_entry(
                &store,
                "bucket-test",
                "agent",
                Some(10),
                Status::Active,
                None,
            )
            .await;
            insert_entry(
                &store,
                "bucket-test",
                "agent",
                Some(11),
                Status::Active,
                None,
            )
            .await;
            // human × 1
            insert_entry(
                &store,
                "bucket-test",
                "human",
                Some(12),
                Status::Active,
                None,
            )
            .await;
            // privileged × 1 (counts as human)
            insert_entry(
                &store,
                "bucket-test",
                "privileged",
                Some(13),
                Status::Active,
                None,
            )
            .await;
            // system × 1
            insert_entry(
                &store,
                "bucket-test",
                "system",
                Some(14),
                Status::Active,
                None,
            )
            .await;
            // direct × 1
            insert_entry(
                &store,
                "bucket-test",
                "direct",
                Some(15),
                Status::Active,
                None,
            )
            .await;
            // unknown-future × 1 (fail-safe bucket)
            insert_entry(
                &store,
                "bucket-test",
                "unknown-future",
                Some(16),
                Status::Active,
                None,
            )
            .await;

            let snapshot = compute_curation_snapshot(&store, "bucket-test", 0, now + 10)
                .await
                .expect("snapshot must succeed");

            assert_eq!(snapshot.corrections_agent, 2, "agent count must be 2");
            assert_eq!(
                snapshot.corrections_human, 2,
                "human+privileged count must be 2"
            );
            assert_eq!(
                snapshot.corrections_system, 3,
                "system+direct+unknown-future count must be 3"
            );
            assert_eq!(
                snapshot.corrections_total, 4,
                "total = agent + human only (NOT system)"
            );
            assert_ne!(
                snapshot.corrections_total, 7,
                "total must NOT include system bucket"
            );
        }

        // CH-U-03: Orphan deprecations — ENTRIES-only, superseded_by IS NULL (AC-04, R-01)
        #[tokio::test]
        async fn test_orphan_deprecations_entries_only_no_audit_log() {
            let dir = tempfile::tempdir().expect("tempdir");
            let store = open_test_store(&dir).await;
            let now = now_ts();

            // Entry A: chain-deprecated (superseded_by IS NOT NULL), updated_at in window.
            insert_entry(
                &store,
                "orphan-test",
                "agent",
                None,
                Status::Deprecated,
                Some(99),
            )
            .await;

            // Entry B: orphan (superseded_by IS NULL), updated_at in window.
            insert_entry(
                &store,
                "orphan-test",
                "agent",
                None,
                Status::Deprecated,
                None,
            )
            .await;

            // Entry C: orphan, but updated_at OUTSIDE window (before cycle_start).
            let entry_c = insert_entry(
                &store,
                "orphan-test",
                "agent",
                None,
                Status::Deprecated,
                None,
            )
            .await;
            let cycle_start = now - 100;
            sqlx::query("UPDATE entries SET updated_at = ?1 WHERE id = ?2")
                .bind(cycle_start - 10)
                .bind(entry_c as i64)
                .execute(store.write_pool_server())
                .await
                .expect("backdate entry C");

            let snapshot = compute_curation_snapshot(&store, "orphan-test", cycle_start, now + 10)
                .await
                .expect("snapshot must succeed");

            assert_eq!(
                snapshot.orphan_deprecations, 1,
                "only entry B (in-window orphan) must be counted"
            );
            assert_eq!(
                snapshot.deprecations_total, 2,
                "entries A and B (both in window) must be counted"
            );
        }

        // CH-U-04: Chain deprecations excluded from orphan count (AC-04)
        #[tokio::test]
        async fn test_chain_deprecations_not_counted_as_orphans() {
            let dir = tempfile::tempdir().expect("tempdir");
            let store = open_test_store(&dir).await;
            let now = now_ts();

            insert_entry(
                &store,
                "chain-test",
                "agent",
                None,
                Status::Deprecated,
                Some(42),
            )
            .await;

            let snapshot = compute_curation_snapshot(&store, "chain-test", 0, now + 10)
                .await
                .expect("snapshot must succeed");

            assert_eq!(
                snapshot.orphan_deprecations, 0,
                "chain-deprecated must not be an orphan"
            );
            assert_eq!(snapshot.deprecations_total, 1, "one deprecation in window");
        }

        // CH-U-05: Out-of-window orphan excluded (AC-18, R-14)
        #[tokio::test]
        async fn test_orphan_outside_cycle_window_not_counted() {
            let dir = tempfile::tempdir().expect("tempdir");
            let store = open_test_store(&dir).await;
            let now = now_ts();
            let cycle_start = now;

            let entry_id = insert_entry(
                &store,
                "window-test",
                "agent",
                None,
                Status::Deprecated,
                None,
            )
            .await;

            sqlx::query("UPDATE entries SET updated_at = ?1 WHERE id = ?2")
                .bind(cycle_start - 1)
                .bind(entry_id as i64)
                .execute(store.write_pool_server())
                .await
                .expect("backdate entry");

            let snapshot = compute_curation_snapshot(&store, "window-test", cycle_start, now + 10)
                .await
                .expect("snapshot must succeed");

            assert_eq!(
                snapshot.orphan_deprecations, 0,
                "orphan before cycle_start must be excluded"
            );
        }

        // CH-U-06: deprecations_total is cycle-window only (AC-17)
        #[tokio::test]
        async fn test_deprecations_total_cycle_window_only() {
            let dir = tempfile::tempdir().expect("tempdir");
            let store = open_test_store(&dir).await;
            let now = now_ts();
            let cycle_start = now;

            // Entry A: deprecated, updated_at BEFORE window.
            let a = insert_entry(
                &store,
                "dep-window-test",
                "agent",
                None,
                Status::Deprecated,
                None,
            )
            .await;
            sqlx::query("UPDATE entries SET updated_at = ?1 WHERE id = ?2")
                .bind(cycle_start - 100)
                .bind(a as i64)
                .execute(store.write_pool_server())
                .await
                .expect("backdate A");

            // Entry B: deprecated, updated_at WITHIN window.
            insert_entry(
                &store,
                "dep-window-test",
                "agent",
                None,
                Status::Deprecated,
                None,
            )
            .await;

            // Entry C: deprecated, updated_at AFTER review_ts.
            let c = insert_entry(
                &store,
                "dep-window-test",
                "agent",
                None,
                Status::Deprecated,
                None,
            )
            .await;
            sqlx::query("UPDATE entries SET updated_at = ?1 WHERE id = ?2")
                .bind(now + 1000)
                .bind(c as i64)
                .execute(store.write_pool_server())
                .await
                .expect("set C future");

            let snapshot =
                compute_curation_snapshot(&store, "dep-window-test", cycle_start, now + 10)
                    .await
                    .expect("snapshot must succeed");

            assert_eq!(
                snapshot.deprecations_total, 1,
                "only entry B (in window) must be counted"
            );
        }

        // CH-U-07: Missing cycle_start event fallback (EC-02)
        #[tokio::test]
        async fn test_snapshot_fallback_when_no_cycle_start_event() {
            let dir = tempfile::tempdir().expect("tempdir");
            let store = open_test_store(&dir).await;
            let now = now_ts();

            let result = compute_curation_snapshot(&store, "fallback-test", 0, now + 10).await;
            assert!(
                result.is_ok(),
                "snapshot must not panic with cycle_start_ts=0"
            );
        }
    }
}
