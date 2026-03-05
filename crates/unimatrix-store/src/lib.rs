#![forbid(unsafe_code)]

mod schema;
mod error;
mod hash;
pub mod migrate;

// Backend-specific modules: redb (default) or SQLite (feature-gated)
#[cfg(not(feature = "backend-sqlite"))]
mod db;
#[cfg(not(feature = "backend-sqlite"))]
mod counter;
#[cfg(not(feature = "backend-sqlite"))]
mod migration;
#[cfg(not(feature = "backend-sqlite"))]
mod write;
#[cfg(not(feature = "backend-sqlite"))]
mod read;
#[cfg(not(feature = "backend-sqlite"))]
mod query;

#[cfg(feature = "backend-sqlite")]
mod sqlite;

// Type modules (shared types + serialization, compiled under both backends).
// Store method impls are cfg-gated inside these files (redb path) or in
// sqlite/{sessions,injection_log,signal}.rs (SQLite path).
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

// Re-exports: redb backend table definitions and helpers
#[cfg(not(feature = "backend-sqlite"))]
pub use schema::{AGENT_REGISTRY, AUDIT_LOG, COUNTERS};
#[cfg(not(feature = "backend-sqlite"))]
pub use schema::{ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, FEATURE_ENTRIES, OUTCOME_INDEX};
#[cfg(not(feature = "backend-sqlite"))]
pub use schema::{SIGNAL_QUEUE, SESSIONS, INJECTION_LOG};
#[cfg(not(feature = "backend-sqlite"))]
pub use schema::CO_ACCESS;
#[cfg(not(feature = "backend-sqlite"))]
pub use counter::{next_entry_id, increment_counter};
#[cfg(not(feature = "backend-sqlite"))]
pub use db::Store;

// Re-exports: SQLite backend
#[cfg(feature = "backend-sqlite")]
pub use sqlite::Store;
#[cfg(feature = "backend-sqlite")]
pub use sqlite::{SqliteReadTransaction, SqliteWriteTransaction};

// Re-exports: SQLite compat layer (table definitions, counter helpers, guards, handles, traits)
// These are consumed by unimatrix-server under backend-sqlite but unused within the store crate.
#[cfg(feature = "backend-sqlite")]
#[allow(unused_imports)]
pub use sqlite::{
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
    // Typed handle structs
    TableU64Blob, TableStrU64, TableStrBlob, TableStrU64Comp,
    TableU64U64Comp, TableU8U64Comp, TableU64U64, MultimapStrU64,
    // Traits for open_table dispatch
    TableSpec, MultimapSpec,
    // Range result wrapper
    RangeResult,
};
