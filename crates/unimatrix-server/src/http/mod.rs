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
