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

use tokio_rustls::TlsAcceptor;

use unimatrix_server::error::ServerError;
use unimatrix_server::http::{
    Env, PublicUrl, build_tls_acceptor, derive_public_url, load_or_generate_cert,
};
use unimatrix_server::infra::config::TlsConfig;

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
