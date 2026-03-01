# Pseudocode: baseline

## Purpose

Implements baseline computation and comparison in `crates/unimatrix-observe/src/baseline.rs`. Pure computation module with no external dependencies beyond types.

## File: `crates/unimatrix-observe/src/baseline.rs`

```
use std::collections::HashMap;
use crate::types::{BaselineComparison, BaselineEntry, BaselineSet, BaselineStatus, MetricVector};
```

### Function: compute_baselines

```
pub fn compute_baselines(history: &[MetricVector]) -> Option<BaselineSet>:
    if history.len() < 3:
        return None

    // Compute universal metric baselines
    let mut universal = HashMap::new()

    // For each universal metric field, collect values across all historical vectors
    // and compute mean + stddev
    let metric_extractors: Vec<(&str, fn(&MetricVector) -> f64)> = vec![
        ("total_tool_calls", |mv| mv.universal.total_tool_calls as f64),
        ("total_duration_secs", |mv| mv.universal.total_duration_secs as f64),
        ("session_count", |mv| mv.universal.session_count as f64),
        ("search_miss_rate", |mv| mv.universal.search_miss_rate),
        ("edit_bloat_total_kb", |mv| mv.universal.edit_bloat_total_kb),
        ("edit_bloat_ratio", |mv| mv.universal.edit_bloat_ratio),
        ("permission_friction_events", |mv| mv.universal.permission_friction_events as f64),
        ("bash_for_search_count", |mv| mv.universal.bash_for_search_count as f64),
        ("cold_restart_events", |mv| mv.universal.cold_restart_events as f64),
        ("coordinator_respawn_count", |mv| mv.universal.coordinator_respawn_count as f64),
        ("parallel_call_rate", |mv| mv.universal.parallel_call_rate),
        ("context_load_before_first_write_kb", |mv| mv.universal.context_load_before_first_write_kb),
        ("total_context_loaded_kb", |mv| mv.universal.total_context_loaded_kb),
        ("post_completion_work_pct", |mv| mv.universal.post_completion_work_pct),
        ("follow_up_issues_created", |mv| mv.universal.follow_up_issues_created as f64),
        ("knowledge_entries_stored", |mv| mv.universal.knowledge_entries_stored as f64),
        ("sleep_workaround_count", |mv| mv.universal.sleep_workaround_count as f64),
        ("agent_hotspot_count", |mv| mv.universal.agent_hotspot_count as f64),
        ("friction_hotspot_count", |mv| mv.universal.friction_hotspot_count as f64),
        ("session_hotspot_count", |mv| mv.universal.session_hotspot_count as f64),
        ("scope_hotspot_count", |mv| mv.universal.scope_hotspot_count as f64),
    ]

    for (name, extractor) in metric_extractors:
        let values: Vec<f64> = history.iter().map(extractor).collect()
        let entry = compute_entry(&values)
        universal.insert(name.to_string(), entry)

    // Compute phase-specific baselines
    let mut phases: HashMap<String, HashMap<String, BaselineEntry>> = HashMap::new()

    // Collect phase metrics across all historical vectors
    // Group by phase name
    let mut phase_durations: HashMap<String, Vec<f64>> = HashMap::new()
    let mut phase_tool_calls: HashMap<String, Vec<f64>> = HashMap::new()

    for mv in history:
        for (phase_name, phase_metrics) in &mv.phases:
            phase_durations.entry(phase_name.clone())
                .or_default()
                .push(phase_metrics.duration_secs as f64)
            phase_tool_calls.entry(phase_name.clone())
                .or_default()
                .push(phase_metrics.tool_call_count as f64)

    for (phase_name, durations) in &phase_durations:
        let mut phase_baselines = HashMap::new()
        // Only compute if we have enough samples for this specific phase
        if durations.len() >= 3:
            phase_baselines.insert("duration_secs".to_string(), compute_entry(durations))
        if let Some(tool_calls) = phase_tool_calls.get(phase_name):
            if tool_calls.len() >= 3:
                phase_baselines.insert("tool_call_count".to_string(), compute_entry(tool_calls))
        if !phase_baselines.is_empty():
            phases.insert(phase_name.clone(), phase_baselines)

    Some(BaselineSet { universal, phases })
```

### Helper: compute_entry

```
fn compute_entry(values: &[f64]) -> BaselineEntry:
    let n = values.len() as f64
    let mean = values.iter().sum::<f64>() / n
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n
    let stddev = if variance > 0.0 { variance.sqrt() } else { 0.0 }

    BaselineEntry {
        mean,
        stddev,
        sample_count: values.len(),
    }
```

Note: Uses population stddev (divide by n, not n-1) since we have the complete historical dataset.

### Function: compare_to_baseline

```
pub fn compare_to_baseline(
    current: &MetricVector,
    baselines: &BaselineSet,
) -> Vec<BaselineComparison>:
    let mut comparisons = Vec::new()

    // Compare universal metrics
    let metric_extractors: Vec<(&str, fn(&MetricVector) -> f64)> = [same list as above]

    for (name, extractor) in metric_extractors:
        let current_value = extractor(current)
        if let Some(entry) = baselines.universal.get(name):
            comparisons.push(make_comparison(
                name, current_value, entry, None
            ))

    // Compare phase-specific metrics
    for (phase_name, phase_metrics) in &current.phases:
        if let Some(phase_baselines) = baselines.phases.get(phase_name):
            // Duration comparison
            if let Some(entry) = phase_baselines.get("duration_secs"):
                comparisons.push(make_comparison(
                    "duration_secs", phase_metrics.duration_secs as f64,
                    entry, Some(phase_name.clone())
                ))
            // Tool call count comparison
            if let Some(entry) = phase_baselines.get("tool_call_count"):
                comparisons.push(make_comparison(
                    "tool_call_count", phase_metrics.tool_call_count as f64,
                    entry, Some(phase_name.clone())
                ))

    comparisons
```

### Helper: make_comparison (ADR-003 guards)

```
fn make_comparison(
    metric_name: &str,
    current_value: f64,
    entry: &BaselineEntry,
    phase: Option<String>,
) -> BaselineComparison:
    let (is_outlier, status) = if entry.stddev == 0.0 && entry.mean == 0.0:
        // Zero mean, zero stddev: any non-zero value is "new signal"
        if current_value != 0.0:
            (false, BaselineStatus::NewSignal)
        else:
            (false, BaselineStatus::Normal)
    else if entry.stddev == 0.0:
        // Non-zero mean, zero stddev: "no variance"
        (false, BaselineStatus::NoVariance)
    else:
        // Normal case: check outlier threshold
        let threshold = entry.mean + 1.5 * entry.stddev
        let outlier = current_value > threshold
        let status = if outlier { BaselineStatus::Outlier } else { BaselineStatus::Normal }
        (outlier, status)

    BaselineComparison {
        metric_name: metric_name.to_string(),
        current_value,
        mean: entry.mean,
        stddev: entry.stddev,
        is_outlier,
        status,
        phase,
    }
```

## Error Handling

- `compute_baselines` returns None for < 3 vectors (minimum history guard)
- `compute_entry` handles zero variance with explicit guard (ADR-003)
- No NaN or Inf can propagate: variance.sqrt() only when variance > 0.0
- Zero division impossible: n is always >= 3 (checked at entry)
- `compare_to_baseline` handles missing phase baselines gracefully (no comparison for unknown phases)

## Key Test Scenarios

### compute_baselines
1. 3 vectors with known values -> verify mean and stddev match expected
2. 2 vectors -> returns None
3. 0 vectors -> returns None
4. 3 vectors with identical values -> stddev = 0.0
5. 3 vectors with all zeros -> mean = 0.0, stddev = 0.0
6. Vectors with different phase names -> separate phase baselines
7. Phase with < 3 data points -> no phase baseline for that phase

### compare_to_baseline
1. Current value > mean + 1.5*stddev -> is_outlier=true, status=Outlier
2. Current value < mean + 1.5*stddev -> is_outlier=false, status=Normal
3. stddev=0, mean=100, current=100 -> status=NoVariance
4. stddev=0, mean=0, current=5 -> status=NewSignal
5. stddev=0, mean=0, current=0 -> status=Normal
6. Phase-specific comparison matches correct phase
7. No NaN or Inf in any BaselineComparison field (explicit assertion)

### Integration with PhaseDurationOutlierRule
- Phase duration outlier detection happens here, not in the detection rule
- compare_to_baseline includes phase duration comparisons when baselines exist
