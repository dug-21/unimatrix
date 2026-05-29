//! HTTP transport modules for vnc-021.
//!
//! Provides HTTPS transport with static bearer token authentication,
//! path-dispatching router, TLS configuration, health endpoint, and
//! connection-limited listener.

pub(crate) mod auth;
pub(crate) mod health;
pub(crate) mod listener;
pub(crate) mod router;
pub(crate) mod tls;
pub(crate) mod token;

// Re-exports for main.rs (binary crate) HTTP startup wiring.
pub use auth::StaticTokenAuthLayer;
pub use listener::start_http_listener;
pub use router::{ObserveContext, PathRouter, ProjectRouter};
pub use tls::build_tls_acceptor;
pub use token::load_or_generate_token;
