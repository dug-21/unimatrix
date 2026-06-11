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
//! The store-resolution funnel itself ([`DefaultResolver`] +
//! `resolve_store(&ProjectKey::Default)`) is constructed inline in the listener
//! wiring so the served `Arc<Store>` reaches MCP only THROUGH the seam (ADR-003,
//! FR-X5 — no bypass). The per-request `PathRouter -> SlugRouter` insertion is a
//! router.rs-scoped follow-up (see the agent report); it is not wired here.

use std::path::Path;
use std::sync::Arc;

use tokio_rustls::TlsAcceptor;

use unimatrix_adapt::{AdaptConfig, AdaptationService};
use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{CoreError, VectorAdapter, VectorConfig};
use unimatrix_server::error::ServerError;
use unimatrix_server::http::{
    Env, ProjectServerInput, ProjectSlug, PublicUrl, build_tls_acceptor, derive_public_url,
    load_or_generate_cert,
};
use unimatrix_server::infra::audit::AuditLog;
use unimatrix_server::infra::categories::CategoryAllowlist;
use unimatrix_server::infra::config::TlsConfig;
use unimatrix_server::infra::embed_handle::EmbedServiceHandle;
use unimatrix_server::infra::registry::AgentRegistry;
use unimatrix_server::server::UnimatrixServer;
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
pub async fn build_project_server(
    base_dir: &Path,
    slug: &ProjectSlug,
    embed_handle: &Arc<EmbedServiceHandle>,
    permissive: bool,
    instructions: Option<String>,
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
    let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));
    let categories = Arc::new(CategoryAllowlist::new());

    let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
    let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

    let server = UnimatrixServer::new(
        Arc::clone(&store),
        async_vector_store,
        Arc::clone(embed_handle), // SHARED stateless model handle (OQ-PR-6)
        registry,
        audit,
        categories,
        Arc::clone(&store),
        vector_index,
        adapt_service,
        instructions,
    );

    Ok(ProjectServerInput {
        slug: slug.clone(),
        store,
        server,
    })
}
