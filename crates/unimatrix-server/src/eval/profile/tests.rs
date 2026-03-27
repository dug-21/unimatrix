//! Tests for the eval profile module (nan-007).

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use crate::infra::config::UnimatrixConfig;

    use super::super::error::EvalError;
    use super::super::types::{AnalyticsMode, EvalProfile};
    use super::super::validation::validate_confidence_weights;
    use crate::eval::profile::parse_profile_toml;

    // -----------------------------------------------------------------------
    // AnalyticsMode tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_analytics_mode_variants_debug() {
        assert_eq!(format!("{:?}", AnalyticsMode::Live), "Live");
        assert_eq!(format!("{:?}", AnalyticsMode::Suppressed), "Suppressed");
    }

    #[test]
    fn test_analytics_mode_eq() {
        assert_eq!(AnalyticsMode::Suppressed, AnalyticsMode::Suppressed);
        assert_ne!(AnalyticsMode::Live, AnalyticsMode::Suppressed);
    }

    // -----------------------------------------------------------------------
    // EvalError display tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_eval_error_display_model_not_found() {
        let err = EvalError::ModelNotFound(PathBuf::from("/nonexistent/model.onnx"));
        let msg = format!("{err}");
        assert!(msg.contains("model not found"), "got: {msg}");
        assert!(msg.contains("/nonexistent/model.onnx"), "got: {msg}");
    }

    #[test]
    fn test_eval_error_display_config_invariant() {
        let err = EvalError::ConfigInvariant("weights sum to 0.91".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("0.91"), "got: {msg}");
    }

    #[test]
    fn test_eval_error_display_live_db_path() {
        let err = EvalError::LiveDbPath {
            supplied: PathBuf::from("/tmp/snap.db"),
            active: PathBuf::from("/home/user/.unimatrix/abc/unimatrix.db"),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("resolves to the active database"),
            "got: {msg}"
        );
        assert!(msg.contains("snap.db"), "got: {msg}");
        assert!(msg.contains("unimatrix.db"), "got: {msg}");
    }

    #[test]
    fn test_eval_error_display_profile_name_collision() {
        let err = EvalError::ProfileNameCollision("baseline".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("duplicate profile name"), "got: {msg}");
        assert!(msg.contains("baseline"), "got: {msg}");
    }

    #[test]
    fn test_eval_error_display_invalid_k() {
        let err = EvalError::InvalidK(0);
        let msg = format!("{err}");
        assert!(msg.contains("--k must be >= 1"), "got: {msg}");
        assert!(msg.contains('0'), "got: {msg}");
    }

    #[test]
    fn test_eval_error_implements_std_error() {
        let err = EvalError::ConfigInvariant("test".to_string());
        let _boxed: Box<dyn std::error::Error> = Box::new(err);
    }

    // -----------------------------------------------------------------------
    // validate_confidence_weights unit tests (C-15, R-09)
    // -----------------------------------------------------------------------

    fn make_config_with_weights(
        base: f64,
        usage: f64,
        fresh: f64,
        help: f64,
        corr: f64,
        trust: f64,
    ) -> UnimatrixConfig {
        use crate::infra::config::{ConfidenceConfig, ConfidenceWeights};
        let mut cfg = UnimatrixConfig::default();
        cfg.confidence = ConfidenceConfig {
            weights: Some(ConfidenceWeights {
                base,
                usage,
                fresh,
                help,
                corr,
                trust,
            }),
        };
        cfg
    }

    #[test]
    fn test_confidence_weights_invariant_no_weights_passes() {
        let cfg = UnimatrixConfig::default();
        assert!(validate_confidence_weights(&cfg).is_ok());
    }

    #[test]
    fn test_confidence_weights_invariant_exact_sum_passes() {
        // 0.20 + 0.15 + 0.17 + 0.15 + 0.15 + 0.10 = 0.92
        let cfg = make_config_with_weights(0.20, 0.15, 0.17, 0.15, 0.15, 0.10);
        assert!(
            validate_confidence_weights(&cfg).is_ok(),
            "sum=0.92 must pass"
        );
    }

    #[test]
    fn test_confidence_weights_invariant_sum_low_fails() {
        let cfg = make_config_with_weights(0.15, 0.15, 0.15, 0.15, 0.15, 0.15);
        let result = validate_confidence_weights(&cfg);
        assert!(result.is_err(), "sum=0.90 must fail");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("0.92"),
            "must mention expected sum; got: {msg}"
        );
        assert!(
            msg.contains("0.90") || msg.contains("0.9"),
            "must mention actual sum; got: {msg}"
        );
    }

    #[test]
    fn test_confidence_weights_invariant_sum_high_fails() {
        // 0.20+0.15+0.18+0.15+0.15+0.10 = 0.93
        let cfg = make_config_with_weights(0.20, 0.15, 0.18, 0.15, 0.15, 0.10);
        let result = validate_confidence_weights(&cfg);
        assert!(result.is_err(), "sum=0.93 must fail");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("0.92"),
            "must mention expected sum; got: {msg}"
        );
    }

    #[test]
    fn test_confidence_weights_invariant_boundary_pass_within_tolerance() {
        // 0.92 + 5e-10 < 0.92 + 1e-9 → should pass
        let nudge = 5e-10_f64;
        let cfg = make_config_with_weights(0.20 + nudge, 0.15, 0.17, 0.15, 0.15, 0.10);
        assert!(
            validate_confidence_weights(&cfg).is_ok(),
            "sum within ±1e-9 must pass"
        );
    }

    #[test]
    fn test_confidence_weights_invariant_boundary_fail_outside_tolerance() {
        let nudge = 2e-9_f64;
        let cfg = make_config_with_weights(0.20 + nudge, 0.15, 0.17, 0.15, 0.15, 0.10);
        let result = validate_confidence_weights(&cfg);
        assert!(result.is_err(), "sum outside ±1e-9 must fail");
    }

    #[test]
    fn test_confidence_weights_invariant_message_names_fields() {
        let cfg = make_config_with_weights(0.10, 0.10, 0.10, 0.10, 0.10, 0.10);
        let result = validate_confidence_weights(&cfg);
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("base="), "must name base field; got: {msg}");
        assert!(msg.contains("usage="), "must name usage field; got: {msg}");
        assert!(msg.contains("fresh="), "must name fresh field; got: {msg}");
        assert!(msg.contains("help="), "must name help field; got: {msg}");
        assert!(msg.contains("corr="), "must name corr field; got: {msg}");
        assert!(msg.contains("trust="), "must name trust field; got: {msg}");
    }

    // -----------------------------------------------------------------------
    // parse_profile_toml tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_profile_toml_baseline_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("baseline.toml");
        std::fs::write(&path, "[profile]\nname = \"baseline\"\n").unwrap();

        let profile = parse_profile_toml(&path).expect("baseline parse must succeed");
        assert_eq!(profile.name, "baseline");
        assert!(profile.description.is_none());
        assert!(profile.config_overrides.confidence.weights.is_none());
    }

    #[test]
    fn test_parse_profile_toml_with_description() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("candidate.toml");
        std::fs::write(
            &path,
            "[profile]\nname = \"candidate-v1\"\ndescription = \"Test higher base weight\"\n",
        )
        .unwrap();

        let profile = parse_profile_toml(&path).expect("parse must succeed");
        assert_eq!(profile.name, "candidate-v1");
        assert_eq!(
            profile.description.as_deref(),
            Some("Test higher base weight")
        );
    }

    #[test]
    fn test_parse_profile_toml_missing_name_fails() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "[profile]\ndescription = \"no name\"\n").unwrap();

        let result = parse_profile_toml(&path);
        assert!(result.is_err(), "missing [profile].name must fail");
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("[profile].name is required"), "got: {msg}");
    }

    #[test]
    fn test_parse_profile_toml_missing_file_fails() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let result = parse_profile_toml(&path);
        assert!(result.is_err(), "missing file must fail");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("I/O error") || msg.to_lowercase().contains("error"),
            "got: {msg}"
        );
    }

    #[test]
    fn test_parse_profile_toml_invalid_toml_fails() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "this is not toml >>>").unwrap();
        let result = parse_profile_toml(&path);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("failed to parse") || msg.contains("parse"),
            "got: {msg}"
        );
    }

    #[test]
    fn test_parse_profile_toml_with_confidence_weights() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("weights.toml");
        std::fs::write(
            &path,
            "[profile]\nname = \"custom-weights\"\n\n[confidence.weights]\nbase  = 0.20\nusage = 0.15\nfresh = 0.17\nhelp  = 0.15\ncorr  = 0.15\ntrust = 0.10\n",
        )
        .unwrap();

        let profile = parse_profile_toml(&path).expect("parse must succeed");
        assert_eq!(profile.name, "custom-weights");
        let weights = profile
            .config_overrides
            .confidence
            .weights
            .expect("weights must be present");
        assert!((weights.base - 0.20).abs() < 1e-9);
        assert!((weights.usage - 0.15).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // Distribution change flag and targets parsing (nan-010, AC-01 – AC-04)
    // -----------------------------------------------------------------------

    /// AC-01, R-02: A valid distribution-change profile is parsed correctly.
    ///
    /// Verifies that `distribution_change = true` and all three targets are extracted
    /// from the [profile] section BEFORE the [profile] strip step. Confirms that
    /// `distribution_change`, `distribution_targets`, and target values round-trip correctly.
    #[test]
    fn test_parse_distribution_change_profile_valid() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ppr-candidate.toml");
        std::fs::write(
            &path,
            "[profile]\n\
             name = \"ppr-candidate\"\n\
             description = \"PPR distribution change candidate\"\n\
             distribution_change = true\n\
             \n\
             [profile.distribution_targets]\n\
             cc_at_k_min = 0.60\n\
             icd_min = 1.20\n\
             mrr_floor = 0.35\n",
        )
        .unwrap();

        let profile =
            parse_profile_toml(&path).expect("valid distribution-change profile must parse");

        assert_eq!(profile.name, "ppr-candidate");
        assert!(
            profile.distribution_change,
            "distribution_change must be true"
        );

        let targets = profile
            .distribution_targets
            .expect("distribution_targets must be Some when distribution_change = true");
        assert!(
            (targets.cc_at_k_min - 0.60).abs() < 1e-9,
            "cc_at_k_min must be 0.60, got {}",
            targets.cc_at_k_min
        );
        assert!(
            (targets.icd_min - 1.20).abs() < 1e-9,
            "icd_min must be 1.20, got {}",
            targets.icd_min
        );
        assert!(
            (targets.mrr_floor - 0.35).abs() < 1e-9,
            "mrr_floor must be 0.35, got {}",
            targets.mrr_floor
        );
    }

    /// AC-02, R-02: `distribution_change = true` with no `[profile.distribution_targets]` fails.
    ///
    /// Verifies that omitting the required targets sub-table returns
    /// `Err(ConfigInvariant)` with a message naming the missing section.
    #[test]
    fn test_parse_distribution_change_missing_targets() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad-candidate.toml");
        std::fs::write(
            &path,
            "[profile]\n\
             name = \"bad-candidate\"\n\
             distribution_change = true\n",
        )
        .unwrap();

        let result = parse_profile_toml(&path);
        assert!(
            result.is_err(),
            "distribution_change = true without targets must fail"
        );
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("distribution_targets"),
            "error must mention 'distribution_targets', got: {msg}"
        );
    }

    /// AC-03: Missing `cc_at_k_min` in `[profile.distribution_targets]` fails with field name.
    #[test]
    fn test_parse_distribution_change_missing_cc_at_k() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing-cc.toml");
        std::fs::write(
            &path,
            "[profile]\n\
             name = \"missing-cc\"\n\
             distribution_change = true\n\
             \n\
             [profile.distribution_targets]\n\
             icd_min = 1.20\n\
             mrr_floor = 0.35\n",
        )
        .unwrap();

        let result = parse_profile_toml(&path);
        assert!(result.is_err(), "missing cc_at_k_min must fail");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("cc_at_k_min"),
            "error must name the missing field 'cc_at_k_min', got: {msg}"
        );
    }

    /// AC-03: Missing `icd_min` in `[profile.distribution_targets]` fails with field name.
    #[test]
    fn test_parse_distribution_change_missing_icd() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing-icd.toml");
        std::fs::write(
            &path,
            "[profile]\n\
             name = \"missing-icd\"\n\
             distribution_change = true\n\
             \n\
             [profile.distribution_targets]\n\
             cc_at_k_min = 0.60\n\
             mrr_floor = 0.35\n",
        )
        .unwrap();

        let result = parse_profile_toml(&path);
        assert!(result.is_err(), "missing icd_min must fail");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("icd_min"),
            "error must name the missing field 'icd_min', got: {msg}"
        );
    }

    /// AC-03: Missing `mrr_floor` in `[profile.distribution_targets]` fails with field name.
    #[test]
    fn test_parse_distribution_change_missing_mrr_floor() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing-mrr.toml");
        std::fs::write(
            &path,
            "[profile]\n\
             name = \"missing-mrr\"\n\
             distribution_change = true\n\
             \n\
             [profile.distribution_targets]\n\
             cc_at_k_min = 0.60\n\
             icd_min = 1.20\n",
        )
        .unwrap();

        let result = parse_profile_toml(&path);
        assert!(result.is_err(), "missing mrr_floor must fail");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("mrr_floor"),
            "error must name the missing field 'mrr_floor', got: {msg}"
        );
    }

    /// AC-04: Absence of `distribution_change` flag → `distribution_change = false`, targets `None`.
    ///
    /// Verifies that a standard (non-distribution-change) profile omitting the flag
    /// is parsed with `distribution_change = false` and `distribution_targets = None`.
    /// This is the existing baseline behavior and must not change (backward compat).
    #[test]
    fn test_parse_no_distribution_change_flag() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("baseline.toml");
        std::fs::write(&path, "[profile]\nname = \"baseline\"\n").unwrap();

        let profile = parse_profile_toml(&path).expect("standard profile must parse");

        assert_eq!(profile.name, "baseline");
        assert!(
            !profile.distribution_change,
            "distribution_change must default to false when flag is absent"
        );
        assert!(
            profile.distribution_targets.is_none(),
            "distribution_targets must be None when distribution_change is false/absent"
        );
    }
}
