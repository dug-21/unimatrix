//! Tests for eval/report (nan-007 D4).
//!
//! Covers: AC-08 (five sections), AC-09 (zero-regression check),
//! C-07 (exit code 0 always), R-12 (OR semantics), R-17 (missing headers).

use std::collections::HashMap;
use tempfile::TempDir;

use super::aggregate::{
    compute_aggregate_stats, compute_cc_at_k_scenario_rows, compute_latency_buckets,
    find_regressions,
};
use super::{
    ComparisonMetrics, ProfileResult, RankChange, ScenarioResult, ScoredEntry, default_comparison,
    run_report,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_profile_result(p_at_k: f64, mrr: f64, latency_ms: u64) -> ProfileResult {
    ProfileResult {
        entries: Vec::new(),
        latency_ms,
        p_at_k,
        mrr,
        cc_at_k: 0.0,
        icd: 0.0,
    }
}

fn make_scenario_result(
    id: &str,
    query: &str,
    baseline_p: f64,
    baseline_mrr: f64,
    candidate_p: f64,
    candidate_mrr: f64,
) -> ScenarioResult {
    let mut profiles = HashMap::new();
    profiles.insert(
        "baseline".to_string(),
        make_profile_result(baseline_p, baseline_mrr, 50),
    );
    profiles.insert(
        "candidate".to_string(),
        make_profile_result(candidate_p, candidate_mrr, 60),
    );
    ScenarioResult {
        scenario_id: id.to_string(),
        query: query.to_string(),
        profiles,
        comparison: ComparisonMetrics {
            kendall_tau: 0.8,
            rank_changes: Vec::new(),
            mrr_delta: candidate_mrr - baseline_mrr,
            p_at_k_delta: candidate_p - baseline_p,
            latency_overhead_ms: 10,
            cc_at_k_delta: 0.0,
            icd_delta: 0.0,
        },
    }
}

/// Write a ScenarioResult as JSON to a temp directory.
fn write_result(dir: &TempDir, result: &ScenarioResult) {
    let path = dir.path().join(format!("{}.json", result.scenario_id));
    let json = serde_json::to_string(result).expect("serialize");
    std::fs::write(path, json).expect("write");
}

// ---------------------------------------------------------------------------
// AC-08: Five sections present (R-17)
// ---------------------------------------------------------------------------

#[test]
fn test_report_contains_all_five_sections() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    let r1 = make_scenario_result("scen-1", "query one", 0.8, 0.7, 0.85, 0.75);
    let r2 = make_scenario_result("scen-2", "query two", 0.6, 0.5, 0.65, 0.55);
    write_result(&results_dir, &r1);
    write_result(&results_dir, &r2);

    let result = run_report(results_dir.path(), None, &out_path);
    assert!(result.is_ok(), "run_report must return Ok(()): {result:?}");

    let content = std::fs::read_to_string(&out_path).expect("read report");

    assert!(
        content.contains("## 1. Summary"),
        "must contain '## 1. Summary'"
    );
    assert!(content.contains("## 2. Notable Ranking Changes"));
    assert!(content.contains("## 3. Latency Distribution"));
    assert!(content.contains("## 4. Entry-Level Analysis"));
    assert!(content.contains("## 5. Zero-Regression Check"));

    // Sections must appear in the order specified in FR-27.
    let pos1 = content.find("## 1. Summary").unwrap();
    let pos2 = content.find("## 2. Notable Ranking Changes").unwrap();
    let pos3 = content.find("## 3. Latency Distribution").unwrap();
    let pos4 = content.find("## 4. Entry-Level Analysis").unwrap();
    let pos5 = content.find("## 5. Zero-Regression Check").unwrap();
    assert!(
        pos1 < pos2 && pos2 < pos3 && pos3 < pos4 && pos4 < pos5,
        "sections must appear in order 1-5"
    );
}

// ---------------------------------------------------------------------------
// R-12: OR semantics — MRR regression only
// ---------------------------------------------------------------------------

#[test]
fn test_zero_regression_check_mrr_regression_only() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    // candidate.mrr=0.4 < baseline.mrr=0.5; P@K equal.
    let r = make_scenario_result("scen-a", "test query", 0.6, 0.5, 0.6, 0.4);
    write_result(&results_dir, &r);

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).expect("read report");
    assert!(
        content.contains("scen-a"),
        "scen-a must appear in regression list (MRR dropped):\n{content}"
    );
    assert!(
        content.contains("MRR dropped"),
        "reason must be 'MRR dropped'"
    );
}

// ---------------------------------------------------------------------------
// R-12: OR semantics — P@K regression only
// ---------------------------------------------------------------------------

#[test]
fn test_zero_regression_check_pak_regression_only() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    // candidate.p_at_k=0.4 < baseline.p_at_k=0.6; MRR equal.
    let r = make_scenario_result("scen-b", "another query", 0.6, 0.5, 0.4, 0.5);
    write_result(&results_dir, &r);

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).expect("read report");
    assert!(
        content.contains("scen-b"),
        "scen-b must appear in regression list (P@K dropped)"
    );
    assert!(
        content.contains("P@K dropped"),
        "reason must be 'P@K dropped'"
    );
}

// ---------------------------------------------------------------------------
// R-12: both metrics drop
// ---------------------------------------------------------------------------

#[test]
fn test_zero_regression_check_both_regression() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    let r = make_scenario_result("scen-c", "both drop", 0.8, 0.7, 0.5, 0.4);
    write_result(&results_dir, &r);

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).expect("read report");
    assert!(
        content.contains("scen-c"),
        "scen-c must appear in regression list"
    );
    assert!(
        content.contains("both MRR and P@K dropped"),
        "reason must name both metrics"
    );
}

// ---------------------------------------------------------------------------
// AC-09: No regressions → explicit empty-list indicator
// ---------------------------------------------------------------------------

#[test]
fn test_zero_regression_check_no_regressions_empty_indicator() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    let r = make_scenario_result("scen-good", "good query", 0.6, 0.5, 0.7, 0.6);
    write_result(&results_dir, &r);

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).expect("read report");
    assert!(
        content.contains("## 5. Zero-Regression Check"),
        "section must be present"
    );
    assert!(
        content.contains("No regressions detected"),
        "must contain explicit empty-list indicator:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// R-12 boundary: exact equal metrics are NOT a regression
// ---------------------------------------------------------------------------

#[test]
fn test_zero_regression_check_exact_equal_metrics_not_regression() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    let r = make_scenario_result("scen-eq", "equal metrics", 0.6, 0.5, 0.6, 0.5);
    write_result(&results_dir, &r);

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).expect("read report");
    assert!(
        content.contains("No regressions detected"),
        "equal metrics must not be a regression:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// C-07: Always returns Ok(()) regardless of regression count
// ---------------------------------------------------------------------------

#[test]
fn test_report_exit_code_zero_regardless_of_regressions() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    for i in 0..5 {
        let r = make_scenario_result(
            &format!("scen-{i}"),
            &format!("query {i}"),
            0.8,
            0.7,
            0.3,
            0.2,
        );
        write_result(&results_dir, &r);
    }

    let result = run_report(results_dir.path(), None, &out_path);
    assert!(
        result.is_ok(),
        "run_report must always return Ok(()) regardless of regressions: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Edge case: empty results directory
// ---------------------------------------------------------------------------

#[test]
fn test_report_empty_results_dir() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    let result = run_report(results_dir.path(), None, &out_path);
    assert!(result.is_ok(), "must succeed on empty dir: {result:?}");

    let content = std::fs::read_to_string(&out_path).expect("read report");
    assert!(content.contains("## 1. Summary"));
    assert!(content.contains("## 2. Notable Ranking Changes"));
    assert!(content.contains("## 3. Latency Distribution"));
    assert!(content.contains("## 4. Entry-Level Analysis"));
    assert!(content.contains("## 5. Zero-Regression Check"));
    assert!(
        content.contains("No regressions detected"),
        "empty results must show empty indicator:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// Edge case: malformed result JSON file skipped
// ---------------------------------------------------------------------------

#[test]
fn test_report_skips_malformed_result_json() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    let r1 = make_scenario_result("scen-valid-1", "valid query 1", 0.7, 0.6, 0.75, 0.65);
    let r2 = make_scenario_result("scen-valid-2", "valid query 2", 0.5, 0.4, 0.55, 0.45);
    write_result(&results_dir, &r1);
    write_result(&results_dir, &r2);

    std::fs::write(
        results_dir.path().join("malformed.json"),
        "this is not json {{{",
    )
    .unwrap();

    let result = run_report(results_dir.path(), None, &out_path);
    assert!(
        result.is_ok(),
        "must succeed even with malformed files: {result:?}"
    );

    let content = std::fs::read_to_string(&out_path).expect("read report");
    assert!(content.contains("## 1. Summary"));
    assert!(content.contains("## 5. Zero-Regression Check"));
    assert!(
        content.contains("Scenarios: 2"),
        "must have 2 valid scenarios: {content}"
    );
}

// ---------------------------------------------------------------------------
// FR-27: Summary table has per-profile rows
// ---------------------------------------------------------------------------

#[test]
fn test_report_summary_table_has_per_profile_rows() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    let r1 = make_scenario_result("s1", "q1", 0.6, 0.5, 0.7, 0.6);
    let r2 = make_scenario_result("s2", "q2", 0.5, 0.4, 0.6, 0.5);
    let r3 = make_scenario_result("s3", "q3", 0.4, 0.3, 0.5, 0.4);
    write_result(&results_dir, &r1);
    write_result(&results_dir, &r2);
    write_result(&results_dir, &r3);

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).expect("read report");
    assert!(
        content.contains("baseline"),
        "must have baseline row:\n{content}"
    );
    assert!(
        content.contains("candidate"),
        "must have candidate row:\n{content}"
    );
    assert!(
        content.contains("P@K") && content.contains("MRR"),
        "must have P@K and MRR columns:\n{content}"
    );
}

// ---------------------------------------------------------------------------
// FR-27: Latency distribution section non-empty
// ---------------------------------------------------------------------------

#[test]
fn test_report_latency_distribution_present() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    let mut r = make_scenario_result("s1", "q1", 0.6, 0.5, 0.7, 0.6);
    if let Some(prof) = r.profiles.get_mut("baseline") {
        prof.latency_ms = 75;
    }
    if let Some(prof) = r.profiles.get_mut("candidate") {
        prof.latency_ms = 120;
    }
    write_result(&results_dir, &r);

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).expect("read report");
    let section_start = content.find("## 3. Latency Distribution").unwrap();
    let section = &content[section_start..];
    assert!(
        section.contains("| 100 |") || section.contains("| 200 |"),
        "latency buckets must appear:\n{section}"
    );
}

// ---------------------------------------------------------------------------
// FR-27: Entry-level analysis identifies promoted and demoted entries
// ---------------------------------------------------------------------------

#[test]
fn test_report_entry_level_analysis_promotion_demotion() {
    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    // Entry 42: rank 5 → rank 1 (promoted). Entry 99: rank 1 → rank 5 (demoted).
    let mut r = make_scenario_result("s1", "q1", 0.6, 0.5, 0.7, 0.6);
    r.comparison.rank_changes = vec![
        RankChange {
            entry_id: 42,
            from_rank: 5,
            to_rank: 1,
        },
        RankChange {
            entry_id: 99,
            from_rank: 1,
            to_rank: 5,
        },
    ];
    r.profiles.get_mut("baseline").unwrap().entries = vec![
        ScoredEntry {
            id: 99,
            title: "Entry Ninety Nine".to_string(),
            category: String::new(),
            final_score: 0.9,
            similarity: 0.9,
            confidence: 0.9,
            status: "active".to_string(),
            nli_rerank_delta: None,
        },
        ScoredEntry {
            id: 42,
            title: "Entry Forty Two".to_string(),
            category: String::new(),
            final_score: 0.5,
            similarity: 0.5,
            confidence: 0.5,
            status: "active".to_string(),
            nli_rerank_delta: None,
        },
    ];
    write_result(&results_dir, &r);

    run_report(results_dir.path(), None, &out_path).expect("run_report");

    let content = std::fs::read_to_string(&out_path).expect("read report");
    let section_start = content.find("## 4. Entry-Level Analysis").unwrap();
    let section = &content[section_start..];

    assert!(
        section.contains("42"),
        "entry 42 must appear in analysis:\n{section}"
    );
    assert!(
        section.contains("99"),
        "entry 99 must appear in analysis:\n{section}"
    );
    assert!(
        section.contains("Promoted") || section.contains("promoted") || section.contains("+4"),
        "promoted section must mention entry 42:\n{section}"
    );
    assert!(
        section.contains("Demoted") || section.contains("demoted") || section.contains("-4"),
        "demoted section must mention entry 99:\n{section}"
    );
}

// ---------------------------------------------------------------------------
// Unit: compute_aggregate_stats — baseline has zero deltas
// ---------------------------------------------------------------------------

#[test]
fn test_compute_aggregate_stats_baseline_has_zero_deltas() {
    let r = make_scenario_result("s1", "q1", 0.6, 0.5, 0.7, 0.6);
    let stats = compute_aggregate_stats(&[r]);

    let baseline = stats.iter().find(|s| s.profile_name == "baseline").unwrap();
    assert_eq!(baseline.p_at_k_delta, 0.0);
    assert_eq!(baseline.mrr_delta, 0.0);
    assert_eq!(baseline.latency_delta_ms, 0.0);
}

// ---------------------------------------------------------------------------
// Unit: find_regressions — multiple regressions sorted worst-first
// ---------------------------------------------------------------------------

#[test]
fn test_find_regressions_sorted_worst_mrr_first() {
    let r1 = make_scenario_result("s1", "q1", 0.6, 0.8, 0.6, 0.3); // MRR delta = 0.5
    let r2 = make_scenario_result("s2", "q2", 0.6, 0.8, 0.6, 0.6); // MRR delta = 0.2
    let query_map = HashMap::new();
    let regressions = find_regressions(&[r1, r2], &query_map);

    assert_eq!(regressions.len(), 2);
    // s1 has larger MRR drop (0.5) than s2 (0.2), so s1 must come first.
    assert_eq!(regressions[0].scenario_id, "s1");
    assert_eq!(regressions[1].scenario_id, "s2");
}

// ---------------------------------------------------------------------------
// Unit: compute_latency_buckets — correct bucket placement
// ---------------------------------------------------------------------------

#[test]
fn test_compute_latency_buckets_correct_placement() {
    let mut r = make_scenario_result("s1", "q1", 0.6, 0.5, 0.7, 0.6);
    r.profiles.get_mut("baseline").unwrap().latency_ms = 50;
    r.profiles.get_mut("candidate").unwrap().latency_ms = 150;

    let buckets = compute_latency_buckets(&[r]);

    let b50 = buckets.iter().find(|b| b.le_ms == 50).unwrap();
    assert_eq!(b50.count, 1, "latency=50 must land in ≤50 bucket");

    let b200 = buckets.iter().find(|b| b.le_ms == 200).unwrap();
    assert_eq!(b200.count, 1, "latency=150 must land in ≤200 bucket");
}

// ---------------------------------------------------------------------------
// Unit: find_regressions — stable when only one profile (no candidate)
// ---------------------------------------------------------------------------

#[test]
fn test_find_regressions_single_profile_no_regressions() {
    let mut r = ScenarioResult {
        scenario_id: "s1".to_string(),
        query: "q".to_string(),
        profiles: HashMap::new(),
        comparison: default_comparison(),
    };
    r.profiles
        .insert("baseline".to_string(), make_profile_result(0.6, 0.5, 50));

    let query_map = HashMap::new();
    let regressions = find_regressions(&[r], &query_map);
    assert!(
        regressions.is_empty(),
        "single profile must produce no regressions"
    );
}

// ---------------------------------------------------------------------------
// nan-008: Helpers for CC@k / ICD aggregate tests
// ---------------------------------------------------------------------------

/// Build a two-profile ScenarioResult with explicit CC@k and ICD values on each
/// profile and the corresponding deltas on the comparison object.
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
        },
    );
    ScenarioResult {
        scenario_id: id.to_string(),
        query: query.to_string(),
        profiles,
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
// nan-008: test_aggregate_stats_cc_at_k_mean (R-11 guard)
// ---------------------------------------------------------------------------

#[test]
fn test_aggregate_stats_cc_at_k_mean() {
    let r1 = make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.8, 0.7, 0.6, 0.8);
    let r2 = make_scenario_result_with_metrics("s2", "q2", 0.6, 0.5, 0.6, 0.7, 0.8, 0.7, 0.8, 1.0);
    let r3 = make_scenario_result_with_metrics("s3", "q3", 0.6, 0.5, 0.2, 0.3, 0.8, 0.7, 0.4, 0.6);

    let stats = compute_aggregate_stats(&[r1, r2, r3]);

    let baseline = stats.iter().find(|s| s.profile_name == "baseline").unwrap();
    // baseline mean_cc_at_k = (0.4 + 0.6 + 0.2) / 3 = 0.4
    assert!(
        (baseline.mean_cc_at_k - 0.4).abs() < 1e-9,
        "baseline mean_cc_at_k expected 0.4, got {}",
        baseline.mean_cc_at_k
    );

    let candidate = stats
        .iter()
        .find(|s| s.profile_name == "candidate")
        .unwrap();
    // candidate mean_cc_at_k = (0.6 + 0.8 + 0.4) / 3 ≈ 0.6
    assert!(
        (candidate.mean_cc_at_k - 0.6).abs() < 1e-9,
        "candidate mean_cc_at_k expected 0.6, got {}",
        candidate.mean_cc_at_k
    );
}

// ---------------------------------------------------------------------------
// nan-008: test_aggregate_stats_icd_mean (R-11 symmetric)
// ---------------------------------------------------------------------------

#[test]
fn test_aggregate_stats_icd_mean() {
    let r1 = make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.8, 0.7, 0.6, 0.8);
    let r2 = make_scenario_result_with_metrics("s2", "q2", 0.6, 0.5, 0.6, 0.7, 0.8, 0.7, 0.8, 1.0);
    let r3 = make_scenario_result_with_metrics("s3", "q3", 0.6, 0.5, 0.2, 0.3, 0.8, 0.7, 0.4, 0.6);

    let stats = compute_aggregate_stats(&[r1, r2, r3]);

    let baseline = stats.iter().find(|s| s.profile_name == "baseline").unwrap();
    // baseline mean_icd = (0.5 + 0.7 + 0.3) / 3 ≈ 0.5
    assert!(
        (baseline.mean_icd - 0.5).abs() < 1e-9,
        "baseline mean_icd expected 0.5, got {}",
        baseline.mean_icd
    );

    let candidate = stats
        .iter()
        .find(|s| s.profile_name == "candidate")
        .unwrap();
    // candidate mean_icd = (0.8 + 1.0 + 0.6) / 3 ≈ 0.8
    assert!(
        (candidate.mean_icd - 0.8).abs() < 1e-9,
        "candidate mean_icd expected 0.8, got {}",
        candidate.mean_icd
    );
}

// ---------------------------------------------------------------------------
// nan-008: test_aggregate_stats_cc_at_k_delta_mean (R-11 for delta)
// ---------------------------------------------------------------------------

#[test]
fn test_aggregate_stats_cc_at_k_delta_mean() {
    // cc_at_k_delta values: 0.2, 0.4, 0.0 (candidate_cc - baseline_cc).
    let r1 = make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.6, 0.7, 0.6, 0.8);
    // cc_at_k_delta = 0.6 - 0.4 = 0.2
    let r2 = make_scenario_result_with_metrics("s2", "q2", 0.6, 0.5, 0.4, 0.5, 0.8, 0.7, 0.8, 0.8);
    // cc_at_k_delta = 0.8 - 0.4 = 0.4
    let r3 = make_scenario_result_with_metrics("s3", "q3", 0.6, 0.5, 0.5, 0.5, 0.8, 0.7, 0.5, 0.8);
    // cc_at_k_delta = 0.5 - 0.5 = 0.0

    let stats = compute_aggregate_stats(&[r1, r2, r3]);

    let candidate = stats
        .iter()
        .find(|s| s.profile_name == "candidate")
        .unwrap();
    let expected = (0.2 + 0.4 + 0.0) / 3.0;
    assert!(
        (candidate.cc_at_k_delta - expected).abs() < 1e-9,
        "candidate cc_at_k_delta expected {expected}, got {}",
        candidate.cc_at_k_delta
    );
}

// ---------------------------------------------------------------------------
// nan-008: test_aggregate_stats_baseline_has_zero_cc_at_k_delta
// ---------------------------------------------------------------------------

#[test]
fn test_aggregate_stats_baseline_has_zero_cc_at_k_delta() {
    let r1 = make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.7, 0.6, 0.7, 0.8);
    let r2 = make_scenario_result_with_metrics("s2", "q2", 0.5, 0.4, 0.3, 0.4, 0.6, 0.5, 0.6, 0.7);

    let stats = compute_aggregate_stats(&[r1, r2]);

    let baseline = stats.iter().find(|s| s.profile_name == "baseline").unwrap();
    assert_eq!(
        baseline.cc_at_k_delta, 0.0,
        "baseline cc_at_k_delta must be 0.0"
    );
    assert_eq!(baseline.icd_delta, 0.0, "baseline icd_delta must be 0.0");
}

// ---------------------------------------------------------------------------
// nan-008: test_cc_at_k_scenario_rows_sort_order (R-12 guard)
// ---------------------------------------------------------------------------

#[test]
fn test_cc_at_k_scenario_rows_sort_order() {
    // Deltas: s1=0.1, s2=-0.3, s3=0.5, s4=-0.1, s5=0.2
    // Expected descending order: s3(0.5), s5(0.2), s1(0.1), s4(-0.1), s2(-0.3)
    let s1 = make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.7, 0.6, 0.5, 0.7);
    // cc_at_k_delta = 0.1
    let s2 = make_scenario_result_with_metrics("s2", "q2", 0.6, 0.5, 0.7, 0.5, 0.7, 0.6, 0.4, 0.7);
    // cc_at_k_delta = -0.3
    let s3 = make_scenario_result_with_metrics("s3", "q3", 0.6, 0.5, 0.2, 0.5, 0.7, 0.6, 0.7, 0.7);
    // cc_at_k_delta = 0.5
    let s4 = make_scenario_result_with_metrics("s4", "q4", 0.6, 0.5, 0.6, 0.5, 0.7, 0.6, 0.5, 0.7);
    // cc_at_k_delta = -0.1
    let s5 = make_scenario_result_with_metrics("s5", "q5", 0.6, 0.5, 0.3, 0.5, 0.7, 0.6, 0.5, 0.7);
    // cc_at_k_delta = 0.2

    let rows = compute_cc_at_k_scenario_rows(&[s1, s2, s3, s4, s5]);

    assert_eq!(rows.len(), 5, "expected 5 rows");
    assert_eq!(rows[0].scenario_id, "s3", "s3 (delta=0.5) must be first");
    assert_eq!(rows[1].scenario_id, "s5", "s5 (delta=0.2) must be second");
    assert_eq!(rows[2].scenario_id, "s1", "s1 (delta=0.1) must be third");
    assert_eq!(rows[3].scenario_id, "s4", "s4 (delta=-0.1) must be fourth");
    assert_eq!(rows[4].scenario_id, "s2", "s2 (delta=-0.3) must be last");
}

// ---------------------------------------------------------------------------
// nan-008: test_cc_at_k_scenario_rows_single_profile_returns_empty
// ---------------------------------------------------------------------------

#[test]
fn test_cc_at_k_scenario_rows_single_profile_returns_empty() {
    // Single-profile result: no candidate, so no comparison rows possible.
    let mut r = ScenarioResult {
        scenario_id: "s1".to_string(),
        query: "q".to_string(),
        profiles: HashMap::new(),
        comparison: default_comparison(),
    };
    r.profiles.insert(
        "baseline".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 50,
            p_at_k: 0.6,
            mrr: 0.5,
            cc_at_k: 0.4,
            icd: 0.5,
        },
    );

    let rows = compute_cc_at_k_scenario_rows(&[r]);
    assert!(
        rows.is_empty(),
        "single-profile result must produce no rows"
    );
}

// ---------------------------------------------------------------------------
// nan-008: test_cc_at_k_scenario_rows_uses_comparison_delta
// ---------------------------------------------------------------------------

#[test]
fn test_cc_at_k_scenario_rows_uses_comparison_delta() {
    // comparison.cc_at_k_delta = 0.3 (stored), but candidate(0.7) - baseline(0.4) = 0.3.
    // The row must use the stored comparison delta, not a recomputed one.
    // We verify this by setting the stored delta to a slightly different value
    // from the naive subtraction to confirm the stored value wins.
    let mut r =
        make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.8, 0.7, 0.7, 0.8);
    // Override comparison.cc_at_k_delta to a distinct stored value (0.3 instead of 0.3).
    // Actually use 0.30000001 to make the stored vs. recomputed distinction explicit.
    r.comparison.cc_at_k_delta = 0.30000001_f64;

    let rows = compute_cc_at_k_scenario_rows(&[r]);
    assert_eq!(rows.len(), 1);
    assert!(
        (rows[0].cc_at_k_delta - 0.30000001_f64).abs() < 1e-12,
        "row must use stored comparison delta, got {}",
        rows[0].cc_at_k_delta
    );
}

// ---------------------------------------------------------------------------
// nan-008: test_cc_at_k_scenario_rows_single_scenario
// ---------------------------------------------------------------------------

#[test]
fn test_cc_at_k_scenario_rows_single_scenario() {
    let r = make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.8, 0.7, 0.7, 0.8);
    // cc_at_k_delta = 0.7 - 0.4 = 0.3

    let rows = compute_cc_at_k_scenario_rows(&[r]);
    assert_eq!(
        rows.len(),
        1,
        "single two-profile scenario must produce 1 row"
    );
    assert!(
        (rows[0].cc_at_k_delta - 0.3).abs() < 1e-9,
        "cc_at_k_delta expected 0.3, got {}",
        rows[0].cc_at_k_delta
    );
}

// ---------------------------------------------------------------------------
// nan-008: test_cc_at_k_scenario_rows_empty
// ---------------------------------------------------------------------------

#[test]
fn test_cc_at_k_scenario_rows_empty() {
    let rows = compute_cc_at_k_scenario_rows(&[]);
    assert!(rows.is_empty(), "empty input must produce empty rows");
}
