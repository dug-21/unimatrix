#![forbid(unsafe_code)]

//! Observation pipeline for Unimatrix: hotspot detection, metric computation,
//! report assembly, and knowledge extraction. Data sourced via ObservationSource
//! trait (col-012). Extraction rules require unimatrix-store (col-013 ADR-001).

pub mod attribution;
pub mod baseline;
pub mod detection;
pub mod domain;
pub mod error;
pub mod extraction;
pub mod metrics;
pub mod phase_narrative;
pub mod report;
pub mod session_metrics;
pub mod source;
pub mod synthesis;
pub mod types;

// Re-exports for public API
pub use attribution::{attribute_sessions, extract_topic_signal};
pub use baseline::{compare_to_baseline, compute_baselines};
pub use detection::{DetectionRule, default_rules, detect_hotspots};
pub use domain::{DomainPack, DomainPackRegistry};
pub use error::{ObserveError, Result};
pub use metrics::compute_metric_vector;
pub use phase_narrative::build_phase_narrative;
pub use report::{build_report, recommendations_for_hotspots};
pub use session_metrics::{compute_context_reload_pct, compute_session_summaries};
pub use source::ObservationSource;
pub use synthesis::synthesize_narratives;
pub use types::{
    AttributionMetadata, BaselineComparison, BaselineEntry, BaselineSet, BaselineStatus,
    CurationBaselineComparison, CurationHealthBlock, CurationHealthSummary, CurationSnapshot,
    CycleEventRecord, EntryAnalysis, EntryRef, EvidenceCluster, EvidenceRecord,
    FeatureKnowledgeReuse, GateResult, HotspotCategory, HotspotFinding, HotspotNarrative,
    MetricVector, ObservationRecord, ObservationStats, ParsedSession, PhaseCategoryComparison,
    PhaseCategoryDist, PhaseMetrics, PhaseNarrative, PhaseStats, Recommendation,
    RetrospectiveReport, SessionSummary, Severity, ToolDistribution, TrendDirection,
    UniversalMetrics,
};
