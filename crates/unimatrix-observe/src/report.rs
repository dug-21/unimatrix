//! RetrospectiveReport assembly from computed metrics and hotspot findings.

use std::collections::HashSet;

use crate::types::{BaselineComparison, EntryAnalysis, HotspotFinding, MetricVector, ObservationRecord, RetrospectiveReport};

/// Build a self-contained RetrospectiveReport.
///
/// The report includes session count (distinct session_ids), total record count,
/// the full MetricVector, all hotspot findings, optional baseline comparison,
/// and optional entry-level analysis from accumulated Flagged signals (col-009).
pub fn build_report(
    feature_cycle: &str,
    records: &[ObservationRecord],
    metrics: MetricVector,
    hotspots: Vec<HotspotFinding>,
    baseline: Option<Vec<BaselineComparison>>,
    entries_analysis: Option<Vec<EntryAnalysis>>,
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
        entries_analysis,
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
        let report = build_report("col-002", &records, MetricVector::default(), vec![], None, None);
        assert_eq!(report.session_count, 3);
    }

    #[test]
    fn test_build_report_total_records() {
        let records = vec![make_record("s1"), make_record("s1"), make_record("s2")];
        let report = build_report("col-002", &records, MetricVector::default(), vec![], None, None);
        assert_eq!(report.total_records, 3);
    }

    #[test]
    fn test_build_report_is_not_cached() {
        let report = build_report("col-002", &[], MetricVector::default(), vec![], None, None);
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
        let report = build_report("col-002", &[], MetricVector::default(), hotspots, None, None);
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
        let report = build_report("col-002", &[], mv, vec![], None, None);
        assert_eq!(report.metrics.universal.total_tool_calls, 42);
    }

    #[test]
    fn test_build_report_feature_cycle() {
        let report = build_report("nxs-001", &[], MetricVector::default(), vec![], None, None);
        assert_eq!(report.feature_cycle, "nxs-001");
    }

    #[test]
    fn test_build_report_empty_records() {
        let report = build_report("col-002", &[], MetricVector::default(), vec![], None, None);
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

        let report = build_report("col-002", &records, mv, hotspots, None, None);
        assert_eq!(report.feature_cycle, "col-002");
        assert_eq!(report.session_count, 2);
        assert_eq!(report.total_records, 2);
        assert_eq!(report.metrics.computed_at, 9999);
        assert_eq!(report.metrics.universal.total_tool_calls, 2);
        assert_eq!(report.hotspots.len(), 1);
        assert!(!report.is_cached);
    }

    // -- entries_analysis tests (col-009) --

    #[test]
    fn test_entries_analysis_absent_when_none() {
        let report = build_report("col-009", &[], MetricVector::default(), vec![], None, None);
        let json = serde_json::to_string(&report).expect("serialize");
        // entries_analysis must be absent — not "entries_analysis": null
        assert!(!json.contains("entries_analysis"));
    }

    #[test]
    fn test_entries_analysis_present_when_some() {
        use crate::types::EntryAnalysis;
        let analysis = vec![EntryAnalysis {
            entry_id: 42,
            title: "Test Entry".to_string(),
            category: "decision".to_string(),
            rework_flag_count: 3,
            injection_count: 0,
            success_session_count: 1,
            rework_session_count: 2,
        }];
        let report = build_report("col-009", &[], MetricVector::default(), vec![], None, Some(analysis));
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(json.contains("entries_analysis"));
        assert!(json.contains("\"entry_id\":42"));
    }

    #[test]
    fn test_build_report_with_entries_analysis() {
        use crate::types::EntryAnalysis;
        let analysis = vec![EntryAnalysis {
            entry_id: 10,
            rework_flag_count: 5,
            ..Default::default()
        }];
        let report = build_report("col-009", &[], MetricVector::default(), vec![], None, Some(analysis));
        let entries = report.entries_analysis.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry_id, 10);
        assert_eq!(entries[0].rework_flag_count, 5);
    }

    #[test]
    fn test_build_report_without_entries_analysis() {
        let report = build_report("col-009", &[], MetricVector::default(), vec![], None, None);
        assert!(report.entries_analysis.is_none());
    }

    #[test]
    fn test_entry_analysis_roundtrip() {
        use crate::types::EntryAnalysis;
        let analysis = EntryAnalysis {
            entry_id: 99,
            title: "Rust conventions".to_string(),
            category: "convention".to_string(),
            rework_flag_count: 2,
            injection_count: 0,
            success_session_count: 5,
            rework_session_count: 1,
        };
        let json = serde_json::to_string(&analysis).expect("serialize");
        let back: EntryAnalysis = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.entry_id, 99);
        assert_eq!(back.title, "Rust conventions");
        assert_eq!(back.rework_flag_count, 2);
        assert_eq!(back.success_session_count, 5);
    }

    #[test]
    fn test_entry_analysis_default() {
        use crate::types::EntryAnalysis;
        let ea = EntryAnalysis::default();
        assert_eq!(ea.entry_id, 0);
        assert!(ea.title.is_empty());
        assert!(ea.category.is_empty());
        assert_eq!(ea.rework_flag_count, 0);
        assert_eq!(ea.injection_count, 0);
        assert_eq!(ea.success_session_count, 0);
        assert_eq!(ea.rework_session_count, 0);
    }
}
