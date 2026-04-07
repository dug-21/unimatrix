//! Shared types for the observation pipeline.
//!
//! Core observation types (HookType, ObservationRecord, ParsedSession, ObservationStats)
//! are defined in unimatrix-core and re-exported here for backward compatibility (col-013 ADR-002).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export core observation types for backward compatibility (col-013 ADR-002)
// HookType enum removed in col-023 ADR-001; use hook_type string constants module instead
pub use unimatrix_core::{ObservationRecord, ObservationStats, ParsedSession};

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
    #[serde(alias = "knowledge_in")]
    pub knowledge_served: u64,
    /// Count of knowledge creation tool calls (context_store).
    #[serde(alias = "knowledge_out")]
    pub knowledge_stored: u64,
    /// Count of knowledge curation tool calls (context_correct, context_deprecate, context_quarantine).
    #[serde(default)]
    pub knowledge_curated: u64,
    /// Session outcome from SessionRecord, populated by handler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// Tool call counts grouped by category for a phase or session (col-026).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolDistribution {
    /// Read-category tool calls (Read, Glob, Grep, etc.).
    #[serde(default)]
    pub read: u64,
    /// Execute-category tool calls (Bash, etc.).
    #[serde(default)]
    pub execute: u64,
    /// Write-category tool calls (Edit, Write, etc.).
    #[serde(default)]
    pub write: u64,
    /// Search-category tool calls (context_search, context_lookup, etc.).
    #[serde(default)]
    pub search: u64,
}

/// Gate outcome classification derived from `cycle_phase_end.outcome` text (col-026).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateResult {
    Pass,
    Fail,
    Rework,
    #[default]
    Unknown,
}

/// Aggregate statistics for one phase window in a feature cycle (col-026).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseStats {
    /// Phase name (e.g., "design", "implementation").
    pub phase: String,
    /// 1-indexed pass number within this phase name for this cycle.
    pub pass_number: u32,
    /// Total number of passes for this phase name in the cycle (>1 = rework).
    pub pass_count: u32,
    /// Duration of this phase window in seconds.
    pub duration_secs: u64,
    /// Phase window start boundary in epoch milliseconds (GAP-1: required by formatter
    /// to map finding evidence timestamps to phase windows for hotspot annotations).
    pub start_ms: i64,
    /// Phase window end boundary in epoch milliseconds, or None if the phase is open
    /// (no cycle_stop event yet). GAP-1: required by formatter for hotspot annotations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<i64>,
    /// Number of distinct sessions with observations in this phase window.
    pub session_count: usize,
    /// Total observation records in this phase window.
    pub record_count: usize,
    /// Deduplicated agent names observed in this window, first-seen order.
    pub agents: Vec<String>,
    /// Tool call distribution by category.
    pub tool_distribution: ToolDistribution,
    /// Knowledge entries served to agents in this window.
    pub knowledge_served: u64,
    /// Knowledge entries stored by agents in this window.
    pub knowledge_stored: u64,
    /// Gate outcome classification for this phase.
    pub gate_result: GateResult,
    /// Raw outcome text from the `cycle_phase_end` event, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_outcome_text: Option<String>,
    /// Finding IDs (e.g., "F-01") associated with this phase; populated by the formatter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hotspot_ids: Vec<String>,
}

/// A reference to a knowledge entry, used in cross-feature reuse reporting (col-026).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryRef {
    /// Entry database ID.
    pub id: u64,
    /// Entry title.
    pub title: String,
    /// Feature cycle that stored this entry.
    pub feature_cycle: String,
    /// Entry category.
    pub category: String,
    /// Number of times this entry was served during the current cycle's sessions.
    pub serve_count: u64,
}

/// Feature-scoped knowledge delivery measurement (col-020b).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureKnowledgeReuse {
    /// Count of distinct entry IDs returned in query result sets during the cycle.
    /// Renamed from `delivery_count` (crt-049). Does NOT imply the agent consumed the entry.
    /// Serde aliases retain round-trip compatibility with pre-crt-049 stored rows
    /// ("delivery_count") and pre-col-020b stored rows ("tier1_reuse_count").
    /// BOTH aliases are required — dropping either silently produces zero on re-review.
    #[serde(alias = "delivery_count")]
    #[serde(alias = "tier1_reuse_count")]
    pub search_exposure_count: u64,
    /// Count of distinct entry IDs explicitly retrieved by agents via context_get
    /// or single-ID context_lookup. Unambiguous consumption signal.
    /// Defaults to 0 when absent in stored JSON (pre-crt-049 rows).
    #[serde(default)]
    pub explicit_read_count: u64,
    /// Cycle-level category breakdown of explicit reads. NOT the Group 10 training input
    /// — Group 10 requires phase-stratified (phase, category) aggregates. [GATE contract AC-13]
    /// Defaults to empty map when absent in stored JSON (pre-crt-049 rows).
    #[serde(default)]
    pub explicit_read_by_category: HashMap<String, u64>,
    /// Entries appearing in 2+ distinct sessions (sub-metric of search_exposure_count).
    #[serde(default)]
    pub cross_session_count: u64,
    /// Delivery counts grouped by entry category.
    pub by_category: HashMap<String, u64>,
    /// Categories with active entries but zero delivery.
    pub category_gaps: Vec<String>,
    /// |explicit_read_ids ∪ injection_ids| (deduplicated). Search exposures excluded. (crt-049)
    #[serde(default)]
    pub total_served: u64,
    /// Knowledge entries created during this cycle.
    #[serde(default)]
    pub total_stored: u64,
    /// Entries originating from prior feature cycles that were served.
    #[serde(default)]
    pub cross_feature_reuse: u64,
    /// Entries stored during this cycle that were also served.
    #[serde(default)]
    pub intra_cycle_reuse: u64,
    /// Top cross-feature entries by serve count (up to 5).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_cross_feature_entries: Vec<EntryRef>,
}

/// Attribution quality metadata for consumer trust assessment (col-020, ADR-003).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributionMetadata {
    /// Sessions with non-NULL feature_cycle matching the topic.
    pub attributed_session_count: usize,
    /// Total sessions discovered for the feature.
    pub total_session_count: usize,
}

/// Type alias for a two-level category distribution keyed by (phase, category) (crt-025).
/// Outer key: phase token. Inner key: category name. Value: entry count.
pub type PhaseCategoryDist = HashMap<String, HashMap<String, u64>>;

/// A single raw row from the `cycle_events` table (crt-025).
///
/// Mapped row-by-row from the SQL query result ordered by `(timestamp ASC, seq ASC)`.
/// `event_type` is one of `"cycle_start"`, `"cycle_phase_end"`, `"cycle_stop"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleEventRecord {
    pub seq: i64,
    pub event_type: String,
    pub phase: Option<String>,
    pub outcome: Option<String>,
    pub next_phase: Option<String>,
    pub timestamp: i64,
}

/// One (phase, category) pair compared against cross-cycle baselines (crt-025).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseCategoryComparison {
    pub phase: String,
    pub category: String,
    pub this_feature_count: u64,
    pub cross_cycle_mean: f64,
    /// Number of distinct prior features contributing to the mean for this (phase, category) pair.
    pub sample_features: usize,
}

/// Phase lifecycle narrative derived from `CYCLE_EVENTS` and `FEATURE_ENTRIES` (crt-025).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseNarrative {
    /// Ordered sequence of active phases (may repeat if phase was re-entered).
    pub phase_sequence: Vec<String>,
    /// Phases that appear more than once in `phase_sequence` (rework signal).
    pub rework_phases: Vec<String>,
    /// Count of `feature_entries` by (phase, category) for this feature.
    /// Outer key: phase token. Inner key: category. Value: count.
    pub per_phase_categories: PhaseCategoryDist,
    /// Cross-cycle comparison; `None` when fewer than 2 prior features have phase data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_cycle_comparison: Option<Vec<PhaseCategoryComparison>>,
}

/// Complete analysis output returned by context_cycle_review.
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "knowledge_reuse"
    )]
    pub feature_knowledge_reuse: Option<FeatureKnowledgeReuse>,
    /// Number of sessions with rework/failed outcomes (col-020).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rework_session_count: Option<u64>,
    /// Fraction of file reads in sessions N+1..N that overlap prior sessions (col-020).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_reload_pct: Option<f64>,
    /// Attribution quality metadata (col-020, ADR-003).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<AttributionMetadata>,
    /// Phase lifecycle narrative derived from CYCLE_EVENTS and FEATURE_ENTRIES (crt-025).
    /// Absent from JSON (not null) when no cycle_events exist for the feature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_narrative: Option<PhaseNarrative>,

    // ── col-026 new fields ────────────────────────────────────────────────
    /// Goal text from `cycle_events.goal` on the `cycle_start` row, if present (col-026).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,

    /// Inferred cycle type from goal keywords (e.g., "Design", "Delivery") (col-026).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_type: Option<String>,

    /// Attribution path used to load observations (col-026).
    /// Values: "cycle_events-first (primary)", "sessions.feature_cycle (legacy)",
    /// "content-scan (fallback)".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution_path: Option<String>,

    /// Whether the cycle is still in progress (col-026, ADR-001).
    /// `None` = no `cycle_events` rows (cannot determine).
    /// `Some(true)` = `cycle_start` present, no `cycle_stop`.
    /// `Some(false)` = `cycle_stop` confirmed present.
    /// NEVER plain `bool` — three-valued semantics are required (ADR-001).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_in_progress: Option<bool>,

    /// Per-phase aggregate statistics computed from `cycle_events` time windows (col-026).
    /// `None` when no `cycle_events` rows exist for this cycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_stats: Option<Vec<PhaseStats>>,

    /// Curation health block computed at review time (crt-047).
    ///
    /// Contains the per-cycle `CurationSnapshot` (raw correction and deprecation counts)
    /// and an optional `CurationBaselineComparison` (σ position vs. rolling baseline).
    ///
    /// `None` when:
    /// - `compute_curation_snapshot()` failed (non-fatal, logged as warning).
    /// - `force=false` memoization hit returned a cached record without curation data
    ///   (i.e., the record was written before crt-047, `schema_version = 1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curation_health: Option<CurationHealthBlock>,
}

// ---------------------------------------------------------------------------
// crt-047: Curation health types
// ---------------------------------------------------------------------------

/// Per-cycle correction and deprecation counts computed from ENTRIES at review time.
///
/// `corrections_total = corrections_agent + corrections_human` (computed, not a raw count).
/// `corrections_system` is informational only; it is excluded from `corrections_total`
/// and from the σ baseline computation (ADR-002, crt-047).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurationSnapshot {
    /// `corrections_agent + corrections_human` — intentional curation only.
    pub corrections_total: u32,
    /// Corrections where `trust_source = 'agent'`.
    pub corrections_agent: u32,
    /// Corrections where `trust_source IN ('human', 'privileged')`.
    pub corrections_human: u32,
    /// Corrections for all other `trust_source` values (informational; not in total).
    pub corrections_system: u32,
    /// All entries with `status = 'deprecated'` in the cycle window (orphan + chain).
    pub deprecations_total: u32,
    /// Entries deprecated AND `superseded_by IS NULL` in the cycle window.
    pub orphan_deprecations: u32,
}

// NOTE: `CurationBaseline` is an intermediate compute type that lives only in
// `unimatrix-server/services/curation_health.rs`. It is not serialized and
// not added here to avoid cross-crate conflicts.

/// σ-distance of the current cycle's curation metrics vs. the rolling baseline.
///
/// Produced by `compare_to_baseline()` when `>= CURATION_MIN_HISTORY` qualifying
/// prior cycles exist in `cycle_review_index`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurationBaselineComparison {
    /// σ distance of `corrections_total` from baseline mean.
    /// Positive = more corrections than normal; negative = fewer.
    pub corrections_total_sigma: f64,
    /// σ distance of `orphan_ratio` from baseline mean.
    /// `orphan_ratio = orphan_deprecations / deprecations_total`; 0.0 when denom = 0.
    pub orphan_ratio_sigma: f64,
    /// Number of prior cycles that contributed to the baseline.
    pub history_cycles: usize,
    /// `false` when either sigma exceeds `CURATION_SIGMA_THRESHOLD` (1.5σ).
    pub within_normal_range: bool,
}

/// Trend direction for the rolling correction rate (crt-047).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrendDirection {
    Increasing,
    Decreasing,
    Stable,
}

/// Aggregate curation health view for `context_status` (crt-047).
///
/// Produced by `compute_curation_summary()` over the last N rows in
/// `cycle_review_index`. `None` when the window is empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurationHealthSummary {
    /// Mean `corrections_total` across the window.
    pub correction_rate_mean: f64,
    /// Population stddev of `corrections_total`.
    pub correction_rate_stddev: f64,
    /// Mean fraction of corrections attributable to agents (%).
    pub agent_pct: f64,
    /// Mean fraction of corrections attributable to humans (%).
    pub human_pct: f64,
    /// Mean orphan ratio (`orphan_deprecations / deprecations_total`).
    pub orphan_ratio_mean: f64,
    /// Population stddev of orphan ratio.
    pub orphan_ratio_stddev: f64,
    /// Trend direction; `None` when fewer than `CURATION_MIN_TREND_HISTORY` cycles.
    pub trend: Option<TrendDirection>,
    /// Number of cycles that contributed to the aggregate.
    pub cycles_in_window: usize,
}

/// Output container for the `context_cycle_review` curation health block (crt-047).
///
/// Serialized into `RetrospectiveReport.curation_health` and stored in `summary_json`.
/// On cache-hit (`force=false`) the stored value is returned as-is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurationHealthBlock {
    /// Raw per-cycle correction and deprecation counts.
    pub snapshot: CurationSnapshot,
    /// σ position vs. rolling baseline; `None` when fewer than 3 qualifying prior cycles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<CurationBaselineComparison>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_string_constants() {
        use unimatrix_core::observation::hook_type;
        assert_eq!(hook_type::PRETOOLUSE, "PreToolUse");
        assert_eq!(hook_type::POSTTOOLUSE, "PostToolUse");
        assert_eq!(hook_type::SUBAGENTSTART, "SubagentStart");
        assert_eq!(hook_type::SUBAGENTSTOPPED, "SubagentStop");
    }

    #[test]
    fn test_observation_record_serde() {
        let record = ObservationRecord {
            ts: 1700000000000,
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: "test-session".to_string(),
            tool: Some("Read".to_string()),
            input: Some(serde_json::json!({"file_path": "/tmp/test.rs"})),
            response_size: None,
            response_snippet: None,
        };
        let json = serde_json::to_string(&record).expect("serialize");
        let back: ObservationRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.ts, 1700000000000);
        assert_eq!(back.event_type, "PreToolUse");
        assert_eq!(back.source_domain, "claude-code");
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
            knowledge_served: 7,
            knowledge_stored: 2,
            knowledge_curated: 1,
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
        assert_eq!(
            back.top_file_zones[0],
            ("crates/unimatrix-store".to_string(), 10)
        );
        assert_eq!(back.agents_spawned, vec!["coder-1".to_string()]);
        assert_eq!(back.knowledge_served, 7);
        assert_eq!(back.knowledge_stored, 2);
        assert_eq!(back.knowledge_curated, 1);
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
            knowledge_served: 0,
            knowledge_stored: 0,
            knowledge_curated: 0,
            outcome: None,
        };

        let json = serde_json::to_string(&summary).expect("serialize");
        assert!(
            !json.contains("outcome"),
            "serialized JSON should not contain 'outcome' key when None"
        );
    }

    #[test]
    fn test_feature_knowledge_reuse_serde_roundtrip() {
        let mut by_cat = HashMap::new();
        by_cat.insert("convention".to_string(), 3);
        by_cat.insert("pattern".to_string(), 2);

        let reuse = FeatureKnowledgeReuse {
            search_exposure_count: 5,
            explicit_read_count: 0,
            explicit_read_by_category: HashMap::new(),
            cross_session_count: 2,
            by_category: by_cat,
            category_gaps: vec!["procedure".to_string()],
            total_served: 0,
            total_stored: 0,
            cross_feature_reuse: 0,
            intra_cycle_reuse: 0,
            top_cross_feature_entries: vec![],
        };

        let json = serde_json::to_string(&reuse).expect("serialize");
        let back: FeatureKnowledgeReuse = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.search_exposure_count, 5);
        assert_eq!(back.cross_session_count, 2);
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
        assert!(report.feature_knowledge_reuse.is_none());
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
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        };

        let json = serde_json::to_string(&report).expect("serialize");
        assert!(
            !json.contains("session_summaries"),
            "session_summaries should be omitted"
        );
        assert!(
            !json.contains("feature_knowledge_reuse"),
            "feature_knowledge_reuse should be omitted"
        );
        assert!(
            !json.contains("rework_session_count"),
            "rework_session_count should be omitted"
        );
        assert!(
            !json.contains("context_reload_pct"),
            "context_reload_pct should be omitted"
        );
        assert!(
            !json.contains("attribution"),
            "attribution should be omitted"
        );
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
                knowledge_served: 1,
                knowledge_stored: 0,
                knowledge_curated: 0,
                outcome: None,
            }]),
            feature_knowledge_reuse: Some(FeatureKnowledgeReuse {
                search_exposure_count: 3,
                explicit_read_count: 0,
                explicit_read_by_category: HashMap::new(),
                cross_session_count: 0,
                by_category: by_cat,
                category_gaps: vec!["procedure".to_string()],
                total_served: 0,
                total_stored: 0,
                cross_feature_reuse: 0,
                intra_cycle_reuse: 0,
                top_cross_feature_entries: vec![],
            }),
            rework_session_count: Some(1),
            context_reload_pct: Some(0.45),
            attribution: Some(AttributionMetadata {
                attributed_session_count: 3,
                total_session_count: 4,
            }),
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        };

        let json = serde_json::to_string(&report).expect("serialize");
        let back: RetrospectiveReport = serde_json::from_str(&json).expect("deserialize");

        let summaries = back.session_summaries.expect("session_summaries present");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].session_id, "s1");

        let json_str = serde_json::to_string(&report).expect("serialize");
        assert!(
            json_str.contains("feature_knowledge_reuse"),
            "serialized JSON should use new field name"
        );
        assert!(
            json_str.contains("search_exposure_count"),
            "serialized JSON should use canonical field name search_exposure_count"
        );
        assert!(
            json_str.contains("knowledge_served"),
            "serialized JSON should use new field name"
        );

        let reuse = back
            .feature_knowledge_reuse
            .expect("feature_knowledge_reuse present");
        assert_eq!(reuse.search_exposure_count, 3);
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

        let summaries = report
            .session_summaries
            .expect("session_summaries populated");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].session_id, "sp1");

        assert!(report.feature_knowledge_reuse.is_none());
        assert!(report.rework_session_count.is_none());
        assert!(report.context_reload_pct.is_none());
        assert!(report.attribution.is_none());
    }

    // ── C4 backward compatibility tests (col-020b) ──────────────────────

    #[test]
    fn test_session_summary_deserialize_pre_col020b() {
        let json = r#"{
            "session_id": "s1",
            "started_at": 0,
            "duration_secs": 0,
            "tool_distribution": {},
            "top_file_zones": [],
            "agents_spawned": [],
            "knowledge_in": 5,
            "knowledge_out": 3
        }"#;

        let result: SessionSummary =
            serde_json::from_str(json).expect("old field names should deserialize via alias");

        assert_eq!(result.knowledge_served, 5);
        assert_eq!(result.knowledge_stored, 3);
        assert_eq!(result.knowledge_curated, 0);
    }

    #[test]
    fn test_session_summary_knowledge_curated_default() {
        let json = r#"{
            "session_id": "s1",
            "started_at": 0,
            "duration_secs": 0,
            "tool_distribution": {},
            "top_file_zones": [],
            "agents_spawned": [],
            "knowledge_served": 2,
            "knowledge_stored": 1
        }"#;

        let result: SessionSummary =
            serde_json::from_str(json).expect("missing knowledge_curated should default");

        assert_eq!(result.knowledge_served, 2);
        assert_eq!(result.knowledge_stored, 1);
        assert_eq!(result.knowledge_curated, 0);
    }

    #[test]
    fn test_session_summary_knowledge_curated_present() {
        let json = r#"{
            "session_id": "s1",
            "started_at": 0,
            "duration_secs": 0,
            "tool_distribution": {},
            "top_file_zones": [],
            "agents_spawned": [],
            "knowledge_served": 2,
            "knowledge_stored": 1,
            "knowledge_curated": 5
        }"#;

        let result: SessionSummary =
            serde_json::from_str(json).expect("knowledge_curated present should deserialize");

        assert_eq!(result.knowledge_curated, 5);
    }

    #[test]
    fn test_feature_knowledge_reuse_deserialize_from_old() {
        let json = r#"{
            "tier1_reuse_count": 7,
            "by_category": {"convention": 4},
            "category_gaps": ["procedure"]
        }"#;

        let result: FeatureKnowledgeReuse =
            serde_json::from_str(json).expect("old tier1_reuse_count should deserialize via alias");

        assert_eq!(result.search_exposure_count, 7);
        assert_eq!(result.cross_session_count, 0);
        assert_eq!(result.by_category.get("convention"), Some(&4));
        assert_eq!(result.category_gaps, vec!["procedure"]);
    }

    #[test]
    fn test_retrospective_report_deserialize_old_knowledge_reuse_field() {
        let json = r#"{
            "feature_cycle": "feat-old",
            "session_count": 2,
            "total_records": 10,
            "metrics": {"computed_at": 0, "universal": {}, "phases": {}},
            "hotspots": [],
            "is_cached": false,
            "knowledge_reuse": {
                "tier1_reuse_count": 3,
                "by_category": {},
                "category_gaps": []
            }
        }"#;

        let report: RetrospectiveReport = serde_json::from_str(json)
            .expect("old knowledge_reuse field should deserialize via alias");

        let reuse = report
            .feature_knowledge_reuse
            .expect("should be Some via alias");
        assert_eq!(reuse.search_exposure_count, 3);
        assert_eq!(reuse.cross_session_count, 0);
    }

    // ── col-026 new type tests ─────────────────────────────────────────────

    #[test]
    fn test_tool_distribution_default() {
        let td = ToolDistribution::default();
        assert_eq!(td.read, 0);
        assert_eq!(td.execute, 0);
        assert_eq!(td.write, 0);
        assert_eq!(td.search, 0);
    }

    #[test]
    fn test_gate_result_default() {
        // GateResult::default() must be Unknown
        let gr = GateResult::default();
        assert_eq!(gr, GateResult::Unknown);
    }

    #[test]
    fn test_gate_result_serde() {
        // Each variant round-trips via JSON
        let cases = [
            (GateResult::Pass, "\"pass\""),
            (GateResult::Fail, "\"fail\""),
            (GateResult::Rework, "\"rework\""),
            (GateResult::Unknown, "\"unknown\""),
        ];
        for (variant, expected_json) in &cases {
            let json = serde_json::to_string(variant).expect("serialize");
            assert_eq!(
                json, *expected_json,
                "variant {:?} serialized incorrectly",
                variant
            );
            let back: GateResult = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, *variant, "variant {:?} did not round-trip", variant);
        }
    }

    #[test]
    fn test_entry_ref_serde() {
        let entry = EntryRef {
            id: 42,
            title: "Serde extension pattern".to_string(),
            feature_cycle: "col-024".to_string(),
            category: "decision".to_string(),
            serve_count: 3,
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        let back: EntryRef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.id, 42);
        assert_eq!(back.title, "Serde extension pattern");
        assert_eq!(back.feature_cycle, "col-024");
        assert_eq!(back.category, "decision");
        assert_eq!(back.serve_count, 3);
    }

    #[test]
    fn test_phase_stats_serde_roundtrip() {
        let ps = PhaseStats {
            phase: "implementation".to_string(),
            pass_number: 1,
            pass_count: 2,
            duration_secs: 3600,
            start_ms: 1_700_000_000_000,
            end_ms: Some(1_700_003_600_000),
            session_count: 3,
            record_count: 150,
            agents: vec!["coder-1".to_string(), "coder-2".to_string()],
            tool_distribution: ToolDistribution {
                read: 50,
                execute: 20,
                write: 30,
                search: 10,
            },
            knowledge_served: 7,
            knowledge_stored: 2,
            gate_result: GateResult::Pass,
            gate_outcome_text: Some("PASS — all tests green".to_string()),
            hotspot_ids: vec!["F-01".to_string()],
        };
        let json = serde_json::to_string(&ps).expect("serialize");
        let back: PhaseStats = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.phase, "implementation");
        assert_eq!(back.pass_number, 1);
        assert_eq!(back.pass_count, 2);
        assert_eq!(back.duration_secs, 3600);
        assert_eq!(back.start_ms, 1_700_000_000_000);
        assert_eq!(back.end_ms, Some(1_700_003_600_000));
        assert_eq!(back.session_count, 3);
        assert_eq!(back.record_count, 150);
        assert_eq!(back.agents, vec!["coder-1", "coder-2"]);
        assert_eq!(back.tool_distribution.read, 50);
        assert_eq!(back.tool_distribution.execute, 20);
        assert_eq!(back.tool_distribution.write, 30);
        assert_eq!(back.tool_distribution.search, 10);
        assert_eq!(back.knowledge_served, 7);
        assert_eq!(back.knowledge_stored, 2);
        assert_eq!(back.gate_result, GateResult::Pass);
        assert_eq!(
            back.gate_outcome_text,
            Some("PASS — all tests green".to_string())
        );
        assert_eq!(back.hotspot_ids, vec!["F-01"]);
    }

    #[test]
    fn test_phase_stats_hotspot_ids_default_empty() {
        // hotspot_ids absent in JSON → deserialized as empty vec
        let json = r#"{
            "phase": "scope",
            "pass_number": 1,
            "pass_count": 1,
            "duration_secs": 600,
            "start_ms": 1700000000000,
            "session_count": 1,
            "record_count": 10,
            "agents": [],
            "tool_distribution": {},
            "knowledge_served": 0,
            "knowledge_stored": 0,
            "gate_result": "unknown"
        }"#;
        let ps: PhaseStats = serde_json::from_str(json).expect("deserialize");
        assert!(ps.hotspot_ids.is_empty());
        assert!(ps.gate_outcome_text.is_none());
    }

    #[test]
    fn test_new_report_fields_absent_when_none() {
        let report = RetrospectiveReport {
            feature_cycle: "col-026".to_string(),
            session_count: 1,
            total_records: 5,
            metrics: MetricVector::default(),
            hotspots: vec![],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        };
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(!json.contains("\"goal\""), "goal should be absent");
        assert!(
            !json.contains("\"cycle_type\""),
            "cycle_type should be absent"
        );
        assert!(
            !json.contains("\"attribution_path\""),
            "attribution_path should be absent"
        );
        assert!(
            !json.contains("\"is_in_progress\""),
            "is_in_progress should be absent (not null)"
        );
        assert!(
            !json.contains("\"phase_stats\""),
            "phase_stats should be absent"
        );
    }

    #[test]
    fn test_new_report_fields_present_when_some() {
        let mut report = RetrospectiveReport {
            feature_cycle: "col-026".to_string(),
            session_count: 2,
            total_records: 20,
            metrics: MetricVector::default(),
            hotspots: vec![],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        };
        report.goal = Some("Design the API surface".to_string());
        report.cycle_type = Some("Design".to_string());
        report.attribution_path = Some("cycle_events-first (primary)".to_string());
        report.is_in_progress = Some(false);
        report.phase_stats = Some(vec![PhaseStats {
            phase: "scope".to_string(),
            pass_number: 1,
            pass_count: 1,
            duration_secs: 600,
            start_ms: 1_700_000_000_000,
            end_ms: Some(1_700_000_600_000),
            session_count: 1,
            record_count: 10,
            agents: vec![],
            tool_distribution: ToolDistribution::default(),
            knowledge_served: 0,
            knowledge_stored: 0,
            gate_result: GateResult::Pass,
            gate_outcome_text: None,
            hotspot_ids: vec![],
        }]);
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(json.contains("\"goal\""), "goal should be present");
        assert!(
            json.contains("\"cycle_type\""),
            "cycle_type should be present"
        );
        assert!(
            json.contains("\"attribution_path\""),
            "attribution_path should be present"
        );
        assert!(
            json.contains("\"is_in_progress\""),
            "is_in_progress should be present"
        );
        assert!(
            json.contains("\"phase_stats\""),
            "phase_stats should be present"
        );

        let back: RetrospectiveReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.goal.as_deref(), Some("Design the API surface"));
        assert_eq!(back.cycle_type.as_deref(), Some("Design"));
        assert_eq!(
            back.attribution_path.as_deref(),
            Some("cycle_events-first (primary)")
        );
        assert_eq!(back.is_in_progress, Some(false));
        assert!(back.phase_stats.is_some());
    }

    #[test]
    fn test_is_in_progress_serde_roundtrip_none() {
        // Deserializing JSON that lacks is_in_progress must yield None, not Some(false)
        let json = r#"{
            "feature_cycle": "col-026",
            "session_count": 1,
            "total_records": 0,
            "metrics": {"computed_at": 0, "universal": {}, "phases": {}},
            "hotspots": [],
            "is_cached": false
        }"#;
        let report: RetrospectiveReport =
            serde_json::from_str(json).expect("missing is_in_progress should deserialize");
        assert!(
            report.is_in_progress.is_none(),
            "is_in_progress must be None, not Some(false)"
        );
    }

    #[test]
    fn test_phase_stats_none_absent_from_json() {
        let mut report = RetrospectiveReport {
            feature_cycle: "col-026".to_string(),
            session_count: 0,
            total_records: 0,
            metrics: MetricVector::default(),
            hotspots: vec![],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        };
        report.phase_stats = None;
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(
            !json.contains("\"phase_stats\""),
            "phase_stats key must be absent when None"
        );
    }

    #[test]
    fn test_phase_stats_some_empty_present_in_json() {
        let mut report = RetrospectiveReport {
            feature_cycle: "col-026".to_string(),
            session_count: 0,
            total_records: 0,
            metrics: MetricVector::default(),
            hotspots: vec![],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        };
        report.phase_stats = Some(vec![]);
        let json = serde_json::to_string(&report).expect("serialize");
        assert!(
            json.contains("\"phase_stats\":[]"),
            "phase_stats key must be present with empty array when Some([])"
        );
    }

    #[test]
    fn test_knowledge_reuse_serde_backward_compat() {
        // Old JSON without new fields — all must default to 0 / empty vec
        let json =
            r#"{"delivery_count":5,"cross_session_count":2,"by_category":{},"category_gaps":[]}"#;
        let reuse: FeatureKnowledgeReuse =
            serde_json::from_str(json).expect("old JSON should deserialize");
        assert_eq!(reuse.cross_feature_reuse, 0);
        assert_eq!(reuse.intra_cycle_reuse, 0);
        assert_eq!(reuse.total_stored, 0);
        assert_eq!(reuse.total_served, 0);
        assert!(reuse.top_cross_feature_entries.is_empty());
    }

    // ── AC-02 [GATE]: triple-alias serde chain (crt-049) ─────────────────────

    #[test]
    fn test_search_exposure_count_deserializes_from_canonical_key() {
        // AC-02 sub-case (a): canonical key "search_exposure_count"
        let json = r#"{"search_exposure_count":42,"by_category":{},"category_gaps":[]}"#;
        let r: FeatureKnowledgeReuse =
            serde_json::from_str(json).expect("canonical key must deserialize");
        assert_eq!(r.search_exposure_count, 42);
    }

    #[test]
    fn test_search_exposure_count_deserializes_from_delivery_count_alias() {
        // AC-02 sub-case (b): alias "delivery_count" (pre-crt-049 stored rows)
        let json = r#"{"delivery_count":42,"by_category":{},"category_gaps":[]}"#;
        let r: FeatureKnowledgeReuse =
            serde_json::from_str(json).expect("delivery_count alias must deserialize");
        assert_eq!(
            r.search_exposure_count, 42,
            "alias delivery_count must resolve to search_exposure_count — not 0"
        );
    }

    #[test]
    fn test_search_exposure_count_deserializes_from_tier1_reuse_count_alias() {
        // AC-02 sub-case (c): alias "tier1_reuse_count" (pre-col-020b stored rows)
        let json = r#"{"tier1_reuse_count":42,"by_category":{},"category_gaps":[]}"#;
        let r: FeatureKnowledgeReuse =
            serde_json::from_str(json).expect("tier1_reuse_count alias must deserialize");
        assert_eq!(
            r.search_exposure_count, 42,
            "alias tier1_reuse_count must resolve to search_exposure_count — not 0"
        );
    }

    #[test]
    fn test_search_exposure_count_serializes_to_canonical_key() {
        // AC-02 sub-case (d): canonical serialization key is "search_exposure_count"
        let r = FeatureKnowledgeReuse {
            search_exposure_count: 99,
            explicit_read_count: 0,
            explicit_read_by_category: HashMap::new(),
            cross_session_count: 0,
            by_category: HashMap::new(),
            category_gaps: vec![],
            total_served: 0,
            total_stored: 0,
            cross_feature_reuse: 0,
            intra_cycle_reuse: 0,
            top_cross_feature_entries: vec![],
        };
        let json = serde_json::to_string(&r).expect("serialize");
        assert!(
            json.contains("\"search_exposure_count\""),
            "canonical key must be search_exposure_count in serialized output"
        );
        assert!(
            !json.contains("\"delivery_count\""),
            "old key delivery_count must NOT appear in serialized output"
        );
        assert!(
            !json.contains("\"tier1_reuse_count\""),
            "old key tier1_reuse_count must NOT appear in serialized output"
        );
    }

    #[test]
    fn test_search_exposure_count_round_trip_all_alias_forms() {
        // AC-02 sub-case (e): deserialize each alias form → serialize → deserialize
        // Final value must be 42 and canonical key must be used
        let alias_forms = [
            r#"{"search_exposure_count":42,"by_category":{},"category_gaps":[]}"#,
            r#"{"delivery_count":42,"by_category":{},"category_gaps":[]}"#,
            r#"{"tier1_reuse_count":42,"by_category":{},"category_gaps":[]}"#,
        ];
        for json in &alias_forms {
            let first: FeatureKnowledgeReuse =
                serde_json::from_str(json).expect("first deserialize must succeed");
            assert_eq!(first.search_exposure_count, 42, "input: {json}");
            let serialized = serde_json::to_string(&first).expect("serialize");
            assert!(
                serialized.contains("\"search_exposure_count\""),
                "re-serialized must use canonical key; input: {json}"
            );
            let back: FeatureKnowledgeReuse =
                serde_json::from_str(&serialized).expect("second deserialize must succeed");
            assert_eq!(
                back.search_exposure_count, 42,
                "round-trip value mismatch; input: {json}"
            );
        }
    }

    // ── AC-01 / AC-13 [GATE partial]: new field definitions (crt-049) ─────────

    #[test]
    fn test_explicit_read_count_defaults_to_zero_when_absent() {
        // AC-01: #[serde(default)] must apply when field absent from stored JSON
        let json = r#"{"search_exposure_count":0,"by_category":{},"category_gaps":[]}"#;
        let r: FeatureKnowledgeReuse =
            serde_json::from_str(json).expect("deserialize without explicit_read_count");
        assert_eq!(
            r.explicit_read_count, 0,
            "explicit_read_count must default to 0"
        );
        assert!(
            r.explicit_read_by_category.is_empty(),
            "explicit_read_by_category must default to empty map"
        );
    }

    #[test]
    fn test_explicit_read_by_category_defaults_to_empty_map_when_absent() {
        // AC-13 partial: field absent in stored JSON → empty map via #[serde(default)]
        let json = r#"{"search_exposure_count":0,"by_category":{},"category_gaps":[]}"#;
        let r: FeatureKnowledgeReuse =
            serde_json::from_str(json).expect("deserialize without explicit_read_by_category");
        assert!(
            r.explicit_read_by_category.is_empty(),
            "explicit_read_by_category must be empty map when absent"
        );
    }

    #[test]
    fn test_explicit_read_by_category_serde_round_trip() {
        // AC-13 partial: category map survives serde round-trip
        let mut cats = HashMap::new();
        cats.insert("decision".to_string(), 2u64);
        cats.insert("pattern".to_string(), 1u64);
        let r = FeatureKnowledgeReuse {
            search_exposure_count: 0,
            explicit_read_count: 3,
            explicit_read_by_category: cats,
            cross_session_count: 0,
            by_category: HashMap::new(),
            category_gaps: vec![],
            total_served: 3,
            total_stored: 0,
            cross_feature_reuse: 0,
            intra_cycle_reuse: 0,
            top_cross_feature_entries: vec![],
        };
        let json = serde_json::to_string(&r).expect("serialize");
        let back: FeatureKnowledgeReuse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            back.explicit_read_by_category.get("decision"),
            Some(&2),
            "decision category must survive round-trip"
        );
        assert_eq!(
            back.explicit_read_by_category.get("pattern"),
            Some(&1),
            "pattern category must survive round-trip"
        );
        assert_eq!(back.explicit_read_count, 3);
    }

    #[test]
    fn test_pre_col026_json_backward_compat() {
        // Full pre-col-026 JSON with none of the five new fields
        let json = r#"{
            "feature_cycle": "col-020",
            "session_count": 3,
            "total_records": 42,
            "metrics": {"computed_at": 0, "universal": {}, "phases": {}},
            "hotspots": [],
            "is_cached": false,
            "feature_knowledge_reuse": {
                "delivery_count": 5,
                "cross_session_count": 2,
                "by_category": {"decision": 3},
                "category_gaps": []
            }
        }"#;
        let report: RetrospectiveReport =
            serde_json::from_str(json).expect("pre-col-026 JSON should deserialize without panic");
        assert!(report.goal.is_none(), "goal must default to None");
        assert!(
            report.cycle_type.is_none(),
            "cycle_type must default to None"
        );
        assert!(
            report.attribution_path.is_none(),
            "attribution_path must default to None"
        );
        assert!(
            report.is_in_progress.is_none(),
            "is_in_progress must default to None"
        );
        assert!(
            report.phase_stats.is_none(),
            "phase_stats must default to None"
        );
        let reuse = report
            .feature_knowledge_reuse
            .expect("feature_knowledge_reuse present");
        assert_eq!(reuse.total_served, 0);
        assert_eq!(reuse.total_stored, 0);
        assert_eq!(reuse.cross_feature_reuse, 0);
        assert_eq!(reuse.intra_cycle_reuse, 0);
        assert!(reuse.top_cross_feature_entries.is_empty());
    }
}
