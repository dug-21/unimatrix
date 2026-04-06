//! Pure function for building a PhaseNarrative from raw cycle event data (crt-025).
//!
//! No I/O, no async. All inputs are &-refs; output is owned. Fully unit-testable
//! without a database connection.

use std::collections::HashMap;

use crate::types::{CycleEventRecord, PhaseCategoryComparison, PhaseCategoryDist, PhaseNarrative};

/// Build a [`PhaseNarrative`] from ordered cycle events and category distributions.
///
/// # Parameters
/// - `events`: `cycle_events` rows ordered by `(timestamp ASC, seq ASC)`.
/// - `current_dist`: feature_entries phase/category counts for the **current** feature.
/// - `cross_dist`: feature_entries phase/category counts keyed by `feature_id` for **prior**
///   features (current feature already excluded by the SQL `WHERE` clause).
///
/// # Guarantees
/// - Never panics on empty or malformed input.
/// - Returns a valid `PhaseNarrative` with empty collections when there are no events.
pub fn build_phase_narrative(
    events: &[CycleEventRecord],
    current_dist: &PhaseCategoryDist,
    cross_dist: &HashMap<String, PhaseCategoryDist>,
) -> PhaseNarrative {
    // === Part 1: Phase Sequence ===
    //
    // Walk events in order. A phase is "entered" when:
    //   - cycle_start has next_phase: the session is entering that phase
    //   - cycle_phase_end has next_phase: the next phase becomes active
    //
    // `phase` on cycle_phase_end is informational (the phase being completed);
    // it is not appended to the sequence — only the incoming next_phase is recorded.
    let mut phase_sequence: Vec<String> = Vec::new();

    for event in events {
        match event.event_type.as_str() {
            "cycle_start" => {
                if let Some(np) = &event.next_phase {
                    // Avoid immediate duplicates when start re-announces the current phase.
                    if phase_sequence.last().map(|s| s.as_str()) != Some(np.as_str()) {
                        phase_sequence.push(np.clone());
                    }
                }
            }
            "cycle_phase_end" => {
                if let Some(np) = &event.next_phase {
                    phase_sequence.push(np.clone());
                }
            }
            // cycle_stop and unknown event types: no phase to add.
            _ => {}
        }
    }

    // === Part 2: Rework Detection ===
    //
    // A phase is "rework" if it appears more than once in phase_sequence.
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for phase in &phase_sequence {
        *counts.entry(phase.as_str()).or_insert(0) += 1;
    }
    let mut rework_phases: Vec<String> = counts
        .into_iter()
        .filter_map(|(phase, count)| {
            if count > 1 {
                Some(phase.to_string())
            } else {
                None
            }
        })
        .collect();
    rework_phases.sort();

    // === Part 3: Per-Phase Categories ===
    //
    // Directly from current_dist (already HashMap<phase, HashMap<category, count>>).
    let per_phase_categories = current_dist.clone();

    // === Part 4: Cross-Cycle Comparison ===
    //
    // FR-10.2: omit when fewer than 2 prior features have phase-tagged rows.
    let sample_features_total = cross_dist.len();

    let cross_cycle_comparison = if sample_features_total < 2 {
        None
    } else {
        // Build a flat map: (phase, category) → Vec<count_per_feature>
        let mut sums: HashMap<(String, String), Vec<u64>> = HashMap::new();

        for dist in cross_dist.values() {
            for (phase, cat_map) in dist {
                for (category, &count) in cat_map {
                    sums.entry((phase.clone(), category.clone()))
                        .or_default()
                        .push(count);
                }
            }
        }

        // For each (phase, category) in the current feature, compute cross-cycle mean.
        let mut comparisons: Vec<PhaseCategoryComparison> = Vec::new();
        for (phase, cat_map) in current_dist {
            for (category, &this_count) in cat_map {
                let key = (phase.clone(), category.clone());
                if let Some(cross_counts) = sums.get(&key) {
                    let n = cross_counts.len();
                    let mean = cross_counts.iter().sum::<u64>() as f64 / n as f64;
                    comparisons.push(PhaseCategoryComparison {
                        phase: phase.clone(),
                        category: category.clone(),
                        this_feature_count: this_count,
                        cross_cycle_mean: mean,
                        sample_features: n,
                    });
                } else {
                    // No prior features had data for this (phase, category) pair.
                    comparisons.push(PhaseCategoryComparison {
                        phase: phase.clone(),
                        category: category.clone(),
                        this_feature_count: this_count,
                        cross_cycle_mean: 0.0,
                        sample_features: 0,
                    });
                }
            }
        }

        // Sort for deterministic output.
        comparisons.sort_by(|a, b| a.phase.cmp(&b.phase).then(a.category.cmp(&b.category)));

        Some(comparisons)
    };

    PhaseNarrative {
        phase_sequence,
        rework_phases,
        per_phase_categories,
        cross_cycle_comparison,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn make_event(
        event_type: &str,
        phase: Option<&str>,
        next_phase: Option<&str>,
    ) -> CycleEventRecord {
        CycleEventRecord {
            seq: 0,
            event_type: event_type.to_string(),
            phase: phase.map(str::to_string),
            outcome: None,
            next_phase: next_phase.map(str::to_string),
            timestamp: 0,
        }
    }

    fn make_event_ts(
        event_type: &str,
        phase: Option<&str>,
        next_phase: Option<&str>,
        seq: i64,
        timestamp: i64,
    ) -> CycleEventRecord {
        CycleEventRecord {
            seq,
            event_type: event_type.to_string(),
            phase: phase.map(str::to_string),
            outcome: None,
            next_phase: next_phase.map(str::to_string),
            timestamp,
        }
    }

    fn empty_dist() -> PhaseCategoryDist {
        HashMap::new()
    }

    fn empty_cross() -> HashMap<String, PhaseCategoryDist> {
        HashMap::new()
    }

    fn make_dist(entries: &[(&str, &str, u64)]) -> PhaseCategoryDist {
        let mut dist: PhaseCategoryDist = HashMap::new();
        for (phase, category, count) in entries {
            dist.entry(phase.to_string())
                .or_default()
                .insert(category.to_string(), *count);
        }
        dist
    }

    fn make_cross(features: &[(&str, &[(&str, &str, u64)])]) -> HashMap<String, PhaseCategoryDist> {
        features
            .iter()
            .map(|(fid, entries)| (fid.to_string(), make_dist(entries)))
            .collect()
    }

    // ── Type Definitions (compile-time) ─────────────────────────────────────

    #[test]
    fn test_phase_narrative_types_defined() {
        // Structural: assert types compile and fields are accessible.
        let rec = CycleEventRecord {
            seq: 1,
            event_type: "cycle_start".to_string(),
            phase: None,
            outcome: None,
            next_phase: Some("scope".to_string()),
            timestamp: 1000,
        };
        assert_eq!(rec.event_type, "cycle_start");

        let narrative = PhaseNarrative {
            phase_sequence: vec!["scope".to_string()],
            rework_phases: vec![],
            per_phase_categories: HashMap::new(),
            cross_cycle_comparison: None,
        };
        assert_eq!(narrative.phase_sequence.len(), 1);

        let cmp = PhaseCategoryComparison {
            phase: "scope".to_string(),
            category: "decision".to_string(),
            this_feature_count: 3,
            cross_cycle_mean: 2.5,
            sample_features: 4,
        };
        assert_eq!(cmp.sample_features, 4);
    }

    // ── Phase Sequence Construction ──────────────────────────────────────────

    #[test]
    fn test_build_phase_narrative_empty_events_empty_sequence() {
        let result = build_phase_narrative(&[], &empty_dist(), &empty_cross());
        assert!(
            result.phase_sequence.is_empty(),
            "phase_sequence should be empty"
        );
        assert!(
            result.rework_phases.is_empty(),
            "rework_phases should be empty"
        );
        assert!(
            result.per_phase_categories.is_empty(),
            "per_phase_categories should be empty"
        );
        assert!(
            result.cross_cycle_comparison.is_none(),
            "cross_cycle_comparison should be None"
        );
    }

    #[test]
    fn test_build_phase_narrative_start_with_next_phase() {
        let events = vec![make_event("cycle_start", None, Some("scope"))];
        let result = build_phase_narrative(&events, &empty_dist(), &empty_cross());
        assert_eq!(result.phase_sequence, vec!["scope"]);
    }

    #[test]
    fn test_build_phase_narrative_phase_end_transition() {
        let events = vec![
            make_event("cycle_start", None, Some("scope")),
            make_event("cycle_phase_end", Some("scope"), Some("design")),
        ];
        let result = build_phase_narrative(&events, &empty_dist(), &empty_cross());
        assert_eq!(result.phase_sequence, vec!["scope", "design"]);
    }

    #[test]
    fn test_build_phase_narrative_full_lifecycle() {
        let events = vec![
            make_event("cycle_start", None, Some("scope")),
            make_event("cycle_phase_end", Some("scope"), Some("design")),
            make_event("cycle_phase_end", Some("design"), Some("implementation")),
            make_event("cycle_stop", None, None),
        ];
        let result = build_phase_narrative(&events, &empty_dist(), &empty_cross());
        assert_eq!(
            result.phase_sequence,
            vec!["scope", "design", "implementation"]
        );
    }

    // ── Rework Detection ────────────────────────────────────────────────────

    #[test]
    fn test_build_phase_narrative_rework_phase_detected() {
        // scope → design → scope (rework)
        let events = vec![
            make_event("cycle_start", None, Some("scope")),
            make_event("cycle_phase_end", Some("scope"), Some("design")),
            make_event("cycle_phase_end", Some("design"), Some("scope")),
        ];
        let result = build_phase_narrative(&events, &empty_dist(), &empty_cross());
        assert_eq!(result.phase_sequence, vec!["scope", "design", "scope"]);
        assert_eq!(result.rework_phases, vec!["scope"]);
    }

    #[test]
    fn test_build_phase_narrative_no_rework_no_rework_phases() {
        let events = vec![
            make_event("cycle_start", None, Some("scope")),
            make_event("cycle_phase_end", Some("scope"), Some("design")),
            make_event("cycle_phase_end", Some("design"), Some("implementation")),
        ];
        let result = build_phase_narrative(&events, &empty_dist(), &empty_cross());
        assert!(result.rework_phases.is_empty());
    }

    // ── Orphaned Events (R-13) ───────────────────────────────────────────────

    #[test]
    fn test_build_phase_narrative_orphaned_phase_end_no_start() {
        // phase_end with no prior cycle_start — no panic, next_phase added
        let events = vec![make_event("cycle_phase_end", Some("scope"), Some("design"))];
        let result = build_phase_narrative(&events, &empty_dist(), &empty_cross());
        assert!(
            !result.phase_sequence.is_empty(),
            "should have at least one phase"
        );
        assert!(
            result.phase_sequence.contains(&"design".to_string()),
            "next_phase 'design' should be in sequence"
        );
    }

    #[test]
    fn test_build_phase_narrative_phase_end_only_sequence() {
        // Only cycle_phase_end events (no cycle_start) — must not panic
        let events = vec![
            make_event("cycle_phase_end", Some("scope"), Some("design")),
            make_event("cycle_phase_end", Some("design"), Some("implementation")),
        ];
        let result = build_phase_narrative(&events, &empty_dist(), &empty_cross());
        assert_eq!(result.phase_sequence, vec!["design", "implementation"]);
    }

    // ── Per-Phase Category Distribution ─────────────────────────────────────

    #[test]
    fn test_build_phase_narrative_per_phase_categories() {
        let dist = make_dist(&[("scope", "decision", 3), ("design", "pattern", 5)]);
        let result = build_phase_narrative(&[], &dist, &empty_cross());
        assert_eq!(result.per_phase_categories["scope"]["decision"], 3);
        assert_eq!(result.per_phase_categories["design"]["pattern"], 5);
    }

    #[test]
    fn test_build_phase_narrative_empty_entries_no_categories() {
        // Events exist but current_dist is empty
        let events = vec![make_event("cycle_start", None, Some("scope"))];
        let result = build_phase_narrative(&events, &empty_dist(), &empty_cross());
        assert!(result.per_phase_categories.is_empty());
    }

    // ── Cross-Cycle Comparison (R-04, FR-10) ────────────────────────────────

    #[test]
    fn test_cross_cycle_comparison_none_when_zero_prior_features() {
        let result = build_phase_narrative(&[], &empty_dist(), &empty_cross());
        assert!(result.cross_cycle_comparison.is_none());
    }

    #[test]
    fn test_cross_cycle_comparison_none_when_one_prior_feature() {
        let cross = make_cross(&[("f1", &[("design", "decision", 2)])]);
        let result = build_phase_narrative(&[], &empty_dist(), &cross);
        assert!(
            result.cross_cycle_comparison.is_none(),
            "below 2-feature threshold should be None"
        );
    }

    #[test]
    fn test_cross_cycle_comparison_some_when_two_prior_features() {
        let current = make_dist(&[("design", "decision", 5)]);
        let cross = make_cross(&[
            ("f1", &[("design", "decision", 2)]),
            ("f2", &[("design", "decision", 2)]),
        ]);
        let result = build_phase_narrative(&[], &current, &cross);
        let comparisons = result
            .cross_cycle_comparison
            .expect("should have comparisons");
        assert_eq!(comparisons.len(), 1);
        assert_eq!(comparisons[0].phase, "design");
        assert_eq!(comparisons[0].category, "decision");
        assert!((comparisons[0].cross_cycle_mean - 2.0).abs() < f64::EPSILON);
        assert_eq!(comparisons[0].sample_features, 2);
    }

    #[test]
    fn test_cross_cycle_comparison_correct_mean() {
        // current: design/decision = 10; priors f1=2, f2=2
        // mean = (2 + 2) / 2 = 2.0 (current feature excluded)
        let current = make_dist(&[("design", "decision", 10)]);
        let cross = make_cross(&[
            ("f1", &[("design", "decision", 2)]),
            ("f2", &[("design", "decision", 2)]),
        ]);
        let result = build_phase_narrative(&[], &current, &cross);
        let comparisons = result
            .cross_cycle_comparison
            .expect("should have comparisons");
        let entry = comparisons
            .iter()
            .find(|c| c.phase == "design" && c.category == "decision")
            .expect("entry should exist");
        assert!(
            (entry.cross_cycle_mean - 2.0).abs() < f64::EPSILON,
            "mean should be 2.0, got {}",
            entry.cross_cycle_mean
        );
        assert_eq!(entry.sample_features, 2);
        assert_eq!(entry.this_feature_count, 10);
    }

    #[test]
    fn test_cross_cycle_excludes_current_feature_data() {
        // Explicitly verify current feature is not in cross-cycle mean computation.
        // current_dist has 10; cross has 2 features of 2 each.
        // Mean should be 2.0, not (10+2+2)/3 = 4.67.
        let current = make_dist(&[("design", "decision", 10)]);
        let cross = make_cross(&[
            ("f1", &[("design", "decision", 2)]),
            ("f2", &[("design", "decision", 2)]),
        ]);
        let result = build_phase_narrative(&[], &current, &cross);
        let comparisons = result.cross_cycle_comparison.expect("Some");
        let entry = comparisons
            .iter()
            .find(|c| c.phase == "design" && c.category == "decision")
            .unwrap();
        assert!(
            (entry.cross_cycle_mean - 2.0).abs() < f64::EPSILON,
            "current feature must not influence cross-cycle mean"
        );
    }

    // ── sample_features per (phase, category) ───────────────────────────────

    #[test]
    fn test_sample_features_reflects_distinct_feature_count_for_pair() {
        // f1 and f2 have design/decision; only f1 has scope/pattern
        let current = make_dist(&[("design", "decision", 5), ("scope", "pattern", 3)]);
        let cross = make_cross(&[
            ("f1", &[("design", "decision", 2), ("scope", "pattern", 1)]),
            ("f2", &[("design", "decision", 3)]),
        ]);
        let result = build_phase_narrative(&[], &current, &cross);
        let comparisons = result.cross_cycle_comparison.expect("Some");

        let design_decision = comparisons
            .iter()
            .find(|c| c.phase == "design" && c.category == "decision")
            .unwrap();
        // Both f1 and f2 have (design, decision)
        assert_eq!(design_decision.sample_features, 2);

        let scope_pattern = comparisons
            .iter()
            .find(|c| c.phase == "scope" && c.category == "pattern")
            .unwrap();
        // Only f1 has (scope, pattern)
        assert_eq!(scope_pattern.sample_features, 1);
    }

    // ── RetrospectiveReport Serialization (R-08) ────────────────────────────

    #[test]
    fn test_retrospective_report_phase_narrative_none_omitted() {
        use crate::types::RetrospectiveReport;
        use unimatrix_store::MetricVector;

        let report = RetrospectiveReport {
            feature_cycle: "feat-001".to_string(),
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

        let json = serde_json::to_string(&report).expect("serialize");
        assert!(
            !json.contains("phase_narrative"),
            "phase_narrative must be absent (not null) when None; got: {}",
            json
        );
    }

    #[test]
    fn test_retrospective_report_phase_narrative_some_serialized() {
        use crate::types::RetrospectiveReport;
        use unimatrix_store::MetricVector;

        let narrative = PhaseNarrative {
            phase_sequence: vec!["scope".to_string(), "design".to_string()],
            rework_phases: vec![],
            per_phase_categories: HashMap::new(),
            cross_cycle_comparison: None,
        };

        let report = RetrospectiveReport {
            feature_cycle: "feat-002".to_string(),
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
            phase_narrative: Some(narrative),
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        };

        let json = serde_json::to_string(&report).expect("serialize");
        assert!(
            json.contains("phase_narrative"),
            "phase_narrative key must be present when Some"
        );
        assert!(
            json.contains("phase_sequence"),
            "phase_sequence must be serialized"
        );
    }

    #[test]
    fn test_retrospective_report_phase_narrative_backward_compat() {
        // Pre-crt-025 JSON without phase_narrative should deserialize without error
        let json = r#"{
            "feature_cycle": "old-feature",
            "session_count": 2,
            "total_records": 20,
            "metrics": {"computed_at": 0, "universal": {}, "phases": {}},
            "hotspots": [],
            "is_cached": false
        }"#;

        let report: crate::types::RetrospectiveReport =
            serde_json::from_str(json).expect("pre-crt-025 JSON should deserialize");
        assert!(
            report.phase_narrative.is_none(),
            "phase_narrative should default to None via #[serde(default)]"
        );
    }

    // ── Timestamp+seq ordering correctness ──────────────────────────────────

    #[test]
    fn test_phase_sequence_follows_timestamp_order() {
        // Events inserted with timestamps out of seq order to verify the caller
        // is expected to pass them pre-sorted; pure function does not re-sort.
        // (Sorting responsibility is on the SQL query: ORDER BY timestamp ASC, seq ASC)
        // This test verifies the function processes in the order given.
        let events = vec![
            make_event_ts("cycle_start", None, Some("scope"), 0, 1000),
            make_event_ts("cycle_phase_end", Some("scope"), Some("design"), 1, 2000),
            make_event_ts(
                "cycle_phase_end",
                Some("design"),
                Some("implementation"),
                2,
                3000,
            ),
        ];
        let result = build_phase_narrative(&events, &empty_dist(), &empty_cross());
        assert_eq!(
            result.phase_sequence,
            vec!["scope", "design", "implementation"]
        );
    }

    // ── Cross-cycle with no matching pair ────────────────────────────────────

    #[test]
    fn test_cross_cycle_pair_not_in_priors_zero_mean() {
        // current has scope/decision; priors have only design/decision
        let current = make_dist(&[("scope", "decision", 4)]);
        let cross = make_cross(&[
            ("f1", &[("design", "decision", 2)]),
            ("f2", &[("design", "decision", 3)]),
        ]);
        let result = build_phase_narrative(&[], &current, &cross);
        let comparisons = result.cross_cycle_comparison.expect("Some with 2 priors");
        let entry = comparisons
            .iter()
            .find(|c| c.phase == "scope" && c.category == "decision")
            .expect("entry for scope/decision should exist");
        assert!((entry.cross_cycle_mean - 0.0).abs() < f64::EPSILON);
        assert_eq!(entry.sample_features, 0);
    }
}
