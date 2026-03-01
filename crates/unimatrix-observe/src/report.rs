//! RetrospectiveReport assembly from computed metrics and hotspot findings.

use std::collections::HashSet;

use crate::types::{BaselineComparison, HotspotFinding, MetricVector, ObservationRecord, RetrospectiveReport};

/// Build a self-contained RetrospectiveReport.
///
/// The report includes session count (distinct session_ids), total record count,
/// the full MetricVector, all hotspot findings, and optional baseline comparison.
pub fn build_report(
    feature_cycle: &str,
    records: &[ObservationRecord],
    metrics: MetricVector,
    hotspots: Vec<HotspotFinding>,
    baseline: Option<Vec<BaselineComparison>>,
) -> RetrospectiveReport {
    let session_count = records
        .iter()
        .map(|r| r.session_id.as_str())
        .collect::<HashSet<_>>()
        .len();

    RetrospectiveReport {
        feature_cycle: feature_cycle.to_string(),
        session_count,
        total_records: records.len(),
        metrics,
        hotspots,
        is_cached: false,
        baseline_comparison: baseline,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        HookType, HotspotCategory, MetricVector, Severity, UniversalMetrics,
    };

    fn make_record(session: &str) -> ObservationRecord {
        ObservationRecord {
            ts: 1000,
            hook: HookType::PreToolUse,
            session_id: session.to_string(),
            tool: Some("Read".to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        }
    }

    #[test]
    fn test_build_report_session_count() {
        let records = vec![
            make_record("s1"),
            make_record("s1"),
            make_record("s2"),
            make_record("s3"),
        ];
        let report = build_report("col-002", &records, MetricVector::default(), vec![], None);
        assert_eq!(report.session_count, 3);
    }

    #[test]
    fn test_build_report_total_records() {
        let records = vec![make_record("s1"), make_record("s1"), make_record("s2")];
        let report = build_report("col-002", &records, MetricVector::default(), vec![], None);
        assert_eq!(report.total_records, 3);
    }

    #[test]
    fn test_build_report_is_not_cached() {
        let report = build_report("col-002", &[], MetricVector::default(), vec![], None);
        assert!(!report.is_cached);
    }

    #[test]
    fn test_build_report_includes_hotspots() {
        let hotspots = vec![HotspotFinding {
            category: HotspotCategory::Friction,
            severity: Severity::Warning,
            rule_name: "test".to_string(),
            claim: "test finding".to_string(),
            measured: 5.0,
            threshold: 2.0,
            evidence: vec![],
        }];
        let report = build_report("col-002", &[], MetricVector::default(), hotspots, None);
        assert_eq!(report.hotspots.len(), 1);
        assert_eq!(report.hotspots[0].rule_name, "test");
    }

    #[test]
    fn test_build_report_includes_metrics() {
        let mut mv = MetricVector::default();
        mv.universal = UniversalMetrics {
            total_tool_calls: 42,
            ..Default::default()
        };
        let report = build_report("col-002", &[], mv, vec![], None);
        assert_eq!(report.metrics.universal.total_tool_calls, 42);
    }

    #[test]
    fn test_build_report_feature_cycle() {
        let report = build_report("nxs-001", &[], MetricVector::default(), vec![], None);
        assert_eq!(report.feature_cycle, "nxs-001");
    }

    #[test]
    fn test_build_report_empty_records() {
        let report = build_report("col-002", &[], MetricVector::default(), vec![], None);
        assert_eq!(report.session_count, 0);
        assert_eq!(report.total_records, 0);
    }

    #[test]
    fn test_build_report_self_contained() {
        let records = vec![make_record("s1"), make_record("s2")];
        let mut mv = MetricVector::default();
        mv.computed_at = 9999;
        mv.universal.total_tool_calls = 2;

        let hotspots = vec![HotspotFinding {
            category: HotspotCategory::Session,
            severity: Severity::Info,
            rule_name: "timeout".to_string(),
            claim: "session timeout".to_string(),
            measured: 3.0,
            threshold: 2.0,
            evidence: vec![],
        }];

        let report = build_report("col-002", &records, mv, hotspots, None);
        assert_eq!(report.feature_cycle, "col-002");
        assert_eq!(report.session_count, 2);
        assert_eq!(report.total_records, 2);
        assert_eq!(report.metrics.computed_at, 9999);
        assert_eq!(report.metrics.universal.total_tool_calls, 2);
        assert_eq!(report.hotspots.len(), 1);
        assert!(!report.is_cached);
    }
}
