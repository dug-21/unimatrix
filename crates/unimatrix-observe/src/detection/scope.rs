//! Scope hotspot detection rules (5 rules).
//!
//! All rules are new in col-002b. PhaseDurationOutlierRule uses ADR-001
//! constructor injection for historical baseline data.

use std::collections::{HashMap, HashSet};

use crate::types::{
    EvidenceRecord, HookType, HotspotCategory, HotspotFinding, MetricVector, ObservationRecord,
    Severity,
};

use super::{
    DetectionRule, find_completion_boundary, input_to_command_string, input_to_file_path, truncate,
};

// -- Rule 1: SourceFileCountRule (FR-04.1) --

pub(crate) struct SourceFileCountRule;

const SOURCE_FILE_COUNT_THRESHOLD: f64 = 6.0;

impl DetectionRule for SourceFileCountRule {
    fn name(&self) -> &str {
        "source_file_count"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Scope
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut written_rs_files: HashSet<String> = HashSet::new();
        let mut evidence = Vec::new();

        for record in records {
            let is_write_post =
                record.tool.as_deref() == Some("Write") && record.hook == HookType::PostToolUse;
            if !is_write_post {
                continue;
            }
            if let Some(path) = record
                .input
                .as_ref()
                .and_then(input_to_file_path)
                .filter(|p| p.ends_with(".rs"))
                .filter(|p| written_rs_files.insert(p.clone()))
            {
                evidence.push(EvidenceRecord {
                    description: "New .rs file written".to_string(),
                    ts: record.ts,
                    tool: Some("Write".to_string()),
                    detail: path,
                });
            }
        }

        let count = written_rs_files.len() as f64;
        if count > SOURCE_FILE_COUNT_THRESHOLD {
            vec![HotspotFinding {
                category: HotspotCategory::Scope,
                severity: Severity::Warning,
                rule_name: "source_file_count".to_string(),
                claim: format!("{count:.0} new .rs source files created"),
                measured: count,
                threshold: SOURCE_FILE_COUNT_THRESHOLD,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}

// -- Rule 2: DesignArtifactCountRule (FR-04.2) --

pub(crate) struct DesignArtifactCountRule;

const DESIGN_ARTIFACT_THRESHOLD: f64 = 25.0;

impl DetectionRule for DesignArtifactCountRule {
    fn name(&self) -> &str {
        "design_artifact_count"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Scope
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut artifact_paths: HashSet<String> = HashSet::new();
        let mut evidence = Vec::new();

        for record in records {
            let tool = record.tool.as_deref().unwrap_or("");
            if tool == "Write" || tool == "Edit" {
                if let Some(input) = &record.input {
                    if let Some(path) = input_to_file_path(input) {
                        if path.contains("product/features/") && artifact_paths.insert(path.clone())
                        {
                            evidence.push(EvidenceRecord {
                                description: "Design artifact modified".to_string(),
                                ts: record.ts,
                                tool: record.tool.clone(),
                                detail: path,
                            });
                        }
                    }
                }
            }
        }

        let count = artifact_paths.len() as f64;
        if count > DESIGN_ARTIFACT_THRESHOLD {
            vec![HotspotFinding {
                category: HotspotCategory::Scope,
                severity: Severity::Info,
                rule_name: "design_artifact_count".to_string(),
                claim: format!(
                    "{count:.0} design artifacts created/modified under product/features/"
                ),
                measured: count,
                threshold: DESIGN_ARTIFACT_THRESHOLD,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}

// -- Rule 3: AdrCountRule (FR-04.3) --

pub(crate) struct AdrCountRule;

const ADR_COUNT_THRESHOLD: f64 = 3.0;

impl DetectionRule for AdrCountRule {
    fn name(&self) -> &str {
        "adr_count"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Scope
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut adr_paths: HashSet<String> = HashSet::new();
        let mut evidence = Vec::new();

        for record in records {
            if record.tool.as_deref() == Some("Write") {
                if let Some(input) = &record.input {
                    if let Some(path) = input_to_file_path(input) {
                        if let Some(filename) = path.rsplit('/').next() {
                            if filename.starts_with("ADR-") && adr_paths.insert(path.clone()) {
                                evidence.push(EvidenceRecord {
                                    description: "ADR created".to_string(),
                                    ts: record.ts,
                                    tool: Some("Write".to_string()),
                                    detail: path,
                                });
                            }
                        }
                    }
                }
            }
        }

        let count = adr_paths.len() as f64;
        if count > ADR_COUNT_THRESHOLD {
            vec![HotspotFinding {
                category: HotspotCategory::Scope,
                severity: Severity::Info,
                rule_name: "adr_count".to_string(),
                claim: format!("{count:.0} ADRs created (threshold: {ADR_COUNT_THRESHOLD})"),
                measured: count,
                threshold: ADR_COUNT_THRESHOLD,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}

// -- Rule 4: PostDeliveryIssuesRule (FR-04.4) --

pub(crate) struct PostDeliveryIssuesRule;

impl DetectionRule for PostDeliveryIssuesRule {
    fn name(&self) -> &str {
        "post_delivery_issues"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Scope
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let boundary = match find_completion_boundary(records) {
            Some(ts) => ts,
            None => return vec![],
        };

        let mut issue_creates = Vec::new();

        for record in records {
            if record.ts > boundary
                && record.tool.as_deref() == Some("Bash")
                && record.hook == HookType::PreToolUse
            {
                if let Some(input) = &record.input {
                    let cmd = input_to_command_string(input);
                    if cmd.contains("gh issue create") {
                        issue_creates.push(EvidenceRecord {
                            description: "Post-delivery issue creation".to_string(),
                            ts: record.ts,
                            tool: Some("Bash".to_string()),
                            detail: truncate(&cmd, 200),
                        });
                    }
                }
            }
        }

        if issue_creates.is_empty() {
            vec![]
        } else {
            let count = issue_creates.len();
            vec![HotspotFinding {
                category: HotspotCategory::Scope,
                severity: Severity::Warning,
                rule_name: "post_delivery_issues".to_string(),
                claim: format!("{count} issues created after task completion"),
                measured: count as f64,
                threshold: 1.0,
                evidence: issue_creates,
            }]
        }
    }
}

// -- Rule 5: PhaseDurationOutlierRule (FR-04.5, ADR-001) --

pub(crate) struct PhaseDurationOutlierRule {
    /// Per-phase baselines: phase_name -> (mean_duration_secs, sample_count).
    /// Stored for ADR-001 compliance. detect() returns empty because phase durations
    /// come from MetricVector (computed after detection). Baseline comparison handles
    /// the actual outlier detection. Field accessible via tests.
    #[cfg_attr(not(test), allow(dead_code))]
    phase_baselines: HashMap<String, (f64, usize)>,
}

impl PhaseDurationOutlierRule {
    pub fn new(history: Option<&[MetricVector]>) -> Self {
        let mut phase_baselines = HashMap::new();

        if let Some(vectors) = history {
            let mut by_phase: HashMap<String, Vec<f64>> = HashMap::new();
            for mv in vectors {
                for (phase_name, phase_metrics) in &mv.phases {
                    by_phase
                        .entry(phase_name.clone())
                        .or_default()
                        .push(phase_metrics.duration_secs as f64);
                }
            }

            for (phase_name, durations) in by_phase {
                if durations.len() >= 3 {
                    let mean = durations.iter().sum::<f64>() / durations.len() as f64;
                    phase_baselines.insert(phase_name, (mean, durations.len()));
                }
            }
        }

        PhaseDurationOutlierRule { phase_baselines }
    }
}

impl DetectionRule for PhaseDurationOutlierRule {
    fn name(&self) -> &str {
        "phase_duration_outlier"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Scope
    }

    fn detect(&self, _records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        // Phase duration outlier detection is handled by the baseline comparison module
        // (compare_to_baseline in baseline.rs) because:
        // 1. Phase durations come from MetricVector, computed AFTER detection runs
        // 2. The DetectionRule trait cannot be changed (col-002 constraint)
        // 3. Records don't carry explicit phase attribution
        //
        // This rule still exists for:
        // - Registration in default_rules() (AC-07: 21 rules)
        // - ADR-001 compliance (constructor injection pattern)
        // - Future enhancement when records carry phase info
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PhaseMetrics;
    use std::collections::BTreeMap;

    fn make_write_rs(ts: u64, path: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PostToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("Write".to_string()),
            input: Some(serde_json::json!({"file_path": path})),
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_write_pre(ts: u64, path: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("Write".to_string()),
            input: Some(serde_json::json!({"file_path": path})),
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_edit(ts: u64, path: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("Edit".to_string()),
            input: Some(serde_json::json!({"file_path": path})),
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_bash(ts: u64, command: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("Bash".to_string()),
            input: Some(serde_json::json!({"command": command})),
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_task_update(ts: u64, task_id: &str, status: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("TaskUpdate".to_string()),
            input: Some(serde_json::json!({"taskId": task_id, "status": status})),
            response_size: None,
            response_snippet: None,
        }
    }

    // -- SourceFileCountRule --

    #[test]
    fn test_source_file_count_fires() {
        let records: Vec<ObservationRecord> = (0..7)
            .map(|i| make_write_rs(i * 1000, &format!("/tmp/file_{i}.rs")))
            .collect();
        let rule = SourceFileCountRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > SOURCE_FILE_COUNT_THRESHOLD);
    }

    #[test]
    fn test_source_file_count_silent() {
        let records: Vec<ObservationRecord> = (0..5)
            .map(|i| make_write_rs(i * 1000, &format!("/tmp/file_{i}.rs")))
            .collect();
        let rule = SourceFileCountRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_source_file_count_deduplicates() {
        let records: Vec<ObservationRecord> = (0..10)
            .map(|i| make_write_rs(i * 1000, "/tmp/same.rs"))
            .collect();
        let rule = SourceFileCountRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_source_file_count_non_rs() {
        let records: Vec<ObservationRecord> = (0..10)
            .map(|i| make_write_rs(i * 1000, &format!("/tmp/file_{i}.md")))
            .collect();
        let rule = SourceFileCountRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_source_file_count_empty() {
        let rule = SourceFileCountRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- DesignArtifactCountRule --

    #[test]
    fn test_design_artifact_fires() {
        let mut records = Vec::new();
        for i in 0..26 {
            records.push(make_edit(
                i * 1000,
                &format!("product/features/col-002b/doc_{i}.md"),
            ));
        }
        let rule = DesignArtifactCountRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > DESIGN_ARTIFACT_THRESHOLD);
    }

    #[test]
    fn test_design_artifact_silent() {
        let mut records = Vec::new();
        for i in 0..24 {
            records.push(make_edit(
                i * 1000,
                &format!("product/features/col-002b/doc_{i}.md"),
            ));
        }
        let rule = DesignArtifactCountRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_design_artifact_outside_features() {
        let mut records = Vec::new();
        for i in 0..30 {
            records.push(make_edit(i * 1000, &format!("/tmp/doc_{i}.md")));
        }
        let rule = DesignArtifactCountRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_design_artifact_empty() {
        let rule = DesignArtifactCountRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- AdrCountRule --

    #[test]
    fn test_adr_count_fires() {
        let records = vec![
            make_write_pre(
                1000,
                "product/features/col-002b/architecture/ADR-001-test.md",
            ),
            make_write_pre(
                2000,
                "product/features/col-002b/architecture/ADR-002-test.md",
            ),
            make_write_pre(
                3000,
                "product/features/col-002b/architecture/ADR-003-test.md",
            ),
            make_write_pre(
                4000,
                "product/features/col-002b/architecture/ADR-004-test.md",
            ),
        ];
        let rule = AdrCountRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > ADR_COUNT_THRESHOLD);
    }

    #[test]
    fn test_adr_count_silent() {
        let records = vec![
            make_write_pre(
                1000,
                "product/features/col-002b/architecture/ADR-001-test.md",
            ),
            make_write_pre(
                2000,
                "product/features/col-002b/architecture/ADR-002-test.md",
            ),
        ];
        let rule = AdrCountRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_adr_count_non_adr() {
        let records = vec![
            make_write_pre(1000, "product/features/col-002b/SCOPE.md"),
            make_write_pre(2000, "product/features/col-002b/SPEC.md"),
            make_write_pre(3000, "product/features/col-002b/RISK.md"),
            make_write_pre(4000, "product/features/col-002b/BRIEF.md"),
        ];
        let rule = AdrCountRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_adr_count_empty() {
        let rule = AdrCountRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- PostDeliveryIssuesRule --

    #[test]
    fn test_post_delivery_issues_fires() {
        let records = vec![
            make_task_update(1000, "1", "completed"),
            make_bash(2000, "gh issue create --title 'Bug found' --label bug"),
        ];
        let rule = PostDeliveryIssuesRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].measured, 1.0);
    }

    #[test]
    fn test_post_delivery_issues_before_completion() {
        let records = vec![
            make_bash(1000, "gh issue create --title 'Bug found'"),
            make_task_update(2000, "1", "completed"),
        ];
        let rule = PostDeliveryIssuesRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_post_delivery_issues_no_completion() {
        let records = vec![make_bash(1000, "gh issue create --title 'Bug found'")];
        let rule = PostDeliveryIssuesRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_post_delivery_issues_empty() {
        let rule = PostDeliveryIssuesRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- PhaseDurationOutlierRule --

    #[test]
    fn test_phase_duration_outlier_returns_empty() {
        // detect() always returns empty per design (baseline comparison handles this)
        let rule = PhaseDurationOutlierRule::new(None);
        assert!(rule.detect(&[]).is_empty());
    }

    #[test]
    fn test_phase_duration_outlier_with_history() {
        let mut phases = BTreeMap::new();
        phases.insert(
            "3a".to_string(),
            PhaseMetrics {
                duration_secs: 100,
                tool_call_count: 10,
            },
        );

        let history: Vec<MetricVector> = (0..3)
            .map(|_| MetricVector {
                computed_at: 0,
                universal: Default::default(),
                phases: phases.clone(),
            })
            .collect();

        let rule = PhaseDurationOutlierRule::new(Some(&history));
        // The rule stores baselines; verify construction works
        assert_eq!(rule.phase_baselines.len(), 1);
        assert!(rule.phase_baselines.contains_key("3a"));
        let (mean, count) = rule.phase_baselines["3a"];
        assert!((mean - 100.0).abs() < f64::EPSILON);
        assert_eq!(count, 3);
    }

    #[test]
    fn test_phase_duration_outlier_insufficient_history() {
        let history: Vec<MetricVector> = (0..2).map(|_| MetricVector::default()).collect();

        let rule = PhaseDurationOutlierRule::new(Some(&history));
        assert!(rule.phase_baselines.is_empty());
    }

    #[test]
    fn test_phase_duration_outlier_name_and_category() {
        let rule = PhaseDurationOutlierRule::new(None);
        assert_eq!(rule.name(), "phase_duration_outlier");
        assert_eq!(rule.category(), HotspotCategory::Scope);
    }
}
