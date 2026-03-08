//! Shared types for the observation pipeline.
//!
//! Core observation types (HookType, ObservationRecord, ParsedSession, ObservationStats)
//! are defined in unimatrix-core and re-exported here for backward compatibility (col-013 ADR-002).

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// Re-export core observation types for backward compatibility (col-013 ADR-002)
pub use unimatrix_core::{HookType, ObservationRecord, ObservationStats, ParsedSession};

// Re-export metric types from unimatrix-store for backward compatibility (nxs-009 ADR-001)
pub use unimatrix_store::{MetricVector, UniversalMetrics, PhaseMetrics};

/// Hotspot category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HotspotCategory {
    Agent,
    Friction,
    Session,
    Scope,
}

/// Finding severity level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// Concrete evidence supporting a hotspot finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRecord {
    /// Description of what this evidence shows.
    pub description: String,
    /// Timestamp of the evidence event (epoch millis).
    pub ts: u64,
    /// Tool involved, if any.
    pub tool: Option<String>,
    /// Detailed information.
    pub detail: String,
}

/// A single hotspot finding from a detection rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotspotFinding {
    /// Category of the hotspot.
    pub category: HotspotCategory,
    /// Severity level.
    pub severity: Severity,
    /// Name of the rule that produced this finding.
    pub rule_name: String,
    /// Human-readable claim about the finding.
    pub claim: String,
    /// The measured value that triggered the finding.
    pub measured: f64,
    /// The threshold that was exceeded.
    pub threshold: f64,
    /// Concrete evidence records.
    pub evidence: Vec<EvidenceRecord>,
}

/// Baseline comparison status for a metric (ADR-003).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BaselineStatus {
    /// Current value within normal range.
    Normal,
    /// Current value exceeds mean + 1.5 * stddev.
    Outlier,
    /// Historical values have zero variance (all identical, non-zero mean).
    NoVariance,
    /// Historical mean and stddev are both zero; current is non-zero.
    NewSignal,
}

/// Statistical summary for one metric across historical data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineEntry {
    /// Arithmetic mean.
    pub mean: f64,
    /// Population standard deviation.
    pub stddev: f64,
    /// Number of data points used.
    pub sample_count: usize,
}

/// Computed statistical baselines for all metrics across historical feature retrospectives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineSet {
    /// Per-metric baselines for universal metrics (key = metric name).
    pub universal: HashMap<String, BaselineEntry>,
    /// Phase-specific baselines (outer key = phase name, inner key = metric name).
    pub phases: HashMap<String, HashMap<String, BaselineEntry>>,
}

/// One metric's current value compared to its historical baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineComparison {
    /// Name of the metric.
    pub metric_name: String,
    /// Current feature's value.
    pub current_value: f64,
    /// Historical mean.
    pub mean: f64,
    /// Historical standard deviation.
    pub stddev: f64,
    /// Whether current exceeds mean + 1.5 * stddev.
    pub is_outlier: bool,
    /// Baseline status classification.
    pub status: BaselineStatus,
    /// Phase name if this is a phase-specific metric.
    pub phase: Option<String>,
}

/// Aggregated entry-level performance data for the retrospective.
///
/// Accumulated across sessions from Flagged signal drains.
/// `injection_count` is populated as 0 in col-009; col-010 provides INJECTION_LOG data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntryAnalysis {
    pub entry_id: u64,
    pub title: String,
    pub category: String,
    pub rework_flag_count: u32,
    pub injection_count: u32,       // reserved for col-010
    pub success_session_count: u32,
    pub rework_session_count: u32,
}

/// Synthesized narrative for a hotspot finding (col-010b).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotspotNarrative {
    /// Hotspot type (matches HotspotFinding.rule_name).
    pub hotspot_type: String,
    /// Human-readable summary of the hotspot.
    pub summary: String,
    /// Timestamp-clustered event groups.
    pub clusters: Vec<EvidenceCluster>,
    /// Top files by occurrence count (max 5).
    pub top_files: Vec<(String, u32)>,
    /// Monotone sequence pattern for sleep_workarounds (e.g., "30s->60s->90s->120s").
    pub sequence_pattern: Option<String>,
}

/// A cluster of events within a time window (col-010b).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCluster {
    /// Start of the time window (unix epoch millis).
    pub window_start: u64,
    /// Number of events in this cluster.
    pub event_count: u32,
    /// Human-readable description.
    pub description: String,
}

/// Actionable recommendation derived from hotspot findings (col-010b).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Hotspot type this recommendation addresses.
    pub hotspot_type: String,
    /// Actionable text.
    pub action: String,
    /// Rationale for the recommendation.
    pub rationale: String,
}

/// Complete analysis output returned by context_retrospective.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrospectiveReport {
    /// The feature analyzed.
    pub feature_cycle: String,
    /// Number of sessions analyzed.
    pub session_count: usize,
    /// Total observation records processed.
    pub total_records: usize,
    /// Computed metric vector.
    pub metrics: MetricVector,
    /// All hotspot findings.
    pub hotspots: Vec<HotspotFinding>,
    /// Whether this is from a previous computation.
    pub is_cached: bool,
    /// Baseline comparison against historical metrics, if available.
    #[serde(default)]
    pub baseline_comparison: Option<Vec<BaselineComparison>>,
    /// Entry-level analysis from Flagged signals, if any were accumulated since last call.
    /// Absent from JSON (not null) when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entries_analysis: Option<Vec<EntryAnalysis>>,
    /// Synthesized narratives for hotspot findings (col-010b).
    /// Present only on the structured-events path; None on JSONL fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narratives: Option<Vec<HotspotNarrative>>,
    /// Actionable recommendations derived from hotspot findings (col-010b).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommendations: Vec<Recommendation>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hooktype_serde_roundtrip() {
        for hook in [HookType::PreToolUse, HookType::PostToolUse, HookType::SubagentStart, HookType::SubagentStop] {
            let json = serde_json::to_string(&hook).expect("serialize");
            let back: HookType = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(hook, back);
        }
    }

    #[test]
    fn test_observation_record_serde() {
        let record = ObservationRecord {
            ts: 1700000000000,
            hook: HookType::PreToolUse,
            session_id: "test-session".to_string(),
            tool: Some("Read".to_string()),
            input: Some(serde_json::json!({"file_path": "/tmp/test.rs"})),
            response_size: None,
            response_snippet: None,
        };
        let json = serde_json::to_string(&record).expect("serialize");
        let back: ObservationRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.ts, 1700000000000);
        assert_eq!(back.hook, HookType::PreToolUse);
        assert_eq!(back.tool, Some("Read".to_string()));
    }

    #[test]
    fn test_hotspot_category_variants() {
        let categories = [HotspotCategory::Agent, HotspotCategory::Friction, HotspotCategory::Session, HotspotCategory::Scope];
        for (i, a) in categories.iter().enumerate() {
            for (j, b) in categories.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn test_severity_variants() {
        let severities = [Severity::Info, Severity::Warning, Severity::Critical];
        for (i, a) in severities.iter().enumerate() {
            for (j, b) in severities.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }
}
