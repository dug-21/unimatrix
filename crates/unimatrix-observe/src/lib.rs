#![forbid(unsafe_code)]

//! Observation pipeline for Unimatrix: JSONL parsing, feature attribution,
//! hotspot detection, metric computation, and report assembly.
//!
//! This crate has no dependency on unimatrix-store or unimatrix-server (ADR-001).

pub mod attribution;
pub mod baseline;
pub mod detection;
pub mod error;
pub mod files;
pub mod metrics;
pub mod parser;
pub mod report;
pub mod types;

// Re-exports for public API
pub use attribution::attribute_sessions;
pub use baseline::{compare_to_baseline, compute_baselines};
pub use detection::{default_rules, detect_hotspots, DetectionRule};
pub use error::{ObserveError, Result};
pub use files::{discover_sessions, identify_expired, observation_dir, scan_observation_stats};
pub use metrics::compute_metric_vector;
pub use parser::{parse_session_file, parse_timestamp};
pub use report::build_report;
pub use types::{
    BaselineComparison, BaselineEntry, BaselineSet, BaselineStatus,
    EntryAnalysis,
    EvidenceRecord, HookType, HotspotCategory, HotspotFinding, MetricVector, ObservationRecord,
    ObservationStats, ParsedSession, PhaseMetrics, RetrospectiveReport, Severity, SessionFile,
    UniversalMetrics, deserialize_metric_vector, serialize_metric_vector,
};
