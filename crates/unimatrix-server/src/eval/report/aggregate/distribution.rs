//! Distribution gate aggregation (nan-010, Component 4).
//!
//! Computes whether a distribution-change candidate profile's mean metrics meet
//! the declared targets. `mrr_floor` is a veto (ADR-003): evaluated independently
//! from the diversity targets (CC@k + ICD). The two failure modes are distinguishable
//! via the separate `diversity_passed` and `mrr_floor_passed` booleans.

use super::AggregateStats;
use crate::eval::profile::types::DistributionTargets;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Per-metric gate detail: declared target, observed actual value, and pass/fail.
///
/// `passed` uses `>=` semantics — exact equality at the target boundary passes
/// (ADR-003: edge case clarification).
#[derive(Debug)]
pub struct MetricGateRow {
    pub target: f64,
    pub actual: f64,
    pub passed: bool,
}

/// Full gate result for one distribution-change candidate profile.
///
/// ADR-003: `mrr_floor` is a veto, structurally separate from diversity targets.
/// Four distinct states are possible:
/// - `diversity_passed=true,  mrr_floor_passed=true`  → `overall_passed=true`
/// - `diversity_passed=true,  mrr_floor_passed=false` → `overall_passed=false`
/// - `diversity_passed=false, mrr_floor_passed=true`  → `overall_passed=false`
/// - `diversity_passed=false, mrr_floor_passed=false` → `overall_passed=false`
#[derive(Debug)]
pub struct DistributionGateResult {
    pub cc_at_k: MetricGateRow,
    pub icd: MetricGateRow,
    /// Veto metric — evaluated independently from `cc_at_k` and `icd`.
    pub mrr_floor: MetricGateRow,
    /// `cc_at_k.passed && icd.passed`
    pub diversity_passed: bool,
    /// `mrr_floor.passed` — separate from `diversity_passed` per ADR-003
    pub mrr_floor_passed: bool,
    /// `diversity_passed && mrr_floor_passed`
    pub overall_passed: bool,
}

// ---------------------------------------------------------------------------
// check_distribution_targets
// ---------------------------------------------------------------------------

/// Compare candidate aggregate statistics against declared distribution targets.
///
/// Reads `mean_cc_at_k`, `mean_icd`, `mean_mrr` from `stats` (candidate profile only —
/// never called with baseline stats). All comparisons use `>=`; NaN actuals fail
/// the gate (Rust `>=` on NaN returns false, which is correct behavior for a
/// computation error upstream).
///
/// This function is infallible — all inputs are well-typed f64 values.
pub fn check_distribution_targets(
    stats: &AggregateStats,
    targets: &DistributionTargets,
) -> DistributionGateResult {
    let cc_at_k_actual = stats.mean_cc_at_k;
    let icd_actual = stats.mean_icd;
    let mrr_actual = stats.mean_mrr;

    let cc_row = MetricGateRow {
        target: targets.cc_at_k_min,
        actual: cc_at_k_actual,
        passed: cc_at_k_actual >= targets.cc_at_k_min,
    };

    let icd_row = MetricGateRow {
        target: targets.icd_min,
        actual: icd_actual,
        passed: icd_actual >= targets.icd_min,
    };

    // mrr_floor is a veto (ADR-003): kept structurally separate from diversity.
    let mrr_row = MetricGateRow {
        target: targets.mrr_floor,
        actual: mrr_actual,
        passed: mrr_actual >= targets.mrr_floor,
    };

    let diversity_passed = cc_row.passed && icd_row.passed;
    let mrr_floor_passed = mrr_row.passed;

    DistributionGateResult {
        cc_at_k: cc_row,
        icd: icd_row,
        mrr_floor: mrr_row,
        diversity_passed,
        mrr_floor_passed,
        overall_passed: diversity_passed && mrr_floor_passed,
    }
}
