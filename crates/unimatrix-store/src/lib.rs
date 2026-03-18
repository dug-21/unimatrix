#![forbid(unsafe_code)]

pub(crate) mod analytics;
mod error;
mod hash;
pub mod pool_config;
mod schema;

pub mod counters;
mod db;
pub mod metrics;
mod migration;
mod migration_compat;
pub mod read;
mod write;
mod write_ext;

mod audit;
pub mod injection_log;
pub mod observations;
pub mod query_log;
pub mod registry;
pub mod sessions;
pub mod signal;
pub mod topic_deliveries;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

// Re-exports: schema types (backend-agnostic)
pub use error::{PoolKind, Result, StoreError};
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

// Re-exports: sqlx backend
pub use analytics::AnalyticsWrite;
pub use db::SqlxStore;
pub use observations::{ObservationRow, ShadowEvalRow};
pub use pool_config::{
    ANALYTICS_QUEUE_CAPACITY, PoolConfig, READ_POOL_ACQUIRE_TIMEOUT, WRITE_POOL_ACQUIRE_TIMEOUT,
};
