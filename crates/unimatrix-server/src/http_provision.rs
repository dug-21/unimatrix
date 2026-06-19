//! HTTP listener provisioning glue (vnc-034 Sub-wave 3 — server integration).
//!
//! Binary-crate wiring helpers kept out of `main.rs` to respect the 500-line
//! module cap. Two responsibilities, both pure glue over already-shipped
//! components (no reimplementation):
//!
//! 1. [`provision_tls`] — derive the single [`PublicUrl`] (C3), first-boot
//!    provision the self-signed cert/key into `{data_dir}/tls/` (SR-01), and
//!    build the [`TlsAcceptor`] from the provisioned PEM files. Returns the
//!    acceptor plus the resolved [`PublicUrl`] so the SAN set the cert was
//!    minted with is observable by the caller (and, later, the bundle echo).
//!
//! The store-resolution funnel itself (the slug-keyed `MultiProjectRouter`
//! `StoreResolver`) is constructed inline in the listener wiring so each per-slug
//! `Arc<Store>` reaches MCP only THROUGH the seam (ADR-003, FR-X5 — no bypass).
//! vnc-038 ADR-004 (#5083) deleted the Wave-1 `DefaultResolver` and the boot-bound
//! `resolve_store(&ProjectKey::Default)`: there is no default store; both MCP and
//! observe resolve per-request through the same funnel keyed by `ProjectKey::Slug`.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use tokio_rustls::TlsAcceptor;

use unimatrix_adapt::{AdaptConfig, AdaptationService};
use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{CoreError, VectorAdapter, VectorConfig};
use unimatrix_engine::confidence::ConfidenceParams;
use unimatrix_observe::domain::DomainPackRegistry;
use unimatrix_server::error::ServerError;
use unimatrix_server::http::{
    Env, ProjectServerInput, ProjectSlug, PublicUrl, build_tls_acceptor, derive_public_url,
    load_or_generate_cert,
};
use unimatrix_server::infra::audit::AuditLog;
use unimatrix_server::infra::categories::CategoryAllowlist;
use unimatrix_server::infra::config::{
    InferenceConfig, TlsConfig, UnimatrixConfig, load_single_config, merge_configs, validate_config,
};
use unimatrix_server::infra::embed_handle::EmbedServiceHandle;
use unimatrix_server::infra::nli_handle::NliServiceHandle;
use unimatrix_server::infra::rayon_pool::RayonPool;
use unimatrix_server::infra::registry::AgentRegistry;
use unimatrix_server::server::UnimatrixServer;
use unimatrix_server::services::ServiceLayer;
use unimatrix_store::{PoolConfig, SqlxStore};
use unimatrix_vector::VectorIndex;

/// Subdirectory under the data dir holding the provisioned TLS material.
const TLS_DIR_NAME: &str = "tls";
/// Provisioned certificate file name (public; 0644).
const CERT_FILE_NAME: &str = "cert.pem";
/// Provisioned private key file name (secret; 0600).
const KEY_FILE_NAME: &str = "key.pem";

/// Derive the public URL (C3), first-boot provision the cert/key (SR-01), and
/// build the TLS acceptor from the provisioned PEM files.
///
/// Mirrors the cert-provisioner pseudocode's initialization sequence: derive
/// the SANs once, provision the cert with them (idempotent — load on reboot),
/// then point a [`TlsConfig`] at the resolved PEM paths so the existing
/// `build_tls_acceptor` (unchanged) loads them. The same files are later read
/// by `client-bundle` to recompute the served leaf DER (C2 parity).
///
/// Returns the built [`TlsAcceptor`] (never `None` in the cloud HTTPS posture —
/// an absent acceptor is a config error here, surfaced loud) and the resolved
/// [`PublicUrl`] the cert SANs were minted from.
///
/// # Errors
///
/// [`ServerError`] when `/data` is unwritable (actionable, names the UID-65532
/// fix via the provisioner), the PEM material is incomplete, or TLS material
/// cannot be loaded into an acceptor. No panic, no `.unwrap()`.
pub fn provision_tls(data_dir: &Path) -> Result<(TlsAcceptor, PublicUrl), ServerError> {
    // C3: one derivation feeds the cert SANs, the bundle base-url, and
    // allowed_hosts — they can never desync (R-09).
    let public_url = derive_public_url(&Env::from_process());

    // SR-01: first-boot self-signed provisioning (idempotent on reboot). Key
    // mode 0600; SANs from the single C3 derivation. PEM buffers are discarded
    // here — the acceptor reads the files build below.
    let _ = load_or_generate_cert(data_dir, &public_url.sans)?;

    // Point a TlsConfig at the provisioned files and let the existing acceptor
    // builder load them (TLS-internally-terminable seam preserved — Constraint 6).
    let tls_dir = data_dir.join(TLS_DIR_NAME);
    let tls_config = TlsConfig {
        enabled: Some(true),
        cert_path: Some(tls_dir.join(CERT_FILE_NAME)),
        key_path: Some(tls_dir.join(KEY_FILE_NAME)),
    };

    let acceptor = build_tls_acceptor(&tls_config)?.ok_or_else(|| {
        ServerError::Config(format!(
            "TLS acceptor not built from provisioned material in {} — \
             expected {CERT_FILE_NAME}/{KEY_FILE_NAME} under the data volume",
            tls_dir.display()
        ))
    })?;

    Ok((acceptor, public_url))
}

/// Database file name within a per-slug data dir (matches the single-project layout).
const PROJECT_DB_NAME: &str = "unimatrix.db";
/// Vector index subdirectory within a per-slug data dir.
const PROJECT_VECTOR_DIR: &str = "vector";

/// Build the per-slug routing input for one validated `[[projects]]` slug
/// (vnc-034 Wave 2, FR-C3 isolation).
///
/// Opens the slug's OWN store at `{base_dir}/{slug}/unimatrix.db` and assembles a
/// per-slug [`UnimatrixServer`] over that store + the slug's own vector index,
/// registry, audit log, and adaptation service. The store / vector index / hash
/// chain / analytics are per-slug (the knowledge-isolation invariant, AC-W2-R3);
/// the stateless embedding-model handle is SHARED (read-only inference, ~87 MB —
/// sharing keeps 1× model memory, OQ-PR-6). The returned [`ProjectServerInput`]
/// carries the slug, its store, and the server so the caller hands it to
/// [`unimatrix_server::http::MultiProjectRouter::from_servers`], which builds the
/// per-slug `McpAdapter` (the sole dispatch route for this key).
///
/// `slug` is an already-validated [`ProjectSlug`] (D1 allowlist), so the single
/// path-join `{base_dir}/{slug}/` CANNOT escape `/data/.unimatrix/` (AC-W2-R6 —
/// escape is unrepresentable, not runtime-rejected).
///
/// # Errors
///
/// The store MUST already exist — `register` is the sole creator (C5, OQ-PR-5);
/// this NEVER auto-creates a slug's store. A missing store dir surfaces a loud,
/// actionable [`ServerError::Config`] naming the `register` remedy. No `.unwrap()`,
/// no panic.
#[allow(clippy::too_many_arguments)]
pub async fn build_project_server(
    base_dir: &Path,
    slug: &ProjectSlug,
    embed_handle: &Arc<EmbedServiceHandle>,
    permissive: bool,
    instructions: Option<String>,
    // crt-056 Wave 1 (ADR-002): config-parity inputs, params-at-end. Every value is an
    // `Arc::clone` of the daemon's RESOLVED config (main.rs:880-898) — the per-slug
    // server reaches the closed 8-field parity over the global config (C-7, AC-1) and
    // shares the ONE loaded model (C-3, AC-2). A missing field is a compile error at the
    // call site, never a silent test-default fallback (anti-Defect-1 guard).
    rayon_pool: &Arc<RayonPool>,
    nli_handle: &Arc<NliServiceHandle>,
    nli_top_k: usize,
    nli_enabled: bool,
    inference_config: &Arc<InferenceConfig>,
    confidence_params: &Arc<ConfidenceParams>,
    categories: &Arc<CategoryAllowlist>,
    observation_registry: &Arc<DomainPackRegistry>,
    // The daemon resolves `boosted_categories` from `config.knowledge.boosted_categories`
    // (main.rs:681-686) and passes that RESOLVED set into its own ServiceLayer
    // (main.rs:889) — NOT `default_boosted_categories_set()`. Thread the SAME resolved
    // set here so AC-1's domain-pack/category parity holds (Gate 3a MUST-CONFIRM).
    boosted_categories: &HashSet<String>,
) -> Result<ProjectServerInput, ServerError> {
    // SINGLE path-join site. `slug` is allowlist-validated, so this cannot escape
    // `{base_dir}/` (AC-W2-R6).
    let data_dir = base_dir.join(slug.as_str());
    let db_path = data_dir.join(PROJECT_DB_NAME);
    let vector_dir = data_dir.join(PROJECT_VECTOR_DIR);

    // No auto-create (C5): the store must already exist. Fail loud + actionable.
    if !db_path.exists() {
        return Err(ServerError::Config(format!(
            "slug '{slug}' in [[projects]] is not registered (no store at {}); \
             run `unimatrix project register {slug}` first — routing never \
             auto-creates a project store",
            db_path.display()
        )));
    }

    let store = Arc::new(
        SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .map_err(|e| {
                ServerError::Config(format!("failed to open store for slug '{slug}': {e}"))
            })?,
    );

    // Per-slug vector index (load existing, else build over the slug's store).
    let vector_config = VectorConfig::default();
    let meta_path = vector_dir.join("unimatrix-vector.meta");
    let vector_index = if meta_path.exists() {
        Arc::new(
            VectorIndex::load(Arc::clone(&store), vector_config, &vector_dir)
                .await
                .map_err(|e| ServerError::Core(CoreError::Vector(e)))?,
        )
    } else {
        Arc::new(
            VectorIndex::new(Arc::clone(&store), vector_config)
                .map_err(|e| ServerError::Core(CoreError::Vector(e)))?,
        )
    };

    // Per-slug subsystems (knowledge isolation, FR-C3).
    let registry = Arc::new(AgentRegistry::new(
        Arc::clone(&store),
        permissive,
        Vec::new(),
    )?);
    registry.bootstrap_defaults()?;
    let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
    // crt-056 ADR-006: `adapt_service` stays PER-SLUG INDEPENDENT STATE (same config).
    // `AdaptConfig::default()` is the resolved adapt value today; #785 would thread it if
    // it becomes operator-configurable. Keep the per-slug construction (NOT shared).
    let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));

    let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
    let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

    // crt-056 ADR-002 CORE CHANGE: build the config-driven per-slug ServiceLayer,
    // mirroring the daemon's own construction (main.rs:880-898) field-for-field with the
    // threaded resolved values. The pre-crt-056 per-slug `CategoryAllowlist::new()` empty
    // default (former line 181) is REPLACED by the threaded operator `categories`; the
    // ONE loaded `nli_handle` is `Arc::clone`d — NEVER `NliServiceHandle::new()` here
    // (C-3, AC-2). `usage_dedup` is per-slug, exactly as the daemon builds its own.
    let usage_dedup = Arc::new(unimatrix_server::infra::usage_dedup::UsageDedup::new());
    let service_layer = ServiceLayer::new(
        Arc::clone(&store),              // store
        Arc::clone(&vector_index),       // vector_index
        Arc::clone(&async_vector_store), // vector_store
        Arc::clone(&store),              // entry_store
        Arc::clone(embed_handle),        // SHARED stateless embed (already shared today)
        Arc::clone(&adapt_service),      // per-slug independent (ADR-006)
        Arc::clone(&audit),
        usage_dedup,
        boosted_categories.clone(), // same RESOLVED set the daemon uses (Gate 3a MUST-CONFIRM)
        Arc::clone(rayon_pool),     // FR-5: shared config-sized pool, NOT size-1
        Arc::clone(nli_handle),     // FR-6/AC-2: the ONE loaded model — Arc::clone, NEVER new()
        nli_top_k,                  // FR-2/FR-3: threaded, not 20-default
        nli_enabled,                // FR-2: config value, NEVER hardcoded false
        Arc::clone(inference_config), // FR-3: resolved fusion/PPR, not ::default()
        Arc::clone(observation_registry), // FR-4: operator domain packs, not builtin-only
        Arc::clone(confidence_params), // FR-3: operator weights, not ::default()
        Arc::clone(categories),     // FR-4: operator allowlist + lifecycle, not ::new()
    );

    // crt-056 ADR-001: hand the config-driven layer to the constructor via `Some(...)`.
    let server = UnimatrixServer::new(
        Arc::clone(&store),
        async_vector_store,
        Arc::clone(embed_handle), // SHARED stateless model handle (OQ-PR-6)
        registry,
        audit,
        Arc::clone(categories), // constructor `categories`: pass the threaded operator set
        Arc::clone(&store),
        Arc::clone(&vector_index),
        adapt_service,
        instructions,
        Some(service_layer),
    );

    Ok(ProjectServerInput {
        slug: slug.clone(),
        store,
        server,
    })
}

/// Per-slug config file name within a slug's data dir (`{base_dir}/{slug}/config.toml`).
/// Shared with Feature B (seeding, #785) — operator hand-places it for Feature A.
///
/// `dead_code`-allowed: the sole caller is the per-slug loop in `main.rs` (vnc-040 Wave 2),
/// landed in a separate wave; this helper ships first. Remove the allow once Wave 2 wires
/// `resolve_slug_config` into the loop.
#[allow(dead_code)]
const PROJECT_CONFIG_NAME: &str = "config.toml";

/// Resolve the per-slug [`UnimatrixConfig`] by overlaying `{base_dir}/{slug}/config.toml`
/// onto the daemon's already-resolved `global` config (vnc-040 C6, ADR-001 #5209).
///
/// Sole owner of the per-slug overlay decision. The THIRD precedence layer atop the
/// established global → project layering (`load_config`), using the IDENTICAL field-level
/// replace discipline (dsn-001 #2286): reuses [`load_single_config`], [`validate_config`],
/// and [`merge_configs`] UNCHANGED — introduces no new load/merge/validate logic.
///
/// - **No file** → [`Cow::Borrowed`]`(global)`: byte-for-byte fallthrough (ADR-002 §4,
///   AC-02, R-03). NO merge, NO load, NO re-derivation — the global config itself is
///   returned, so the single-project / local-UDS majority sees zero behavior change.
/// - **File present** → load → per-file validate (AC-08a) → merge → **post-merge validate**
///   (ADR-003 #5199, SR-01, AC-08b, the #3905 third-layer fix) → [`Cow::Owned`]`(merged)`.
///
/// The post-merge [`validate_config`] is MANDATORY: it runs after [`merge_configs`] and
/// before the merged config is returned, catching cross-field invariants (the
/// `InferenceConfig` sum-of-six fusion-weight constraint, PPR/confidence/custom-preset/size
/// bounds) that EACH file passes alone but the field-by-field merge violates (#3905).
/// Per-file validation alone is provably insufficient for these.
///
/// The reused [`load_single_config`] carries the 64 KiB size cap (#2395) and the
/// `#[cfg(unix)]` `mode() & 0o022` permission check (R-10), now EXERCISED on the new,
/// untrusted per-slug file surface — not assumed. The hash-pin divergence `tracing::warn`
/// (AC-05) is emitted INSIDE [`merge_configs`] unchanged; this helper neither adds nor
/// suppresses it.
///
/// # Errors
///
/// Any load / per-file-validate / post-merge-validate failure returns a
/// [`ServerError::Config`] NAMING the offending slug file — startup fails loud, never a
/// silent request-time fallback (#4583, R-11). No `.unwrap()` / `.expect()` / panic on any
/// path. A missing file is NOT an error (it is the fallthrough sentinel).
///
/// `dead_code`-allowed until Wave 2: the sole caller is the per-slug loop in `main.rs`
/// (vnc-040 Wave 2, landed separately). Remove the allow once the loop calls this.
#[allow(dead_code)]
pub fn resolve_slug_config<'a>(
    base_dir: &Path,
    slug: &ProjectSlug,
    global: &'a UnimatrixConfig,
) -> Result<Cow<'a, UnimatrixConfig>, ServerError> {
    // (1) Probe path — single-site derivation; `slug` is allowlist-validated, so this
    //     CANNOT escape `{base_dir}/{slug}/` (AC-W2-R6, same join as build_project_server).
    let path = base_dir.join(slug.as_str()).join(PROJECT_CONFIG_NAME);

    // (2) NO-FILE ARM — fallthrough sentinel (ADR-002 §4, FR-08, AC-02, R-03).
    //     A metadata probe that is_file (not a bare .exists() that would also accept a
    //     directory). NotFound is NOT an error — it is the global-only path.
    let is_file = std::fs::metadata(&path)
        .map(|m| m.is_file())
        .unwrap_or(false);
    if !is_file {
        // The global config itself — NO merge, NO re-derivation.
        return Ok(Cow::Borrowed(global));
    }

    // (3) FILE-PRESENT ARM — load → per-file validate → merge → post-merge validate.
    tracing::debug!(slug = %slug, path = %path.display(), "resolving per-slug config overlay");

    // 3a. Parse + hardening (REUSE — 64 KiB cap #2395 + #[cfg(unix)] 0o022 check, R-10).
    let slug_file =
        load_single_config(&path).map_err(|e| config_err(slug, &path, &e.to_string()))?;

    // 3b. Per-file validation (FR-01, AC-08a).
    validate_config(&slug_file, &path).map_err(|e| config_err(slug, &path, &e.to_string()))?;

    // 3c. Merge — THIRD precedence layer (FR-01, FR-02). REUSE merge_configs UNCHANGED.
    //     The LIVE signature takes OWNED values; `global` is borrowed, so clone it once to
    //     feed the merge (one clone per slug-with-a-file, startup-only, negligible).
    //     hash-pin global-wins (#4655) + instructions project-wins (config.rs:3863) ride
    //     INSIDE merge_configs.
    let merged = merge_configs(global.clone(), slug_file);

    // 3d. POST-MERGE re-validation (ADR-003, SR-01, FR-07, AC-08b, R-01) — MANDATORY, after
    //     the merge, before return. Catches cross-field violations (fusion-weight sum-of-six,
    //     PPR, confidence, custom-preset, size bounds) each file passes alone (#3905).
    validate_config(&merged, &path).map_err(|e| config_err(slug, &path, &e.to_string()))?;

    // 3e. Return the owned merged config.
    Ok(Cow::Owned(merged))
}

/// Build a slug-named, startup-fatal [`ServerError::Config`] for a per-slug overlay failure
/// (NFR-05, R-11). Every failure path names the offending slug AND its file path so the
/// operator can locate and fix it.
///
/// `dead_code`-allowed until Wave 2 wires `resolve_slug_config` (its only caller).
#[allow(dead_code)]
fn config_err(slug: &ProjectSlug, path: &Path, detail: &str) -> ServerError {
    ServerError::Config(format!(
        "per-slug config for slug '{}' at {}: {detail}",
        slug.as_str(),
        path.display()
    ))
}

#[cfg(test)]
mod slug_config_tests;
