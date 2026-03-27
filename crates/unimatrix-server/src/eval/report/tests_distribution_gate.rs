//! Tests for the Distribution Gate feature (nan-010).
//!
//! Covers: Component 3 (Runner Profile Meta Sidecar) write/read round-trip,
//! schema validation, and atomic write verification.
//!
//! Additional tests for Components 5, 6, and 7 (aggregation, rendering,
//! sidecar load) will be added in subsequent implementation steps.

use tempfile::TempDir;

use crate::eval::profile::{DistributionTargets, EvalProfile};
use crate::eval::runner::profile_meta::{write_profile_meta, ProfileMetaFile};
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
