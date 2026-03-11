//! Baseline computation and comparison for MetricVectors.
//!
//! Computes per-metric mean/stddev from historical feature retrospectives
//! and compares current metrics against those baselines.
//! ADR-003: explicit guards for zero-stddev, NaN prevention, four status modes.

use std::collections::HashMap;

use crate::types::{BaselineComparison, BaselineEntry, BaselineSet, BaselineStatus, MetricVector};

/// Minimum number of historical MetricVectors required for baseline computation.
const MIN_HISTORY: usize = 3;

/// Compute statistical baselines from historical MetricVectors.
///
/// Returns `None` if fewer than `MIN_HISTORY` vectors are provided.
/// Uses population standard deviation (divide by n).
pub fn compute_baselines(history: &[MetricVector]) -> Option<BaselineSet> {
    if history.len() < MIN_HISTORY {
        return None;
    }

    // Compute universal metric baselines
    let universal = compute_universal_baselines(history);

    // Compute phase-specific baselines
    let phases = compute_phase_baselines(history);

    Some(BaselineSet { universal, phases })
}

/// Compare current metrics against baselines, returning per-metric comparisons.
///
/// Outlier threshold: mean + 1.5 * stddev (ADR-003).
pub fn compare_to_baseline(
    current: &MetricVector,
    baselines: &BaselineSet,
) -> Vec<BaselineComparison> {
    let mut comparisons = Vec::new();

    // Compare universal metrics
    for (name, extractor) in universal_metric_extractors() {
        let current_value = extractor(current);
        if let Some(entry) = baselines.universal.get(name) {
            comparisons.push(make_comparison(name, current_value, entry, None));
        }
    }

    // Compare phase-specific metrics
    for (phase_name, phase_metrics) in &current.phases {
        if let Some(phase_baselines) = baselines.phases.get(phase_name) {
            if let Some(entry) = phase_baselines.get("duration_secs") {
                comparisons.push(make_comparison(
                    "duration_secs",
                    phase_metrics.duration_secs as f64,
                    entry,
                    Some(phase_name.clone()),
                ));
            }
            if let Some(entry) = phase_baselines.get("tool_call_count") {
                comparisons.push(make_comparison(
                    "tool_call_count",
                    phase_metrics.tool_call_count as f64,
                    entry,
                    Some(phase_name.clone()),
                ));
            }
        }
    }

    comparisons
}

// -- Internal helpers --

type MetricExtractor = (&'static str, fn(&MetricVector) -> f64);

fn universal_metric_extractors() -> Vec<MetricExtractor> {
    vec![
        ("total_tool_calls", |mv| {
            mv.universal.total_tool_calls as f64
        }),
        ("total_duration_secs", |mv| {
            mv.universal.total_duration_secs as f64
        }),
        ("session_count", |mv| mv.universal.session_count as f64),
        ("search_miss_rate", |mv| mv.universal.search_miss_rate),
        ("edit_bloat_total_kb", |mv| mv.universal.edit_bloat_total_kb),
        ("edit_bloat_ratio", |mv| mv.universal.edit_bloat_ratio),
        ("permission_friction_events", |mv| {
            mv.universal.permission_friction_events as f64
        }),
        ("bash_for_search_count", |mv| {
            mv.universal.bash_for_search_count as f64
        }),
        ("cold_restart_events", |mv| {
            mv.universal.cold_restart_events as f64
        }),
        ("coordinator_respawn_count", |mv| {
            mv.universal.coordinator_respawn_count as f64
        }),
        ("parallel_call_rate", |mv| mv.universal.parallel_call_rate),
        ("context_load_before_first_write_kb", |mv| {
            mv.universal.context_load_before_first_write_kb
        }),
        ("total_context_loaded_kb", |mv| {
            mv.universal.total_context_loaded_kb
        }),
        ("post_completion_work_pct", |mv| {
            mv.universal.post_completion_work_pct
        }),
        ("follow_up_issues_created", |mv| {
            mv.universal.follow_up_issues_created as f64
        }),
        ("knowledge_entries_stored", |mv| {
            mv.universal.knowledge_entries_stored as f64
        }),
        ("sleep_workaround_count", |mv| {
            mv.universal.sleep_workaround_count as f64
        }),
        ("agent_hotspot_count", |mv| {
            mv.universal.agent_hotspot_count as f64
        }),
        ("friction_hotspot_count", |mv| {
            mv.universal.friction_hotspot_count as f64
        }),
        ("session_hotspot_count", |mv| {
            mv.universal.session_hotspot_count as f64
        }),
        ("scope_hotspot_count", |mv| {
            mv.universal.scope_hotspot_count as f64
        }),
    ]
}

fn compute_universal_baselines(history: &[MetricVector]) -> HashMap<String, BaselineEntry> {
    let mut universal = HashMap::new();

    for (name, extractor) in universal_metric_extractors() {
        let values: Vec<f64> = history.iter().map(extractor).collect();
        universal.insert(name.to_string(), compute_entry(&values));
    }

    universal
}

fn compute_phase_baselines(
    history: &[MetricVector],
) -> HashMap<String, HashMap<String, BaselineEntry>> {
    let mut phase_durations: HashMap<String, Vec<f64>> = HashMap::new();
    let mut phase_tool_calls: HashMap<String, Vec<f64>> = HashMap::new();

    for mv in history {
        for (phase_name, phase_metrics) in &mv.phases {
            phase_durations
                .entry(phase_name.clone())
                .or_default()
                .push(phase_metrics.duration_secs as f64);
            phase_tool_calls
                .entry(phase_name.clone())
                .or_default()
                .push(phase_metrics.tool_call_count as f64);
        }
    }

    let mut phases = HashMap::new();

    for (phase_name, durations) in &phase_durations {
        let mut phase_baselines = HashMap::new();

        if durations.len() >= MIN_HISTORY {
            phase_baselines.insert("duration_secs".to_string(), compute_entry(durations));
        }
        if let Some(tool_calls) = phase_tool_calls.get(phase_name) {
            if tool_calls.len() >= MIN_HISTORY {
                phase_baselines.insert("tool_call_count".to_string(), compute_entry(tool_calls));
            }
        }

        if !phase_baselines.is_empty() {
            phases.insert(phase_name.clone(), phase_baselines);
        }
    }

    phases
}

fn compute_entry(values: &[f64]) -> BaselineEntry {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let stddev = if variance > 0.0 { variance.sqrt() } else { 0.0 };

    BaselineEntry {
        mean,
        stddev,
        sample_count: values.len(),
    }
}

/// Create a BaselineComparison with ADR-003 arithmetic guards.
fn make_comparison(
    metric_name: &str,
    current_value: f64,
    entry: &BaselineEntry,
    phase: Option<String>,
) -> BaselineComparison {
    let (is_outlier, status) = if entry.stddev == 0.0 && entry.mean == 0.0 {
        // Zero mean, zero stddev: any non-zero value is "new signal"
        if current_value != 0.0 {
            (false, BaselineStatus::NewSignal)
        } else {
            (false, BaselineStatus::Normal)
        }
    } else if entry.stddev == 0.0 {
        // Non-zero mean, zero stddev: "no variance"
        (false, BaselineStatus::NoVariance)
    } else {
        // Normal case: check outlier threshold
        let threshold = entry.mean + 1.5 * entry.stddev;
        let outlier = current_value > threshold;
        let status = if outlier {
            BaselineStatus::Outlier
        } else {
            BaselineStatus::Normal
        };
        (outlier, status)
    };

    BaselineComparison {
        metric_name: metric_name.to_string(),
        current_value,
        mean: entry.mean,
        stddev: entry.stddev,
        is_outlier,
        status,
        phase,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PhaseMetrics, UniversalMetrics};
    use std::collections::BTreeMap;

    fn make_mv(total_tool_calls: u64, session_count: u64) -> MetricVector {
        MetricVector {
            computed_at: 0,
            universal: UniversalMetrics {
                total_tool_calls,
                session_count,
                ..Default::default()
            },
            phases: BTreeMap::new(),
        }
    }

    fn make_mv_with_phases(phases: BTreeMap<String, PhaseMetrics>) -> MetricVector {
        MetricVector {
            computed_at: 0,
            universal: UniversalMetrics::default(),
            phases,
        }
    }

    // -- compute_baselines --

    #[test]
    fn test_compute_baselines_min_3() {
        assert!(compute_baselines(&[]).is_none());
        assert!(compute_baselines(&[MetricVector::default()]).is_none());
        assert!(compute_baselines(&[MetricVector::default(), MetricVector::default()]).is_none());
    }

    #[test]
    fn test_compute_baselines_3_vectors() {
        let history = vec![make_mv(10, 2), make_mv(20, 4), make_mv(30, 6)];
        let baselines = compute_baselines(&history).expect("should have baselines");

        let tc = baselines.universal.get("total_tool_calls").unwrap();
        assert!((tc.mean - 20.0).abs() < f64::EPSILON);
        assert_eq!(tc.sample_count, 3);
        // stddev: sqrt(((10-20)^2 + (20-20)^2 + (30-20)^2) / 3) = sqrt(200/3) ~= 8.165
        assert!((tc.stddev - 8.16496580927726).abs() < 0.001);

        let sc = baselines.universal.get("session_count").unwrap();
        assert!((sc.mean - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_baselines_identical_values() {
        let history = vec![make_mv(42, 3), make_mv(42, 3), make_mv(42, 3)];
        let baselines = compute_baselines(&history).unwrap();

        let tc = baselines.universal.get("total_tool_calls").unwrap();
        assert!((tc.mean - 42.0).abs() < f64::EPSILON);
        assert!((tc.stddev - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_baselines_all_zeros() {
        let history = vec![make_mv(0, 0), make_mv(0, 0), make_mv(0, 0)];
        let baselines = compute_baselines(&history).unwrap();

        let tc = baselines.universal.get("total_tool_calls").unwrap();
        assert!((tc.mean - 0.0).abs() < f64::EPSILON);
        assert!((tc.stddev - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_baselines_phase_specific() {
        let mut phases_a = BTreeMap::new();
        phases_a.insert(
            "3a".to_string(),
            PhaseMetrics {
                duration_secs: 100,
                tool_call_count: 10,
            },
        );
        phases_a.insert(
            "3b".to_string(),
            PhaseMetrics {
                duration_secs: 50,
                tool_call_count: 5,
            },
        );

        let mut phases_b = BTreeMap::new();
        phases_b.insert(
            "3a".to_string(),
            PhaseMetrics {
                duration_secs: 200,
                tool_call_count: 20,
            },
        );
        phases_b.insert(
            "3b".to_string(),
            PhaseMetrics {
                duration_secs: 60,
                tool_call_count: 6,
            },
        );

        let mut phases_c = BTreeMap::new();
        phases_c.insert(
            "3a".to_string(),
            PhaseMetrics {
                duration_secs: 300,
                tool_call_count: 30,
            },
        );
        phases_c.insert(
            "3b".to_string(),
            PhaseMetrics {
                duration_secs: 70,
                tool_call_count: 7,
            },
        );

        let history = vec![
            make_mv_with_phases(phases_a),
            make_mv_with_phases(phases_b),
            make_mv_with_phases(phases_c),
        ];

        let baselines = compute_baselines(&history).unwrap();

        // Phase "3a": durations [100, 200, 300] -> mean=200
        let phase_3a = baselines.phases.get("3a").unwrap();
        let dur = phase_3a.get("duration_secs").unwrap();
        assert!((dur.mean - 200.0).abs() < f64::EPSILON);

        // Phase "3b": durations [50, 60, 70] -> mean=60
        let phase_3b = baselines.phases.get("3b").unwrap();
        let dur = phase_3b.get("duration_secs").unwrap();
        assert!((dur.mean - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compute_baselines_phase_insufficient_data() {
        // Only 2 vectors have phase "3a", which is < MIN_HISTORY
        let mut phases_a = BTreeMap::new();
        phases_a.insert(
            "3a".to_string(),
            PhaseMetrics {
                duration_secs: 100,
                tool_call_count: 10,
            },
        );

        let history = vec![
            make_mv_with_phases(phases_a.clone()),
            make_mv_with_phases(phases_a),
            MetricVector::default(), // no phases
        ];

        let baselines = compute_baselines(&history).unwrap();
        assert!(baselines.phases.get("3a").is_none());
    }

    // -- compare_to_baseline --

    #[test]
    fn test_compare_outlier() {
        let entry = BaselineEntry {
            mean: 100.0,
            stddev: 20.0,
            sample_count: 5,
        };
        let comp = make_comparison("test", 140.0, &entry, None);
        // threshold = 100 + 1.5 * 20 = 130. Current 140 > 130 -> outlier
        assert!(comp.is_outlier);
        assert_eq!(comp.status, BaselineStatus::Outlier);
    }

    #[test]
    fn test_compare_normal() {
        let entry = BaselineEntry {
            mean: 100.0,
            stddev: 20.0,
            sample_count: 5,
        };
        let comp = make_comparison("test", 120.0, &entry, None);
        // threshold = 130. Current 120 < 130 -> normal
        assert!(!comp.is_outlier);
        assert_eq!(comp.status, BaselineStatus::Normal);
    }

    #[test]
    fn test_compare_no_variance() {
        let entry = BaselineEntry {
            mean: 100.0,
            stddev: 0.0,
            sample_count: 5,
        };
        let comp = make_comparison("test", 100.0, &entry, None);
        assert!(!comp.is_outlier);
        assert_eq!(comp.status, BaselineStatus::NoVariance);
    }

    #[test]
    fn test_compare_new_signal() {
        let entry = BaselineEntry {
            mean: 0.0,
            stddev: 0.0,
            sample_count: 5,
        };
        let comp = make_comparison("test", 5.0, &entry, None);
        assert!(!comp.is_outlier);
        assert_eq!(comp.status, BaselineStatus::NewSignal);
    }

    #[test]
    fn test_compare_zero_on_zero() {
        let entry = BaselineEntry {
            mean: 0.0,
            stddev: 0.0,
            sample_count: 5,
        };
        let comp = make_comparison("test", 0.0, &entry, None);
        assert!(!comp.is_outlier);
        assert_eq!(comp.status, BaselineStatus::Normal);
    }

    #[test]
    fn test_compare_no_nan_or_inf() {
        // Test all edge case entries produce valid f64 values
        let entries = vec![
            BaselineEntry {
                mean: 0.0,
                stddev: 0.0,
                sample_count: 3,
            },
            BaselineEntry {
                mean: 100.0,
                stddev: 0.0,
                sample_count: 3,
            },
            BaselineEntry {
                mean: 100.0,
                stddev: 20.0,
                sample_count: 3,
            },
        ];

        for entry in &entries {
            for &val in &[0.0, 1.0, 100.0, 1000.0] {
                let comp = make_comparison("test", val, entry, None);
                assert!(!comp.mean.is_nan(), "mean is NaN");
                assert!(!comp.mean.is_infinite(), "mean is Inf");
                assert!(!comp.stddev.is_nan(), "stddev is NaN");
                assert!(!comp.stddev.is_infinite(), "stddev is Inf");
                assert!(!comp.current_value.is_nan(), "current is NaN");
                assert!(!comp.current_value.is_infinite(), "current is Inf");
            }
        }
    }

    #[test]
    fn test_compare_to_baseline_full() {
        let history = vec![make_mv(10, 2), make_mv(20, 4), make_mv(30, 6)];
        let baselines = compute_baselines(&history).unwrap();

        // Current with high tool calls
        let current = make_mv(100, 4);
        let comparisons = compare_to_baseline(&current, &baselines);

        // Find total_tool_calls comparison
        let tc = comparisons
            .iter()
            .find(|c| c.metric_name == "total_tool_calls")
            .unwrap();
        assert!((tc.mean - 20.0).abs() < f64::EPSILON);
        assert!(tc.is_outlier); // 100 >> 20 + 1.5 * ~8.16
    }

    #[test]
    fn test_compare_to_baseline_with_phases() {
        let mut phases = BTreeMap::new();
        phases.insert(
            "3a".to_string(),
            PhaseMetrics {
                duration_secs: 100,
                tool_call_count: 10,
            },
        );

        let history: Vec<MetricVector> = (0..3)
            .map(|_| make_mv_with_phases(phases.clone()))
            .collect();

        let baselines = compute_baselines(&history).unwrap();

        // Current with longer duration
        let mut current_phases = BTreeMap::new();
        current_phases.insert(
            "3a".to_string(),
            PhaseMetrics {
                duration_secs: 100,
                tool_call_count: 10,
            },
        );
        let current = make_mv_with_phases(current_phases);
        let comparisons = compare_to_baseline(&current, &baselines);

        // Should have phase-specific comparisons
        let phase_comps: Vec<&BaselineComparison> =
            comparisons.iter().filter(|c| c.phase.is_some()).collect();
        assert!(!phase_comps.is_empty());
    }
}
