#![forbid(unsafe_code)]

mod schema;
mod error;
mod hash;

mod db;
mod txn;
pub mod read;
mod write;
mod write_ext;
mod migration;
pub mod counters;
mod migration_compat;
pub mod metrics;

pub mod signal;
pub mod sessions;
pub mod injection_log;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

// Re-export rusqlite for downstream crates that use direct SQL
pub use rusqlite;

// Re-exports: schema types (backend-agnostic)
pub use schema::{EntryRecord, Status, NewEntry, QueryFilter, TimeRange, DatabaseConfig};
pub use schema::{status_counter_key};
pub use schema::{CoAccessRecord, co_access_key};
pub use schema::{AgentRecord, TrustLevel, Capability, AuditEvent, Outcome};
pub use hash::compute_content_hash;
pub use error::{StoreError, Result};
pub use read::StatusAggregates;
pub use signal::{SignalRecord, SignalType, SignalSource};
pub use sessions::{SessionRecord, SessionLifecycleStatus, GcStats, TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS};
pub use injection_log::InjectionLogRecord;
pub use metrics::{MetricVector, UniversalMetrics, PhaseMetrics, UNIVERSAL_METRICS_FIELDS};

// Re-exports: SQLite backend
pub use db::Store;
pub use txn::SqliteWriteTransaction;
