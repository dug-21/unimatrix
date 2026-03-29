//! MCP transport layer modules.
//!
//! Contains MCP tool handlers, identity resolution, response formatting,
//! and ToolContext for handler ceremony reduction.

pub(crate) mod context;
pub mod identity;
pub mod knowledge_reuse;
pub mod response;
mod serde_util;
pub mod tools;
