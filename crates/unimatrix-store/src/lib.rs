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
pub use schema::{ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, FEATURE_ENTRIES};
pub use schema::{serialize_entry, deserialize_entry, status_counter_key};
pub use schema::{CO_ACCESS, CoAccessRecord, co_access_key, serialize_co_access, deserialize_co_access};
pub use hash::compute_content_hash;
pub use counter::{next_entry_id, increment_counter};
pub use db::Store;
pub use error::{StoreError, Result};
