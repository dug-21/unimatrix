//! Unit tests for [`resolve_slug_config`] (vnc-040 C6, ADR-001/002/003).
//!
//! Owns: R-01 (AC-08b post-merge re-validation, Critical), R-03 (no-file arm half),
//! R-10 (DoS / permission hardening), R-11 (slug-named, startup-fatal). File-present
//! tests write a temp `{base_dir}/{slug}/config.toml`; the helper REUSES the real
//! `load_single_config` / `validate_config` / `merge_configs`, so the 64 KiB cap (#2395)
//! and the `#[cfg(unix)]` `mode()&0o022` check are exercised, not stubbed.
//!
//! The `__<invariant>` / `__<class>` test-name suffixes are the test-plan-mandated
//! naming convention (test-plan/resolve_slug_config.md); `non_snake_case` is allowed
//! here so those names survive verbatim.
#![allow(non_snake_case)]

use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};

use unimatrix_server::error::ServerError;
use unimatrix_server::http::ProjectSlug;
use unimatrix_server::infra::config::{UnimatrixConfig, validate_config};

use super::resolve_slug_config;

// --- Harness ------------------------------------------------------------------

/// A unique temp `base_dir` under the OS temp dir, cleaned on drop. No external crate.
struct TempBase {
    dir: PathBuf,
}

impl TempBase {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "vnc040-resolve-{}-{}-{}",
            std::process::id(),
            n,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("create temp base dir");
        Self { dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }

    /// Create `{base}/{slug}/` and return the slug's config.toml path (file not written).
    fn slug_config_path(&self, slug: &str) -> PathBuf {
        let slug_dir = self.dir.join(slug);
        fs::create_dir_all(&slug_dir).expect("create slug dir");
        slug_dir.join("config.toml")
    }

    /// Write `contents` to `{base}/{slug}/config.toml`.
    fn write_slug_config(&self, slug: &str, contents: &str) -> PathBuf {
        let p = self.slug_config_path(slug);
        fs::write(&p, contents).expect("write slug config");
        p
    }
}

impl Drop for TempBase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn slug(s: &str) -> ProjectSlug {
    ProjectSlug::try_from(s).expect("valid test slug")
}

/// Parse a config from TOML text the same way `load_single_config` does (for building a
/// `global` that mirrors what a file would deserialize to, without filesystem hardening).
fn config_from_toml(text: &str) -> UnimatrixConfig {
    toml::from_str(text).expect("parse global config TOML")
}

// --- AC-08b / R-01 — Post-merge cross-field re-validation (CRITICAL) ----------

/// Canonical merged-only violation: the `InferenceConfig` sum-of-six fusion-weight
/// constraint (`w_sim + w_nli + w_conf + w_coac + w_util + w_prov <= 1.0`, config.rs).
///
/// Construction (defaults: w_sim=0.50, w_conf=0.35, others 0.0; baseline sum 0.85):
/// - GLOBAL sets `w_conf = 0.45` (and w_sim stays default 0.50) → global sum 0.95. Valid alone.
/// - PER-SLUG sets `w_sim = 0.10` (lowers it, valid alone) AND `w_coac = 0.50` →
///   per-slug-file sum = 0.10 + 0.35 + 0.50 = 0.95. Valid alone.
/// - MERGE (per-field replace: project-if-non-default else global): w_sim←0.10 (project),
///   w_conf←0.45 (global, project left default), w_coac←0.50 (project) →
///   merged sum = 0.10 + 0.45 + 0.50 = 1.05 > 1.0. INVALID merged.
fn global_w_conf_045() -> UnimatrixConfig {
    config_from_toml("[inference]\nw_conf = 0.45\n")
}

const SLUG_FILE_LOWERS_WSIM_RAISES_WCOAC: &str = "[inference]\nw_sim = 0.10\nw_coac = 0.50\n";

#[test]
fn test_resolve_merged_violation_fails_loud_naming_slug__fusion_weight_sum() {
    let base = TempBase::new();
    let global = global_w_conf_045();
    base.write_slug_config("alpha", SLUG_FILE_LOWERS_WSIM_RAISES_WCOAC);

    let result = resolve_slug_config(base.path(), &slug("alpha"), &global);

    match result {
        Err(ServerError::Config(msg)) => {
            assert!(
                msg.contains("alpha"),
                "error must name the offending slug; got: {msg}"
            );
            assert!(
                msg.contains("config.toml"),
                "error must name the offending slug file; got: {msg}"
            );
        }
        other => panic!("expected ServerError::Config naming the slug, got {other:?}"),
    }
}

/// The load-bearing negative (#3905): each file passes `validate_config` ALONE, proving
/// per-file validation is necessary-but-insufficient — only the post-merge call catches it.
#[test]
fn test_per_file_validation_alone_does_not_catch_merged_violation__fusion_weight_sum() {
    let base = TempBase::new();
    let global = global_w_conf_045();
    let slug_path = base.write_slug_config("alpha", SLUG_FILE_LOWERS_WSIM_RAISES_WCOAC);
    let slug_file = config_from_toml(SLUG_FILE_LOWERS_WSIM_RAISES_WCOAC);

    // Each file individually valid.
    assert!(
        validate_config(&global, Path::new("global.toml")).is_ok(),
        "global must pass validate_config alone"
    );
    assert!(
        validate_config(&slug_file, &slug_path).is_ok(),
        "per-slug file must pass validate_config alone (per-file insufficiency, #3905)"
    );
}

/// Construction proof: the helper runs `validate_config(&merged)` INSIDE itself, AFTER the
/// merge, BEFORE return. Observed via the merged-violation error firing from the helper
/// while both files are individually valid (asserted in the test above) — the only step
/// that can reject this input is the post-merge validation.
#[test]
fn test_resolve_runs_post_merge_validate_inside_helper_after_merge() {
    let base = TempBase::new();
    let global = global_w_conf_045();
    base.write_slug_config("alpha", SLUG_FILE_LOWERS_WSIM_RAISES_WCOAC);

    // Both files pass per-file validation (proved separately); therefore the error below
    // can ONLY originate from the post-merge validate_config inside resolve_slug_config.
    let err = resolve_slug_config(base.path(), &slug("alpha"), &global)
        .expect_err("merged config must be rejected post-merge");
    match err {
        ServerError::Config(msg) => assert!(msg.contains("alpha")),
        other => panic!("expected post-merge Config error, got {other:?}"),
    }
}

#[test]
fn test_resolve_valid_merge_passes_no_false_positive() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    // Per-slug lowers w_sim (0.50→0.10) AND raises w_coac (0.0→0.20). Merged (per-field
    // replace): w_sim=0.10, w_conf=0.35 (default), w_coac=0.20 → sum 0.65 <= 1.0. Valid.
    base.write_slug_config("beta", "[inference]\nw_sim = 0.10\nw_coac = 0.20\n");

    let resolved = resolve_slug_config(base.path(), &slug("beta"), &global)
        .expect("valid merge must not spuriously fail");

    assert!(
        matches!(resolved, Cow::Owned(_)),
        "file present ⇒ Cow::Owned"
    );
}

// --- AC-02 / R-03 — No-file fallthrough (helper half) -------------------------

#[test]
fn test_resolve_no_file_returns_cow_borrowed_global_no_merge() {
    let base = TempBase::new();
    // Make the slug dir but write NO config.toml.
    let _ = base.slug_config_path("gamma");
    // Give the global a distinctive value so we can prove identity, not a rebuilt clone.
    let global = config_from_toml("[server]\ninstructions = \"global-only\"\n");

    let resolved =
        resolve_slug_config(base.path(), &slug("gamma"), &global).expect("no-file path is Ok");

    // Borrowed arm — NO merge ran, NO re-derivation; the returned ref IS &global.
    match &resolved {
        Cow::Borrowed(r) => {
            assert!(
                std::ptr::eq(*r, &global),
                "no-file arm must return Cow::Borrowed(&global) — same address (no clone/merge)"
            );
        }
        Cow::Owned(_) => panic!("no file ⇒ must be Cow::Borrowed, not Owned (no merge allowed)"),
    }
    assert_eq!(
        resolved.server.instructions.as_deref(),
        Some("global-only"),
        "fallthrough value must equal the global byte-for-byte"
    );
}

#[test]
fn test_resolve_empty_file_merges_to_global_equivalent() {
    let base = TempBase::new();
    let global = config_from_toml("[server]\ninstructions = \"keep-me\"\n");
    // Present but empty / all-default file.
    base.write_slug_config("delta", "");

    let resolved = resolve_slug_config(base.path(), &slug("delta"), &global)
        .expect("empty file is valid and merges");

    // Degenerate fallthrough: this arm returns Owned, but the served value equals global.
    assert!(
        matches!(resolved, Cow::Owned(_)),
        "file present ⇒ Cow::Owned"
    );
    assert_eq!(
        resolved.server.instructions.as_deref(),
        Some("keep-me"),
        "empty per-slug file must not diverge from global in served values"
    );
}

// --- AC-03 — single overlayable key changes only that key ---------------------

#[test]
fn test_resolve_single_key_overlay_changes_only_that_key() {
    let base = TempBase::new();
    let global = config_from_toml("[inference]\nnli_top_k = 5\nw_conf = 0.20\n");
    // Per-slug overrides ONLY w_conf; nli_top_k must fall through from global.
    base.write_slug_config("epsilon", "[inference]\nw_conf = 0.30\n");

    let resolved = resolve_slug_config(base.path(), &slug("epsilon"), &global)
        .expect("single-key overlay valid");

    assert_eq!(resolved.inference.nli_top_k, 5, "sibling key falls through");
    assert!(
        (resolved.inference.w_conf - 0.30).abs() < f64::EPSILON,
        "overridden key takes the per-slug value"
    );
}

// --- AC-08a / R-10 — DoS + permission hardening on the per-slug path ----------

#[test]
fn test_resolve_rejects_oversized_file_before_parse() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    // > 64 KiB (CONFIG_MAX_BYTES = 65536). Valid-ish TOML prefix; size cap fires first.
    let mut big = String::from("[server]\ninstructions = \"");
    big.push_str(&"a".repeat(70_000));
    big.push_str("\"\n");
    base.write_slug_config("zeta", &big);

    let err = resolve_slug_config(base.path(), &slug("zeta"), &global)
        .expect_err("oversized per-slug file must be rejected at load (#2395)");
    match err {
        ServerError::Config(msg) => assert!(msg.contains("zeta"), "names slug; got {msg}"),
        other => panic!("expected Config error for oversized file, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn test_resolve_rejects_world_or_group_writable_file() {
    use std::os::unix::fs::PermissionsExt;

    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    let path = base.write_slug_config("eta", "[server]\ninstructions = \"ok\"\n");
    // mode() & 0o022 != 0 — group/world writable. The reused check_permissions rejects it.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).expect("chmod 0666");

    let err = resolve_slug_config(base.path(), &slug("eta"), &global)
        .expect_err("group/world-writable per-slug file must be rejected (R-10)");
    match err {
        ServerError::Config(msg) => assert!(msg.contains("eta"), "names slug; got {msg}"),
        other => panic!("expected Config error for writable file, got {other:?}"),
    }
}

// --- AC-08a / R-11 — Slug-named, startup-fatal error for every invalid class --

#[test]
fn test_resolve_invalid_class_fails_loud_naming_slug__malformed_toml() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    base.write_slug_config("theta", "this is = = not valid toml [[[");

    let err = resolve_slug_config(base.path(), &slug("theta"), &global)
        .expect_err("malformed TOML must fail loud");
    match err {
        ServerError::Config(msg) => {
            assert!(msg.contains("theta"), "names slug; got {msg}");
            assert!(msg.contains("config.toml"), "names file; got {msg}");
        }
        other => panic!("expected Config error for malformed TOML, got {other:?}"),
    }
}

#[test]
fn test_resolve_invalid_class_fails_loud_naming_slug__unknown_category() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    // boosted_categories referencing a category not in categories ⇒ per-file validate error.
    base.write_slug_config(
        "iota",
        "[knowledge]\ncategories = [\"decision\"]\nboosted_categories = [\"nonexistent\"]\n",
    );

    let err = resolve_slug_config(base.path(), &slug("iota"), &global)
        .expect_err("unknown category must fail per-file validation");
    match err {
        ServerError::Config(msg) => assert!(msg.contains("iota"), "names slug; got {msg}"),
        other => panic!("expected Config error for unknown category, got {other:?}"),
    }
}

#[test]
fn test_resolve_invalid_class_fails_loud_naming_slug__oversized_instructions() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    // Oversized [server] instructions (under the 64 KiB file cap, over the field cap).
    let oversized = format!("[server]\ninstructions = \"{}\"\n", "x".repeat(20_000));
    base.write_slug_config("kappa", &oversized);

    let err = resolve_slug_config(base.path(), &slug("kappa"), &global)
        .expect_err("oversized instructions must fail per-file validation");
    match err {
        ServerError::Config(msg) => assert!(msg.contains("kappa"), "names slug; got {msg}"),
        other => panic!("expected Config error for oversized instructions, got {other:?}"),
    }
}

// --- File-Present Order Proof -------------------------------------------------

/// On a valid file the helper executes the full ordered pipeline and returns
/// `Ok(Cow::Owned(merged))`; a per-slug override is reflected in the merged result.
#[test]
fn test_resolve_file_present_executes_full_order() {
    let base = TempBase::new();
    let global = config_from_toml("[server]\ninstructions = \"global\"\n");
    base.write_slug_config("lambda", "[server]\ninstructions = \"per-slug-wins\"\n");

    let resolved = resolve_slug_config(base.path(), &slug("lambda"), &global)
        .expect("valid file resolves through the full order");

    assert!(
        matches!(resolved, Cow::Owned(_)),
        "file present ⇒ Cow::Owned"
    );
    assert_eq!(
        resolved.server.instructions.as_deref(),
        Some("per-slug-wins"),
        "instructions merged project-wins (load→validate→merge→validate ran in order)"
    );
}
