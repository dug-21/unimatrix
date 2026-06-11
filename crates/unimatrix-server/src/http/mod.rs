//! HTTP transport modules for vnc-021.
//!
//! Provides HTTPS transport with static bearer token authentication,
//! path-dispatching router, TLS configuration, health endpoint, and
//! connection-limited listener.

pub(crate) mod auth;
pub(crate) mod cert_provisioner;
pub(crate) mod health;
pub(crate) mod listener;
pub(crate) mod public_url;
pub(crate) mod router;
pub(crate) mod tls;
pub(crate) mod token;

// Re-exports for main.rs (binary crate) HTTP startup wiring.
pub use auth::StaticTokenAuthLayer;
// CertProvisioner (vnc-034, SR-01) — first-boot self-signed cert/key provisioner.
// Wired into the listener startup path in Sub-wave 3.
pub use cert_provisioner::{CertPem, KeyPem, load_or_generate_cert};
pub use listener::start_http_listener;
// C3 public-URL derivation (vnc-034) — single source for bundle base-url,
// cert SANs, and allowed_hosts. Wired into the listener cert provisioning.
pub use public_url::{Env, PublicUrl, derive_public_url};
pub use router::{ObserveContext, PathRouter};
// C4 isolation seam (vnc-034 ADR-003/005) — the single store-resolution + dispatch
// funnel. Constructed in the listener wiring so the served store reaches MCP only
// via `resolve_store`/`adapter_for` (no bypass). Wave 2 swaps `DefaultResolver` ->
// `MultiProjectRouter` at the same `SlugRouter::new` call site; the resolver owns
// per-key MCP dispatch (the fixed single-project router is gone — funnel-elimination).
pub use router::{
    DefaultResolver, MultiProjectRouter, ProjectKey, ProjectServerInput, ProjectSlug, RouteError,
    SlugRouter, StoreResolver,
};
pub use tls::build_tls_acceptor;
// C2 fingerprint oracle (vnc-034, ADR-002) — consumed by client-bundle wiring and the
// cross-stack parity corpus test (tests/fingerprint_parity.rs).
pub use tls::{FP_PREFIX, fingerprint_leaf_der, leaf_der_from_pem};
pub use token::load_or_generate_token;
