//! Shared types for the observation pipeline.
//!
//! Core observation types (HookType, ObservationRecord, ParsedSession, ObservationStats)
//! are defined in unimatrix-core and re-exported here for backward compatibility (col-013 ADR-002).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export core observation types for backward compatibility (col-013 ADR-002)
pub use unimatrix_core::{HookType, ObservationRecord, ObservationStats, ParsedSession};

// Re-export metric types from unimatrix-store for backward compatibility (nxs-009 ADR-001)
pub use unimatrix_store::{MetricVector, PhaseMetrics, UniversalMetrics};

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
    pub injection_count: u32, // reserved for col-010
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

/// Per-session activity profile computed from observation records (col-020).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Session identifier.
    pub session_id: String,
    /// Earliest observation timestamp in the session (epoch millis).
    pub started_at: u64,
    /// Duration in seconds (max_ts - min_ts) / 1000.
    pub duration_secs: u64,
    /// Tool usage counts grouped by category (read, write, execute, search, store, spawn, other).
    pub tool_distribution: HashMap<String, u64>,
    /// Top 5 directory zones by file touch frequency, descending.
    pub top_file_zones: Vec<(String, u64)>,
    /// Names of agents spawned in this session.
    pub agents_spawned: Vec<String>,
    /// Count of knowledge retrieval tool calls (context_search, context_lookup, context_get).
    pub knowledge_in: u64,
    /// Count of knowledge store tool calls (context_store).
    pub knowledge_out: u64,
    /// Session outcome from SessionRecord, populated by handler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// Cross-session knowledge reuse measurement (col-020).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeReuse {
    /// Distinct entry IDs reused across sessions (Tier 1).
    pub tier1_reuse_count: u64,
    /// Reuse counts grouped by entry category.
    pub by_category: HashMap<String, u64>,
    /// Categories with active entries but zero reuse.
    pub category_gaps: Vec<String>,
}

/// Attribution quality metadata for consumer trust assessment (col-020, ADR-003).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributionMetadata {
    /// Sessions with non-NULL feature_cycle matching the topic.
    pub attributed_session_count: usize,
    /// Total sessions discovered for the feature.
    pub total_session_count: usize,
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
    /// Per-session activity profiles (col-020).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_summaries: Option<Vec<SessionSummary>>,
    /// Cross-session knowledge reuse measurement (col-020).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_reuse: Option<KnowledgeReuse>,
    /// Number of sessions with rework/failed outcomes (col-020).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rework_session_count: Option<u64>,
    /// Fraction of file reads in sessions N+1..N that overlap prior sessions (col-020).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_reload_pct: Option<f64>,
    /// Attribution quality metadata (col-020, ADR-003).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<AttributionMetadata>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hooktype_serde_roundtrip() {
        for hook in [
            HookType::PreToolUse,
            HookType::PostToolUse,
            HookType::SubagentStart,
            HookType::SubagentStop,
        ] {
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
        let categories = [
            HotspotCategory::Agent,
            HotspotCategory::Friction,
            HotspotCategory::Session,
            HotspotCategory::Scope,
        ];
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

    // ── C2 serde tests (col-020) ──────────────────────────────────────

    #[test]
    fn test_session_summary_serde_roundtrip() {
        let mut tool_dist = HashMap::new();
        tool_dist.insert("read".to_string(), 5);
        tool_dist.insert("write".to_string(), 3);

        let summary = SessionSummary {
            session_id: "sess-001".to_string(),
            started_at: 1700000000000,
            duration_secs: 120,
            tool_distribution: tool_dist,
            top_file_zones: vec![
                ("crates/unimatrix-store".to_string(), 10),
                ("crates/unimatrix-core".to_string(), 4),
            ],
            agents_spawned: vec!["coder-1".to_string()],
            knowledge_in: 7,
            knowledge_out: 2,
            outcome: Some("success".to_string()),
        };

        let json = serde_json::to_string(&summary).expect("serialize");
        let back: SessionSummary = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.session_id, "sess-001");
        assert_eq!(back.started_at, 1700000000000);
        assert_eq!(back.duration_secs, 120);
        assert_eq!(back.tool_distribution.get("read"), Some(&5));
        assert_eq!(back.tool_distribution.get("write"), Some(&3));
        assert_eq!(back.top_file_zones.len(), 2);
        assert_eq!(back.top_file_zones[0], ("crates/unimatrix-store".to_string(), 10));
        assert_eq!(back.agents_spawned, vec!["coder-1".to_string()]);
        assert_eq!(back.knowledge_in, 7);
        assert_eq!(back.knowledge_out, 2);
        assert_eq!(back.outcome, Some("success".to_string()));
    }

    #[test]
    fn test_session_summary_outcome_none_omitted() {
        let summary = SessionSummary {
            session_id: "sess-002".to_string(),
            started_at: 1700000000000,
            duration_secs: 60,
            tool_distribution: HashMap::new(),
            top_file_zones: vec![],
            agents_spawned: vec![],
            knowledge_in: 0,
            knowledge_out: 0,
            outcome: None,
        };

        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(
            !json.contains("outcome"),
            "serialized JSON should not contain 'outcome' key when None"
        );
    }

    #[test]
    fn test_knowledge_reuse_serde_roundtrip() {
        let mut by_cat = HashMap::new();
        by_cat.insert("convention".to_string(), 3);
        by_cat.insert("pattern".to_string(), 2);

        let reuse = KnowledgeReuse {
            tier1_reuse_count: 5,
            by_category: by_cat,
            category_gaps: vec!["procedure".to_string()],
        };

        let json = serde_json::to_string(&reuse).expect("serialize");
        let back: KnowledgeReuse = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.tier1_reuse_count, 5);
        assert_eq!(back.by_category.get("convention"), Some(&3));
        assert_eq!(back.by_category.get("pattern"), Some(&2));
        assert_eq!(back.category_gaps, vec!["procedure".to_string()]);
    }

    #[test]
    fn test_attribution_metadata_serde_roundtrip() {
        let attr = AttributionMetadata {
            attributed_session_count: 7,
            total_session_count: 10,
        };

        let json = serde_json::to_string(&attr).expect("serialize");
        let back: AttributionMetadata = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.attributed_session_count, 7);
        assert_eq!(back.total_session_count, 10);
    }

    #[test]
    fn test_retrospective_report_deserialize_pre_col020() {
        // Pre-col-020 JSON: no session_summaries, knowledge_reuse,
        // rework_session_count, context_reload_pct, or attribution fields.
        let json = r#"{
            "feature_cycle": "old-feature",
            "session_count": 3,
            "total_records": 42,
            "metrics": {"computed_at": 0, "universal": {}, "phases": {}},
            "hotspots": [],
            "is_cached": false
        }"#;

        let report: RetrospectiveReport =
            serde_json::from_str(json).expect("pre-col-020 JSON should deserialize");

        assert_eq!(report.feature_cycle, "old-feature");
        assert!(report.session_summaries.is_none());
        assert!(report.knowledge_reuse.is_none());
        assert!(report.rework_session_count.is_none());
        assert!(report.context_reload_pct.is_none());
        assert!(report.attribution.is_none());
    }

    #[test]
    fn test_retrospective_report_serialize_none_fields_omitted() {
        let report = RetrospectiveReport {
            feature_cycle: "feat-001".to_string(),
            session_count: 1,
            total_records: 10,
            metrics: MetricVector::default(),
            hotspots: vec![],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: None,
            knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
        };

        let json = serde_json::to_string(&report).expect("serialize");
        assert!(!json.contains("session_summaries"), "session_summaries should be omitted");
        assert!(!json.contains("knowledge_reuse"), "knowledge_reuse should be omitted");
        assert!(!json.contains("rework_session_count"), "rework_session_count should be omitted");
        assert!(!json.contains("context_reload_pct"), "context_reload_pct should be omitted");
        assert!(!json.contains("attribution"), "attribution should be omitted");
    }

    #[test]
    fn test_retrospective_report_roundtrip_with_new_fields() {
        let mut by_cat = HashMap::new();
        by_cat.insert("convention".to_string(), 2);

        let report = RetrospectiveReport {
            feature_cycle: "feat-002".to_string(),
            session_count: 4,
            total_records: 100,
            metrics: MetricVector::default(),
            hotspots: vec![],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: Some(vec![SessionSummary {
                session_id: "s1".to_string(),
                started_at: 1700000000000,
                duration_secs: 300,
                tool_distribution: HashMap::new(),
                top_file_zones: vec![],
                agents_spawned: vec![],
                knowledge_in: 1,
                knowledge_out: 0,
                outcome: None,
            }]),
            knowledge_reuse: Some(KnowledgeReuse {
                tier1_reuse_count: 3,
                by_category: by_cat,
                category_gaps: vec!["procedure".to_string()],
            }),
            rework_session_count: Some(1),
            context_reload_pct: Some(0.45),
            attribution: Some(AttributionMetadata {
                attributed_session_count: 3,
                total_session_count: 4,
            }),
        };

        let json = serde_json::to_string(&report).expect("serialize");
        let back: RetrospectiveReport = serde_json::from_str(&json).expect("deserialize");

        let summaries = back.session_summaries.expect("session_summaries present");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].session_id, "s1");

        let reuse = back.knowledge_reuse.expect("knowledge_reuse present");
        assert_eq!(reuse.tier1_reuse_count, 3);
        assert_eq!(reuse.by_category.get("convention"), Some(&2));
        assert_eq!(reuse.category_gaps, vec!["procedure"]);

        assert_eq!(back.rework_session_count, Some(1));
        assert!((back.context_reload_pct.unwrap() - 0.45).abs() < f64::EPSILON);

        let attr = back.attribution.expect("attribution present");
        assert_eq!(attr.attributed_session_count, 3);
        assert_eq!(attr.total_session_count, 4);
    }

    #[test]
    fn test_retrospective_report_partial_new_fields() {
        // JSON with only session_summaries present; other col-020 fields absent.
        let json = r#"{
            "feature_cycle": "feat-partial",
            "session_count": 2,
            "total_records": 20,
            "metrics": {"computed_at": 0, "universal": {}, "phases": {}},
            "hotspots": [],
            "is_cached": false,
            "session_summaries": [
                {
                    "session_id": "sp1",
                    "started_at": 1700000000000,
                    "duration_secs": 60,
                    "tool_distribution": {},
                    "top_file_zones": [],
                    "agents_spawned": [],
                    "knowledge_in": 0,
                    "knowledge_out": 0
                }
            ]
        }"#;

        let report: RetrospectiveReport =
            serde_json::from_str(json).expect("partial new fields should deserialize");

        let summaries = report.session_summaries.expect("session_summaries populated");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].session_id, "sp1");

        assert!(report.knowledge_reuse.is_none());
        assert!(report.rework_session_count.is_none());
        assert!(report.context_reload_pct.is_none());
        assert!(report.attribution.is_none());
    }
}
