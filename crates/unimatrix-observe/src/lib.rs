#![forbid(unsafe_code)]

//! Observation pipeline for Unimatrix: hotspot detection, metric computation,
//! and report assembly. Data sourced via ObservationSource trait (col-012).
//!
//! This crate has no dependency on unimatrix-store or unimatrix-server (ADR-001).

pub mod attribution;
pub mod baseline;
pub mod detection;
pub mod error;
pub mod metrics;
pub mod report;
pub mod source;
pub mod synthesis;
pub mod types;

// Re-exports for public API
pub use attribution::attribute_sessions;
pub use baseline::{compare_to_baseline, compute_baselines};
pub use detection::{default_rules, detect_hotspots, DetectionRule};
pub use error::{ObserveError, Result};
pub use metrics::compute_metric_vector;
pub use report::{build_report, recommendations_for_hotspots};
pub use source::ObservationSource;
pub use synthesis::synthesize_narratives;
pub use types::{
    BaselineComparison, BaselineEntry, BaselineSet, BaselineStatus,
    EntryAnalysis, EvidenceCluster,
    EvidenceRecord, HookType, HotspotCategory, HotspotFinding, HotspotNarrative,
    MetricVector, ObservationRecord,
    ObservationStats, ParsedSession, PhaseMetrics, Recommendation, RetrospectiveReport,
    Severity,
    UniversalMetrics, deserialize_metric_vector, serialize_metric_vector,
};
