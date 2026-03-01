# Pseudocode: observe-report

## Purpose

Assemble a self-contained RetrospectiveReport from computed metrics and hotspot findings.

## File: `crates/unimatrix-observe/src/report.rs`

### build_report

```
pub fn build_report(
    feature_cycle: &str,
    records: &[ObservationRecord],
    metrics: MetricVector,
    hotspots: Vec<HotspotFinding>,
) -> RetrospectiveReport {
    let session_count = records.iter()
        .map(|r| r.session_id.as_str())
        .collect::<HashSet<_>>()
        .len();

    RetrospectiveReport {
        feature_cycle: feature_cycle.to_string(),
        session_count,
        total_records: records.len(),
        metrics,
        hotspots,
        is_cached: false,
    }
}
```

## Error Handling

- No errors -- report assembly is infallible given valid inputs

## Key Test Scenarios

- Report includes correct session_count from distinct session_ids
- Report includes correct total_records count
- Report has is_cached = false for fresh computation
- Report includes all hotspot findings
- Report includes the full MetricVector
- Report is self-contained (AC-22)
