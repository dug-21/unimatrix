#![forbid(unsafe_code)]

//! Unimatrix MCP knowledge server library.
//!
//! This crate provides the MCP server that exposes Unimatrix's knowledge engine
//! to AI agents via stdio transport. Modules are public for integration testing.
//!
//! The `confidence`, `coaccess`, and `project` modules are re-exported from
//! `unimatrix-engine` for backward compatibility (col-006 extraction).

pub use unimatrix_engine::confidence;
pub use unimatrix_engine::coaccess;
pub use unimatrix_engine::project;

pub mod audit;
pub mod categories;
pub mod coherence;
pub mod contradiction;
pub mod embed_handle;
pub mod error;
pub mod hook;
pub mod identity;
pub mod outcome_tags;
pub mod pidfile;
pub mod registry;
pub mod response;
pub mod scanning;
pub mod server;
pub mod session;
pub mod shutdown;
pub mod tools;
pub mod uds_listener;
pub mod usage_dedup;
pub mod validation;
