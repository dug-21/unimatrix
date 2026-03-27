// Distribution gate aggregation (nan-010)
// Full implementation in Wave 2.
use super::AggregateStats;
use crate::eval::profile::types::DistributionTargets;

pub(crate) struct MetricGateRow {
    pub target: f64,
    pub actual: f64,
    pub passed: bool,
}

pub(crate) struct DistributionGateResult {
    pub cc_at_k: MetricGateRow,
    pub icd: MetricGateRow,
    pub mrr_floor: MetricGateRow,
    pub diversity_passed: bool,
    pub mrr_floor_passed: bool,
    pub overall_passed: bool,
}

#[allow(dead_code)]
pub(super) fn check_distribution_targets(
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
