//! Distribution pipeline tests for eval/report (nan-008).
//!
//! Tests that call `run_report` end-to-end and verify CC@k/ICD round-trip
//! and all-sections ordering. Split from tests_distribution.rs for
//! 500-line file size compliance.

use std::collections::HashMap;
use tempfile::TempDir;

use super::{ComparisonMetrics, ProfileResult, ScenarioResult, run_report};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_result(dir: &TempDir, result: &ScenarioResult) {
    let path = dir.path().join(format!("{}.json", result.scenario_id));
    let json = serde_json::to_string(result).expect("serialize");
    std::fs::write(path, json).expect("write");
}

#[allow(clippy::too_many_arguments)]
fn make_scenario_result_with_metrics(
    id: &str,
    query: &str,
    baseline_p: f64,
    baseline_mrr: f64,
    baseline_cc: f64,
    baseline_icd: f64,
    candidate_p: f64,
    candidate_mrr: f64,
    candidate_cc: f64,
    candidate_icd: f64,
) -> ScenarioResult {
    let mut profiles = HashMap::new();
    profiles.insert(
        "baseline".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 50,
            p_at_k: baseline_p,
            mrr: baseline_mrr,
            cc_at_k: baseline_cc,
            icd: baseline_icd,
            ..Default::default()
        },
    );
    profiles.insert(
        "candidate".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 60,
            p_at_k: candidate_p,
            mrr: candidate_mrr,
            cc_at_k: candidate_cc,
            icd: candidate_icd,
            ..Default::default()
        },
    );
    ScenarioResult {
        scenario_id: id.to_string(),
        query: query.to_string(),
        profiles,
        phase: None,
        comparison: ComparisonMetrics {
            kendall_tau: 0.8,
            rank_changes: Vec::new(),
            mrr_delta: candidate_mrr - baseline_mrr,
            p_at_k_delta: candidate_p - baseline_p,
            latency_overhead_ms: 10,
            cc_at_k_delta: candidate_cc - baseline_cc,
            icd_delta: candidate_icd - baseline_icd,
        },
    }
}

// ---------------------------------------------------------------------------
// nan-008 AC-12 (ADR-003): Round-trip JSON serialization + render preserves
// non-trivial CC@k, ICD, delta values and section 7 follows section 5.
// ---------------------------------------------------------------------------

#[test]
fn test_report_round_trip_cc_at_k_icd_fields_and_section_6() {
    let mut profiles = HashMap::new();
    profiles.insert(
        "baseline".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 45,
            p_at_k: 0.600,
            mrr: 0.500,
            cc_at_k: 0.714,
            icd: 0.857,
            ..Default::default()
        },
    );
    profiles.insert(
        "candidate".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 55,
            p_at_k: 0.700,
            mrr: 0.600,
            cc_at_k: 0.857,
            icd: 1.234,
            ..Default::default()
        },
    );
    let original = ScenarioResult {
        scenario_id: "round-trip-01".to_string(),
        query: "test round trip query".to_string(),
        profiles,
        phase: None,
        comparison: ComparisonMetrics {
            kendall_tau: 0.9,
            rank_changes: Vec::new(),
            mrr_delta: 0.100,
            p_at_k_delta: 0.100,
            latency_overhead_ms: 10,
            cc_at_k_delta: 0.143,
            icd_delta: 0.377,
        },
    };

    let json = serde_json::to_string(&original).expect("serialize ScenarioResult");
    let deserialized: ScenarioResult =
        serde_json::from_str(&json).expect("deserialize ScenarioResult");

    let cand = deserialized.profiles.get("candidate").unwrap();
    assert!(
        (cand.cc_at_k - 0.857).abs() < 1e-9,
        "cc_at_k must round-trip: expected 0.857, got {}",
        cand.cc_at_k
    );
    assert!(
        (cand.icd - 1.234).abs() < 1e-9,
        "icd must round-trip: expected 1.234, got {}",
        cand.icd
    );
    assert!(
        (deserialized.comparison.cc_at_k_delta - 0.143).abs() < 1e-9,
        "cc_at_k_delta must round-trip: expected 0.143, got {}",
        deserialized.comparison.cc_at_k_delta
    );

    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    write_result(&results_dir, &deserialized);
    run_report(results_dir.path(), None, &out_path).expect("run_report must succeed");

    let content = std::fs::read_to_string(&out_path).expect("read report");

    assert!(
        content.contains("0.857"),
        "rendered report must contain cc_at_k value 0.857:\n{content}"
    );
    assert!(
        content.contains("1.234"),
        "rendered report must contain icd value 1.234:\n{content}"
    );
    assert!(
        content.contains("0.143"),
        "rendered report must contain cc_at_k_delta value 0.143:\n{content}"
    );
    assert!(
        content.contains("## 7. Distribution Analysis"),
        "rendered report must contain '## 7. Distribution Analysis':\n{content}"
    );
    assert!(
        !content.contains("## 6. Distribution Analysis"),
        "old '## 6. Distribution Analysis' heading must not appear:\n{content}"
    );

    let pos5 = content.find("## 5.").expect("section 5 must be present");
    let pos7 = content.find("## 7.").expect("section 7 must be present");
    assert!(
        pos5 < pos7,
        "section 5 must appear before section 7: pos5={pos5}, pos7={pos7}"
    );
}

// ---------------------------------------------------------------------------
// nan-008 AC-13: All sections present in correct order, CC@k and ICD in
// Summary, no section duplicated. (phase is null so section 6 absent.)
// ---------------------------------------------------------------------------

#[test]
fn test_report_contains_all_six_sections() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    let r1 = make_scenario_result_with_metrics(
        "s1",
        "query one",
        0.7,
        0.6,
        0.4,
        0.8,
        0.8,
        0.7,
        0.6,
        1.1,
    );
    let r2 = make_scenario_result_with_metrics(
        "s2",
        "query two",
        0.6,
        0.5,
        0.5,
        0.9,
        0.7,
        0.6,
        0.7,
        1.2,
    );
    write_result(&results_dir, &r1);
    write_result(&results_dir, &r2);

    run_report(results_dir.path(), None, &out_path).expect("run_report must succeed");

    let content = std::fs::read_to_string(&out_path).expect("read report");

    assert!(content.contains("## 1. Summary"), "section 1 missing");
    assert!(
        content.contains("## 2. Notable Ranking Changes"),
        "section 2 missing"
    );
    assert!(
        content.contains("## 3. Latency Distribution"),
        "section 3 missing"
    );
    assert!(
        content.contains("## 4. Entry-Level Analysis"),
        "section 4 missing"
    );
    assert!(
        content.contains("## 5. Zero-Regression Check"),
        "section 5 missing"
    );
    assert!(
        content.contains("## 7. Distribution Analysis"),
        "section 7 (Distribution Analysis) missing"
    );
    assert!(
        !content.contains("## 6. Distribution Analysis"),
        "old '## 6. Distribution Analysis' heading must not appear"
    );

    let pos1 = content.find("## 1.").expect("## 1. not found");
    let pos2 = content.find("## 2.").expect("## 2. not found");
    let pos3 = content.find("## 3.").expect("## 3. not found");
    let pos4 = content.find("## 4.").expect("## 4. not found");
    let pos5 = content.find("## 5.").expect("## 5. not found");
    let pos7 = content.find("## 7.").expect("## 7. not found");
    assert!(
        pos1 < pos2 && pos2 < pos3 && pos3 < pos4 && pos4 < pos5 && pos5 < pos7,
        "sections must appear in order 1-5, 7"
    );

    let summary_start = content.find("## 1. Summary").unwrap();
    let summary_end = content.find("## 2.").unwrap();
    let summary_section = &content[summary_start..summary_end];
    assert!(
        summary_section.contains("CC@k"),
        "Summary must contain 'CC@k':\n{summary_section}"
    );
    assert!(
        summary_section.contains("ICD"),
        "Summary must contain 'ICD':\n{summary_section}"
    );

    for heading in ["## 1.", "## 2.", "## 3.", "## 4.", "## 5.", "## 7."] {
        let count = content.matches(heading).count();
        assert_eq!(
            count, 1,
            "heading '{heading}' must appear exactly once, found {count}"
        );
    }
}

// ===========================================================================
// nan-018 — report-extension pipeline tests: exit-code invariance (R-17),
// correlated section (AC-04/AC-14), backward-compat deserialization (#3548).
// ===========================================================================

/// Build a two-profile scenario carrying explicit trust + cost on each profile.
#[allow(clippy::too_many_arguments)]
fn scenario_with_trust_cost(
    id: &str,
    baseline_cost: f64,
    candidate_cost: f64,
    baseline_absence: bool,
    baseline_rank: bool,
    candidate_absence: bool,
    candidate_rank: bool,
    candidate_violations: Vec<String>,
) -> ScenarioResult {
    use super::TrustOutcome;
    let mut profiles = HashMap::new();
    profiles.insert(
        "baseline".to_string(),
        ProfileResult {
            p_at_k: 0.6,
            mrr: 0.5,
            cost_tokens: baseline_cost,
            trust: TrustOutcome {
                absence_pass: baseline_absence,
                rank_pass: baseline_rank,
                violations: vec![],
            },
            ..Default::default()
        },
    );
    profiles.insert(
        "candidate".to_string(),
        ProfileResult {
            p_at_k: 0.6,
            mrr: 0.5,
            cost_tokens: candidate_cost,
            trust: TrustOutcome {
                absence_pass: candidate_absence,
                rank_pass: candidate_rank,
                violations: candidate_violations,
            },
            ..Default::default()
        },
    );
    ScenarioResult {
        scenario_id: id.to_string(),
        query: "q".to_string(),
        profiles,
        phase: None,
        comparison: super::default_comparison(),
    }
}

/// R-17 (LOAD-BEARING): `run_report` returns Ok (exit 0) with a TRUST regression
/// present — the flip is reported in the body, never via exit code.
#[test]
fn test_eval_report_exit_code_unchanged_with_trust_regression() {
    let dir = TempDir::new().unwrap();
    let out = TempDir::new().unwrap();
    let out_path = out.path().join("report.md");

    // baseline passes absence; candidate flips it (trust regression), equal cost.
    let r = scenario_with_trust_cost(
        "trust-reg",
        10.0,
        10.0,
        true,
        true,
        false,
        true,
        vec!["absence: forbidden 'F' present at rank 0".to_string()],
    );
    write_result(&dir, &r);

    let res = run_report(dir.path(), None, &out_path);
    assert!(
        res.is_ok(),
        "run_report must return Ok with a trust regression (R-17)"
    );

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(
        content.contains("trust flipped"),
        "trust flip must be reported in the BODY:\n{content}"
    );
}

/// R-17: `run_report` returns Ok (exit 0) with COST growth present (advisory).
#[test]
fn test_eval_report_exit_code_unchanged_with_cost_growth() {
    let dir = TempDir::new().unwrap();
    let out = TempDir::new().unwrap();
    let out_path = out.path().join("report.md");

    // cost grows; trust + relevance hold.
    let r = scenario_with_trust_cost("cost-reg", 10.0, 25.0, true, true, true, true, vec![]);
    write_result(&dir, &r);

    let res = run_report(dir.path(), None, &out_path);
    assert!(
        res.is_ok(),
        "run_report must return Ok with cost growth (advisory, R-17)"
    );

    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(
        content.contains("cost grew (advisory)"),
        "cost growth must be reported as advisory in the BODY:\n{content}"
    );
}

/// AC-04/AC-14: one report contains the correlated section with trust + P@K/MRR +
/// cost for the SAME scenario, in one section.
#[test]
fn test_report_surfaces_correlated_trust_relevance_cost() {
    let dir = TempDir::new().unwrap();
    let out = TempDir::new().unwrap();
    let out_path = out.path().join("report.md");

    let r = scenario_with_trust_cost("corr-1", 10.0, 12.0, true, true, true, true, vec![]);
    write_result(&dir, &r);

    run_report(dir.path(), None, &out_path).expect("run_report");
    let content = std::fs::read_to_string(&out_path).unwrap();

    let start = content
        .find("## 5C. Correlated")
        .expect("correlated section heading present");
    let end = content[start..]
        .find("\n## ")
        .map(|i| start + i)
        .unwrap_or(content.len());
    let section = &content[start..end];

    // Header carries all four families: Trust, P@K, MRR, Cost.
    for col in ["Trust", "P@K", "MRR", "Cost"] {
        assert!(
            section.contains(col),
            "correlated section missing '{col}':\n{section}"
        );
    }
    // Both profiles of the same scenario appear in this one section.
    assert!(
        section.contains("corr-1"),
        "scenario id must appear in correlated section"
    );
    assert!(section.contains("baseline") && section.contains("candidate"));
}

/// #3548 named backward-compat: a pre-nan-018 result JSON (no cost_tokens / trust,
/// entries without `content`) deserializes and renders without error (exit 0).
#[test]
fn test_report_backward_compat_pre_nan018_json() {
    let dir = TempDir::new().unwrap();
    let out = TempDir::new().unwrap();
    let out_path = out.path().join("report.md");

    // Hand-written legacy result file: NO cost_tokens, NO trust, entry has no content.
    let legacy = r#"{
        "scenario_id": "legacy-1",
        "query": "legacy query",
        "profiles": {
            "baseline": {
                "entries": [{"id": 1, "title": "L", "category": "decision",
                    "final_score": 0.5, "similarity": 0.4, "confidence": 0.3,
                    "status": "Active", "nli_rerank_delta": null}],
                "latency_ms": 5, "p_at_k": 0.5, "mrr": 0.5, "cc_at_k": 0.0, "icd": 0.0
            }
        },
        "comparison": {"kendall_tau": 1.0, "rank_changes": [], "mrr_delta": 0.0,
            "p_at_k_delta": 0.0, "latency_overhead_ms": 0, "cc_at_k_delta": 0.0, "icd_delta": 0.0},
        "phase": null
    }"#;
    std::fs::write(dir.path().join("legacy-1.json"), legacy).unwrap();

    let res = run_report(dir.path(), None, &out_path);
    assert!(
        res.is_ok(),
        "pre-nan-018 result JSON must deserialize + render without error: {res:?}"
    );
    let content = std::fs::read_to_string(&out_path).unwrap();
    assert!(
        content.contains("## 5C. Correlated"),
        "correlated section still rendered"
    );
}
