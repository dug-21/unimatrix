#![forbid(unsafe_code)]

//! Unimatrix MCP knowledge server library.
//!
//! This crate provides the MCP server that exposes Unimatrix's knowledge engine
//! to AI agents via stdio transport. Modules are public for integration testing.

pub mod audit;
pub mod categories;
pub mod coaccess;
pub mod coherence;
pub mod confidence;
pub mod contradiction;
pub mod embed_handle;
pub mod error;
pub mod identity;
pub mod outcome_tags;
pub mod pidfile;
pub mod project;
pub mod registry;
pub mod response;
pub mod scanning;
pub mod server;
pub mod shutdown;
pub mod tools;
pub mod usage_dedup;
pub mod validation;
