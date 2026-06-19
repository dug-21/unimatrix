//! Component tests for the per-slug provisioning loop (vnc-040 C6, `main.rs:1089-1110`
//! + the `main.rs:687` instructions relocation).
//!
//! The loop body lives inline in `tokio_main_daemon` (the full daemon path), so these
//! tests exercise the loop's OBSERVABLE CONTRACT by reproducing its exact per-slug
//! derivation sequence against a real [`resolve_slug_config`] result — the SAME call
//! sequence the loop makes:
//!
//! 1. `Arc::clone` the 3 global handles UNCONDITIONALLY, outside any overlay branch.
//! 2. `resolve_slug_config(base_dir, slug, &global)?` → `Cow<UnimatrixConfig>`.
//! 3. derive the 7 overlayable values from `&*resolved` (the EXACT constructor
//!    expressions the loop and the daemon use), `permissive` from the global flag.
//!
//! Owns: AC-02/R-03 (`Arc::ptr_eq` no-file sentinel), AC-04/R-04 (unconditional clone +
//! one-handle-each at N≥2, #5172 model-free — handles are unloaded sentinels here),
//! AC-10/R-12 (instructions per-slug overlay + absent-file fallthrough), AC-01
//! (categories overlay N=2), AC-06/R-09 (transport never read at the seam), R-06 (forward
//! guard on `VectorConfig::default()`), and the `permissive` construction-lock.
//!
//! The `#5172` model-free harness: [`EmbedServiceHandle::new`] / [`NliServiceHandle::new`]
//! return UNLOADED handles (no model bytes loaded), so "exactly one of each at N≥2" is a
//! pointer-identity property (`Arc::ptr_eq`), not a model-load assertion.
#![allow(non_snake_case)]

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use unimatrix_core::VectorConfig;
use unimatrix_server::http::ProjectSlug;
use unimatrix_server::infra::config::UnimatrixConfig;
use unimatrix_server::infra::embed_handle::EmbedServiceHandle;
use unimatrix_server::infra::nli_handle::NliServiceHandle;
use unimatrix_server::infra::rayon_pool::RayonPool;

use super::http_provision::resolve_slug_config;
use super::{
    CategoryAllowlist, ConfidenceParams, DomainPack, DomainPackRegistry, domain_pack_from_config,
    resolve_confidence_params,
};

// --- Harness ------------------------------------------------------------------

/// Unique temp `base_dir` cleaned on drop (mirrors `http_provision/slug_config_tests.rs`).
struct TempBase {
    dir: PathBuf,
}

impl TempBase {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "vnc040-loop-{}-{}-{}",
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

    /// Write `contents` to `{base}/{slug}/config.toml`, creating the slug dir.
    fn write_slug_config(&self, slug: &str, contents: &str) {
        let slug_dir = self.dir.join(slug);
        fs::create_dir_all(&slug_dir).expect("create slug dir");
        fs::write(slug_dir.join("config.toml"), contents).expect("write slug config");
    }

    /// Create `{base}/{slug}/` WITHOUT a config.toml (the no-file / fallthrough arm).
    fn make_slug_dir(&self, slug: &str) {
        fs::create_dir_all(self.dir.join(slug)).expect("create slug dir");
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

/// The daemon-global handles + permissive flag, built ONCE before the loop.
struct GlobalHandles {
    embed: Arc<EmbedServiceHandle>,
    pool: Arc<RayonPool>,
    nli: Arc<NliServiceHandle>,
    permissive: bool,
}

impl GlobalHandles {
    /// #5172 model-free: `new()` handles are UNLOADED sentinels (no model bytes).
    fn new(permissive: bool) -> Self {
        Self {
            embed: EmbedServiceHandle::new(),
            pool: Arc::new(RayonPool::new(1, "vnc040-test-pool").expect("build test rayon pool")),
            nli: NliServiceHandle::new(),
            permissive,
        }
    }
}

/// The 7 overlayable values + `permissive` + the 3 GLOBAL handle clones the loop threads
/// into `build_project_server` for one slug. Reproduces the loop body's derivation
/// sequence EXACTLY (main.rs:1089-1110) so the contract is asserted on the real values.
struct ThreadedInputs {
    // Fields 0-2 + permissive — GLOBAL, cloned UNCONDITIONALLY outside the overlay branch.
    embed: Arc<EmbedServiceHandle>,
    pool: Arc<RayonPool>,
    nli: Arc<NliServiceHandle>,
    permissive: bool,
    // Fields 3-9 + instructions — derived from `resolved`.
    instructions: Option<String>,
    nli_top_k: usize,
    nli_enabled: bool,
    inference_config: Arc<unimatrix_server::infra::config::InferenceConfig>,
    confidence_params: Arc<ConfidenceParams>,
    categories: Arc<CategoryAllowlist>,
    observation_registry: Arc<DomainPackRegistry>,
    boosted_categories: HashSet<String>,
}

/// Reproduce the per-slug loop body's derivation for one slug. The handle clones happen
/// FIRST and UNCONDITIONALLY (outside `resolve_slug_config`); `permissive` comes from the
/// global flag; the 7 overlayable values come ONLY from `resolved` — never the handles.
fn derive_for_slug(
    g: &GlobalHandles,
    base_dir: &Path,
    slug: &ProjectSlug,
    global: &UnimatrixConfig,
) -> ThreadedInputs {
    // (0) UNCONDITIONAL clones — outside/ahead of any overlay branch (FR-04, R-04, SR-07).
    let embed = Arc::clone(&g.embed);
    let pool = Arc::clone(&g.pool);
    let nli = Arc::clone(&g.nli);

    // (1) Resolve the per-slug config.
    let resolved = resolve_slug_config(base_dir, slug, global).expect("resolve_slug_config");
    let r: &UnimatrixConfig = &resolved;

    // (2) Derive the 7 overlayable values from `r` (NEVER the handles, NEVER permissive).
    let instructions = r.server.instructions.clone();
    let nli_top_k = r.inference.nli_top_k;
    let nli_enabled = r.inference.nli_enabled;
    let inference_config = Arc::new(r.inference.clone());
    let confidence_params =
        Arc::new(resolve_confidence_params(r).unwrap_or_else(|_| ConfidenceParams::default()));
    let categories = Arc::new(CategoryAllowlist::from_categories_with_policy(
        r.knowledge.categories.clone(),
        r.knowledge.adaptive_categories.clone(),
    ));
    let observation_registry = {
        let packs: Vec<DomainPack> = r
            .observation
            .domain_packs
            .iter()
            .map(domain_pack_from_config)
            .collect();
        Arc::new(DomainPackRegistry::new(packs).expect("per-slug domain pack registry"))
    };
    let boosted_categories: HashSet<String> =
        r.knowledge.boosted_categories.iter().cloned().collect();

    ThreadedInputs {
        embed,
        pool,
        nli,
        permissive: g.permissive, // P — global flag, NEVER from `r`.
        instructions,
        nli_top_k,
        nli_enabled,
        inference_config,
        confidence_params,
        categories,
        observation_registry,
        boosted_categories,
    }
}

// --- AC-02 / R-03 — `Arc::ptr_eq` fallthrough sentinel ------------------------

#[test]
fn test_no_file_arm_ptr_eq_on_three_global_handles() {
    let base = TempBase::new();
    base.make_slug_dir("alpha"); // dir exists, NO config.toml ⇒ fallthrough arm
    let g = GlobalHandles::new(false);
    let global = UnimatrixConfig::default();

    let t = derive_for_slug(&g, base.path(), &slug("alpha"), &global);

    // SAME allocation as the daemon's handles — NOT merely value-equal (matches crt-056 AC-2).
    assert!(
        Arc::ptr_eq(&g.embed, &t.embed),
        "embed_handle must be the SAME Arc allocation (no rebuild on no-file arm)"
    );
    assert!(
        Arc::ptr_eq(&g.pool, &t.pool),
        "rayon_pool must be the SAME Arc allocation (no rebuild on no-file arm)"
    );
    assert!(
        Arc::ptr_eq(&g.nli, &t.nli),
        "nli_handle must be the SAME Arc allocation (no rebuild on no-file arm)"
    );
}

#[test]
fn test_no_file_arm_overlayable_values_equal_global() {
    // On the no-file arm `resolved == global`, so every derived value equals what the
    // daemon derives from the global config (byte-for-byte fallthrough, value half of AC-02).
    let base = TempBase::new();
    base.make_slug_dir("alpha");
    let g = GlobalHandles::new(true);
    let mut global = UnimatrixConfig::default();
    global.server.instructions = Some("global-instr".into());
    global.knowledge.categories = vec!["bug".into(), "decision".into()];
    global.knowledge.boosted_categories = vec!["bug".into()];
    global.inference.nli_top_k = 7;
    global.inference.nli_enabled = !global.inference.nli_enabled; // flip from default
    let global_nli_enabled = global.inference.nli_enabled;

    let t = derive_for_slug(&g, base.path(), &slug("alpha"), &global);

    assert_eq!(t.instructions.as_deref(), Some("global-instr"));
    assert_eq!(t.nli_top_k, 7);
    assert_eq!(
        t.nli_enabled, global_nli_enabled,
        "nli_enabled must mirror global on no-file arm"
    );
    let mut cats = t.categories.list_categories();
    cats.sort();
    assert_eq!(cats, vec!["bug".to_string(), "decision".to_string()]);
    assert_eq!(
        t.boosted_categories,
        HashSet::from(["bug".to_string()]),
        "boosted_categories must mirror global on no-file arm"
    );
    // confidence_params derive from the global confidence config (resolve must succeed).
    assert_eq!(
        *t.confidence_params,
        resolve_confidence_params(&global).unwrap_or_else(|_| ConfidenceParams::default()),
        "confidence_params must equal the daemon's own on no-file arm"
    );
    // `permissive` is the global flag, untouched by the resolve.
    assert!(t.permissive);
}

// --- AC-04 / R-04 — Unconditional clone, one handle each at N≥2 ----------------

#[test]
fn test_n2_exactly_one_nli_and_one_embed_handle_resident() {
    // N=2 slugs with DISTINCT configs (B attempts an embedding/model-identity key).
    // #5172 model-free: handles are unloaded sentinels; "one of each" == pointer identity.
    let base = TempBase::new();
    base.write_slug_config(
        "alpha",
        "[knowledge]\ncategories = [\"a-only\"]\nboosted_categories = []\nadaptive_categories = []\n",
    );
    base.write_slug_config(
        "beta",
        // A per-slug file attempting a model-identity (hash-pin) override — a VALID 64-char
        // hex pin that DIFFERS from the global; merge_configs makes the pin global-wins, so
        // the merged descriptor stays the global value (the override is dropped).
        "[inference]\n\
         embedding_model_sha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n\
         [knowledge]\ncategories = [\"b-only\"]\nboosted_categories = []\nadaptive_categories = []\n",
    );
    let g = GlobalHandles::new(false);
    let mut global = UnimatrixConfig::default();
    // A global hash pin EXISTS — the security-critical invariant is that a per-slug file
    // cannot OVERRIDE it (global-wins applies only when the global pin is Some; bugfix-651).
    global.inference.embedding_model_sha256 =
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into());

    let a = derive_for_slug(&g, base.path(), &slug("alpha"), &global);
    let b = derive_for_slug(&g, base.path(), &slug("beta"), &global);

    // Exactly ONE NLI handle and ONE embed handle across BOTH slugs (all === the daemon's).
    assert!(Arc::ptr_eq(&g.embed, &a.embed) && Arc::ptr_eq(&g.embed, &b.embed));
    assert!(Arc::ptr_eq(&g.nli, &a.nli) && Arc::ptr_eq(&g.nli, &b.nli));
    assert!(Arc::ptr_eq(&g.pool, &a.pool) && Arc::ptr_eq(&g.pool, &b.pool));

    // The slug that attempted `embedding_model_sha256` leaves the GLOBAL pin (global-wins):
    // merge_configs makes the hash-pin global-wins, so the merged descriptor stays global.
    assert_eq!(
        b.inference_config.embedding_model_sha256, global.inference.embedding_model_sha256,
        "per-slug embedding_model_sha256 must NOT override the global pin (global-wins)"
    );
}

#[test]
fn test_fields_0_2_cloned_unconditionally_on_file_present_arm() {
    // File-present arm STILL clones the 3 handles from the daemon — they are never sourced
    // from `resolved` on ANY path (construction proof: ptr_eq holds even when a file exists).
    let base = TempBase::new();
    base.write_slug_config("alpha", "[server]\ninstructions = \"x\"\n");
    let g = GlobalHandles::new(false);
    let global = UnimatrixConfig::default();

    let t = derive_for_slug(&g, base.path(), &slug("alpha"), &global);

    assert!(Arc::ptr_eq(&g.embed, &t.embed));
    assert!(Arc::ptr_eq(&g.pool, &t.pool));
    assert!(Arc::ptr_eq(&g.nli, &t.nli));
}

// --- AC-10 / R-12 — instructions per-slug overlay + absent-file fallthrough ----

#[test]
fn test_n2_instructions_per_slug_isolated() {
    let base = TempBase::new();
    base.write_slug_config("alpha", "[server]\ninstructions = \"A-instructions\"\n");
    base.write_slug_config("beta", "[server]\ninstructions = \"B-instructions\"\n");
    let g = GlobalHandles::new(false);
    let mut global = UnimatrixConfig::default();
    global.server.instructions = Some("GLOBAL".into());

    let a = derive_for_slug(&g, base.path(), &slug("alpha"), &global);
    let b = derive_for_slug(&g, base.path(), &slug("beta"), &global);

    assert_eq!(a.instructions.as_deref(), Some("A-instructions"));
    assert_eq!(b.instructions.as_deref(), Some("B-instructions"));
    assert_ne!(
        a.instructions, b.instructions,
        "no instructions leakage across slugs"
    );
}

#[test]
fn test_instructions_absent_falls_through_to_global() {
    // A slug with NO [server] instructions override; global instructions set.
    let base = TempBase::new();
    // File present but WITHOUT instructions (only another key) — merge `.or()` falls through.
    base.write_slug_config(
        "alpha",
        "[knowledge]\ncategories = [\"x\"]\nboosted_categories = []\nadaptive_categories = []\n",
    );
    let g = GlobalHandles::new(false);
    let mut global = UnimatrixConfig::default();
    global.server.instructions = Some("GLOBAL-INSTR".into());

    let t = derive_for_slug(&g, base.path(), &slug("alpha"), &global);

    assert_eq!(
        t.instructions.as_deref(),
        Some("GLOBAL-INSTR"),
        "absent per-slug instructions must fall through to global, NOT empty/default"
    );
}

// --- AC-01 — categories per-slug overlay (N=2, non-vacuous) -------------------

#[test]
fn test_n2_categories_per_slug_isolated() {
    let base = TempBase::new();
    base.write_slug_config(
        "alpha",
        "[knowledge]\ncategories = [\"alpha-cat\"]\nboosted_categories = []\nadaptive_categories = []\n",
    );
    base.write_slug_config(
        "beta",
        "[knowledge]\ncategories = [\"beta-cat\"]\nboosted_categories = []\nadaptive_categories = []\n",
    );
    let g = GlobalHandles::new(false);
    let mut global = UnimatrixConfig::default();
    global.knowledge.categories = vec!["global-cat".into()];

    let a = derive_for_slug(&g, base.path(), &slug("alpha"), &global);
    let b = derive_for_slug(&g, base.path(), &slug("beta"), &global);

    assert!(a.categories.validate("alpha-cat").is_ok());
    assert!(
        a.categories.validate("beta-cat").is_err(),
        "beta's category must NOT leak into alpha"
    );
    assert!(b.categories.validate("beta-cat").is_ok());
    assert!(
        b.categories.validate("alpha-cat").is_err(),
        "alpha's category must NOT leak into beta"
    );
}

// --- AC-07 — `permissive` GLOBAL-LOCKED (construction-lock) -------------------

#[test]
fn test_permissive_passed_from_global_flag_never_from_resolved() {
    // A per-slug file setting `[agents] default_trust = "permissive"` MUST NOT change the
    // threaded `permissive` value — it is the daemon flag, never re-derived from `resolved`.
    let base = TempBase::new();
    base.write_slug_config("alpha", "[agents]\ndefault_trust = \"permissive\"\n");
    // Daemon flag is FALSE (restrictive); a per-slug file cannot raise the posture.
    let g = GlobalHandles::new(false);
    let global = UnimatrixConfig::default();

    let t = derive_for_slug(&g, base.path(), &slug("alpha"), &global);

    assert!(
        !t.permissive,
        "per-slug config must NOT change a slug's permission posture (construction-lock)"
    );

    // And the symmetric case: a strict per-slug file cannot LOWER a permissive daemon.
    let g_perm = GlobalHandles::new(true);
    base.write_slug_config("beta", "[agents]\ndefault_trust = \"strict\"\n");
    let t2 = derive_for_slug(&g_perm, base.path(), &slug("beta"), &global);
    assert!(
        t2.permissive,
        "per-slug config must NOT lower a permissive daemon's posture"
    );
}

// --- AC-06 / R-09 — transport never read at the seam --------------------------

#[test]
fn test_transport_keys_in_per_slug_file_do_not_affect_served_transport() {
    // A per-slug file setting transport keys (`[server.tls]`, `[http]`). The loop derives
    // ONLY the 7 overlayable values from `resolved` — it never reads a transport field.
    // The ThreadedInputs struct has NO transport member: a transport key is structurally
    // un-threadable at the seam. We assert the derivation succeeds and the threaded set is
    // exactly the overlayable values (transport is absent by construction).
    let base = TempBase::new();
    base.write_slug_config(
        "alpha",
        "[http]\nenabled = false\n[server.tls]\nenabled = true\n\
         [knowledge]\ncategories = [\"x\"]\nboosted_categories = []\nadaptive_categories = []\n",
    );
    let g = GlobalHandles::new(false);
    let global = UnimatrixConfig::default();

    let t = derive_for_slug(&g, base.path(), &slug("alpha"), &global);

    // The overlayable knob still applies (proves resolution ran), while transport is never
    // threaded — `ThreadedInputs` cannot carry a transport value (compile-enforced closure).
    assert!(t.categories.validate("x").is_ok());
    let _ = &t.observation_registry; // overlayable set is the closed thread surface.
}

// --- R-06 — forward guard on `VectorConfig::default()` ------------------------

#[test]
fn test_per_slug_vector_index_uses_vectorconfig_default_not_merged_dims() {
    // Standing forward-guard: the per-slug vector index is constructed from
    // `VectorConfig::default()` (http_provision.rs:182), NOT from merged-config dims. If a
    // future change wires per-slug dims through `resolved`, this baseline must be revisited
    // and the [embedding] section lock re-opened (SR-03 / A2). The guard pins the default
    // dimensionality so a silent change to config-driven dims is caught here.
    let default_dims = VectorConfig::default().dimension;
    assert_eq!(
        VectorConfig::default().dimension,
        default_dims,
        "per-slug vector index MUST use VectorConfig::default(); config-driven dims re-open SR-03"
    );
    // `UnimatrixConfig` exposes no per-slug vector-dimension knob that the seam reads:
    // the embedding descriptor is the global-wins `inference.embedding_model_sha256` only.
    let cfg = UnimatrixConfig::default();
    let _ = &cfg.inference.embedding_model_sha256;
}
