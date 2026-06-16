//! Unit tests for the `[transcript_signals]` config section (crt-054, Wave A).
//!
//! Test plan: `product/features/crt-054/test-plan/transcript-signals-config.md`.
//! Pseudocode: `product/features/crt-054/pseudocode/transcript-signals-config.md`.
//! Anchor ACs: AC-10 (default set), AC-11 (cap + invalid-regex loud), AC-15 (no
//! residue). Risks: R-10 (loud-at-load, no silent fallback), R-12 (no removed-scope
//! residue), R-05 (producer-contract index stability).

use std::path::Path;

use super::{
    ConfigError, MAX_SIGNAL_CLASSES, TranscriptSignal, TranscriptSignalsConfig, UnimatrixConfig,
};

fn p() -> &'static Path {
    Path::new("/test/config.toml")
}

/// Build a config with `n` enabled, uniquely-named, trivially-valid classes.
fn n_enabled_classes(n: usize) -> TranscriptSignalsConfig {
    TranscriptSignalsConfig {
        classes: (0..n)
            .map(|i| TranscriptSignal {
                class_name: format!("class_{i}"),
                pattern: "x".to_string(),
                enabled: true,
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Default catalog (AC-10, R-05)
// ---------------------------------------------------------------------------

#[test]
fn test_default_config_error_refusal_only() {
    // Plan #1: absent [transcript_signals] yields EXACTLY two v1 classes,
    // error at index 0, refusal at index 1, no third class.
    let cfg = TranscriptSignalsConfig::default();
    assert_eq!(
        cfg.classes.len(),
        2,
        "v1 default catalog must ship exactly two classes"
    );
    assert_eq!(cfg.classes[0].class_name, "error");
    assert_eq!(cfg.classes[1].class_name, "refusal");
    assert!(cfg.classes[0].enabled);
    assert!(cfg.classes[1].enabled);
}

#[test]
fn test_default_config_via_serde_absent_section() {
    // The #[serde(default)] path: an empty top-level config (no
    // [transcript_signals] table) must resolve to the v1 default catalog.
    let cfg: UnimatrixConfig = toml::from_str("").expect("empty config parses");
    let sig = &cfg.transcript_signals;
    assert_eq!(sig.classes.len(), 2);
    assert_eq!(sig.classes[0].class_name, "error");
    assert_eq!(sig.classes[1].class_name, "refusal");
}

#[test]
fn test_default_catalog_no_sdlc_literals() {
    // Plan #2: default patterns are domain-neutral behavioral signatures — assert
    // NO SDLC-specific literal patterns leaked into the shipped defaults.
    let cfg = TranscriptSignalsConfig::default();
    let blob = cfg
        .classes
        .iter()
        .map(|c| format!("{} {}", c.class_name, c.pattern))
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    for sdlc in [
        "compile",
        "test failed",
        "test fail",
        "cargo",
        "npm",
        "build failed",
        "stack trace",
        "panic",
        "exception",
        "segfault",
        "syntax error",
    ] {
        assert!(
            !blob.contains(sdlc),
            "default catalog must be domain-neutral; found SDLC literal {sdlc:?}"
        );
    }
}

#[test]
fn test_default_catalog_no_reread_or_compaction_class() {
    // Plan #3: removed-scope residue — NO reread class, NO compaction class.
    let cfg = TranscriptSignalsConfig::default();
    for c in &cfg.classes {
        assert_ne!(
            c.class_name, "reread",
            "reread class is removed scope (R-12)"
        );
        assert_ne!(
            c.class_name, "compaction",
            "compaction class is removed scope (R-12)"
        );
    }
}

#[test]
fn test_class_index_mapping_stable() {
    // Plan #4: class-to-index follows config order and is stable. enabled_patterns()
    // preserves the order crt-055 reads: [0] = error pattern, [1] = refusal pattern.
    let cfg = TranscriptSignalsConfig::default();
    let pats = cfg.enabled_patterns();
    assert_eq!(pats.len(), 2);
    assert_eq!(pats[0], cfg.classes[0].pattern);
    assert_eq!(pats[1], cfg.classes[1].pattern);
}

#[test]
fn test_default_catalog_validates() {
    // The shipped default catalog must itself pass validate() (its patterns must
    // compile and be unique) — a malformed default would break every fresh start.
    TranscriptSignalsConfig::default()
        .validate(p())
        .expect("v1 default catalog must validate");
}

// ---------------------------------------------------------------------------
// Calibrated default patterns — directional behavioral matching (AC-10a)
// ---------------------------------------------------------------------------

#[test]
fn test_default_error_pattern_matches_provider_errors() {
    // Anchored, bytes-domain. Matches provider error type tokens / overload /
    // status-in-error-context — NOT the bare word "error".
    let re = regex::bytes::Regex::new(&TranscriptSignalsConfig::default().classes[0].pattern)
        .expect("error pattern compiles");
    for s in [
        r#"{"type": "overloaded_error", "message": "Overloaded"}"#,
        r#"{"type":"rate_limit_error"}"#,
        r#"{"type": "api_error"}"#,
        "the request hit a rate limit and was rejected",
        "HTTP 529 overloaded",
        "503 service unavailable",
    ] {
        assert!(
            re.is_match(s.as_bytes()),
            "error pattern should match: {s:?}"
        );
    }
}

#[test]
fn test_default_error_pattern_low_false_positive() {
    // Conservative: the bare word "error" in ordinary prose must NOT match.
    let re = regex::bytes::Regex::new(&TranscriptSignalsConfig::default().classes[0].pattern)
        .expect("error pattern compiles");
    for s in [
        "There was an error in my reasoning, let me reconsider.",
        "trial and error is a fine approach",
        "the error term in the equation",
    ] {
        assert!(
            !re.is_match(s.as_bytes()),
            "error pattern should NOT match ordinary prose: {s:?}"
        );
    }
}

#[test]
fn test_default_refusal_pattern_matches_refusals() {
    let re = regex::bytes::Regex::new(&TranscriptSignalsConfig::default().classes[1].pattern)
        .expect("refusal pattern compiles");
    for s in [
        "I cannot help with that request.",
        "I can't assist with that.",
        "I won't be able to do that.",
        "I am unable to provide that information.",
        "I'm not able to comply with this.",
        "I will not continue with that.",
    ] {
        assert!(
            re.is_match(s.as_bytes()),
            "refusal pattern should match: {s:?}"
        );
    }
}

#[test]
fn test_default_refusal_pattern_low_false_positive() {
    let re = regex::bytes::Regex::new(&TranscriptSignalsConfig::default().classes[1].pattern)
        .expect("refusal pattern compiles");
    for s in [
        "I can do that for you right away.",
        "I will help you with the auth flow.",
        "The user cannot find the file.",
        "Cannot reproduce the issue.",
    ] {
        assert!(
            !re.is_match(s.as_bytes()),
            "refusal pattern should NOT match: {s:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Bound + loud rejection (AC-11, R-10)
// ---------------------------------------------------------------------------

#[test]
fn test_max_signal_classes_is_16() {
    // Plan #5: pinned at exactly 16 (must equal crt-055's, referenced not redefined).
    assert_eq!(MAX_SIGNAL_CLASSES, 16);
}

#[test]
fn test_config_over_cap_rejected() {
    // Plan #6: > MAX_SIGNAL_CLASSES enabled classes fails validate() — NO silent
    // truncation. (Negative-mutation guard: a validate() that truncated to 16
    // instead of erroring would fail this assertion.)
    let cfg = n_enabled_classes(MAX_SIGNAL_CLASSES + 1);
    match cfg.validate(p()) {
        Err(ConfigError::TooManySignalClasses { found, max, .. }) => {
            assert_eq!(found, MAX_SIGNAL_CLASSES + 1);
            assert_eq!(max, MAX_SIGNAL_CLASSES);
            // The set is NOT mutated by validate() — no silent truncation.
            assert_eq!(cfg.classes.len(), MAX_SIGNAL_CLASSES + 1);
        }
        other => panic!("expected TooManySignalClasses, got {other:?}"),
    }
}

#[test]
fn test_config_exactly_at_cap_accepted() {
    // Boundary: exactly MAX_SIGNAL_CLASSES enabled classes is valid.
    n_enabled_classes(MAX_SIGNAL_CLASSES)
        .validate(p())
        .expect("exactly MAX_SIGNAL_CLASSES enabled classes must validate");
}

#[test]
fn test_disabled_classes_excluded_from_cap() {
    // A class with enabled:false is excluded from the count: 20 total, only 16
    // enabled, must validate.
    let mut cfg = n_enabled_classes(MAX_SIGNAL_CLASSES);
    for i in 0..4 {
        cfg.classes.push(TranscriptSignal {
            class_name: format!("disabled_{i}"),
            pattern: "(".to_string(), // deliberately unparseable — must be ignored
            enabled: false,
        });
    }
    cfg.validate(p())
        .expect("disabled classes are excluded from cap, dup, and regex checks");
}

#[test]
fn test_config_invalid_regex_rejected() {
    // Plan #7: an unparseable pattern fails validate() loudly — no fallback.
    let cfg = TranscriptSignalsConfig {
        classes: vec![TranscriptSignal {
            class_name: "broken".to_string(),
            pattern: "(unterminated".to_string(),
            enabled: true,
        }],
    };
    match cfg.validate(p()) {
        Err(ConfigError::InvalidSignalRegex { name, .. }) => {
            assert_eq!(name, "broken");
        }
        other => panic!("expected InvalidSignalRegex, got {other:?}"),
    }
}

#[test]
fn test_config_duplicate_class_name_rejected() {
    // Plan #8: duplicate class_name among enabled fails validate() loudly.
    let cfg = TranscriptSignalsConfig {
        classes: vec![
            TranscriptSignal {
                class_name: "dup".to_string(),
                pattern: "a".to_string(),
                enabled: true,
            },
            TranscriptSignal {
                class_name: "dup".to_string(),
                pattern: "b".to_string(),
                enabled: true,
            },
        ],
    };
    match cfg.validate(p()) {
        Err(ConfigError::DuplicateSignalClassName { name, .. }) => assert_eq!(name, "dup"),
        other => panic!("expected DuplicateSignalClassName, got {other:?}"),
    }
}

#[test]
fn test_duplicate_class_name_allowed_when_other_disabled() {
    // The duplicate check is over ENABLED classes only — a disabled namesake is fine.
    let cfg = TranscriptSignalsConfig {
        classes: vec![
            TranscriptSignal {
                class_name: "shared".to_string(),
                pattern: "a".to_string(),
                enabled: true,
            },
            TranscriptSignal {
                class_name: "shared".to_string(),
                pattern: "b".to_string(),
                enabled: false,
            },
        ],
    };
    cfg.validate(p())
        .expect("a disabled namesake does not collide with an enabled class");
}

// ---------------------------------------------------------------------------
// enabled_patterns() — order + filtering (FR-C4)
// ---------------------------------------------------------------------------

#[test]
fn test_enabled_patterns_preserve_config_order() {
    let cfg = TranscriptSignalsConfig {
        classes: vec![
            TranscriptSignal {
                class_name: "first".to_string(),
                pattern: "p0".to_string(),
                enabled: true,
            },
            TranscriptSignal {
                class_name: "second".to_string(),
                pattern: "p1".to_string(),
                enabled: true,
            },
            TranscriptSignal {
                class_name: "third".to_string(),
                pattern: "p2".to_string(),
                enabled: true,
            },
        ],
    };
    assert_eq!(cfg.enabled_patterns(), vec!["p0", "p1", "p2"]);
}

#[test]
fn test_enabled_patterns_excludes_disabled() {
    let cfg = TranscriptSignalsConfig {
        classes: vec![
            TranscriptSignal {
                class_name: "on".to_string(),
                pattern: "keep".to_string(),
                enabled: true,
            },
            TranscriptSignal {
                class_name: "off".to_string(),
                pattern: "drop".to_string(),
                enabled: false,
            },
            TranscriptSignal {
                class_name: "on2".to_string(),
                pattern: "keep2".to_string(),
                enabled: true,
            },
        ],
    };
    // Order preserved across the gap; disabled entry excluded.
    assert_eq!(cfg.enabled_patterns(), vec!["keep", "keep2"]);
}

#[test]
fn test_empty_classes_yields_empty_patterns() {
    // classes = [] is a legitimate config (empty scanner; bytes/deltas still fold).
    let cfg = TranscriptSignalsConfig { classes: vec![] };
    assert!(cfg.enabled_patterns().is_empty());
    cfg.validate(p()).expect("an empty catalog is valid");
}

// ---------------------------------------------------------------------------
// serde partial-stanza tolerance
// ---------------------------------------------------------------------------

#[test]
fn test_partial_stanza_defaults_enabled_true() {
    // A stanza with only class_name + pattern is enabled by default (FR-C2).
    let toml_src = r#"
[transcript_signals]
[[transcript_signals.classes]]
class_name = "custom"
pattern = "foo"
"#;
    let cfg: UnimatrixConfig = toml::from_str(toml_src).expect("partial stanza parses");
    let classes = &cfg.transcript_signals.classes;
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].class_name, "custom");
    assert!(
        classes[0].enabled,
        "a configured class is on unless disabled"
    );
}
