//! Full-pipeline phase tests for eval/report (nan-009).
//!
//! Tests that call `run_report` end-to-end and verify phase-related
//! rendering: section 6 presence/absence, section 2 phase labels,
//! backward-compat serde, and the primary ADR-002 dual-type round-trip guard.

use std::collections::HashMap;
use tempfile::TempDir;

use super::{ComparisonMetrics, ProfileResult, ScenarioResult, default_comparison, run_report};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_result_with_phase(
    id: &str,
    phase: Option<&str>,
    p_at_k: f64,
    mrr: f64,
    cc_at_k: f64,
    icd: f64,
) -> ScenarioResult {
    let mut profiles = HashMap::new();
    profiles.insert(
        "baseline".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 50,
            p_at_k,
            mrr,
            cc_at_k,
            icd,
        },
    );
    ScenarioResult {
        scenario_id: id.to_string(),
        query: format!("query for {id}"),
        profiles,
        phase: phase.map(|s| s.to_string()),
        comparison: default_comparison(),
    }
}

fn write_result(dir: &TempDir, result: &ScenarioResult) {
    let path = dir.path().join(format!("{}.json", result.scenario_id));
    let json = serde_json::to_string(result).expect("serialize");
    std::fs::write(path, json).expect("write");
}

// ---------------------------------------------------------------------------
// Pipeline tests
// ---------------------------------------------------------------------------

/// R-07, AC-04: Section 6 must be absent when all phases are null.
#[test]
fn test_render_phase_section_absent_when_stats_empty() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    write_result(
        &results_dir,
        &make_result_with_phase("s1", None, 0.7, 0.6, 0.5, 1.0),
    );
    write_result(
        &results_dir,
        &make_result_with_phase("s2", None, 0.5, 0.4, 0.3, 0.8),
    );
    write_result(
        &results_dir,
        &make_result_with_phase("s3", None, 0.6, 0.5, 0.4, 0.9),
    );

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(
        !content.contains("## 6. Phase-Stratified Metrics"),
        "section 6 must be absent when all phases are null:\n{content}"
    );
}

/// R-09, AC-04: Full pipeline with all-null phases omits section 6 and preserves section 7.
#[test]
fn test_report_round_trip_null_phase_only_no_section_6() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    write_result(
        &results_dir,
        &make_result_with_phase("n1", None, 0.7, 0.6, 0.5, 1.0),
    );
    write_result(
        &results_dir,
        &make_result_with_phase("n2", None, 0.5, 0.4, 0.3, 0.8),
    );

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).unwrap();

    assert!(
        !content.contains("## 6. Phase-Stratified Metrics"),
        "section 6 absent when all phases null"
    );
    assert!(
        content.contains("## 7. Distribution Analysis"),
        "section 7 Distribution Analysis always present"
    );
    assert!(
        !content.contains("## 6. Distribution Analysis"),
        "old heading must never appear"
    );
}

/// R-01: Null bucket label must be "(unset)", never "(none)".
#[test]
fn test_report_round_trip_phase_section_null_label() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    write_result(
        &results_dir,
        &make_result_with_phase("d1", Some("delivery"), 0.8, 0.7, 0.5, 1.0),
    );
    write_result(
        &results_dir,
        &make_result_with_phase("n1", None, 0.5, 0.4, 0.3, 0.7),
    );

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).unwrap();

    assert!(
        content.contains("## 6. Phase-Stratified Metrics"),
        "section 6 must be present"
    );
    assert!(
        content.contains("(unset)"),
        "null-phase bucket label must be '(unset)'"
    );
    assert!(
        !content.contains("(none)"),
        "'(none)' must never appear — canonical is '(unset)' (R-01)"
    );
}

/// AC-04 omission condition: report section 6 absent when all phases null.
#[test]
fn test_report_section_6_omitted_when_all_phases_null() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    write_result(
        &results_dir,
        &make_result_with_phase("x1", None, 0.6, 0.5, 0.4, 0.9),
    );

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(
        !content.contains("## 6. Phase-Stratified Metrics"),
        "section 6 must be absent when all phases are null (AC-04)"
    );
}

/// AC-04: Section 6 present when at least one phase is non-null.
#[test]
fn test_report_section_6_present_when_phase_non_null() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    write_result(
        &results_dir,
        &make_result_with_phase("d1", Some("delivery"), 0.8, 0.7, 0.5, 1.0),
    );

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(
        content.contains("## 6. Phase-Stratified Metrics"),
        "section 6 must be present (AC-04)"
    );
    assert!(
        content.contains("delivery"),
        "'delivery' phase label must appear in section 6"
    );
}

/// R-12, AC-08: Phase label must appear in section 2 when non-null.
#[test]
fn test_report_section_2_includes_phase_label_when_non_null() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    let mut profiles = HashMap::new();
    profiles.insert(
        "baseline".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 50,
            p_at_k: 0.6,
            mrr: 0.5,
            cc_at_k: 0.4,
            icd: 0.8,
        },
    );
    profiles.insert(
        "candidate".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 60,
            p_at_k: 0.7,
            mrr: 0.6,
            cc_at_k: 0.5,
            icd: 0.9,
        },
    );
    let result = ScenarioResult {
        scenario_id: "phase-s2-01".to_string(),
        query: "notable ranking query".to_string(),
        profiles,
        phase: Some("delivery".to_string()),
        comparison: ComparisonMetrics {
            kendall_tau: 0.1,
            rank_changes: Vec::new(),
            mrr_delta: 0.1,
            p_at_k_delta: 0.1,
            latency_overhead_ms: 10,
            cc_at_k_delta: 0.1,
            icd_delta: 0.1,
        },
    };
    write_result(&results_dir, &result);

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).unwrap();
    let section_2_start = content.find("## 2. Notable Ranking Changes").unwrap();
    let section_3_start = content.find("## 3. Latency Distribution").unwrap();
    let section_2_content = &content[section_2_start..section_3_start];

    assert!(
        section_2_content.contains("delivery"),
        "phase label must appear in section 2 for non-null phase scenario:\n{section_2_content}"
    );
}

/// R-12, AC-08: Phase label must NOT appear in section 2 when null.
#[test]
fn test_report_section_2_phase_label_null_absent() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    let mut profiles = HashMap::new();
    profiles.insert(
        "baseline".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 50,
            p_at_k: 0.6,
            mrr: 0.5,
            cc_at_k: 0.4,
            icd: 0.8,
        },
    );
    profiles.insert(
        "candidate".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 60,
            p_at_k: 0.4,
            mrr: 0.3,
            cc_at_k: 0.3,
            icd: 0.7,
        },
    );
    let result = ScenarioResult {
        scenario_id: "null-phase-s2-01".to_string(),
        query: "notable ranking null phase query".to_string(),
        profiles,
        phase: None,
        comparison: ComparisonMetrics {
            kendall_tau: 0.1,
            rank_changes: Vec::new(),
            mrr_delta: -0.2,
            p_at_k_delta: -0.2,
            latency_overhead_ms: 10,
            cc_at_k_delta: -0.1,
            icd_delta: -0.1,
        },
    };
    write_result(&results_dir, &result);

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).unwrap();
    let section_2_start = content.find("## 2. Notable Ranking Changes").unwrap();
    let section_3_start = content.find("## 3. Latency Distribution").unwrap();
    let section_2_content = &content[section_2_start..section_3_start];

    assert!(
        !section_2_content.contains("(unset)"),
        "null-phase scenarios must not show '(unset)' in section 2:\n{section_2_content}"
    );
    assert!(
        !section_2_content.contains("**Phase**:"),
        "no phase annotation on null-phase scenarios in section 2:\n{section_2_content}"
    );
}

/// ADR-002, R-02, R-03, AC-11, AC-12: Primary dual-type round-trip guard.
///
/// Writes a runner-side ScenarioResult with non-null phase, calls run_report,
/// and asserts both section 6 and section 7 are present in correct order.
/// This test detects partial dual-type updates where the report-side ScenarioResult
/// copy is missing the phase field.
#[test]
fn test_report_round_trip_phase_section_7_distribution() {
    use crate::eval::runner::{
        ComparisonMetrics as RunnerComparisonMetrics, ProfileResult as RunnerProfileResult,
        ScenarioResult as RunnerScenarioResult,
    };

    let mut runner_profiles = HashMap::new();
    runner_profiles.insert(
        "baseline".to_string(),
        RunnerProfileResult {
            entries: Vec::new(),
            latency_ms: 45,
            p_at_k: 0.750,
            mrr: 0.650,
            cc_at_k: 0.600,
            icd: 1.100,
        },
    );
    let runner_result = RunnerScenarioResult {
        scenario_id: "round-trip-phase-01".to_string(),
        query: "round-trip phase guard query".to_string(),
        profiles: runner_profiles,
        comparison: RunnerComparisonMetrics {
            kendall_tau: 1.0,
            rank_changes: Vec::new(),
            mrr_delta: 0.0,
            p_at_k_delta: 0.0,
            latency_overhead_ms: 0,
            cc_at_k_delta: 0.0,
            icd_delta: 0.0,
        },
        phase: Some("delivery".to_string()),
    };

    let json = serde_json::to_string(&runner_result).expect("serialize runner ScenarioResult");

    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    std::fs::write(results_dir.path().join("delivery-01.json"), &json).expect("write result file");

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).unwrap();

    // (1) New section present.
    assert!(
        content.contains("## 6. Phase-Stratified Metrics"),
        "section 6 Phase-Stratified Metrics must be present:\n{content}"
    );

    // (2) Renumbered section present.
    assert!(
        content.contains("## 7. Distribution Analysis"),
        "section 7 Distribution Analysis must be present (was section 6):\n{content}"
    );

    // (3) Phase value appears in section 6 (catches dual-type partial update, R-03).
    assert!(
        content.contains("delivery"),
        "'delivery' phase label must appear in section 6 table:\n{content}"
    );

    // (4) Section order guard (SR-02, R-02).
    let pos6 = content.find("## 6.").expect("section 6 must be present");
    let pos7 = content.find("## 7.").expect("section 7 must be present");
    assert!(
        pos6 < pos7,
        "section 6 must appear before section 7: pos6={pos6}, pos7={pos7}"
    );

    // (5) Old heading absent (SR-02 negative guard).
    assert!(
        !content.contains("## 6. Distribution Analysis"),
        "old '## 6. Distribution Analysis' heading must NOT appear:\n{content}"
    );
}
