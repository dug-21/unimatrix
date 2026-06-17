//! Unit tests for the `[[projects]]` config section + slug validation (vnc-034 Wave 2).
//!
//! Test plan: `product/features/vnc-034/wave2/test-plan/projects-config.md`.
//! Locked decisions: **D1** allowlist `^[a-z0-9][a-z0-9-]{0,62}$` (reused from the merged
//! Wave-1 `ProjectSlug::TryFrom` — NOT re-implemented), **D2** no config-overlay, **D5**
//! reserved-slug refusal (`v1`/`health`/`observe`/`tools`), SEPARATE from the D1 charset
//! check. Requirements: FR-C2, FR-C5, FR-C6; AC-W2-R2, AC-W2-R6; R-03.

use std::path::Path;

use crate::http::ProjectSlug;
use crate::http::router::parse_project_key as parse_key_for_test;
use crate::http::{ProjectKey, RouteError};

use super::{
    ConfigError, ProjectConfigEntry, RESERVED_SLUGS, UnimatrixConfig, is_reserved_slug,
    validate_projects_config,
};

/// A throwaway config-file path for error context in the validator (no I/O occurs —
/// `validate_projects_config` never touches the filesystem; the path is diagnostics only).
fn test_path() -> &'static Path {
    Path::new("/tmp/unimatrix-test/config.toml")
}

/// Build `[[projects]]` entries from raw slug strings (pre-validation).
fn entries(slugs: &[&str]) -> Vec<ProjectConfigEntry> {
    slugs
        .iter()
        .map(|s| ProjectConfigEntry {
            slug: (*s).to_owned(),
        })
        .collect()
}

// ===========================================================================
// A. `[[projects]]` config parse (FR-C2)
// ===========================================================================

#[test]
fn test_projects_config_parses_slug_list() {
    let toml_str = r#"
[[projects]]
slug = "alpha"

[[projects]]
slug = "beta"
"#;
    let config: UnimatrixConfig = toml::from_str(toml_str).expect("parse");
    assert_eq!(config.projects.len(), 2);
    assert_eq!(config.projects[0].slug, "alpha");
    assert_eq!(config.projects[1].slug, "beta");
}

#[test]
fn test_projects_config_entry_fields_validate_to_slug_and_derive_dir() {
    // Each entry carries at minimum a validated slug; the per-slug data dir is derived
    // by path-join from the VALIDATED slug only — never from raw input (AC-W2-R6).
    let validated =
        validate_projects_config(&entries(&["alpha"]), test_path()).expect("alpha valid");
    assert_eq!(validated.len(), 1);
    let slug = &validated[0];
    let root = Path::new("/data/.unimatrix");
    let dir = root.join(slug.as_str());
    assert_eq!(dir, Path::new("/data/.unimatrix/alpha"));
    assert!(
        dir.starts_with(root),
        "derived dir must stay under the root"
    );
}

#[test]
fn test_projects_config_duplicate_slug_rejected() {
    // Two identical slugs → loud ConfigError at load (NOT last-wins silence).
    let err = validate_projects_config(&entries(&["alpha", "alpha"]), test_path())
        .expect_err("duplicate must fail");
    match err {
        ConfigError::ProjectSlugDuplicate { value, .. } => assert_eq!(value, "alpha"),
        other => panic!("expected ProjectSlugDuplicate, got {other:?}"),
    }
}

// ===========================================================================
// B. Backward-compat: `[[projects]]` absent ⇒ Default (FR-C6, AC-W2-R2, R-13)
// ===========================================================================

#[test]
fn test_projects_absent_yields_empty_registry() {
    let toml_str = r#"
[http]
enabled = true
"#;
    let config: UnimatrixConfig = toml::from_str(toml_str).expect("parse");
    assert!(
        config.projects.is_empty(),
        "absent [[projects]] must default to empty, not error"
    );
    // And the empty list validates cleanly (Ok, empty Vec — not an error).
    let validated = validate_projects_config(&config.projects, test_path()).expect("empty ok");
    assert!(validated.is_empty());
}

#[test]
fn test_projects_absent_no_default_alias() {
    // vnc-038 ADR-004 (#5083): the `/v1/tools/... -> Default` alias is DELETED.
    // `tools` now parses as a slug *candidate*, never a default store. With no
    // `[[projects]]` declared it resolves to UnknownProject at the resolver
    // (`tools` is reserved/unregisterable), never a silent default (AC-09/R-10).
    let config = UnimatrixConfig::default();
    assert!(config.projects.is_empty());
    assert_eq!(
        parse_key_for_test("/v1/tools/call").expect("parse"),
        ProjectKey::Slug(ProjectSlug::try_from("tools").expect("valid charset")),
        "`/v1/tools/...` must parse `tools` as a slug candidate, NEVER a Default alias"
    );
}

// ===========================================================================
// C. Slug validation at config load (FR-C5, R-03)
// ===========================================================================

#[test]
fn test_config_slug_validation_uses_projectslug_newtype() {
    // An invalid slug ("My_Project": uppercase AND underscore) fails at load with a
    // ConfigError naming the offending slug — the SAME rejection the router gives.
    let err = validate_projects_config(&entries(&["My_Project"]), test_path())
        .expect_err("invalid slug must fail");
    match err {
        ConfigError::ProjectSlugInvalid { value, .. } => assert_eq!(value, "My_Project"),
        other => panic!("expected ProjectSlugInvalid, got {other:?}"),
    }
    // Proof the config path delegates to the SAME newtype, not a hand-rolled check:
    // the newtype also rejects it.
    assert!(ProjectSlug::try_from("My_Project").is_err());
}

#[test]
fn test_config_invalid_slug_message_names_canonical_d1_grammar() {
    // The Display string MUST contain the literal canonical D1 regex — the gate/PR review
    // asserts this exact value, so a drift to `[a-z0-9_-]{0,63}` cannot pass review.
    let err = validate_projects_config(&entries(&["BAD"]), test_path())
        .expect_err("invalid slug must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("^[a-z0-9][a-z0-9-]{0,62}$"),
        "message must name the canonical D1 grammar, got: {msg}"
    );
    assert!(
        msg.contains("no underscore"),
        "message must call out the no-underscore rule, got: {msg}"
    );
}

// ===========================================================================
// SECURITY corpus at config load (AC-W2-R6 / SR-09 / R-03) — config delegates to
// the seam newtype, so the FULL traversal/encoding corpus is rejected at config load.
// ===========================================================================

#[test]
fn test_config_rejects_full_security_corpus() {
    let over_length = "a".repeat(64);
    let corpus: Vec<&str> = vec![
        "../etc",     // T-SEC-01 path traversal
        "..",         // T-SEC-02 parent-dir token
        "a/../b",     // T-SEC-03 embedded traversal
        "%2e%2e%2f",  // T-SEC-04 encoded ../
        "a%2fb",      // T-SEC-05 encoded / mid-slug
        "%2e",        // T-SEC-06 encoded .
        "/abs/path",  // T-SEC-07 absolute path
        "a/b",        // T-SEC-08 bare separator
        "a\\b",       // T-SEC-09 backslash separator
        "Alpha",      // T-SEC-10 uppercase
        "",           // T-SEC-11 empty
        "-alpha",     // T-SEC-12 leading hyphen
        "al pha",     // T-SEC-13 whitespace
        "alpha.beta", // T-SEC-14 dot separator
        "my_project", // T-SEC-15 DISCRIMINATOR: underscore
        &over_length, // T-SEC-16 DISCRIMINATOR: 64-char
    ];
    for bad in corpus {
        let err = validate_projects_config(&entries(&[bad]), test_path()).pipe_err_for_test();
        match err {
            ConfigError::ProjectSlugInvalid { value, .. } => assert_eq!(value, bad),
            other => panic!("expected ProjectSlugInvalid for {bad:?}, got {other:?}"),
        }
    }
}

#[test]
fn test_config_accepts_canonical_valid_slugs() {
    let max63 = "a".repeat(63); // T-SEC-17 exact upper bound
    let valid: Vec<&str> = vec![
        "a",       // T-SEC-18 single char
        "alpha-1", // T-SEC-19 interior hyphen
        "a1-b2",   // T-SEC-19 alnum + hyphen
        "1alpha",  // T-SEC-20 leading digit
        &max63,
    ];
    for ok in valid {
        let validated = validate_projects_config(&entries(&[ok]), test_path())
            .unwrap_or_else(|_| panic!("config must accept {ok:?}"));
        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0].as_str(), ok);
    }
}

#[test]
fn test_no_accepted_slug_escapes_data_dir() {
    // For every accepted slug the derived path stays within the root — escape is
    // unrepresentable, not merely rejected (AC-W2-R6 closing clause).
    let root = Path::new("/data/.unimatrix");
    for ok in ["a", "alpha-1", "a1-b2", "1alpha", "project-1"] {
        let validated = validate_projects_config(&entries(&[ok]), test_path()).expect("valid slug");
        let joined = root.join(validated[0].as_str());
        assert!(joined.starts_with(root), "{ok} escaped the root");
        assert!(
            !validated[0].as_str().contains("..") && !validated[0].as_str().contains('/'),
            "{ok} contains a path component"
        );
    }
}

// ===========================================================================
// D. D2 — NO config-overlay surface introduced (locked out of scope)
// ===========================================================================

#[test]
fn test_no_per_project_config_overlay_merge() {
    // A `[[projects]]` entry exposes only the identity field `slug` — no `config`,
    // `overlay`, or `inherit` sub-table participates in merge. Structural: an entry
    // is `slug`-only; serde ignores unknown keys (no deny_unknown_fields), so an
    // overlay sub-table is silently dropped, never merged into a nested config.
    let toml_str = r#"
[[projects]]
slug = "alpha"
overlay = { preset = "authoritative" }
"#;
    let config: UnimatrixConfig = toml::from_str(toml_str).expect("parse");
    assert_eq!(config.projects.len(), 1);
    assert_eq!(config.projects[0].slug, "alpha");
    // The top-level preset is untouched — no overlay was applied.
    assert_eq!(config.profile.preset, super::Preset::Collaborative);
}

// ===========================================================================
// RESERVED-SLUG TABLE — D5 (route-grammar refusal; SEPARATE from charset)
// ===========================================================================

#[test]
fn test_reserved_slugs_rejected_at_config_load() {
    // T-RSV-01..04 — every reserved segment is charset-valid yet rejected as reserved.
    for reserved in ["tools", "v1", "health", "observe"] {
        let err = validate_projects_config(&entries(&[reserved]), test_path()).pipe_err_for_test();
        match err {
            ConfigError::ProjectSlugReserved { value, .. } => assert_eq!(value, reserved),
            other => panic!("expected ProjectSlugReserved for {reserved}, got {other:?}"),
        }
    }
}

#[test]
fn test_reserved_check_is_separate_from_charset() {
    // THE discriminator (T-RSV-01): `tools` PASSES the charset newtype yet is REJECTED
    // by the reserved guard. A charset-only impl would wrongly accept it.
    assert!(
        ProjectSlug::try_from("tools").is_ok(),
        "tools is charset-valid (lowercase alnum, 5 chars)"
    );
    let err = validate_projects_config(&entries(&["tools"]), test_path())
        .expect_err("tools must be rejected as reserved");
    assert!(
        matches!(err, ConfigError::ProjectSlugReserved { .. }),
        "tools must be ProjectSlugReserved, NOT ProjectSlugInvalid — the two checks are independent"
    );
}

#[test]
fn test_reserved_slug_message_is_accurate_to_new_grammar() {
    // vnc-038 ADR-005: the message must NO LONGER claim a default-project alias exists
    // (deleted by ADR-004). It must accurately describe the post-cutover route grammar.
    let err =
        validate_projects_config(&entries(&["tools"]), test_path()).expect_err("tools reserved");
    let msg = err.to_string();
    assert!(msg.contains("reserved"), "message must say reserved: {msg}");
    assert!(
        !msg.contains("default-project alias"),
        "message must NOT claim a default-project alias exists (deleted, ADR-004): {msg}"
    );
    assert!(
        msg.contains("/v1/{slug}/"),
        "message must name the post-cutover grammar: {msg}"
    );
}

#[test]
fn test_observe_is_reserved() {
    // R-08 sc.2 — `observe` is now the live per-slug sub-route segment
    // (`/v1/{slug}/observe`, ADR-003), so a slug `observe` must be unregisterable.
    let observe = ProjectSlug::try_from("observe").expect("observe is charset-valid");
    assert!(
        is_reserved_slug(&observe),
        "observe must be reserved so a slug cannot shadow /v1/{{slug}}/observe"
    );
    let err = validate_projects_config(&entries(&["observe"]), test_path())
        .expect_err("observe must be rejected as reserved");
    assert!(matches!(err, ConfigError::ProjectSlugReserved { .. }));
}

#[test]
fn test_reserved_set_covers_route_segments() {
    // R-08 sc.2 — bind the reserved set to the live grammar: every route segment of the
    // NEW grammar that could collide with a slug is in RESERVED_SLUGS. No segment is both
    // routable AND registerable.
    for segment in ["v1", "health", "observe"] {
        assert!(
            RESERVED_SLUGS.contains(&segment),
            "{segment} is a live route segment and MUST be reserved (cannot be registered)"
        );
    }
}

#[test]
fn test_tools_reservation_locked() {
    // R-08 sc.3 / OQ-3 — LOCK the chosen `tools`-reserved state. A silent flip
    // (un-reserving `tools` without intent) fails here. If the human un-reserves `tools`,
    // this test changes by one assertion — documented, not silent.
    let tools = ProjectSlug::try_from("tools").expect("tools is charset-valid");
    assert!(
        is_reserved_slug(&tools),
        "tools is CURRENTLY reserved (conservative, OQ-3). Flipping this is a deliberate \
         decision (un-reserve + test), not an incidental edit."
    );
    assert!(
        RESERVED_SLUGS.contains(&"tools"),
        "tools must remain in RESERVED_SLUGS until OQ-3 is resolved to un-reserve"
    );
}

#[test]
fn test_every_reserved_name_rejected() {
    // R-08 sc.1 — registration-rejection table: EVERY reserved name is rejected at the
    // parse edge. Mirrors the register-CLI guard (it imports RESERVED_SLUGS / is_reserved_slug).
    for reserved in RESERVED_SLUGS {
        let slug = ProjectSlug::try_from(reserved).expect("reserved names are charset-valid");
        assert!(
            is_reserved_slug(&slug),
            "{reserved} must be rejected by is_reserved_slug (register-edge guard)"
        );
        let err = validate_projects_config(&entries(&[reserved]), test_path())
            .expect_err("reserved name must be rejected at config load");
        assert!(
            matches!(err, ConfigError::ProjectSlugReserved { .. }),
            "{reserved} must reject as ProjectSlugReserved"
        );
    }
}

#[test]
fn test_reserved_set_exact_match_only() {
    // T-RSV-05 — only EXACT matches are reserved. Near-misses pass the reserved guard
    // (and the charset), so they validate cleanly. Guards against over-broad
    // starts_with/contains.
    for ok in [
        "toolsx",
        "v1-prod",
        "healthcheck",
        "observer",
        "v1x",
        "healthy",
    ] {
        let validated = validate_projects_config(&entries(&[ok]), test_path())
            .unwrap_or_else(|_| panic!("{ok} is not reserved and must validate"));
        assert_eq!(validated[0].as_str(), ok);
    }
}

#[test]
fn test_is_reserved_slug_helper_and_constant() {
    assert_eq!(RESERVED_SLUGS, ["v1", "health", "observe", "tools"]);
    for r in RESERVED_SLUGS {
        let slug = ProjectSlug::try_from(r).expect("reserved words are charset-valid");
        assert!(is_reserved_slug(&slug), "{r} must be reserved");
    }
    let ok = ProjectSlug::try_from("alpha").expect("valid");
    assert!(!is_reserved_slug(&ok));
}

// ===========================================================================
// Round-trip parity: a config-produced ProjectSlug equals a route-produced one
// (single grammar, no second validator).
// ===========================================================================

#[test]
fn test_config_slug_roundtrips_with_route_grammar() {
    let from_config =
        validate_projects_config(&entries(&["alpha"]), test_path()).expect("alpha valid");
    let from_route = match parse_key_for_test("/v1/alpha/tools").expect("parse") {
        ProjectKey::Slug(s) => s,
        other => panic!("expected Slug, got {other:?}"),
    };
    assert_eq!(
        from_config[0], from_route,
        "config and route grammar must yield the identical ProjectSlug"
    );
}

#[test]
fn test_config_and_route_reject_identical_corpus() {
    // Drive the reject corpus through BOTH the config-load path and the route grammar;
    // assert identical rejection (guards against a drifting second validator).
    for bad in ["../etc", "a%2fb", "Alpha", "my_project", "a.b", "-x"] {
        let config_rejected = validate_projects_config(&entries(&[bad]), test_path()).is_err();
        let route_rejected = matches!(
            parse_key_for_test(&format!("/v1/{bad}/tools")),
            Err(RouteError::InvalidSlug(_))
        );
        assert!(config_rejected, "config must reject {bad:?}");
        assert!(
            route_rejected,
            "route grammar must reject {bad:?} too (single grammar)"
        );
    }
}

/// Tiny test-only ergonomic: turn `Result<_, ConfigError>` from a known-error path into
/// the error, panicking with context if it was unexpectedly Ok. Keeps the corpus loops
/// terse without `.unwrap_err()` swallowing the value-vs-variant assertion.
trait PipeErrForTest {
    fn pipe_err_for_test(self) -> ConfigError;
}

impl PipeErrForTest for Result<Vec<ProjectSlug>, ConfigError> {
    fn pipe_err_for_test(self) -> ConfigError {
        self.expect_err("expected an error in this corpus row")
    }
}
