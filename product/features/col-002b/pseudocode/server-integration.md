# Pseudocode: server-integration

## Purpose

Extends the `context_retrospective` handler in `crates/unimatrix-server/src/tools.rs` to:
1. Load historical MetricVectors
2. Pass history to `default_rules()` for PhaseDurationOutlierRule
3. Compute baselines and compare current metrics
4. Include baseline comparison in the report

Also modifies `crates/unimatrix-observe/src/report.rs` and `types.rs`.

## Changes to `crates/unimatrix-observe/src/types.rs`

### New types

```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BaselineStatus {
    Normal,
    Outlier,
    NoVariance,
    NewSignal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineEntry {
    pub mean: f64,
    pub stddev: f64,
    pub sample_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineSet {
    pub universal: HashMap<String, BaselineEntry>,
    pub phases: HashMap<String, HashMap<String, BaselineEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineComparison {
    pub metric_name: String,
    pub current_value: f64,
    pub mean: f64,
    pub stddev: f64,
    pub is_outlier: bool,
    pub status: BaselineStatus,
    pub phase: Option<String>,
}
```

### RetrospectiveReport extension

```
pub struct RetrospectiveReport {
    pub feature_cycle: String,
    pub session_count: usize,
    pub total_records: usize,
    pub metrics: MetricVector,
    pub hotspots: Vec<HotspotFinding>,
    pub is_cached: bool,
    #[serde(default)]                                    // NEW
    pub baseline_comparison: Option<Vec<BaselineComparison>>,  // NEW
}
```

The `#[serde(default)]` ensures backward compatibility -- old serialized reports deserialize with `baseline_comparison: None`.

## Changes to `crates/unimatrix-observe/src/report.rs`

### build_report signature change

```
pub fn build_report(
    feature_cycle: &str,
    records: &[ObservationRecord],
    metrics: MetricVector,
    hotspots: Vec<HotspotFinding>,
    baseline: Option<Vec<BaselineComparison>>,    // NEW parameter
) -> RetrospectiveReport:
    let session_count = records.iter()
        .map(|r| r.session_id.as_str())
        .collect::<HashSet<_>>()
        .len()

    RetrospectiveReport {
        feature_cycle: feature_cycle.to_string(),
        session_count,
        total_records: records.len(),
        metrics,
        hotspots,
        is_cached: false,
        baseline_comparison: baseline,    // NEW field
    }
```

## Changes to `crates/unimatrix-observe/src/lib.rs`

Add to module declarations:
```
pub mod baseline;
```

Add to re-exports:
```
pub use baseline::{compute_baselines, compare_to_baseline};
pub use types::{BaselineComparison, BaselineEntry, BaselineSet, BaselineStatus};
```

## Changes to `crates/unimatrix-server/src/tools.rs`

### context_retrospective handler modifications

The handler is modified in two places:

#### Step 7: Run analysis pipeline (modified)

Old:
```rust
let rules = unimatrix_observe::default_rules();
let hotspots = unimatrix_observe::detect_hotspots(&attributed, &rules);
```

New:
```rust
// 7a. Load historical MetricVectors for baseline
let all_metrics = tokio::task::spawn_blocking({
    let store = Arc::clone(&store);
    move || store.list_all_metrics()
})
.await
.unwrap()
.map_err(|e| ServerError::Core(CoreError::Store(e)))
.map_err(rmcp::ErrorData::from)?;

// 7b. Deserialize historical vectors, excluding current feature
let mut history: Vec<unimatrix_observe::MetricVector> = Vec::new();
for (fc, bytes) in &all_metrics {
    if fc != &feature_cycle {  // Exclude current feature (FR-09.3)
        if let Ok(mv) = unimatrix_observe::deserialize_metric_vector(bytes) {
            history.push(mv);
        }
        // Skip deserialization failures silently (R-12 mitigation)
    }
}

// 7c. Run detection with history for PhaseDurationOutlierRule
let history_slice = if history.is_empty() { None } else { Some(history.as_slice()) };
let rules = unimatrix_observe::default_rules(history_slice);
let hotspots = unimatrix_observe::detect_hotspots(&attributed, &rules);
```

#### Step 10: Build and return report (modified)

Old:
```rust
let report = unimatrix_observe::build_report(
    &feature_cycle,
    &attributed,
    metrics,
    hotspots,
);
```

New:
```rust
// 10a. Compute baseline comparison
let baseline = unimatrix_observe::compute_baselines(&history)
    .map(|baselines| unimatrix_observe::compare_to_baseline(&metrics, &baselines));

// 10b. Build report with baseline
let report = unimatrix_observe::build_report(
    &feature_cycle,
    &attributed,
    metrics,
    hotspots,
    baseline,
);
```

#### Cached report path (step 6, modified)

The cached report also needs the baseline_comparison field:

```rust
let report = unimatrix_observe::RetrospectiveReport {
    feature_cycle: feature_cycle.clone(),
    session_count: 0,
    total_records: 0,
    metrics: mv,
    hotspots: vec![],
    is_cached: true,
    baseline_comparison: None,  // Cached reports have no baseline
};
```

## Changes to `crates/unimatrix-observe/src/detection/mod.rs`

### default_rules signature change

```
pub fn default_rules(history: Option<&[MetricVector]>) -> Vec<Box<dyn DetectionRule>>:
    vec![
        // Friction (2 existing + 2 new)
        Box::new(friction::PermissionRetriesRule),
        Box::new(friction::SleepWorkaroundsRule),
        Box::new(friction::SearchViaBashRule),
        Box::new(friction::OutputParsingStruggleRule),
        // Session (1 existing + 4 new)
        Box::new(session::SessionTimeoutRule),
        Box::new(session::ColdRestartRule),
        Box::new(session::CoordinatorRespawnsRule),
        Box::new(session::PostCompletionWorkRule),
        Box::new(session::ReworkEventsRule),
        // Agent (7 new)
        Box::new(agent::ContextLoadRule),
        Box::new(agent::LifespanRule),
        Box::new(agent::FileBreadthRule),
        Box::new(agent::RereadRateRule),
        Box::new(agent::MutationSpreadRule),
        Box::new(agent::CompileCyclesRule),
        Box::new(agent::EditBloatRule),
        // Scope (5 new)
        Box::new(scope::SourceFileCountRule),
        Box::new(scope::DesignArtifactCountRule),
        Box::new(scope::AdrCountRule),
        Box::new(scope::PostDeliveryIssuesRule),
        Box::new(scope::PhaseDurationOutlierRule::new(history)),
    ]
```

## Error Handling

- `store.list_all_metrics()` error propagated as ServerError
- Individual MetricVector deserialization failures silently skipped (corrupted data tolerance)
- Current feature excluded from history to prevent self-comparison (FR-09.3)
- Empty history results in `default_rules(None)` -- all rules use absolute thresholds
- `compute_baselines` returns None when < 3 vectors -- baseline is None in report
- All existing tests for build_report need updating to pass the new `baseline` parameter

## Key Test Scenarios

1. build_report with Some(baseline) -> report.baseline_comparison is Some
2. build_report with None baseline -> report.baseline_comparison is None
3. RetrospectiveReport serde roundtrip with baseline_comparison field
4. RetrospectiveReport serde roundtrip WITHOUT baseline_comparison (serde(default) compat)
5. Server handler loads history, excludes current feature
6. Server handler with < 3 historical vectors -> no baseline in report
