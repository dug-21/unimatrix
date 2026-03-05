#![forbid(unsafe_code)]

//! Unimatrix MCP knowledge server library.
//!
//! This crate provides the MCP server that exposes Unimatrix's knowledge engine
//! to AI agents via stdio transport. Modules are public for integration testing.
//!
//! Module groups (vnc-008):
//! - `infra/`: Cross-cutting infrastructure (audit, registry, session, etc.)
//! - `mcp/`: MCP transport layer (tools, identity, response formatting)
//! - `uds/`: UDS transport layer (hook listener, hook subcommand)
//! - `services/`: Transport-agnostic business logic
//!
//! The `confidence`, `coaccess`, and `project` modules are re-exported from
//! `unimatrix-engine` for backward compatibility (col-006 extraction).

pub use unimatrix_engine::confidence;
pub use unimatrix_engine::coaccess;
pub use unimatrix_engine::project;

pub mod background;
pub mod infra;
pub mod mcp;
pub mod uds;
pub mod error;
pub mod server;
pub mod services;

// Re-exports for external consumers (main.rs, integration tests).
pub use uds::listener as uds_listener;
