//! Phase-related unit tests for eval/report (nan-009).
//!
//! Unit tests for `compute_phase_stats` and `render_phase_section`.
//! Serde backward-compat tests (AC-06) also live here.
//! Full-pipeline (run_report) tests are in tests_phase_pipeline.rs.
//!
//! Covers: AC-05 (unset sorts last), AC-06 (backward compat serde),
//! R-01 (null bucket label), R-07 (all-null → empty), R-08 (unset sorts last),
//! R-09 (empty input → empty string).

use std::collections::HashMap;

use super::aggregate::compute_phase_stats;
use super::render_phase::render_phase_section;
use super::{PhaseAggregateStats, ProfileResult, ScenarioResult, default_comparison};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a report-module `ScenarioResult` with one "baseline" profile
/// and explicit phase + metric values.
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

// ---------------------------------------------------------------------------
// compute_phase_stats — unit tests
// ---------------------------------------------------------------------------

/// R-01: The null bucket must use exactly "(unset)", never "(none)".
///
/// Mixed input (one non-null + one null phase) is used so the all-null guard
/// does not elide the null bucket. The label of the null bucket is what is verified.
#[test]
fn test_compute_phase_stats_null_bucket_label() {
    let results = vec![
        make_result_with_phase("s1", Some("delivery"), 0.8, 0.7, 0.5, 1.0),
        make_result_with_phase("s2", None, 0.5, 0.4, 0.3, 0.7),
    ];

    let stats = compute_phase_stats(&results);

    assert_eq!(
        stats.len(),
        2,
        "expected 2 buckets: 'delivery' and '(unset)'"
    );

    let unset = stats.iter().find(|s| s.phase_label != "delivery");
    assert!(unset.is_some(), "null bucket must be present");
    let unset = unset.unwrap();

    assert_eq!(
        unset.phase_label, "(unset)",
        "null bucket label must be exactly '(unset)'"
    );
    assert_ne!(
        unset.phase_label, "(none)",
        "'(none)' must never appear — canonical is '(unset)' (R-01)"
    );
}

/// EC-01, FM-03: Empty input must produce empty output without panicking.
#[test]
fn test_compute_phase_stats_empty_input_returns_empty() {
    let results: Vec<ScenarioResult> = vec![];

    let stats = compute_phase_stats(&results);

    assert!(
        stats.is_empty(),
        "empty input must produce empty output, not panic"
    );
}

/// R-07, AC-09 item 4: All-null phases must return empty vec (section 6 omitted per AC-04).
#[test]
fn test_compute_phase_stats_all_null_returns_empty() {
    let results = vec![
        make_result_with_phase("s1", None, 0.7, 0.6, 0.5, 1.0),
        make_result_with_phase("s2", None, 0.5, 0.4, 0.3, 0.8),
        make_result_with_phase("s3", None, 0.6, 0.5, 0.4, 0.9),
    ];

    let stats = compute_phase_stats(&results);

    assert!(
        stats.is_empty(),
        "all-null phases must return empty vec (section 6 omitted per AC-04)"
    );
}

/// R-08, AC-05: "(unset)" must sort last despite ASCII ordering ('(' < 'a').
#[test]
fn test_compute_phase_stats_null_bucket_sorts_last() {
    let results = vec![
        make_result_with_phase("s1", Some("delivery"), 0.8, 0.7, 0.5, 1.0),
        make_result_with_phase("s2", Some("design"), 0.7, 0.6, 0.4, 0.9),
        make_result_with_phase("s3", None, 0.5, 0.4, 0.3, 0.7),
        make_result_with_phase("s4", Some("bugfix"), 0.6, 0.5, 0.3, 0.8),
        make_result_with_phase("s5", Some("delivery"), 0.9, 0.8, 0.6, 1.1),
    ];

    let stats = compute_phase_stats(&results);

    // 3 named phases + 1 null bucket = 4 entries.
    assert_eq!(stats.len(), 4, "expected 4 phase buckets");

    // Named phases in alphabetical order.
    assert_eq!(stats[0].phase_label, "bugfix", "first must be 'bugfix'");
    assert_eq!(
        stats[1].phase_label, "delivery",
        "second must be 'delivery'"
    );
    assert_eq!(stats[2].phase_label, "design", "third must be 'design'");

    // Null bucket unconditionally last — even though '(' < 'b' in ASCII.
    assert_eq!(
        stats.last().unwrap().phase_label,
        "(unset)",
        "null bucket must be last (R-08, AC-05)"
    );

    // Counts correct.
    assert_eq!(stats[1].scenario_count, 2, "'delivery' must have count 2");
    assert_eq!(stats[3].scenario_count, 1, "null bucket must have count 1");

    // Mean P@K for "delivery": (0.8 + 0.9) / 2 = 0.85.
    assert!(
        (stats[1].mean_p_at_k - 0.85).abs() < 1e-9,
        "delivery mean_p_at_k expected 0.85, got {}",
        stats[1].mean_p_at_k
    );
}

/// AC-09 item 3: Correct grouping and means across multiple named phases.
#[test]
fn test_compute_phase_stats_single_phase() {
    let results = vec![
        make_result_with_phase("s1", Some("delivery"), 0.8, 0.7, 0.5, 1.0),
        make_result_with_phase("s2", Some("delivery"), 0.6, 0.5, 0.3, 0.8),
    ];

    let stats = compute_phase_stats(&results);

    assert_eq!(stats.len(), 1, "one phase → one bucket");
    assert_eq!(stats[0].phase_label, "delivery");
    assert_eq!(stats[0].scenario_count, 2);
    // mean_p_at_k = (0.8 + 0.6) / 2 = 0.7
    assert!(
        (stats[0].mean_p_at_k - 0.7).abs() < 1e-9,
        "mean_p_at_k expected 0.7, got {}",
        stats[0].mean_p_at_k
    );
}

/// AC-09 item 3: Multiple phases — correct grouping and mean arithmetic.
#[test]
fn test_compute_phase_stats_multiple_phases() {
    let results = vec![
        make_result_with_phase("s1", Some("delivery"), 1.0, 0.8, 0.6, 1.2),
        make_result_with_phase("s2", Some("delivery"), 0.0, 0.2, 0.4, 0.8),
        make_result_with_phase("s3", Some("design"), 0.6, 0.6, 0.3, 1.0),
    ];

    let stats = compute_phase_stats(&results);

    assert_eq!(stats.len(), 2, "delivery and design → 2 buckets");

    let delivery = stats.iter().find(|s| s.phase_label == "delivery").unwrap();
    assert_eq!(delivery.scenario_count, 2);
    assert!(
        (delivery.mean_p_at_k - 0.5).abs() < 1e-9,
        "delivery mean_p_at_k expected 0.5"
    );
    assert!(
        (delivery.mean_mrr - 0.5).abs() < 1e-9,
        "delivery mean_mrr expected 0.5"
    );
    assert!(
        (delivery.mean_cc_at_k - 0.5).abs() < 1e-9,
        "delivery mean_cc_at_k expected 0.5"
    );
    assert!(
        (delivery.mean_icd - 1.0).abs() < 1e-9,
        "delivery mean_icd expected 1.0"
    );

    let design = stats.iter().find(|s| s.phase_label == "design").unwrap();
    assert_eq!(design.scenario_count, 1);
    assert!(
        (design.mean_p_at_k - 0.6).abs() < 1e-9,
        "design mean_p_at_k expected 0.6"
    );
    assert!(
        (design.mean_mrr - 0.6).abs() < 1e-9,
        "design mean_mrr expected 0.6"
    );
}

/// Mean value correctness across groups (companion to null_bucket_sorts_last).
#[test]
fn test_compute_phase_stats_mean_values_correct() {
    let results = vec![
        make_result_with_phase("s1", Some("delivery"), 1.0, 0.8, 0.6, 1.2),
        make_result_with_phase("s2", Some("delivery"), 0.0, 0.2, 0.4, 0.8),
    ];

    let stats = compute_phase_stats(&results);

    assert_eq!(stats.len(), 1);
    assert!(
        (stats[0].mean_p_at_k - 0.5).abs() < 1e-9,
        "mean_p_at_k = (1.0 + 0.0)/2 = 0.5"
    );
    assert!(
        (stats[0].mean_mrr - 0.5).abs() < 1e-9,
        "mean_mrr = (0.8 + 0.2)/2 = 0.5"
    );
    assert!(
        (stats[0].mean_cc_at_k - 0.5).abs() < 1e-9,
        "mean_cc_at_k = (0.6 + 0.4)/2 = 0.5"
    );
    assert!(
        (stats[0].mean_icd - 1.0).abs() < 1e-9,
        "mean_icd = (1.2 + 0.8)/2 = 1.0"
    );
}

// ---------------------------------------------------------------------------
// render_phase_section — unit tests
// ---------------------------------------------------------------------------

/// R-09: Empty input must return empty string with no heading.
#[test]
fn test_render_phase_section_empty_input_returns_empty_string() {
    let stats: &[PhaseAggregateStats] = &[];

    let output = render_phase_section(stats);

    assert_eq!(
        output, "",
        "empty stats must produce empty string, not a heading"
    );
    assert!(
        !output.contains("## 6."),
        "no section heading for empty stats"
    );
}

/// R-09: render_phase_section produces a table header when stats is non-empty.
#[test]
fn test_render_phase_section_renders_table_header() {
    let stats = vec![PhaseAggregateStats {
        phase_label: "delivery".to_string(),
        scenario_count: 1,
        mean_p_at_k: 0.8,
        mean_mrr: 0.7,
        mean_cc_at_k: 0.5,
        mean_icd: 1.0,
    }];

    let output = render_phase_section(&stats);

    assert!(
        output.contains("## 6. Phase-Stratified Metrics"),
        "must contain section 6 heading"
    );
    assert!(
        output.contains("| Phase | Count | P@K | MRR | CC@k | ICD |"),
        "must contain table header row"
    );
    assert!(
        output.contains("delivery"),
        "must contain 'delivery' phase label"
    );
}

/// Null bucket "(unset)" must appear in rendered output.
#[test]
fn test_render_phase_section_renders_unset_bucket() {
    let stats = vec![
        PhaseAggregateStats {
            phase_label: "delivery".to_string(),
            scenario_count: 2,
            mean_p_at_k: 0.8,
            mean_mrr: 0.7,
            mean_cc_at_k: 0.5,
            mean_icd: 1.0,
        },
        PhaseAggregateStats {
            phase_label: "(unset)".to_string(),
            scenario_count: 1,
            mean_p_at_k: 0.5,
            mean_mrr: 0.4,
            mean_cc_at_k: 0.3,
            mean_icd: 0.7,
        },
    ];

    let output = render_phase_section(&stats);

    assert!(
        output.contains("(unset)"),
        "rendered output must contain '(unset)' bucket"
    );
    assert!(
        !output.contains("(none)"),
        "'(none)' must never appear — canonical is '(unset)'"
    );
}

// ---------------------------------------------------------------------------
// Serde backward-compat tests (AC-06)
// ---------------------------------------------------------------------------

/// AC-06, EC-05, NFR-01: Legacy result without "phase" key deserializes as None.
#[test]
fn test_scenario_result_phase_absent_key_deserializes_as_none() {
    // JSON without any "phase" key — represents a pre-nan-009 result file.
    let json = r#"{
        "scenario_id": "legacy-01",
        "query": "what is context_search?",
        "profiles": {},
        "comparison": {
            "kendall_tau": 0.8,
            "rank_changes": [],
            "mrr_delta": 0.1,
            "p_at_k_delta": 0.05,
            "latency_overhead_ms": 10,
            "cc_at_k_delta": 0.0,
            "icd_delta": 0.0
        }
    }"#;

    let result: ScenarioResult =
        serde_json::from_str(json).expect("legacy result must deserialize without error");

    assert!(
        result.phase.is_none(),
        "missing 'phase' key must default to None (NFR-01)"
    );
}

/// AC-06, EC-06: Explicit "phase":null deserializes as None.
#[test]
fn test_report_deserializes_explicit_null_phase_key() {
    let json = r#"{
        "scenario_id": "null-phase-01",
        "query": "some query",
        "phase": null,
        "profiles": {},
        "comparison": {
            "kendall_tau": 0.0,
            "rank_changes": [],
            "mrr_delta": 0.0,
            "p_at_k_delta": 0.0,
            "latency_overhead_ms": 0,
            "cc_at_k_delta": 0.0,
            "icd_delta": 0.0
        }
    }"#;

    let result: ScenarioResult =
        serde_json::from_str(json).expect("explicit null phase must deserialize without error");

    assert!(
        result.phase.is_none(),
        "explicit null 'phase' must deserialize as None (EC-06)"
    );
}
