//! Shared types for the observation pipeline.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{ObserveError, Result};

/// Hook event type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookType {
    PreToolUse,
    PostToolUse,
    SubagentStart,
    SubagentStop,
}

/// A single normalized observation record from a Claude Code hook event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationRecord {
    /// Unix epoch milliseconds.
    pub ts: u64,
    /// Hook event type.
    pub hook: HookType,
    /// Claude Code session identifier.
    pub session_id: String,
    /// Tool name (PreToolUse/PostToolUse) or agent type (SubagentStart). None for SubagentStop.
    pub tool: Option<String>,
    /// Tool input object or prompt snippet (as Value::String). None for SubagentStop.
    pub input: Option<serde_json::Value>,
    /// Response byte count (PostToolUse only).
    pub response_size: Option<u64>,
    /// First 500 chars of response (PostToolUse only).
    pub response_snippet: Option<String>,
}

/// Metadata for a discovered session file.
#[derive(Debug, Clone)]
pub struct SessionFile {
    /// Path to the .jsonl file.
    pub path: PathBuf,
    /// Session ID extracted from filename.
    pub session_id: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Last modified time as Unix epoch seconds.
    pub modified_at: u64,
}

/// A parsed session with its records.
#[derive(Debug, Clone)]
pub struct ParsedSession {
    /// Session ID.
    pub session_id: String,
    /// Parsed observation records, sorted by timestamp.
    pub records: Vec<ObservationRecord>,
}

/// Aggregate statistics about observation files.
#[derive(Debug, Clone)]
pub struct ObservationStats {
    /// Number of .jsonl files.
    pub file_count: u64,
    /// Total size of all files in bytes.
    pub total_size_bytes: u64,
    /// Age of oldest file in days.
    pub oldest_file_age_days: u64,
    /// Session IDs of files approaching 60-day cleanup (45-59 days old).
    pub approaching_cleanup: Vec<String>,
}

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

/// Universal metrics applicable to any agentic workflow.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct UniversalMetrics {
    #[serde(default)]
    pub total_tool_calls: u64,
    #[serde(default)]
    pub total_duration_secs: u64,
    #[serde(default)]
    pub session_count: u64,
    #[serde(default)]
    pub search_miss_rate: f64,
    #[serde(default)]
    pub edit_bloat_total_kb: f64,
    #[serde(default)]
    pub edit_bloat_ratio: f64,
    #[serde(default)]
    pub permission_friction_events: u64,
    #[serde(default)]
    pub bash_for_search_count: u64,
    #[serde(default)]
    pub cold_restart_events: u64,
    #[serde(default)]
    pub coordinator_respawn_count: u64,
    #[serde(default)]
    pub parallel_call_rate: f64,
    #[serde(default)]
    pub context_load_before_first_write_kb: f64,
    #[serde(default)]
    pub total_context_loaded_kb: f64,
    #[serde(default)]
    pub post_completion_work_pct: f64,
    #[serde(default)]
    pub follow_up_issues_created: u64,
    #[serde(default)]
    pub knowledge_entries_stored: u64,
    #[serde(default)]
    pub sleep_workaround_count: u64,
    #[serde(default)]
    pub agent_hotspot_count: u64,
    #[serde(default)]
    pub friction_hotspot_count: u64,
    #[serde(default)]
    pub session_hotspot_count: u64,
    #[serde(default)]
    pub scope_hotspot_count: u64,
}

/// Per-phase metrics.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PhaseMetrics {
    #[serde(default)]
    pub duration_secs: u64,
    #[serde(default)]
    pub tool_call_count: u64,
}

/// Structured numeric telemetry for one retrospected feature.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricVector {
    #[serde(default)]
    pub computed_at: u64,
    #[serde(default)]
    pub universal: UniversalMetrics,
    #[serde(default)]
    pub phases: BTreeMap<String, PhaseMetrics>,
}

impl Default for MetricVector {
    fn default() -> Self {
        MetricVector {
            computed_at: 0,
            universal: UniversalMetrics::default(),
            phases: BTreeMap::new(),
        }
    }
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
}

/// Serialize a MetricVector to bincode bytes (ADR-002).
///
/// Uses `bincode::serde::encode_to_vec` with `standard()` config,
/// matching the workspace convention from unimatrix-store.
pub fn serialize_metric_vector(mv: &MetricVector) -> Result<Vec<u8>> {
    let bytes = bincode::serde::encode_to_vec(mv, bincode::config::standard())?;
    Ok(bytes)
}

/// Deserialize a MetricVector from bincode bytes (ADR-002).
pub fn deserialize_metric_vector(bytes: &[u8]) -> Result<MetricVector> {
    let (mv, _) =
        bincode::serde::decode_from_slice::<MetricVector, _>(bytes, bincode::config::standard())
            .map_err(|e| ObserveError::Serialization(e.to_string()))?;
    Ok(mv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_vector_roundtrip() {
        let mut mv = MetricVector::default();
        mv.computed_at = 1700000000;
        mv.universal.total_tool_calls = 42;
        mv.universal.session_count = 3;
        mv.phases.insert(
            "3a".to_string(),
            PhaseMetrics {
                duration_secs: 600,
                tool_call_count: 15,
            },
        );

        let bytes = serialize_metric_vector(&mv).expect("serialize");
        let deserialized = deserialize_metric_vector(&bytes).expect("deserialize");
        assert_eq!(mv, deserialized);
    }

    #[test]
    fn test_metric_vector_all_defaults() {
        let mv = MetricVector::default();
        let bytes = serialize_metric_vector(&mv).expect("serialize");
        let deserialized = deserialize_metric_vector(&bytes).expect("deserialize");
        assert_eq!(mv, deserialized);
        assert_eq!(deserialized.computed_at, 0);
        assert_eq!(deserialized.universal.total_tool_calls, 0);
        assert!(deserialized.phases.is_empty());
    }

    #[test]
    fn test_metric_vector_with_phases() {
        let mut mv = MetricVector::default();
        mv.phases.insert("3a".to_string(), PhaseMetrics { duration_secs: 100, tool_call_count: 10 });
        mv.phases.insert("3b".to_string(), PhaseMetrics { duration_secs: 200, tool_call_count: 20 });
        mv.phases.insert("3c".to_string(), PhaseMetrics { duration_secs: 50, tool_call_count: 5 });

        let bytes = serialize_metric_vector(&mv).expect("serialize");
        let deserialized = deserialize_metric_vector(&bytes).expect("deserialize");
        assert_eq!(deserialized.phases.len(), 3);
        assert_eq!(deserialized.phases["3a"].duration_secs, 100);
        assert_eq!(deserialized.phases["3b"].tool_call_count, 20);
    }

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
