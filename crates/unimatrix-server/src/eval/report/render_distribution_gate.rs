// Distribution Gate renderer (nan-010)
// Full implementation in Wave 3.
use super::AggregateStats;
use super::aggregate::distribution::DistributionGateResult;

pub(super) enum HeadingLevel {
    Single,
    Multi { index: usize },
}

#[allow(dead_code)]
pub(super) fn render_distribution_gate_section(
    _profile_name: &str,
    _gate: &DistributionGateResult,
    _baseline_stats: &AggregateStats,
    _heading_level: HeadingLevel,
) -> String {
    String::new()
}
