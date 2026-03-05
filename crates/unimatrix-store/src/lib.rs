#![forbid(unsafe_code)]

mod schema;
mod error;
mod hash;

mod db;
mod txn;
mod read;
mod write;
mod write_ext;
mod migration;
mod tables;
mod handles;
mod dispatch;

pub mod signal;
pub mod sessions;
pub mod injection_log;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

// Re-exports: schema types (backend-agnostic)
pub use schema::{EntryRecord, Status, NewEntry, QueryFilter, TimeRange, DatabaseConfig};
pub use schema::{serialize_entry, deserialize_entry, status_counter_key};
pub use schema::{CoAccessRecord, co_access_key, serialize_co_access, deserialize_co_access};
pub use hash::compute_content_hash;
pub use error::{StoreError, Result};
pub use signal::{SignalRecord, SignalType, SignalSource, serialize_signal, deserialize_signal};
pub use sessions::{SessionRecord, SessionLifecycleStatus, GcStats, TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS};
pub use injection_log::InjectionLogRecord;

// Re-exports: SQLite backend
pub use db::Store;
pub use txn::{SqliteReadTransaction, SqliteWriteTransaction};

// Re-exports: compat layer (table definitions, counter helpers, guards, handles, traits)
#[allow(unused_imports)]
pub use tables::{
    // Table definition constants
    ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX,
    STATUS_INDEX, VECTOR_MAP, COUNTERS, OUTCOME_INDEX, AUDIT_LOG,
    AGENT_REGISTRY, FEATURE_ENTRIES, CO_ACCESS, SIGNAL_QUEUE,
    SESSIONS, INJECTION_LOG,
    // Counter helpers
    next_entry_id, increment_counter, decrement_counter,
    // Table definition types
    SqliteTableDef, SqliteMultimapDef,
    // Guard types
    BlobGuard, U64Guard, UnitGuard, CompositeKeyGuard, U64KeyGuard,
    // Range result wrapper
    RangeResult,
};
#[allow(unused_imports)]
pub use handles::{
    TableU64Blob, TableStrU64, TableStrBlob, TableStrU64Comp,
    TableU64U64Comp, TableU8U64Comp, TableU64U64, MultimapStrU64,
};
#[allow(unused_imports)]
pub use dispatch::{TableSpec, MultimapSpec};
