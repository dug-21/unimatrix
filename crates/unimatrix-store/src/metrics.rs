//! Metric types for the observation pipeline (nxs-009).
//!
//! `MetricVector`, `UniversalMetrics`, and `PhaseMetrics` are defined here
//! in the store crate (ADR-001) following the `EntryRecord` pattern.
//! Re-exported by `unimatrix-observe` and `unimatrix-core` for backward compatibility.

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

/// SQL column names for `UniversalMetrics`, in declaration order.
/// Used by the column-field alignment structural test (R-03, C-06).
pub const UNIVERSAL_METRICS_FIELDS: &[&str] = &[
    "total_tool_calls",
    "total_duration_secs",
    "session_count",
    "search_miss_rate",
    "edit_bloat_total_kb",
    "edit_bloat_ratio",
    "permission_friction_events",
    "bash_for_search_count",
    "cold_restart_events",
    "coordinator_respawn_count",
    "parallel_call_rate",
    "context_load_before_first_write_kb",
    "total_context_loaded_kb",
    "post_completion_work_pct",
    "follow_up_issues_created",
    "knowledge_entries_stored",
    "sleep_workaround_count",
    "agent_hotspot_count",
    "friction_hotspot_count",
    "session_hotspot_count",
    "scope_hotspot_count",
];

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
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MetricVector {
    #[serde(default)]
    pub computed_at: u64,
    #[serde(default)]
    pub universal: UniversalMetrics,
    #[serde(default)]
    pub phases: BTreeMap<String, PhaseMetrics>,
}
