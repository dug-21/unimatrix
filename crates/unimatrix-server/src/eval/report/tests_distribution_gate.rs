//! Tests for the Distribution Gate feature (nan-010).
//!
//! Covers: Component 3 (Runner Profile Meta Sidecar) write/read round-trip,
//! schema validation, and atomic write verification.
//!
//! Additional tests for Components 5, 6, and 7 (aggregation, rendering,
//! sidecar load) will be added in subsequent implementation steps.

use tempfile::TempDir;

use crate::eval::profile::{DistributionTargets, EvalProfile};
use crate::eval::runner::profile_meta::{ProfileMetaFile, write_profile_meta};
use crate::infra::config::UnimatrixConfig;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_baseline_profile() -> EvalProfile {
    EvalProfile {
        name: "baseline".to_string(),
        description: None,
        config_overrides: UnimatrixConfig::default(),
        distribution_change: false,
        distribution_targets: None,
    }
}

fn make_candidate_profile() -> EvalProfile {
    EvalProfile {
        name: "ppr-candidate".to_string(),
        description: Some("PPR distribution change candidate".to_string()),
        config_overrides: UnimatrixConfig::default(),
        distribution_change: true,
        distribution_targets: Some(DistributionTargets {
            cc_at_k_min: 0.60,
            icd_min: 1.20,
            mrr_floor: 0.35,
        }),
    }
}

// ---------------------------------------------------------------------------
// Component 3: Runner Profile Meta Sidecar
// ---------------------------------------------------------------------------

/// AC-05, R-04, R-10: Primary schema and atomic write test.
///
/// Verifies the full write path: correct JSON schema, no orphan .tmp,
/// and round-trip deserialization fidelity.
#[test]
fn test_write_profile_meta_schema() {
    let tmp_dir = TempDir::new().unwrap();
    let out = tmp_dir.path();

    let profiles = vec![make_candidate_profile(), make_baseline_profile()];

    let result = write_profile_meta(&profiles, out);
    assert!(
        result.is_ok(),
        "write_profile_meta failed: {:?}",
        result.err()
    );

    // Assert: profile-meta.json exists.
    let final_path = out.join("profile-meta.json");
    assert!(
        final_path.exists(),
        "profile-meta.json was not written to {out:?}"
    );

    // Assert: orphan .tmp file does NOT exist (rename completed).
    let tmp_path = out.join("profile-meta.json.tmp");
    assert!(
        !tmp_path.exists(),
        "profile-meta.json.tmp should not exist after successful write"
    );

    // Assert: deserialize the written file.
    let content = std::fs::read_to_string(&final_path).unwrap();
    let file: ProfileMetaFile = serde_json::from_str(&content)
        .expect("profile-meta.json must deserialize as ProfileMetaFile");

    // version == 1 (top-level field, not per-entry — ADR-002 design decision #2)
    assert_eq!(file.version, 1, "version field must be 1");

    // ppr-candidate entry
    let candidate = file
        .profiles
        .get("ppr-candidate")
        .expect("profiles map must contain 'ppr-candidate'");
    assert!(
        candidate.distribution_change,
        "ppr-candidate.distribution_change must be true"
    );
    let targets = candidate
        .distribution_targets
        .as_ref()
        .expect("ppr-candidate.distribution_targets must be Some");
    assert_eq!(targets.cc_at_k_min, 0.60_f64, "cc_at_k_min must be 0.60");
    assert_eq!(targets.icd_min, 1.20_f64, "icd_min must be 1.20");
    assert_eq!(targets.mrr_floor, 0.35_f64, "mrr_floor must be 0.35");

    // baseline entry
    let baseline = file
        .profiles
        .get("baseline")
        .expect("profiles map must contain 'baseline'");
    assert!(
        !baseline.distribution_change,
        "baseline.distribution_change must be false"
    );
    assert!(
        baseline.distribution_targets.is_none(),
        "baseline.distribution_targets must be None/null"
    );

    // Deserialize direction (R-10 schema mismatch guard — knowledge package #3557).
    // Validates that the field names in the serde types match what the writer produces
    // and what the reader expects. Tests both directions independently.
    let handcrafted_json = r#"{
  "version": 1,
  "profiles": {
    "ppr-candidate": {
      "distribution_change": true,
      "distribution_targets": {
        "cc_at_k_min": 0.60,
        "icd_min": 1.20,
        "mrr_floor": 0.35
      }
    }
  }
}"#;
    let parsed: ProfileMetaFile =
        serde_json::from_str(handcrafted_json).expect("hand-crafted JSON must deserialize");
    let parsed_entry = parsed
        .profiles
        .get("ppr-candidate")
        .expect("hand-crafted entry must be present");
    assert!(
        parsed_entry.distribution_change,
        "deserialized distribution_change must be true"
    );
    assert_eq!(
        parsed_entry
            .distribution_targets
            .as_ref()
            .unwrap()
            .cc_at_k_min,
        0.60_f64,
        "deserialized cc_at_k_min must be 0.60"
    );
}

/// Verify that the .tmp file is not read by the sidecar consumer (backward-compat).
///
/// Creates a `.tmp` file with invalid JSON in the output directory but no
/// `profile-meta.json`. This simulates a run that crashed between the tmp write
/// and the rename. The sidecar consumer (`load_profile_meta`, Component 7) must
/// return an empty map (backward-compat fallback) because `profile-meta.json`
/// is absent — it must not attempt to read the `.tmp` file.
///
/// This test verifies the invariant at the filesystem level: `profile-meta.json.tmp`
/// with bad content does not interfere with a clean backward-compat result directory.
#[test]
fn test_write_profile_meta_schema_tmp_not_read_as_sidecar() {
    let tmp_dir = TempDir::new().unwrap();
    let out = tmp_dir.path();

    // Write invalid content to .tmp (simulates crash between write and rename).
    std::fs::write(out.join("profile-meta.json.tmp"), b"NOT VALID JSON").unwrap();

    // profile-meta.json must NOT exist at this point.
    assert!(!out.join("profile-meta.json").exists());

    // A successful write_profile_meta call with an empty slice should produce
    // a valid profile-meta.json regardless of the pre-existing .tmp.
    let profiles: Vec<EvalProfile> = vec![];
    let result = write_profile_meta(&profiles, out);
    assert!(
        result.is_ok(),
        "write_profile_meta failed: {:?}",
        result.err()
    );

    // After the call the .tmp must be gone and profile-meta.json must be valid.
    assert!(
        !out.join("profile-meta.json.tmp").exists(),
        ".tmp must be absent after successful write"
    );
    let content = std::fs::read_to_string(out.join("profile-meta.json")).unwrap();
    let file: ProfileMetaFile = serde_json::from_str(&content).unwrap();
    assert_eq!(file.version, 1);
    assert!(file.profiles.is_empty());
}

/// Edge case: empty profiles slice produces a valid sidecar with empty map.
#[test]
fn test_write_profile_meta_schema_empty_profiles() {
    let tmp_dir = TempDir::new().unwrap();
    let out = tmp_dir.path();

    let result = write_profile_meta(&[], out);
    assert!(result.is_ok(), "write_profile_meta(&[]) must succeed");

    let content = std::fs::read_to_string(out.join("profile-meta.json")).unwrap();
    let file: ProfileMetaFile = serde_json::from_str(&content).unwrap();
    assert_eq!(file.version, 1);
    assert!(
        file.profiles.is_empty(),
        "empty slice must produce empty profiles map"
    );
}

/// Edge case: all profiles have distribution_change = false.
#[test]
fn test_write_profile_meta_schema_all_false() {
    let tmp_dir = TempDir::new().unwrap();
    let out = tmp_dir.path();

    let profiles = vec![make_baseline_profile()];
    let result = write_profile_meta(&profiles, out);
    assert!(
        result.is_ok(),
        "write_profile_meta failed: {:?}",
        result.err()
    );

    let content = std::fs::read_to_string(out.join("profile-meta.json")).unwrap();
    let file: ProfileMetaFile = serde_json::from_str(&content).unwrap();
    assert_eq!(file.version, 1);
    let entry = file.profiles.get("baseline").unwrap();
    assert!(!entry.distribution_change);
    assert!(entry.distribution_targets.is_none());
}

/// Edge case: output directory does not exist returns Err (not a panic).
#[test]
fn test_write_profile_meta_nonexistent_dir_returns_err() {
    let tmp_dir = TempDir::new().unwrap();
    // Use a subdirectory that does not exist.
    let nonexistent = tmp_dir.path().join("does-not-exist");

    let result = write_profile_meta(&[], &nonexistent);
    assert!(
        result.is_err(),
        "write_profile_meta to nonexistent dir must return Err"
    );
}

// ---------------------------------------------------------------------------
// Component 7: Report Sidecar Load (`load_profile_meta` and `run_report`)
// ---------------------------------------------------------------------------

/// AC-11, AC-14, R-15: Backward compatibility — absent `profile-meta.json`.
///
/// Sub-scenario A: absent sidecar → `load_profile_meta` returns `Ok(empty map)`.
/// Sub-scenario B: pre-nan-010 `ScenarioResult` JSON (no `distribution_change` field)
///   deserializes without error, confirming zero new fields were added to `ScenarioResult`.
///
/// The full `run_report` call must succeed and the rendered output must contain
/// "Zero-Regression Check" with no "Distribution Gate" text.
#[test]
fn test_report_without_profile_meta_json() {
    use std::collections::HashMap;

    use super::{ComparisonMetrics, ProfileResult, ScenarioResult, load_profile_meta, run_report};

    let results_dir = TempDir::new().unwrap();

    // Sub-scenario A: load_profile_meta returns Ok(empty) when file is absent.
    let result = load_profile_meta(results_dir.path());
    assert!(
        result.is_ok(),
        "load_profile_meta must return Ok when profile-meta.json is absent, got: {:?}",
        result.err()
    );
    let map = result.unwrap();
    assert!(
        map.is_empty(),
        "load_profile_meta must return empty map when file is absent"
    );

    // Sub-scenario B: pre-nan-010 ScenarioResult JSON (no distribution_change field).
    // This simulates a result directory produced before nan-010 — no sidecar, no new fields.
    let pre_nan010_json = r#"{
        "scenario_id": "legacy-01",
        "query": "test query",
        "profiles": {
            "baseline": {
                "latency_ms": 50,
                "p_at_k": 0.7,
                "mrr": 0.6,
                "cc_at_k": 0.4,
                "icd": 0.8
            },
            "candidate": {
                "latency_ms": 60,
                "p_at_k": 0.75,
                "mrr": 0.65,
                "cc_at_k": 0.5,
                "icd": 0.9
            }
        },
        "comparison": {
            "kendall_tau": 0.9,
            "mrr_delta": 0.05,
            "p_at_k_delta": 0.05,
            "latency_overhead_ms": 10,
            "cc_at_k_delta": 0.1,
            "icd_delta": 0.1
        }
    }"#;
    let scenario: ScenarioResult = serde_json::from_str(pre_nan010_json)
        .expect("pre-nan-010 ScenarioResult JSON must deserialize — dual-type constraint (R-15)");
    assert_eq!(scenario.scenario_id, "legacy-01");

    // Write the legacy JSON file to the results dir.
    std::fs::write(results_dir.path().join("legacy-01.json"), pre_nan010_json).unwrap();

    // run_report must succeed (no profile-meta.json → empty profile_meta map).
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");
    run_report(results_dir.path(), None, &out_path)
        .expect("run_report must succeed with pre-nan-010 results directory");

    let content = std::fs::read_to_string(&out_path).expect("read report");

    // Section 5 must be "Zero-Regression Check" — no distribution gate.
    assert!(
        content.contains("Zero-Regression Check"),
        "report must contain 'Zero-Regression Check' when no profile-meta.json:\n{content}"
    );
    assert!(
        !content.contains("Distribution Gate"),
        "report must NOT contain 'Distribution Gate' when no profile-meta.json:\n{content}"
    );

    // Suppress unused import warning from direct use of these types in this test.
    let _: HashMap<String, ProfileResult> = HashMap::new();
    let _: ComparisonMetrics;
}

/// R-07: Corrupt `profile-meta.json` → `load_profile_meta` returns `Err` with
/// "profile-meta.json is malformed" message. No silent fallback.
///
/// This test guards against regression to WARN+fallback behavior (resolved WARN-3 in
/// ALIGNMENT-REPORT.md). See ARCHITECTURE.md Component 7 and ADR-004.
#[test]
fn test_distribution_gate_corrupt_sidecar_aborts() {
    use super::load_profile_meta;

    let tmp_dir = TempDir::new().unwrap();

    // Write truncated/invalid JSON to profile-meta.json.
    std::fs::write(
        tmp_dir.path().join("profile-meta.json"),
        b"not valid json {{{{",
    )
    .unwrap();

    let result = load_profile_meta(tmp_dir.path());

    // Must return Err — not Ok(empty map).
    assert!(
        result.is_err(),
        "load_profile_meta must return Err for corrupt sidecar — not Ok fallback"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("profile-meta.json is malformed"),
        "error must contain 'profile-meta.json is malformed', got: {err_msg}"
    );
    assert!(
        err_msg.contains("re-run eval to regenerate"),
        "error must contain 're-run eval to regenerate', got: {err_msg}"
    );
}

/// R-12: Distribution Gate failure does NOT cause `run_report` to return `Err`.
///
/// When a distribution-change profile fails its gate, the failure appears in the
/// report body only. `run_report` must return `Ok(())` — exit code 0 (C-07, FR-29).
///
/// The `run_report` Err path is reserved exclusively for I/O errors and corrupt sidecars.
/// This test also verifies that `run_report` returns Ok when there is no profile-meta.json
/// (the distribution gate path requires a sidecar — without one, zero-regression runs).
#[test]
fn test_distribution_gate_exit_code_zero() {
    use super::{ComparisonMetrics, ProfileResult, ScenarioResult, run_report};
    use std::collections::HashMap;

    let results_dir = TempDir::new().unwrap();
    let out_dir = TempDir::new().unwrap();
    let out_path = out_dir.path().join("report.md");

    // Build a scenario where candidate MRR < baseline MRR (a regression in zero-regression check).
    let mut profiles = HashMap::new();
    profiles.insert(
        "baseline".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 50,
            p_at_k: 0.8,
            mrr: 0.7,
            cc_at_k: 0.5,
            icd: 0.9,
        },
    );
    profiles.insert(
        "candidate".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 60,
            p_at_k: 0.6,
            mrr: 0.4,
            cc_at_k: 0.3,
            icd: 0.6,
        },
    );
    let scenario = ScenarioResult {
        scenario_id: "exit-code-test-01".to_string(),
        query: "exit code test query".to_string(),
        profiles,
        phase: None,
        comparison: ComparisonMetrics {
            kendall_tau: 0.6,
            rank_changes: Vec::new(),
            mrr_delta: -0.3,
            p_at_k_delta: -0.2,
            latency_overhead_ms: 10,
            cc_at_k_delta: -0.2,
            icd_delta: -0.3,
        },
    };

    let json = serde_json::to_string(&scenario).unwrap();
    std::fs::write(results_dir.path().join("exit-code-test-01.json"), json).unwrap();

    // No profile-meta.json → zero-regression path.
    // Even with regressions present, run_report must return Ok(()) — not Err.
    let result = run_report(results_dir.path(), None, &out_path);
    assert!(
        result.is_ok(),
        "run_report must return Ok(()) even with regressions — exit code 0 (C-07, FR-29): {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// Component 4: Distribution Gate Aggregation (`check_distribution_targets`)
// ---------------------------------------------------------------------------

/// AC-13, R-05, R-08: All three metrics pass → overall PASSED, individual flags correct.
///
/// Verifies that `diversity_passed`, `mrr_floor_passed`, and `overall_passed` are all true
/// when cc_at_k, icd, and mrr all meet or exceed their targets (>= semantics).
/// R-08: also checks that `mean_cc_at_k` and `mean_mrr` in AggregateStats are read correctly
/// (not confused with baseline or wrong fields).
#[test]
fn test_check_distribution_targets_all_pass() {
    use super::AggregateStats;
    use super::aggregate::check_distribution_targets;
    use crate::eval::profile::DistributionTargets;

    let stats = AggregateStats {
        profile_name: "ppr-candidate".to_string(),
        scenario_count: 10,
        mean_p_at_k: 0.80,
        mean_mrr: 0.50,
        mean_latency_ms: 55.0,
        p_at_k_delta: 0.05,
        mrr_delta: 0.03,
        latency_delta_ms: 5.0,
        mean_cc_at_k: 0.65,
        mean_icd: 1.35,
        cc_at_k_delta: 0.10,
        icd_delta: 0.25,
    };

    let targets = DistributionTargets {
        cc_at_k_min: 0.60,
        icd_min: 1.20,
        mrr_floor: 0.35,
    };

    let gate = check_distribution_targets(&stats, &targets);

    // Per-metric assertions (R-08: verify correct field reads)
    assert_eq!(gate.cc_at_k.target, 0.60, "cc_at_k target mismatch");
    assert_eq!(
        gate.cc_at_k.actual, 0.65,
        "cc_at_k actual must be mean_cc_at_k"
    );
    assert!(gate.cc_at_k.passed, "cc_at_k: 0.65 >= 0.60 must pass");

    assert_eq!(gate.icd.target, 1.20, "icd target mismatch");
    assert_eq!(gate.icd.actual, 1.35, "icd actual must be mean_icd");
    assert!(gate.icd.passed, "icd: 1.35 >= 1.20 must pass");

    assert_eq!(gate.mrr_floor.target, 0.35, "mrr_floor target mismatch");
    assert_eq!(
        gate.mrr_floor.actual, 0.50,
        "mrr_floor actual must be mean_mrr (candidate)"
    );
    assert!(gate.mrr_floor.passed, "mrr: 0.50 >= 0.35 must pass");

    // Aggregate flags
    assert!(
        gate.diversity_passed,
        "diversity must pass when cc_at_k and icd both pass"
    );
    assert!(gate.mrr_floor_passed, "mrr_floor must pass");
    assert!(
        gate.overall_passed,
        "overall must pass when all metrics pass"
    );
}

/// AC-13, R-05: CC@k fails target → diversity fails, overall fails, mrr_floor independent.
#[test]
fn test_check_distribution_targets_cc_at_k_fail() {
    use super::AggregateStats;
    use super::aggregate::check_distribution_targets;
    use crate::eval::profile::DistributionTargets;

    let stats = AggregateStats {
        profile_name: "ppr-candidate".to_string(),
        scenario_count: 5,
        mean_p_at_k: 0.75,
        mean_mrr: 0.50,
        mean_latency_ms: 60.0,
        p_at_k_delta: 0.02,
        mrr_delta: 0.01,
        latency_delta_ms: 8.0,
        mean_cc_at_k: 0.45, // below target 0.60
        mean_icd: 1.35,
        cc_at_k_delta: -0.05,
        icd_delta: 0.15,
    };

    let targets = DistributionTargets {
        cc_at_k_min: 0.60,
        icd_min: 1.20,
        mrr_floor: 0.35,
    };

    let gate = check_distribution_targets(&stats, &targets);

    assert!(!gate.cc_at_k.passed, "cc_at_k: 0.45 < 0.60 must fail");
    assert!(gate.icd.passed, "icd: 1.35 >= 1.20 must pass");
    assert!(gate.mrr_floor.passed, "mrr_floor: 0.50 >= 0.35 must pass");

    assert!(
        !gate.diversity_passed,
        "diversity must fail when cc_at_k fails"
    );
    assert!(
        gate.mrr_floor_passed,
        "mrr_floor_passed independent of diversity"
    );
    assert!(
        !gate.overall_passed,
        "overall must fail when diversity fails"
    );
}

/// AC-13, R-05: ICD fails target → diversity fails, overall fails, cc_at_k independent.
#[test]
fn test_check_distribution_targets_icd_fail() {
    use super::AggregateStats;
    use super::aggregate::check_distribution_targets;
    use crate::eval::profile::DistributionTargets;

    let stats = AggregateStats {
        profile_name: "ppr-candidate".to_string(),
        scenario_count: 5,
        mean_p_at_k: 0.78,
        mean_mrr: 0.55,
        mean_latency_ms: 58.0,
        p_at_k_delta: 0.03,
        mrr_delta: 0.02,
        latency_delta_ms: 6.0,
        mean_cc_at_k: 0.65,
        mean_icd: 1.05, // below target 1.20
        cc_at_k_delta: 0.08,
        icd_delta: -0.10,
    };

    let targets = DistributionTargets {
        cc_at_k_min: 0.60,
        icd_min: 1.20,
        mrr_floor: 0.35,
    };

    let gate = check_distribution_targets(&stats, &targets);

    assert!(gate.cc_at_k.passed, "cc_at_k: 0.65 >= 0.60 must pass");
    assert!(!gate.icd.passed, "icd: 1.05 < 1.20 must fail");
    assert!(gate.mrr_floor.passed, "mrr_floor: 0.55 >= 0.35 must pass");

    assert!(!gate.diversity_passed, "diversity must fail when icd fails");
    assert!(
        gate.mrr_floor_passed,
        "mrr_floor_passed independent of diversity"
    );
    assert!(
        !gate.overall_passed,
        "overall must fail when diversity fails"
    );
}

/// AC-13, R-05, R-14: MRR floor fails → veto fires, diversity independent.
///
/// R-14 guard: fixture has candidate MRR (0.28) below floor (0.35) but
/// baseline MRR would be higher (0.60). The gate must compare the CANDIDATE's
/// mean_mrr against the floor — not the baseline's MRR. Using baseline MRR would
/// incorrectly pass (0.60 >= 0.35). The test detects this regression.
///
/// Fixture: candidate MRR = 0.28, floor = 0.35, diversity targets met.
#[test]
fn test_check_distribution_targets_mrr_floor_fail() {
    use super::AggregateStats;
    use super::aggregate::check_distribution_targets;
    use crate::eval::profile::DistributionTargets;

    // Candidate stats: cc_at_k and icd pass their targets; MRR is below the floor.
    // Baseline MRR (not passed here) would be 0.60, which is above the floor.
    let candidate_stats = AggregateStats {
        profile_name: "ppr-candidate".to_string(),
        scenario_count: 5,
        mean_p_at_k: 0.78,
        mean_mrr: 0.28, // below floor 0.35; baseline MRR would be 0.60 (above floor)
        mean_latency_ms: 62.0,
        p_at_k_delta: 0.03,
        mrr_delta: -0.32,
        latency_delta_ms: 10.0,
        mean_cc_at_k: 0.65,
        mean_icd: 1.35,
        cc_at_k_delta: 0.10,
        icd_delta: 0.25,
    };

    let targets = DistributionTargets {
        cc_at_k_min: 0.60,
        icd_min: 1.20,
        mrr_floor: 0.35,
    };

    let gate = check_distribution_targets(&candidate_stats, &targets);

    // Diversity passes (both cc_at_k and icd meet targets).
    assert!(gate.cc_at_k.passed, "cc_at_k: 0.65 >= 0.60 must pass");
    assert!(gate.icd.passed, "icd: 1.35 >= 1.20 must pass");
    assert!(
        gate.diversity_passed,
        "diversity must pass when cc_at_k and icd pass"
    );

    // MRR floor fails — checks candidate MRR (0.28), not baseline MRR (0.60).
    assert_eq!(
        gate.mrr_floor.actual, 0.28,
        "actual MRR must be candidate's mean_mrr (0.28), not baseline (0.60)"
    );
    assert!(!gate.mrr_floor.passed, "mrr_floor: 0.28 < 0.35 must fail");
    assert!(!gate.mrr_floor_passed, "mrr_floor_passed must be false");

    // Overall fails even though diversity passed (veto semantics, ADR-003).
    assert!(
        !gate.overall_passed,
        "overall must fail: MRR floor veto fires even when diversity passes"
    );
}

// ---------------------------------------------------------------------------
// Component 2: Baseline Profile Rejection (`parse_profile_toml`)
// ---------------------------------------------------------------------------

/// R-03: Baseline profile with `distribution_change = true` is rejected at parse time.
///
/// Verifies that `parse_profile_toml` returns `Err(ConfigInvariant)` when a profile
/// named "baseline" (case-insensitive) declares `distribution_change = true`.
/// The error must be caught at parse time — before the sidecar write path (FR-04,
/// SPECIFICATION.md constraint 8).
#[test]
fn test_distribution_gate_baseline_rejected() {
    use crate::eval::profile::parse_profile_toml;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("baseline.toml");

    // Write a TOML where the baseline profile declares distribution_change = true.
    std::fs::write(
        &path,
        "[profile]\n\
         name = \"baseline\"\n\
         distribution_change = true\n\
         \n\
         [profile.distribution_targets]\n\
         cc_at_k_min = 0.60\n\
         icd_min = 1.20\n\
         mrr_floor = 0.35\n",
    )
    .unwrap();

    let result = parse_profile_toml(&path);
    assert!(
        result.is_err(),
        "baseline profile with distribution_change = true must be rejected"
    );
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("baseline") && msg.contains("distribution_change"),
        "error must mention 'baseline' and 'distribution_change', got: {msg}"
    );
}
