//! RetrospectiveReport assembly from computed metrics and hotspot findings.

use std::collections::HashSet;

use crate::types::{
    BaselineComparison, EntryAnalysis, HotspotFinding, MetricVector, ObservationRecord,
    Recommendation, RetrospectiveReport,
};

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
        narratives: None,
        recommendations: vec![],
        session_summaries: None,
        feature_knowledge_reuse: None,
        rework_session_count: None,
        context_reload_pct: None,
        attribution: None,
        phase_narrative: None,
        goal: None,
        cycle_type: None,
        attribution_path: None,
        is_in_progress: None,
        phase_stats: None,
    }
}

/// Generate actionable recommendations for recognized hotspot types (col-010b).
///
/// Covers four hotspot types: permission_retries, coordinator_respawns,
/// sleep_workarounds, and compile_cycles (only when measured > 10.0).
/// Returns empty Vec for unrecognized types or when no hotspots are recognized.
pub fn recommendations_for_hotspots(hotspots: &[HotspotFinding]) -> Vec<Recommendation> {
    hotspots.iter().filter_map(recommendation_for).collect()
}

fn recommendation_for(hotspot: &HotspotFinding) -> Option<Recommendation> {
    match hotspot.rule_name.as_str() {
        "permission_retries" => Some(Recommendation {
            hotspot_type: "permission_retries".into(),
            action: "Add common build/test commands to settings.json allowlist".into(),
            rationale: format!(
                "{} permission retries detected -- agents lose time waiting for approval",
                hotspot.measured as u64
            ),
        }),
        "coordinator_respawns" => Some(Recommendation {
            hotspot_type: "coordinator_respawns".into(),
            action: "Review coordinator agent lifespan and handoff patterns".into(),
            rationale: format!(
                "{} coordinator respawns detected -- may indicate premature termination or context overflow",
                hotspot.measured as u64
            ),
        }),
        "sleep_workarounds" => Some(Recommendation {
            hotspot_type: "sleep_workarounds".into(),
            action: "Use run_in_background + TaskOutput instead of sleep polling".into(),
            rationale: format!(
                "{} sleep workaround events detected -- sleep polling wastes agent time",
                hotspot.measured as u64
            ),
        }),
        "compile_cycles" if hotspot.measured > 10.0 => Some(Recommendation {
            hotspot_type: "compile_cycles".into(),
            action: "Batch field additions before compiling — high compile cycle counts typically \
                     indicate iterative per-field struct changes or cascading type errors; \
                     complete type definitions and resolve compiler errors in-memory before each build"
                .into(),
            rationale: format!(
                "{:.0} compile cycles detected — each compile-check-fix loop adds 2–6 compile events; \
                 batch changes to logical units (complete struct definitions, full impl blocks) \
                 before building to reduce total compile count",
                hotspot.measured
            ),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{HotspotCategory, MetricVector, Severity, UniversalMetrics};

    fn make_record(session: &str) -> ObservationRecord {
        ObservationRecord {
            ts: 1000,
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
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
        let report = build_report(
            "col-002",
            &records,
            MetricVector::default(),
            vec![],
            None,
            None,
        );
        assert_eq!(report.session_count, 3);
    }

    #[test]
    fn test_build_report_total_records() {
        let records = vec![make_record("s1"), make_record("s1"), make_record("s2")];
        let report = build_report(
            "col-002",
            &records,
            MetricVector::default(),
            vec![],
            None,
            None,
        );
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
        let report = build_report(
            "col-002",
            &[],
            MetricVector::default(),
            hotspots,
            None,
            None,
        );
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
        let report = build_report(
            "col-009",
            &[],
            MetricVector::default(),
            vec![],
            None,
            Some(analysis),
        );
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
        let report = build_report(
            "col-009",
            &[],
            MetricVector::default(),
            vec![],
            None,
            Some(analysis),
        );
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

    // -- col-010b: recommendation tests (T-ES-09) --

    #[test]
    fn test_recommendation_permission_retries() {
        let hotspot = HotspotFinding {
            category: HotspotCategory::Friction,
            severity: Severity::Warning,
            rule_name: "permission_retries".to_string(),
            claim: "retries".to_string(),
            measured: 8.0,
            threshold: 3.0,
            evidence: vec![],
        };
        let recs = recommendations_for_hotspots(&[hotspot]);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].hotspot_type, "permission_retries");
        assert!(recs[0].action.contains("allowlist"));
        assert!(!recs[0].rationale.is_empty());
    }

    #[test]
    fn test_recommendation_coordinator_respawns() {
        let hotspot = HotspotFinding {
            category: HotspotCategory::Agent,
            severity: Severity::Warning,
            rule_name: "coordinator_respawns".to_string(),
            claim: "respawns".to_string(),
            measured: 5.0,
            threshold: 2.0,
            evidence: vec![],
        };
        let recs = recommendations_for_hotspots(&[hotspot]);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].hotspot_type, "coordinator_respawns");
        assert!(recs[0].action.contains("lifespan"));
    }

    #[test]
    fn test_recommendation_sleep_workarounds() {
        let hotspot = HotspotFinding {
            category: HotspotCategory::Friction,
            severity: Severity::Warning,
            rule_name: "sleep_workarounds".to_string(),
            claim: "sleep".to_string(),
            measured: 4.0,
            threshold: 1.0,
            evidence: vec![],
        };
        let recs = recommendations_for_hotspots(&[hotspot]);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].hotspot_type, "sleep_workarounds");
        assert!(recs[0].action.contains("run_in_background"));
    }

    #[test]
    fn test_recommendation_compile_cycles_above_threshold() {
        let hotspot = HotspotFinding {
            category: HotspotCategory::Session,
            severity: Severity::Warning,
            rule_name: "compile_cycles".to_string(),
            claim: "compile".to_string(),
            measured: 15.0,
            threshold: 10.0,
            evidence: vec![],
        };
        let recs = recommendations_for_hotspots(&[hotspot]);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].hotspot_type, "compile_cycles");
        assert!(
            recs[0].action.contains("batch") || recs[0].action.contains("iterative"),
            "compile_cycles action must reference batching or iterative compilation, got: {}",
            recs[0].action
        );
        assert!(
            !recs[0].action.contains("allowlist"),
            "compile_cycles action must not mention allowlist, got: {}",
            recs[0].action
        );
        assert!(
            !recs[0].action.contains("settings.json"),
            "compile_cycles action must not reference settings.json, got: {}",
            recs[0].action
        );
    }

    #[test]
    fn test_recommendation_compile_cycles_below_threshold() {
        let hotspot = HotspotFinding {
            category: HotspotCategory::Session,
            severity: Severity::Info,
            rule_name: "compile_cycles".to_string(),
            claim: "compile".to_string(),
            measured: 5.0,
            threshold: 10.0,
            evidence: vec![],
        };
        let recs = recommendations_for_hotspots(&[hotspot]);
        assert!(recs.is_empty());
    }

    #[test]
    fn test_compile_cycles_action_no_allowlist() {
        // T-CC-01 (AC-19): compile_cycles action must not reference allowlist or settings.json
        let hotspot = HotspotFinding {
            category: HotspotCategory::Session,
            severity: Severity::Warning,
            rule_name: "compile_cycles".to_string(),
            claim: "compile".to_string(),
            measured: 15.0,
            threshold: 10.0,
            evidence: vec![],
        };
        let recs = recommendations_for_hotspots(&[hotspot]);
        assert_eq!(recs.len(), 1);
        assert!(
            !recs[0].action.contains("allowlist"),
            "compile_cycles action must not contain 'allowlist'"
        );
        assert!(
            !recs[0].action.contains("settings.json"),
            "compile_cycles action must not contain 'settings.json'"
        );
    }

    #[test]
    fn test_permission_friction_recommendation_independence() {
        // T-CC-02 (AC-19): permission_retries still uses allowlist; compile_cycles uses batching.
        // The two recommendation templates share no text.
        let pr_hotspot = HotspotFinding {
            category: HotspotCategory::Friction,
            severity: Severity::Warning,
            rule_name: "permission_retries".to_string(),
            claim: "retries".to_string(),
            measured: 8.0,
            threshold: 3.0,
            evidence: vec![],
        };
        let cc_hotspot = HotspotFinding {
            category: HotspotCategory::Session,
            severity: Severity::Warning,
            rule_name: "compile_cycles".to_string(),
            claim: "compile".to_string(),
            measured: 15.0,
            threshold: 10.0,
            evidence: vec![],
        };

        let pr_recs = recommendations_for_hotspots(&[pr_hotspot]);
        let cc_recs = recommendations_for_hotspots(&[cc_hotspot]);

        assert_eq!(pr_recs.len(), 1);
        assert_eq!(cc_recs.len(), 1);

        // permission_retries CAN reference allowlist (its correct recommendation)
        assert!(
            pr_recs[0].action.contains("allowlist"),
            "permission_retries action should reference allowlist (its correct fix)"
        );
        // permission_retries must NOT reference compile_cycles concepts
        assert!(
            !pr_recs[0].action.contains("batch"),
            "permission_retries must not reference batch compilation"
        );
        assert!(
            !pr_recs[0].action.contains("iterative"),
            "permission_retries must not reference iterative compilation"
        );

        // compile_cycles must NOT reference allowlist
        assert!(
            !cc_recs[0].action.contains("allowlist"),
            "compile_cycles action must not reference allowlist"
        );

        // The two action strings must be distinct
        assert_ne!(
            pr_recs[0].action, cc_recs[0].action,
            "permission_retries and compile_cycles must have distinct action text"
        );
    }

    #[test]
    fn test_compile_cycles_rationale_no_threshold_language() {
        // T-CC-03 (AC-13): compile_cycles rationale must not contain "(threshold: N)"
        let hotspot = HotspotFinding {
            category: HotspotCategory::Session,
            severity: Severity::Warning,
            rule_name: "compile_cycles".to_string(),
            claim: "compile".to_string(),
            measured: 20.0,
            threshold: 10.0,
            evidence: vec![],
        };
        let recs = recommendations_for_hotspots(&[hotspot]);
        assert_eq!(recs.len(), 1);
        assert!(
            !recs[0].rationale.contains("(threshold:"),
            "compile_cycles rationale must not contain threshold language, got: {}",
            recs[0].rationale
        );
    }

    #[test]
    fn test_compile_cycles_at_threshold_boundary() {
        // Edge case: measured == 10.0 is not above threshold (guard is > 10.0), no recommendation
        let hotspot = HotspotFinding {
            category: HotspotCategory::Session,
            severity: Severity::Info,
            rule_name: "compile_cycles".to_string(),
            claim: "compile".to_string(),
            measured: 10.0,
            threshold: 10.0,
            evidence: vec![],
        };
        let recs = recommendations_for_hotspots(&[hotspot]);
        assert!(
            recs.is_empty(),
            "measured == 10.0 should not trigger recommendation (guard is > 10.0)"
        );
    }

    #[test]
    fn test_recommendation_unknown_type() {
        let hotspot = HotspotFinding {
            category: HotspotCategory::Scope,
            severity: Severity::Info,
            rule_name: "unknown_hotspot".to_string(),
            claim: "something".to_string(),
            measured: 1.0,
            threshold: 0.5,
            evidence: vec![],
        };
        let recs = recommendations_for_hotspots(&[hotspot]);
        assert!(recs.is_empty());
    }

    #[test]
    fn test_recommendation_empty_hotspots() {
        let recs = recommendations_for_hotspots(&[]);
        assert!(recs.is_empty());
    }

    // -- col-010b: new fields serde tests (T-ES-10, T-ES-11, T-ES-12) --

    #[test]
    fn test_narratives_absent_when_none() {
        let report = build_report("col-010b", &[], MetricVector::default(), vec![], None, None);
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(!json.contains("narratives"));
        assert!(!json.contains("recommendations"));
    }

    #[test]
    fn test_narratives_present_when_some() {
        use crate::types::{EvidenceCluster, HotspotNarrative};
        let mut report = build_report("col-010b", &[], MetricVector::default(), vec![], None, None);
        report.narratives = Some(vec![HotspotNarrative {
            hotspot_type: "test".to_string(),
            summary: "test summary".to_string(),
            clusters: vec![EvidenceCluster {
                window_start: 1000,
                event_count: 2,
                description: "2 events".to_string(),
            }],
            top_files: vec![("src/lib.rs".to_string(), 3)],
            sequence_pattern: None,
        }]);
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(json.contains("narratives"));
        assert!(json.contains("test summary"));
    }

    #[test]
    fn test_recommendations_present_when_nonempty() {
        use crate::types::Recommendation;
        let mut report = build_report("col-010b", &[], MetricVector::default(), vec![], None, None);
        report.recommendations = vec![Recommendation {
            hotspot_type: "test".to_string(),
            action: "do something".to_string(),
            rationale: "because".to_string(),
        }];
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(json.contains("recommendations"));
        assert!(json.contains("do something"));
    }

    #[test]
    fn test_backward_compat_deserialization() {
        // Pre-col-010b JSON without narratives/recommendations
        let json = r#"{
            "feature_cycle": "old",
            "session_count": 1,
            "total_records": 5,
            "metrics": {"computed_at": 0, "universal": {}, "phases": {}},
            "hotspots": [],
            "is_cached": false
        }"#;
        let report: RetrospectiveReport = serde_json::from_str(json).expect("deserialize");
        assert!(report.narratives.is_none());
        assert!(report.recommendations.is_empty());
    }
}
