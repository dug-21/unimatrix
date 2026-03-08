#![forbid(unsafe_code)]

//! Unimatrix shared business logic.
//!
//! Contains modules extracted from `unimatrix-server` for shared use by both
//! MCP tool handlers (stdio) and UDS hook handlers:
//! - `confidence` — confidence score computation
//! - `coaccess` — co-access pair generation and boost computation
//! - `project` — project root detection, hash, data directory management
//!
//! Also contains col-006 additions:
//! - `wire` — wire protocol types for hook IPC
//! - `transport` — Transport trait and LocalTransport
//! - `auth` — peer credential extraction and authentication
//! - `event_queue` — graceful degradation event queue

pub mod project;
pub mod confidence;
pub mod coaccess;
pub mod wire;
pub mod transport;
pub mod auth;
pub mod event_queue;

#[cfg(any(test, feature = "test-support"))]
pub mod test_scenarios;
