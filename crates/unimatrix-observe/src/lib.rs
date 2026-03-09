#![forbid(unsafe_code)]

//! Observation pipeline for Unimatrix: hotspot detection, metric computation,
//! report assembly, and knowledge extraction. Data sourced via ObservationSource
//! trait (col-012). Extraction rules require unimatrix-store (col-013 ADR-001).

pub mod attribution;
pub mod baseline;
pub mod detection;
pub mod error;
pub mod extraction;
pub mod metrics;
pub mod report;
pub mod source;
pub mod synthesis;
pub mod types;

// Re-exports for public API
pub use attribution::{attribute_sessions, extract_topic_signal};
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
    UniversalMetrics,
};
