//! Metric types for the observation pipeline (nxs-009).
//!
//! `MetricVector`, `UniversalMetrics`, and `PhaseMetrics` are defined here
//! in the store crate (ADR-001) following the `EntryRecord` pattern.
//! Re-exported by `unimatrix-observe` and `unimatrix-core` for backward compatibility.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// SQL column names for `UniversalMetrics`, in declaration order.
/// Used by the column-field alignment structural test (R-03, C-06).
///
/// The first 21 entries correspond exactly to the 21 typed fields of `UniversalMetrics`.
/// The 22nd entry `"domain_metrics_json"` is the extension column added in schema v14
/// (ADR-006). It is excluded from the field-name-to-column alignment check and verified
/// separately by name.
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
    // 22nd entry: extension JSON column for non-claude-code domain metrics (schema v14, ADR-006).
    // Excluded from the typed field-alignment check; verified separately by name.
    "domain_metrics_json",
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
    /// Extension metrics from non-claude-code domain packs, keyed by
    /// `"<source_domain>.<metric_name>"`. Always empty for claude-code sessions.
    /// Stored as `domain_metrics_json` (nullable TEXT) in `observation_metrics` (schema v14, ADR-006).
    /// `#[serde(default)]` ensures v13 rows (without the column) deserialize as empty map.
    #[serde(default)]
    pub domain_metrics: HashMap<String, f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // T-MET-01: UNIVERSAL_METRICS_FIELDS count == 22 (R-11, AC-10)
    // -------------------------------------------------------------------------
    #[test]
    fn test_universal_metrics_fields_count_is_22() {
        assert_eq!(
            UNIVERSAL_METRICS_FIELDS.len(),
            22,
            "UNIVERSAL_METRICS_FIELDS must have 22 entries (21 typed + domain_metrics_json)"
        );
    }

    // -------------------------------------------------------------------------
    // T-MET-02: Original 21 field names preserved in declaration order (R-11)
    // -------------------------------------------------------------------------
    #[test]
    fn test_universal_metrics_fields_original_21_unchanged() {
        let expected_original_21: &[&str] = &[
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
        assert_eq!(
            &UNIVERSAL_METRICS_FIELDS[0..21],
            expected_original_21,
            "First 21 entries of UNIVERSAL_METRICS_FIELDS must be unchanged from v13"
        );
    }

    // -------------------------------------------------------------------------
    // T-MET-03: domain_metrics_json is the 22nd entry (R-11)
    // -------------------------------------------------------------------------
    #[test]
    fn test_universal_metrics_fields_22nd_is_domain_metrics_json() {
        assert_eq!(
            UNIVERSAL_METRICS_FIELDS[21], "domain_metrics_json",
            "22nd entry must be 'domain_metrics_json'"
        );
    }

    // -------------------------------------------------------------------------
    // T-MET-04: Negative test documentation — removing a field would fail T-MET-01
    // (R-11 validation): the assertion in T-MET-01 is the guard.
    // This test serves as compile-time documentation only; T-MET-01 enforces it.
    // -------------------------------------------------------------------------
    #[test]
    fn test_universal_metrics_fields_negative_removal_detected() {
        // If UNIVERSAL_METRICS_FIELDS were shortened to 21, T-MET-01 would fail.
        // We verify the invariant by confirming the length is still >= 22.
        assert!(
            UNIVERSAL_METRICS_FIELDS.len() >= 22,
            "Removing any entry from UNIVERSAL_METRICS_FIELDS breaks the structural test"
        );
    }

    // -------------------------------------------------------------------------
    // T-MET-05: MetricVector has domain_metrics HashMap field (compile-time check)
    // -------------------------------------------------------------------------
    #[test]
    fn test_metric_vector_has_domain_metrics_field() {
        let mv = MetricVector {
            computed_at: 0,
            universal: UniversalMetrics::default(),
            phases: BTreeMap::new(),
            domain_metrics: HashMap::new(),
        };
        // Compile-time: field exists and accepts HashMap<String, f64>.
        assert!(mv.domain_metrics.is_empty());
    }

    // -------------------------------------------------------------------------
    // T-MET-05b: MetricVector serde round-trip preserves domain_metrics (ADR-006)
    // -------------------------------------------------------------------------
    #[test]
    fn test_metric_vector_serde_round_trip_with_domain_metrics() {
        let mut mv = MetricVector::default();
        mv.domain_metrics.insert("sre.error_rate".to_string(), 0.05);
        mv.domain_metrics.insert("sre.mttr_secs".to_string(), 300.0);

        let json = serde_json::to_string(&mv).expect("serialize");
        let back: MetricVector = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.domain_metrics.get("sre.error_rate"), Some(&0.05));
        assert_eq!(back.domain_metrics.get("sre.mttr_secs"), Some(&300.0));
    }

    // -------------------------------------------------------------------------
    // T-MET-05c: MetricVector deserialization without domain_metrics (v13 compat)
    // Simulates deserializing a JSON blob that lacks the domain_metrics field.
    // #[serde(default)] must produce HashMap::new().
    // -------------------------------------------------------------------------
    #[test]
    fn test_metric_vector_deserialize_without_domain_metrics_field() {
        // JSON without domain_metrics key — simulates pre-v14 serialized data.
        let json = r#"{"computed_at":42,"universal":{},"phases":{}}"#;
        let mv: MetricVector = serde_json::from_str(json).expect("deserialize");
        assert!(
            mv.domain_metrics.is_empty(),
            "Missing domain_metrics in JSON must deserialize as empty HashMap"
        );
    }
}
