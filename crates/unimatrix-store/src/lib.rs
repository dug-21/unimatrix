#![forbid(unsafe_code)]

mod schema;
mod error;
mod db;
mod counter;
mod hash;
mod migration;
mod write;
mod read;
mod query;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

pub use schema::{EntryRecord, Status, NewEntry, QueryFilter, TimeRange, DatabaseConfig};
pub use schema::{AGENT_REGISTRY, AUDIT_LOG, COUNTERS};
pub use db::Store;
pub use error::{StoreError, Result};
