#![forbid(unsafe_code)]

//! Observation pipeline for Unimatrix: hotspot detection, metric computation,
//! report assembly, and knowledge extraction. Data sourced via ObservationSource
//! trait (col-012). Extraction rules require unimatrix-store (col-013 ADR-001).

pub mod attribution;
pub mod baseline;
pub mod cycle_aggregates;
pub mod detection;
pub mod distill;
pub mod domain;
pub mod error;
pub mod extraction;
pub mod fail_loud_guard;
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
pub use cycle_aggregates::{
    PhaseAggregates, SessionOutcome, is_rework_outcome, populate_rank_1_2_3,
    reckon_knowledge_reuse_served, reckon_phase_aggregates, reckon_rework_ratio,
};
pub use detection::{DetectionRule, default_rules, detect_hotspots};
pub use domain::{DomainPack, DomainPackRegistry};
pub use error::{ObserveError, Result};
pub use fail_loud_guard::{
    CycleAggregates, CycleContext, MetricAvailability, compute_availability, render_context_reload,
    render_metric, render_metrics_block, render_ratio,
};
pub use metrics::compute_metric_vector;
pub use phase_narrative::build_phase_narrative;
pub use report::{build_report, recommendations_for_hotspots};
pub use session_metrics::{
    compute_context_reload_pct, compute_session_summaries, normalize_tool_name,
};
pub use source::ObservationSource;
pub use synthesis::synthesize_narratives;
pub use types::{
    AttributionMetadata, BaselineComparison, BaselineEntry, BaselineSet, BaselineStatus,
    CandidateProvenance, CurationBaselineComparison, CurationHealthBlock, CurationHealthSummary,
    CurationSnapshot, CycleEventRecord, EntryAnalysis, EntryRef, EvidenceCluster, EvidenceRecord,
    FamilyHint, FeatureKnowledgeReuse, GateResult, HotspotCategory, HotspotFinding,
    HotspotNarrative, MetricVector, ObservationRecord, ObservationStats, ParsedSession,
    PhaseCategoryComparison, PhaseCategoryDist, PhaseMetrics, PhaseNarrative, PhaseStats,
    Recommendation, RetrospectiveReport, SessionLossInfo, SessionSummary, Severity,
    ToolDistribution, TranscriptCandidate, TranscriptCandidatesSection, TrendDirection,
    UniversalMetrics,
};
