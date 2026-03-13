//! Cross-cutting infrastructure modules.
//!
//! Contains shared utilities used by both MCP and UDS transports:
//! agent registry, session management, audit logging, validation,
//! content scanning, and other infrastructure concerns.

pub mod audit;
pub mod categories;
pub mod coherence;
pub mod contradiction;
pub mod embed_handle;
pub mod outcome_tags;
pub mod pidfile;
pub mod registry;
pub mod scanning;
pub mod session;
pub mod shutdown;
pub mod timeout;
pub mod usage_dedup;
pub mod validation;
