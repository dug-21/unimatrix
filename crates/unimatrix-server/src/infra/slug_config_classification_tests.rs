//! Drift-guard + exhaustiveness tests for the ADR-004 canonical per-slug-vs-global
//! classification (vnc-040, FR-16, R-14/AC-11).
//!
//! Test plan: `product/features/vnc-040/test-plan/slug_config_classification.md`.
//!
//! The registry (`PER_SLUG_CONFIG_CLASSIFICATION`) is the single source of truth for
//! the per-slug overlay seam. These tests PIN `merge_configs`' real overlay-vs-lock
//! behavior to the registry so the two can never silently diverge (crt-031
//! anti-divergence). They are DATA-ONLY: they do NOT change `merge_configs`' logic.
//!
//! ## Two GlobalLocked mechanisms (load-bearing distinction)
//!
//! `GlobalLocked` keys come in two flavors with DIFFERENT enforcement points:
//!
//! 1. **Merge-locked** (`*_sha256` hash pins): the lock lives INSIDE `merge_configs`
//!    via the global-wins carve-out (#4655/#4649). `merge_configs(global, slug)` yields
//!    the global value even when the per-slug value differs. The drift-guard asserts
//!    this directly on the merged config.
//! 2. **Construction-locked** (`inference.rayon_pool_size`, `tls`, `http`, `permissive`):
//!    `merge_configs` does NOT lock these — they would win project-wins in the merge.
//!    Their lock is held BY CONSTRUCTION in the per-slug loop (component 3,
//!    `per_slug_loop`): the loop NEVER sources the pool handle / transport / process
//!    posture from the merged config (FR-15, ADR-002 §1). They are therefore EXEMPT
//!    from the merge-level assertion and are listed in `CONSTRUCTION_LOCKED_KEYS`.
//!    `permissive` additionally has no `UnimatrixConfig` field at all (daemon flag).
//!
//! Splitting them here is what keeps the drift-guard HONEST: asserting "merged == global"
//! for `tls`/`http`/`rayon_pool_size` would be a false claim about `merge_configs`.

use super::{
    is_per_slug_overlayable, merge_configs, ConfidenceWeights, DomainPackConfig, KnowledgeConfig,
    OverlayDisposition, UnimatrixConfig, PER_SLUG_CONFIG_CLASSIFICATION,
};

/// GlobalLocked keys whose lock is enforced BY CONSTRUCTION in `per_slug_loop`, NOT by
/// `merge_configs`. The drift-guard skips the merge-level "global wins" assertion for
/// these — `merge_configs` legitimately lets the project value win for them; the seam
/// never reads them (FR-15, ADR-002 §1). See module docs.
const CONSTRUCTION_LOCKED_KEYS: &[&str] =
    &["inference.rayon_pool_size", "tls", "http", "permissive"];

fn is_construction_locked(key: &str) -> bool {
    CONSTRUCTION_LOCKED_KEYS.contains(&key)
}

/// Build a `(global, per_slug)` pair that differs ONLY on `key`. The global value is
/// `value_A`, the per-slug value is a DISTINCT `value_B`; every other field is the
/// compiled default. Returns `None` for keys with no `UnimatrixConfig` field
/// (`permissive` — the daemon process flag).
fn pair_differing_on(key: &str) -> Option<(UnimatrixConfig, UnimatrixConfig)> {
    let mut global = UnimatrixConfig::default();
    let mut slug = UnimatrixConfig::default();

    match key {
        "knowledge.categories" => {
            global.knowledge.categories = vec!["alpha".to_string()];
            slug.knowledge.categories = vec!["beta".to_string()];
        }
        "knowledge.boosted_categories" => {
            global.knowledge.boosted_categories = vec!["g-boost".to_string()];
            slug.knowledge.boosted_categories = vec!["s-boost".to_string()];
        }
        "knowledge.adaptive_categories" => {
            global.knowledge.adaptive_categories = vec!["g-adaptive".to_string()];
            slug.knowledge.adaptive_categories = vec!["s-adaptive".to_string()];
        }
        "confidence.weights" => {
            global.confidence.weights = Some(weights(0.40));
            slug.confidence.weights = Some(weights(0.42));
        }
        "observation.domain_packs" => {
            global.observation.domain_packs = vec![domain_pack("global-domain")];
            slug.observation.domain_packs = vec![domain_pack("slug-domain")];
        }
        "inference.nli_top_k" => {
            global.inference.nli_top_k = 11;
            slug.inference.nli_top_k = 22;
        }
        "inference.nli_enabled" => {
            global.inference.nli_enabled = false;
            slug.inference.nli_enabled = true;
        }
        "inference.w_sim" => {
            global.inference.w_sim = 0.11;
            slug.inference.w_sim = 0.22;
        }
        "inference.w_nli" => {
            global.inference.w_nli = 0.11;
            slug.inference.w_nli = 0.22;
        }
        "inference.w_conf" => {
            global.inference.w_conf = 0.11;
            slug.inference.w_conf = 0.22;
        }
        "inference.w_coac" => {
            global.inference.w_coac = 0.11;
            slug.inference.w_coac = 0.22;
        }
        "inference.w_util" => {
            global.inference.w_util = 0.11;
            slug.inference.w_util = 0.22;
        }
        "inference.w_prov" => {
            global.inference.w_prov = 0.11;
            slug.inference.w_prov = 0.22;
        }
        "inference.ppr_alpha" => {
            global.inference.ppr_alpha = 0.11;
            slug.inference.ppr_alpha = 0.22;
        }
        "inference.ppr_blend_weight" => {
            global.inference.ppr_blend_weight = 0.11;
            slug.inference.ppr_blend_weight = 0.22;
        }
        "server.instructions" => {
            global.server.instructions = Some("global instructions".to_string());
            slug.server.instructions = Some("slug instructions".to_string());
        }
        "inference.embedding_model_sha256" => {
            global.inference.embedding_model_sha256 = Some("a".repeat(64));
            slug.inference.embedding_model_sha256 = Some("b".repeat(64));
        }
        "inference.nli_model_sha256" => {
            global.inference.nli_model_sha256 = Some("c".repeat(64));
            slug.inference.nli_model_sha256 = Some("d".repeat(64));
        }
        "inference.rayon_pool_size" => {
            global.inference.rayon_pool_size = 4;
            slug.inference.rayon_pool_size = 8;
        }
        "tls" => {
            global.tls.enabled = Some(true);
            slug.tls.enabled = Some(false);
        }
        "http" => {
            global.http.content_port = 9001;
            slug.http.content_port = 9002;
        }
        // Daemon process flag — no `UnimatrixConfig` field, no merge arm.
        "permissive" => return None,
        other => panic!(
            "registry key {other:?} has no value-setter in pair_differing_on — \
             add one (drift-guard would otherwise under-cover this key)"
        ),
    }

    Some((global, slug))
}

/// Read the merged value at `key` and assert it equals the EXPECTED side
/// (`true` ⇒ per-slug value won, `false` ⇒ global value held). Each arm names
/// the concrete struct accessor — this is the binding from `entry.key` strings
/// to real fields.
fn assert_merged_side(merged: &UnimatrixConfig, key: &str, slug_should_win: bool) {
    macro_rules! check {
        ($actual:expr, $a:expr, $b:expr) => {{
            let expected = if slug_should_win { $b } else { $a };
            assert_eq!(
                $actual, expected,
                "key {key:?}: merged value did not match the {} side (slug_should_win={slug_should_win})",
                if slug_should_win { "per-slug" } else { "global" }
            );
        }};
    }

    match key {
        "knowledge.categories" => check!(
            &merged.knowledge.categories,
            &vec!["alpha".to_string()],
            &vec!["beta".to_string()]
        ),
        "knowledge.boosted_categories" => check!(
            &merged.knowledge.boosted_categories,
            &vec!["g-boost".to_string()],
            &vec!["s-boost".to_string()]
        ),
        "knowledge.adaptive_categories" => check!(
            &merged.knowledge.adaptive_categories,
            &vec!["g-adaptive".to_string()],
            &vec!["s-adaptive".to_string()]
        ),
        "confidence.weights" => {
            check!(
                &merged.confidence.weights,
                &Some(weights(0.40)),
                &Some(weights(0.42))
            )
        }
        "observation.domain_packs" => check!(
            &merged.observation.domain_packs,
            &vec![domain_pack("global-domain")],
            &vec![domain_pack("slug-domain")]
        ),
        "inference.nli_top_k" => check!(merged.inference.nli_top_k, 11, 22),
        "inference.nli_enabled" => check!(merged.inference.nli_enabled, false, true),
        "inference.w_sim" => check!(merged.inference.w_sim, 0.11, 0.22),
        "inference.w_nli" => check!(merged.inference.w_nli, 0.11, 0.22),
        "inference.w_conf" => check!(merged.inference.w_conf, 0.11, 0.22),
        "inference.w_coac" => check!(merged.inference.w_coac, 0.11, 0.22),
        "inference.w_util" => check!(merged.inference.w_util, 0.11, 0.22),
        "inference.w_prov" => check!(merged.inference.w_prov, 0.11, 0.22),
        "inference.ppr_alpha" => check!(merged.inference.ppr_alpha, 0.11, 0.22),
        "inference.ppr_blend_weight" => check!(merged.inference.ppr_blend_weight, 0.11, 0.22),
        "server.instructions" => check!(
            &merged.server.instructions,
            &Some("global instructions".to_string()),
            &Some("slug instructions".to_string())
        ),
        "inference.embedding_model_sha256" => check!(
            &merged.inference.embedding_model_sha256,
            &Some("a".repeat(64)),
            &Some("b".repeat(64))
        ),
        "inference.nli_model_sha256" => check!(
            &merged.inference.nli_model_sha256,
            &Some("c".repeat(64)),
            &Some("d".repeat(64))
        ),
        "inference.rayon_pool_size" => check!(merged.inference.rayon_pool_size, 4, 8),
        "tls" => check!(merged.tls.enabled, Some(true), Some(false)),
        "http" => check!(merged.http.content_port, 9001, 9002),
        other => panic!("key {other:?} has no accessor in assert_merged_side"),
    }
}

fn weights(base: f64) -> ConfidenceWeights {
    // The exact sum is irrelevant here — these configs never reach `validate_config`;
    // the drift-guard only inspects which LAYER the merged value came from.
    ConfidenceWeights {
        base,
        usage: 0.10,
        fresh: 0.10,
        help: 0.10,
        corr: 0.10,
        trust: 0.10,
    }
}

fn domain_pack(domain: &str) -> DomainPackConfig {
    DomainPackConfig {
        source_domain: domain.to_string(),
        event_types: vec!["evt".to_string()],
        categories: vec!["lesson-learned".to_string()],
        rule_file: None,
    }
}

// ---------------------------------------------------------------------------
// AC-11 / R-14 — Machine-checked drift-guard (MANDATORY centerpiece)
// ---------------------------------------------------------------------------

#[test]
fn test_classification_drift_guard_every_entry_matches_merge_configs() {
    for entry in PER_SLUG_CONFIG_CLASSIFICATION {
        let key = entry.key;

        // Construction-locked keys (incl. `permissive`) are NOT locked by
        // `merge_configs`; their lock lives in `per_slug_loop`. Skip the merge-level
        // assertion for them (see module docs) — but they MUST still be classifiable.
        if is_construction_locked(key) {
            assert_eq!(
                entry.disposition,
                OverlayDisposition::GlobalLocked,
                "key {key:?} is in CONSTRUCTION_LOCKED_KEYS but not classified GlobalLocked"
            );
            continue;
        }

        let (global, slug) = pair_differing_on(key)
            .unwrap_or_else(|| panic!("non-construction key {key:?} must have a config pair"));

        // merge_configs CONSUMES both owned args → pass clones (startup-only cost in prod).
        let merged = merge_configs(global.clone(), slug.clone());

        match entry.disposition {
            // Overlayable ⇒ the per-slug value won.
            OverlayDisposition::PerSlugOverlayable => {
                assert_merged_side(&merged, key, /* slug_should_win = */ true);
            }
            // Merge-locked (the `*_sha256` carve-out) ⇒ the global value held even
            // though the per-slug value differs.
            OverlayDisposition::GlobalLocked => {
                assert_merged_side(&merged, key, /* slug_should_win = */ false);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Exhaustiveness vs the seam-relevant field set (carry-item 9, R-07)
// ---------------------------------------------------------------------------

/// Mechanically DERIVE the set of seam keys that `[knowledge]` contributes, by
/// EXHAUSTIVELY destructuring `KnowledgeConfig`. This is the recurrence fix
/// (carry-item / security re-review): the previous exhaustiveness test compared two
/// HAND-MAINTAINED lists (the registry vs a `const EXPECTED_CLASSIFIED_KEYS` array),
/// which is itself a duplicate-of-truth — exactly the crt-031 anti-pattern the
/// registry exists to kill. A hand-list can NEVER catch a real overlaid field that
/// was forgotten in BOTH places, which is how `adaptive_categories` slipped through.
///
/// Here the expected `knowledge.*` key set is bound to the ACTUAL struct field set:
/// the destructuring pattern is exhaustive, so adding ANY new field to
/// `KnowledgeConfig` makes THIS function fail to compile until the author classifies
/// it below (either as a seam key with a registry entry, or explicitly as a non-seam
/// field). The compiler — not a human-maintained list — is the completeness oracle.
///
/// Why this is sound (not another hand-list-vs-hand-list):
///  * The match binds every `KnowledgeConfig` field by name; `KnowledgeConfig` is a
///    plain struct (no `..` rest pattern), so a 5th field added tomorrow breaks the
///    build at THIS site. There is no way to add an overlayable knowledge field and
///    silently skip classification.
///  * Each field maps to EITHER `Some(stable_key)` (⇒ it MUST have a registry entry,
///    asserted below) OR `None` (⇒ explicitly NOT read at the `build_project_server`
///    seam — `freshness_half_life_hours` is resolved into the preset elsewhere, never
///    in the per-slug loop, main.rs:1119-1151). The `None` arm is a documented,
///    reviewed decision, not an omission.
fn knowledge_seam_keys() -> Vec<&'static str> {
    // A representative value is irrelevant — we only enumerate the field SET. The
    // exhaustive destructure is the load-bearing part.
    let KnowledgeConfig {
        categories: _,
        boosted_categories: _,
        adaptive_categories: _,
        freshness_half_life_hours: _,
    } = KnowledgeConfig::default();

    // Map each field → its seam key, or None if it is not a per-slug seam input.
    // Adding a field above forces adding an arm here (or the destructure won't compile).
    let per_field: [(&str, Option<&'static str>); 4] = [
        ("categories", Some("knowledge.categories")),
        ("boosted_categories", Some("knowledge.boosted_categories")),
        ("adaptive_categories", Some("knowledge.adaptive_categories")),
        // Not read in the per-slug loop (resolved into the freshness preset elsewhere).
        ("freshness_half_life_hours", None),
    ];

    per_field.iter().filter_map(|(_, key)| *key).collect()
}

/// The closed checklist of every NON-`knowledge` config key/section reachable at the
/// `build_project_server` call site (the §9 verdict rows, rendered as stable ids).
/// The `knowledge.*` rows are NOT listed here — they are derived mechanically by
/// [`knowledge_seam_keys`] from the live struct, so the registry can never drift from
/// the actual `KnowledgeConfig` shape (the seam that recurred). Adding a
/// `build_project_server`-relevant key without a registry entry — or a registry entry
/// without a checklist row — fails the exhaustiveness test. This is the row-set guard
/// that materialized at the design gate (`embed_handle`, then `instructions`/`permissive`).
const EXPECTED_CLASSIFIED_KEYS_NON_KNOWLEDGE: &[&str] = &[
    // Overlayable
    "confidence.weights",
    "observation.domain_packs",
    "inference.nli_top_k",
    "inference.nli_enabled",
    "inference.w_sim",
    "inference.w_nli",
    "inference.w_conf",
    "inference.w_coac",
    "inference.w_util",
    "inference.w_prov",
    "inference.ppr_alpha",
    "inference.ppr_blend_weight",
    "server.instructions",
    // Global-locked
    "inference.embedding_model_sha256",
    "inference.nli_model_sha256",
    "inference.rayon_pool_size",
    "permissive",
    "tls",
    "http",
];

#[test]
fn test_classification_registry_exhaustive_vs_seam_field_set() {
    use std::collections::BTreeSet;

    let registry: BTreeSet<&str> = PER_SLUG_CONFIG_CLASSIFICATION
        .iter()
        .map(|e| e.key)
        .collect();

    // Expected = mechanically-derived knowledge keys ∪ the non-knowledge checklist.
    // The knowledge half is bound to the live `KnowledgeConfig` struct, so a new
    // overlayable knowledge field cannot be forgotten in both the registry and here.
    let mut expected: BTreeSet<&str> = EXPECTED_CLASSIFIED_KEYS_NON_KNOWLEDGE
        .iter()
        .copied()
        .collect();
    for key in knowledge_seam_keys() {
        expected.insert(key);
    }

    let missing: Vec<&&str> = expected.difference(&registry).collect();
    let extra: Vec<&&str> = registry.difference(&expected).collect();

    assert!(
        missing.is_empty(),
        "seam-relevant keys absent from PER_SLUG_CONFIG_CLASSIFICATION: {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "registry keys with no seam checklist row (closed-set violation): {extra:?}"
    );

    // No duplicate keys in the registry.
    assert_eq!(
        registry.len(),
        PER_SLUG_CONFIG_CLASSIFICATION.len(),
        "PER_SLUG_CONFIG_CLASSIFICATION contains duplicate keys"
    );

    // Every overlayable key actually has a value-setter + accessor (cross-checks that
    // the drift-guard covers it, not just that the row exists).
    for entry in PER_SLUG_CONFIG_CLASSIFICATION {
        if !is_construction_locked(entry.key) {
            assert!(
                pair_differing_on(entry.key).is_some(),
                "registry key {:?} has no drift-guard coverage",
                entry.key
            );
        }
    }
}

// ---------------------------------------------------------------------------
// `is_per_slug_overlayable` predicate
// ---------------------------------------------------------------------------

#[test]
fn test_is_per_slug_overlayable_matches_registry_disposition() {
    for entry in PER_SLUG_CONFIG_CLASSIFICATION {
        let expected = entry.disposition == OverlayDisposition::PerSlugOverlayable;
        assert_eq!(
            is_per_slug_overlayable(entry.key),
            expected,
            "predicate disagrees with registry disposition for key {:?}",
            entry.key
        );
    }
}

#[test]
fn test_is_per_slug_overlayable_unknown_key_returns_false() {
    // Contract (pseudocode §predicate): an unknown / non-seam key is conservatively
    // not-overlayable. Total — never panics.
    assert!(!is_per_slug_overlayable("totally.unknown.key"));
    assert!(!is_per_slug_overlayable(""));
    assert!(!is_per_slug_overlayable("inference"));
}

#[test]
fn test_is_per_slug_overlayable_sampled_dispositions() {
    // Sampled overlayable.
    assert!(is_per_slug_overlayable("server.instructions"));
    assert!(is_per_slug_overlayable("knowledge.categories"));
    assert!(is_per_slug_overlayable("inference.nli_top_k"));
    // Sampled locked.
    assert!(!is_per_slug_overlayable("inference.embedding_model_sha256"));
    assert!(!is_per_slug_overlayable("inference.nli_model_sha256"));
    assert!(!is_per_slug_overlayable("permissive"));
    assert!(!is_per_slug_overlayable("tls"));
    assert!(!is_per_slug_overlayable("inference.rayon_pool_size"));
}

// ---------------------------------------------------------------------------
// AC-05 / R-05 — Hash-pin global-wins + warn (the merge half)
// ---------------------------------------------------------------------------

#[test]
#[tracing_test::traced_test]
fn test_sha256_pins_global_wins_under_per_slug_pairing() {
    let mut global = UnimatrixConfig::default();
    let mut slug = UnimatrixConfig::default();
    global.inference.embedding_model_sha256 = Some("a".repeat(64));
    global.inference.nli_model_sha256 = Some("c".repeat(64));
    slug.inference.embedding_model_sha256 = Some("b".repeat(64));
    slug.inference.nli_model_sha256 = Some("d".repeat(64));

    let merged = merge_configs(global.clone(), slug.clone());

    assert_eq!(
        merged.inference.embedding_model_sha256,
        Some("a".repeat(64)),
        "embedding_model_sha256 global pin did not win"
    );
    assert_eq!(
        merged.inference.nli_model_sha256,
        Some("c".repeat(64)),
        "nli_model_sha256 global pin did not win"
    );

    // The divergence is warned (security-critical, #4655/#4649).
    assert!(
        logs_contain("global hash pin takes precedence"),
        "expected a tracing::warn naming the ignored per-project hash pin"
    );
}

#[test]
fn test_no_global_pin_plus_per_slug_pin_does_not_silently_lock() {
    // Global pin unset, per-slug pin set: the per-slug pin falls through via `.or()`
    // (documented absence semantics) — it is NOT suppressed, but neither does it
    // describe a second model (the handle is global by construction, AC-04). This
    // corroborates the descriptor-lock semantics at the merge level.
    let mut global = UnimatrixConfig::default();
    let mut slug = UnimatrixConfig::default();
    global.inference.embedding_model_sha256 = None;
    slug.inference.embedding_model_sha256 = Some("b".repeat(64));

    let merged = merge_configs(global, slug);
    assert_eq!(
        merged.inference.embedding_model_sha256,
        Some("b".repeat(64))
    );
}

// ---------------------------------------------------------------------------
// R-02 — Inference-arm overlay coverage under the C6 (global→per-slug) call shape
// ---------------------------------------------------------------------------

#[test]
fn test_inference_overlayable_fields_overlay_siblings_fall_through() {
    // Override ONLY nli_top_k; leave every sibling inference field global. Assert the
    // overridden field == per-slug AND a non-overridden sibling == global. Exercises
    // the inline `InferenceConfig {…}` literal (#4070) for the global→per-slug shape.
    let mut global = UnimatrixConfig::default();
    let mut slug = UnimatrixConfig::default();
    global.inference.nli_top_k = 11;
    global.inference.w_sim = 0.33; // sibling left untouched on the slug side
    slug.inference.nli_top_k = 22;

    let merged = merge_configs(global, slug);

    assert_eq!(
        merged.inference.nli_top_k, 22,
        "overridden field must take per-slug value"
    );
    assert_eq!(
        merged.inference.w_sim, 0.33,
        "non-overridden inference sibling must fall through to global"
    );
}

#[test]
fn test_option_field_set_global_unset_per_slug_retains_global() {
    // R-02 sibling edge case: Option set in global, unset in per-slug → global retained
    // via `.or()` (server.instructions exercises the same arm).
    let mut global = UnimatrixConfig::default();
    let slug = UnimatrixConfig::default(); // instructions = None
    global.server.instructions = Some("global only".to_string());

    let merged = merge_configs(global, slug);
    assert_eq!(merged.server.instructions, Some("global only".to_string()));
}

#[test]
fn test_list_field_override_replaces_not_appends() {
    // #2286 replace semantics (AC-03): a per-slug categories list REPLACES the global
    // list wholesale — no append/merge.
    let mut global = UnimatrixConfig::default();
    let mut slug = UnimatrixConfig::default();
    global.knowledge.categories = vec!["g1".to_string(), "g2".to_string()];
    slug.knowledge.categories = vec!["s1".to_string()];

    let merged = merge_configs(global, slug);
    assert_eq!(merged.knowledge.categories, vec!["s1".to_string()]);
}

// ---------------------------------------------------------------------------
// R-08 — nli_top_k / nli_enabled overlayable as runtime params
// ---------------------------------------------------------------------------

#[test]
fn test_nli_runtime_params_overlay_and_are_classified_overlayable() {
    let mut global = UnimatrixConfig::default();
    let mut slug = UnimatrixConfig::default();
    global.inference.nli_top_k = 11;
    global.inference.nli_enabled = false;
    slug.inference.nli_top_k = 33;
    slug.inference.nli_enabled = true;

    let merged = merge_configs(global, slug);

    assert_eq!(merged.inference.nli_top_k, 33);
    assert!(merged.inference.nli_enabled);

    // Registry classifies both overlayable (runtime params, not model identity).
    assert!(is_per_slug_overlayable("inference.nli_top_k"));
    assert!(is_per_slug_overlayable("inference.nli_enabled"));
}
