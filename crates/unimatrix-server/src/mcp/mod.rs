//! MCP transport layer modules.
//!
//! Contains MCP tool handlers, identity resolution, response formatting,
//! and ToolContext for handler ceremony reduction.

pub(crate) mod activity_fold_handler;
pub(crate) mod context;
pub(crate) mod distill_handler;
pub(crate) mod edge_write;
pub(crate) mod graph_read;
pub mod identity;
pub mod knowledge_reuse;
pub mod response;
mod serde_util;
pub mod tools;

// ADR-004 vnc-018: EdgeRecord re-exported here for #597/#598 consumers.
pub use graph_read::EdgeRecord;
