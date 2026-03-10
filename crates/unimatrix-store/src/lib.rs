#![forbid(unsafe_code)]

mod error;
mod hash;
mod schema;

pub mod counters;
mod db;
pub mod metrics;
mod migration;
mod migration_compat;
pub mod read;
mod txn;
mod write;
mod write_ext;

pub mod injection_log;
pub mod query_log;
pub mod sessions;
pub mod signal;
pub mod topic_deliveries;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

// Re-export rusqlite for downstream crates that use direct SQL
pub use rusqlite;

// Re-exports: schema types (backend-agnostic)
pub use error::{Result, StoreError};
pub use hash::compute_content_hash;
pub use injection_log::InjectionLogRecord;
pub use metrics::{MetricVector, PhaseMetrics, UNIVERSAL_METRICS_FIELDS, UniversalMetrics};
pub use query_log::QueryLogRecord;
pub use read::StatusAggregates;
pub use schema::status_counter_key;
pub use schema::{AgentRecord, AuditEvent, Capability, Outcome, TrustLevel};
pub use schema::{CoAccessRecord, co_access_key};
pub use schema::{DatabaseConfig, EntryRecord, NewEntry, QueryFilter, Status, TimeRange};
pub use sessions::{
    DELETE_THRESHOLD_SECS, GcStats, SessionLifecycleStatus, SessionRecord, TIMED_OUT_THRESHOLD_SECS,
};
pub use signal::{SignalRecord, SignalSource, SignalType};
pub use topic_deliveries::TopicDeliveryRecord;

// Re-exports: SQLite backend
pub use db::Store;
pub use txn::SqliteWriteTransaction;
