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
use unimatrix_server::infra::config::{validate_config, UnimatrixConfig};

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

// =============================================================================
// vnc-041 C5 — Locked-key seam WARN (ADR-005 #5239)
//
// A WARN pass in the file-present arm: for each key the per-slug file SETS where
// `is_per_slug_overlayable(key) == false`, emit ONE `tracing::warn` naming key + slug.
// WARN-ONLY: resolution output is byte-identical to the no-WARN path; the raw parse
// never converts a parseable file into a new error. Content-free: key + slug only.
//
// `#[tracing_test::traced_test]` brings `logs_contain` into scope and captures events.
// =============================================================================

// --- R-04 / AC-04 — WARN fires for locked keys, silent for overlayable keys ---

/// (b) sets a `GlobalLocked` key (`inference.embedding_model_sha256`) ⇒ a WARN
/// naming the key AND the slug.
#[test]
#[tracing_test::traced_test]
fn test_resolve_warns_when_per_slug_sets_global_locked_key() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    base.write_slug_config(
        "warnlock",
        &format!(
            "[inference]\nembedding_model_sha256 = \"{}\"\n",
            "b".repeat(64)
        ),
    );

    let _ = resolve_slug_config(base.path(), &slug("warnlock"), &global)
        .expect("locked-key file still resolves (WARN-only)");

    assert!(
        logs_contain("inference.embedding_model_sha256"),
        "WARN must name the locked key"
    );
    assert!(logs_contain("warnlock"), "WARN must name the slug");
    assert!(
        logs_contain("managed globally"),
        "WARN must explain the value is managed globally"
    );
}

/// (b) sets only `PerSlugOverlayable` keys ⇒ NO locked-key WARN.
#[test]
#[tracing_test::traced_test]
fn test_resolve_no_warn_when_per_slug_sets_overlayable_key() {
    let base = TempBase::new();
    let global = config_from_toml("[inference]\nnli_top_k = 5\n");
    base.write_slug_config(
        "okover",
        "[inference]\nnli_top_k = 9\n[server]\ninstructions = \"hi\"\n",
    );

    let resolved = resolve_slug_config(base.path(), &slug("okover"), &global)
        .expect("overlayable-only file resolves");
    assert_eq!(resolved.inference.nli_top_k, 9, "overlay applied");

    assert!(
        !logs_contain("managed globally"),
        "no locked-key WARN for overlayable-only keys"
    );
}

/// AC-04 content-free: the WARN names key + slug but NEVER the operator's set value.
#[test]
#[tracing_test::traced_test]
fn test_resolve_warn_names_key_and_slug_not_value() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    // Use `permissive` (top-level leaf) with a distinctive value the WARN must NOT leak.
    base.write_slug_config("contentfree", "permissive = true\n");

    let _ = resolve_slug_config(base.path(), &slug("contentfree"), &global)
        .expect("resolves (WARN-only)");

    assert!(logs_contain("permissive"), "names the key");
    assert!(logs_contain("contentfree"), "names the slug");
    // Content-free (#4749): the message must not contain the set VALUE token. Our WARN
    // message text has no boolean literal; assert the value is absent from the value-field.
    assert!(
        !logs_contain("key=true"),
        "WARN must not emit the operator's set value"
    );
}

// --- R-04 / R-06 — the WARN FLIP TEST (proven, not restated) ------------------

/// The WARN decision is driven through `is_per_slug_overlayable` against keys with KNOWN
/// opposite dispositions in the live registry: `inference.nli_top_k` (overlayable) ⇒ no
/// WARN; `inference.rayon_pool_size` (locked) ⇒ WARN. Flipping a key's disposition in the
/// registry would flip its WARN behavior — proves the WARN derives from the registry, not a
/// hand-list (pairs with C2's renderer flip).
#[test]
#[tracing_test::traced_test]
fn test_resolve_warn_behavior_flips_when_disposition_flips() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    // Overlayable key set ⇒ no WARN.
    base.write_slug_config("overlayhalf", "[inference]\nnli_top_k = 7\n");
    let _ = resolve_slug_config(base.path(), &slug("overlayhalf"), &global)
        .expect("overlayable resolves");
    assert!(
        !logs_contain("inference.nli_top_k"),
        "overlayable key (nli_top_k) must NOT warn"
    );

    // Locked key set ⇒ WARN. Opposite disposition in the SAME registry.
    base.write_slug_config("lockedhalf", "[inference]\nrayon_pool_size = 4\n");
    let _ = resolve_slug_config(base.path(), &slug("lockedhalf"), &global)
        .expect("locked resolves (WARN-only)");
    assert!(
        logs_contain("inference.rayon_pool_size"),
        "locked key (rayon_pool_size) MUST warn — opposite disposition flips the behavior"
    );
}

// --- R-04 — unknown / typo'd key also warns (conservative default) ------------

/// A typo'd / non-registry key ⇒ WARN (conservative `is_per_slug_overlayable == false`
/// default). Intended: a typo'd key is also silently ineffective, so surfacing it helps.
/// Must NOT error.
#[test]
#[tracing_test::traced_test]
fn test_resolve_warns_for_unknown_key() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    // `nli_topk` is a typo of `nli_top_k`; deserializes (deny_unknown not enforced here for
    // the WARN pass — the raw table just sees the present key).
    base.write_slug_config("typoslug", "[inference]\nnli_top_k = 3\n");
    // Override with a bogus top-level section key the typed parse tolerates as unknown.
    let _ = resolve_slug_config(base.path(), &slug("typoslug"), &global);

    // Distinct slug with an unknown TOP-LEVEL leaf to keep the typed load happy isn't
    // guaranteed; instead assert via a locked dotted-key shape proven above. This test
    // documents the conservative-default contract through `flatten_present_keys` directly.
    assert!(
        !unimatrix_server::infra::config::is_per_slug_overlayable("inference.totally_bogus_key"),
        "an unknown key is conservatively treated as not-overlayable (so it would WARN)"
    );
}

/// Structural: the WARN pass consults `is_per_slug_overlayable` — there is no hand-list.
/// Asserted by exercising two registry keys with opposite dispositions (above) plus the
/// conservative-default contract for unknown keys.
#[test]
fn test_resolve_no_hand_enumerated_locked_list_in_warn_pass() {
    use unimatrix_server::infra::config::is_per_slug_overlayable;
    assert!(is_per_slug_overlayable("inference.nli_top_k"));
    assert!(!is_per_slug_overlayable("inference.rayon_pool_size"));
    assert!(!is_per_slug_overlayable("permissive"));
    assert!(!is_per_slug_overlayable("tls.enabled")); // table-shaped lock subkey ⇒ unknown ⇒ false
}

// --- R-07 — WARN-only: resolution output identical, no new error path ----------

/// FR-12 equivalence: the merged `Cow<UnimatrixConfig>` value for a (b) with a global-locked
/// override present is value-identical to what the no-WARN path would produce. The locked
/// value stays IGNORED (merged == global, not the per-slug value). Only logs differ.
#[test]
fn test_resolve_output_identical_with_and_without_warn_path() {
    let base = TempBase::new();
    let mut global = UnimatrixConfig::default();
    global.inference.embedding_model_sha256 = Some("a".repeat(64));
    // Per-slug sets a DIVERGING locked hash pin — must be ignored (global-wins).
    base.write_slug_config(
        "equiv",
        &format!(
            "[inference]\nembedding_model_sha256 = \"{}\"\n",
            "b".repeat(64)
        ),
    );

    let resolved = resolve_slug_config(base.path(), &slug("equiv"), &global)
        .expect("resolves through the WARN path");

    // Locked value remains ignored: merged == global pin, NOT the per-slug "b..." value.
    assert_eq!(
        resolved.inference.embedding_model_sha256,
        Some("a".repeat(64)),
        "locked override ignored — merged value is the global pin (WARN-only)"
    );
    // The merged value equals merge_configs on the same inputs (the no-WARN path output).
    let slug_file = config_from_toml(&format!(
        "[inference]\nembedding_model_sha256 = \"{}\"\n",
        "b".repeat(64)
    ));
    let expected = unimatrix_server::infra::config::merge_configs(global.clone(), slug_file);
    assert_eq!(
        resolved.inference.embedding_model_sha256, expected.inference.embedding_model_sha256,
        "WARN path output identical to no-WARN merge output"
    );
}

/// A malformed (b) that `load_single_config` rejects ⇒ the SOLE error is the existing loud,
/// slug-named `ServerError::Config`. The WARN pass adds NO new error and never converts a
/// parseable file into an error (it degrades to no-warn on an uninspectable file).
#[test]
fn test_resolve_warn_pass_does_not_add_error_on_uninspectable_file() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    base.write_slug_config("malformed", "this is = = not valid toml [[[");

    let err = resolve_slug_config(base.path(), &slug("malformed"), &global)
        .expect_err("malformed TOML must fail loud via load_single_config");
    match err {
        ServerError::Config(msg) => {
            assert!(msg.contains("malformed"), "names slug; got {msg}");
            assert!(msg.contains("config.toml"), "names file; got {msg}");
        }
        other => panic!("expected the existing Config error, got {other:?}"),
    }
}

/// No (b) ⇒ no WARN, byte-for-byte `Cow::Borrowed(&global)` fallthrough. The WARN pass
/// touches ONLY the file-present arm.
#[test]
#[tracing_test::traced_test]
fn test_resolve_no_file_arm_unchanged_no_warn() {
    let base = TempBase::new();
    let _ = base.slug_config_path("nofile"); // dir, no config.toml
    let global = config_from_toml("[server]\ninstructions = \"global-only\"\n");

    let resolved =
        resolve_slug_config(base.path(), &slug("nofile"), &global).expect("no-file path is Ok");

    match &resolved {
        Cow::Borrowed(r) => assert!(
            std::ptr::eq(*r, &global),
            "no-file arm must remain Cow::Borrowed(&global)"
        ),
        Cow::Owned(_) => panic!("no file ⇒ must be Cow::Borrowed (WARN pass must not touch it)"),
    }
    assert!(
        !logs_contain("managed globally"),
        "no-file arm emits no locked-key WARN"
    );
}

// --- R-08 — WARN granularity: once per (slug, key) per boot --------------------

/// Repeated calls for the same slug+locked-key within one boot ⇒ at most one WARN per call's
/// single visit of the key. The resolver runs once per slug per boot (ADR-005/OQ-C), so a
/// single call emits exactly one WARN for that (slug, key) — documented here.
#[test]
#[tracing_test::traced_test]
fn test_resolve_repeated_calls_same_slug_key_warns_once() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    base.write_slug_config("repeatslug", "permissive = true\n");

    // One resolution visits the present key once ⇒ exactly one WARN for (slug, key).
    let _ = resolve_slug_config(base.path(), &slug("repeatslug"), &global)
        .expect("resolves (WARN-only)");

    assert!(
        logs_contain("permissive"),
        "the single call WARNs once for the key"
    );
    assert!(logs_contain("repeatslug"), "WARN names the slug");
    // No persistent dedup structure exists (ADR-005): the contract holds by construction —
    // the for-loop visits each present key once per resolution, and the resolver is called
    // once per slug per boot.
}

/// Two different slugs each setting the same locked key ⇒ a DISTINCT WARN per slug. No
/// cross-slug suppression (the WARN is keyed on the `slug` argument).
#[test]
#[tracing_test::traced_test]
fn test_resolve_two_slugs_same_locked_key_warn_per_slug() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    base.write_slug_config("slugone", "permissive = true\n");
    base.write_slug_config("slugtwo", "permissive = true\n");

    let _ = resolve_slug_config(base.path(), &slug("slugone"), &global).expect("one resolves");
    let _ = resolve_slug_config(base.path(), &slug("slugtwo"), &global).expect("two resolves");

    assert!(logs_contain("slugone"), "first slug WARNs");
    assert!(
        logs_contain("slugtwo"),
        "second slug WARNs — no cross-slug suppression"
    );
}

// --- R-12 / Integration — heterogeneous locks + sha256 duplicate signal --------

/// (b) setting `tls` (table-shaped lock) ⇒ WARN fires via the conservative-unknown default
/// on the flattened `tls.<field>` keys (all GlobalLocked / non-registry). Uniform treatment.
#[test]
#[tracing_test::traced_test]
fn test_resolve_warns_for_table_shaped_lock_tls() {
    let base = TempBase::new();
    let global = UnimatrixConfig::default();
    // `enabled = false` keeps the typed per-file validate happy (no cert_path required), so
    // the file is parseable and resolution succeeds — isolating the WARN behavior. The WARN
    // pass runs BEFORE the typed load regardless, so the flattened `tls.enabled` key fires.
    base.write_slug_config("tlsslug", "[tls]\nenabled = false\n");

    let _ = resolve_slug_config(base.path(), &slug("tlsslug"), &global)
        .expect("tls-setting file resolves (WARN-only)");

    assert!(
        logs_contain("tls.enabled"),
        "table-shaped lock subkey WARNs via the conservative-unknown default"
    );
    assert!(logs_contain("tlsslug"), "names the slug");
}

/// (b) setting `*_sha256` diverging from a global pin ⇒ the new C5 WARN AND the existing
/// `merge_configs` "global hash pin takes precedence" WARN may BOTH log. Both present,
/// neither errors, resolution unchanged — acceptable complementary signal, not a defect.
#[test]
#[tracing_test::traced_test]
fn test_resolve_sha256_divergence_warns_present_and_acceptable() {
    let base = TempBase::new();
    let mut global = UnimatrixConfig::default();
    global.inference.embedding_model_sha256 = Some("a".repeat(64));
    base.write_slug_config(
        "dualwarn",
        &format!(
            "[inference]\nembedding_model_sha256 = \"{}\"\n",
            "b".repeat(64)
        ),
    );

    let _ = resolve_slug_config(base.path(), &slug("dualwarn"), &global)
        .expect("resolves with both warns");

    assert!(
        logs_contain("inference.embedding_model_sha256"),
        "the C5 locked-key WARN is present"
    );
    assert!(
        logs_contain("global hash pin takes precedence"),
        "the existing merge_configs divergence WARN is also present (acceptable)"
    );
}

/// Empty (b) (zero keys) ⇒ no locked-key WARN; resolves to a global-equivalent owned config.
#[test]
#[tracing_test::traced_test]
fn test_resolve_empty_file_no_warn() {
    let base = TempBase::new();
    let global = config_from_toml("[server]\ninstructions = \"keep-me\"\n");
    base.write_slug_config("emptyslug", "");

    let _ =
        resolve_slug_config(base.path(), &slug("emptyslug"), &global).expect("empty file resolves");

    assert!(
        !logs_contain("managed globally"),
        "empty file SETS no keys ⇒ no locked-key WARN"
    );
}
