#![forbid(unsafe_code)]

pub(crate) mod analytics;
pub mod embedding;
mod error;
mod hash;
pub mod pool_config;
mod schema;

pub mod counters;
mod db;
pub mod metrics;
pub mod migration;
mod migration_compat;
pub mod read;
mod write;
mod write_ext;

mod audit;
pub mod cycle_review_index;
pub mod goal_clusters;
pub mod injection_log;
pub mod observations;
pub mod query_log;
pub mod registry;
pub mod retention;
pub mod sessions;
pub mod signal;
pub mod topic_deliveries;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

// Re-exports: embedding blob serialization helpers (ADR-001, crt-043)
pub use embedding::{decode_goal_embedding, encode_goal_embedding};
// Re-exports: goal_clusters struct (crt-046)
pub use goal_clusters::GoalClusterRow;

// Re-exports: schema types (backend-agnostic)
pub use error::{PoolKind, Result, StoreError};
pub use hash::compute_content_hash;
pub use injection_log::InjectionLogRecord;
pub use metrics::{MetricVector, PhaseMetrics, UNIVERSAL_METRICS_FIELDS, UniversalMetrics};
#[doc(hidden)]
pub use query_log::PhaseOutcomeRow;
pub use query_log::{PhaseFreqRow, QueryLogRecord};
pub use read::{
    CO_ACCESS_GRAPH_MIN_COUNT, ContradictEdgeRow, EDGE_SOURCE_CO_ACCESS,
    EDGE_SOURCE_COSINE_SUPPORTS, EDGE_SOURCE_NLI, EDGE_SOURCE_S1, EDGE_SOURCE_S2, EDGE_SOURCE_S8,
    GraphCohesionMetrics, GraphEdgeRow, StatusAggregates,
};
pub use retention::{CycleGcStats, UnattributedGcStats};
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
pub use cycle_review_index::{
    CurationBaselineRow, CurationSnapshotRow, CycleReviewRecord, SUMMARY_SCHEMA_VERSION,
};
pub use db::SqlxStore;
pub use observations::{ObservationRow, ShadowEvalRow};
pub use pool_config::{
    ANALYTICS_QUEUE_CAPACITY, PoolConfig, READ_POOL_ACQUIRE_TIMEOUT, WRITE_POOL_ACQUIRE_TIMEOUT,
};
