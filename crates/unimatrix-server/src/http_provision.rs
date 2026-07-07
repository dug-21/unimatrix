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

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

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
use unimatrix_server::infra::config::{InferenceConfig, RetentionConfig, StoreConfig, TlsConfig};
use unimatrix_server::infra::embed_handle::EmbedServiceHandle;
use unimatrix_server::infra::nli_handle::NliServiceHandle;
use unimatrix_server::infra::rayon_pool::RayonPool;
use unimatrix_server::infra::registry::AgentRegistry;
use unimatrix_server::infra::session::{HeldBufferScan, SessionRegistry};
use unimatrix_server::infra::transcript_activity::SignatureScanner;
use unimatrix_server::infra::transcript_hold::{AuditLogPurgeSink, TranscriptHold};
use unimatrix_server::server::{PendingEntriesAnalysis, UnimatrixServer};
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
    // vnc-046 Wave 2 (ADR-002 P3 + P1 scanner): the config-snapshot params-at-end, every
    // value an `Arc::clone`/compile of THIS slug's RESOLVED config (main.rs:982/985/989 +
    // the per-slug scanner compiled at the call site from `r.transcript_signals`). A
    // missing field is a compile error at the call site, never a silent test-default
    // fallback (crt-056 anti-Defect-1). The scanner is compiled at the call site (not here)
    // because this fn takes resolved config as explicit params, not `r` itself (WARN-1).
    store_config: &Arc<StoreConfig>,
    retention_config: &Arc<RetentionConfig>,
    signal_class_names: &Arc<Vec<String>>,
    signature_scanner: &Arc<SignatureScanner>,
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
        // Graceful degradation (Architectural Principle 5): a torn/missing dump
        // (e.g. SIGKILL mid-dump leaving a meta over a missing graph) must NOT
        // hard-abort this slug's boot. Fall back to an empty index; the capped
        // run_maintenance heal pass (services/status.rs) re-populates it from
        // the store on the next tick. See GH-824 / lesson #5272.
        match VectorIndex::load(Arc::clone(&store), vector_config.clone(), &vector_dir).await {
            Ok(idx) => Arc::new(idx),
            Err(e) => {
                tracing::warn!(
                    slug = %slug,
                    vector_dir = %vector_dir.display(),
                    error = %e,
                    "failed to load per-slug vector index; booting empty and \
                     deferring to maintenance heal pass"
                );
                Arc::new(
                    VectorIndex::new(Arc::clone(&store), vector_config)
                        .map_err(|e| ServerError::Core(CoreError::Vector(e)))?,
                )
            }
        }
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
    // vnc-046 (ADR-002): `Arc::clone(&audit)` (was a move) keeps this slug's audit alive
    // for the TranscriptHold built below; `mut` so the P1/P3 fields can be set on it.
    let mut server = UnimatrixServer::new(
        Arc::clone(&store),
        async_vector_store,
        Arc::clone(embed_handle), // SHARED stateless model handle (OQ-PR-6)
        registry,
        Arc::clone(&audit),
        Arc::clone(categories), // constructor `categories`: pass the threaded operator set
        Arc::clone(&store),
        Arc::clone(&vector_index),
        adapt_service,
        instructions,
        Some(service_layer),
    );

    // ── vnc-046 ADR-002 P1: per-slug registry+hold+scanner TRIPLE + pending ──────────────
    // Full construction parity with the daemon path (main.rs:830-861). `build_project_server`
    // set NONE of these before, so `UnimatrixServer::new`'s test-defaults were read at runtime
    // — the #930 split-brain (empty registry / unshared default hold) + a default
    // `PendingEntriesAnalysis` + an empty `SignatureScanner` that yields all-zero
    // `signal_class_counts` (hollow FR-9, AC-07 parity break). The registry and hold move as a
    // constructed PAIR (F1/SR-03): registry-alone splits the purge gate → held buffers never
    // purge → unbounded memory growth. Set INSIDE this fn, so they land BEFORE the
    // main.rs:1229 tick loop clones them (FR-3) — no reorder at the call site.
    //
    // (1) This slug's TranscriptHold over the slug's own audit (mirror main.rs:830-838).
    let transcript_hold = Arc::new(TranscriptHold::new(
        retention_config.transcript_hold_max_sessions,
        Arc::new(AuditLogPurgeSink::new(Arc::clone(&audit))),
    ));
    // (2) This slug's SessionRegistry PAIRED with the hold + the per-slug scanner (mirror
    //     main.rs:846-853): cap + hold + scanner — the full daemon triple, not a two-of-three
    //     subset. The transcript cap now actually reaches this slug's buffers (vnc-040
    //     [retention], N1 gap). The scanner is per-slug — compiled at the call site from THIS
    //     slug's resolved `r.transcript_signals` (ADR-002 OQ-2), so accumulated counts match
    //     the per-slug class NAMES set in P3 below (a global scanner would count against a
    //     different class set — the names-vs-counts split-brain #930 exists to close).
    server.session_registry = Arc::new(
        SessionRegistry::with_transcript_cap(retention_config.transcript_buffer_max_bytes)
            .with_transcript_hold(Arc::clone(&transcript_hold) as Arc<dyn HeldBufferScan>)
            .with_signature_scanner(Arc::clone(signature_scanner)),
    );
    // (3) Fresh per-slug pending accumulator (mirror main.rs:861).
    server.pending_entries_analysis = Arc::new(Mutex::new(PendingEntriesAnalysis::new()));
    // (4) The hold — the SAME instance wired into the registry above (PAIR, never omit; F1/SR-03).
    server.transcript_hold = transcript_hold;

    // ── vnc-046 ADR-002 P3: set the 5 config-snapshot server fields (mirror main.rs:978-990) ─
    // `observation_registry` + `inference_config` are already params (threaded for the
    // ServiceLayer) — also assign them to the server fields (they only fed the layer before).
    // `store_config` / `retention_config` / `signal_class_names` are the 3 new params-at-end.
    server.observation_registry = Arc::clone(observation_registry);
    server.inference_config = Arc::clone(inference_config);
    server.store_config = Arc::clone(store_config);
    server.retention_config = Arc::clone(retention_config);
    server.transcript_signal_class_names = Arc::clone(signal_class_names);

    Ok(ProjectServerInput {
        slug: slug.clone(),
        store,
        server,
        // #823: carry the per-slug vector dir into shutdown so this slug's HNSW
        // index is dumped to its OWN `{slug}/vector/` (it was a dropped local).
        vector_dir,
    })
}

/// Per-slug config overlay resolution — split into a focused module (vnc-046 Wave 2,
/// 500-line cap). `resolve_slug_config` is re-exported so the `main.rs` per-slug loop
/// call site (`http_provision::resolve_slug_config`) is unchanged.
mod slug_config;
pub use slug_config::resolve_slug_config;

#[cfg(test)]
mod boot_fallback_tests;

#[cfg(test)]
mod construction_parity_tests;
