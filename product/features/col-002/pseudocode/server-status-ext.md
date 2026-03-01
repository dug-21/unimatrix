# Pseudocode: server-status-ext

## Purpose

Extend context_status with observation health fields. Add 5 new fields to StatusReport. Update format functions.

## File: `crates/unimatrix-server/src/response.rs` (modifications)

### StatusReport Extensions

Add these fields to the existing StatusReport struct:

```
/// Number of observation JSONL files.
pub observation_file_count: u64,
/// Total size of observation files in bytes.
pub observation_total_size_bytes: u64,
/// Age of oldest observation file in days.
pub observation_oldest_file_days: u64,
/// Session IDs approaching 60-day cleanup.
pub observation_approaching_cleanup: Vec<String>,
/// Number of feature cycles with stored metrics.
pub retrospected_feature_count: u64,
```

### format_status_report Updates

In the summary format, add an observation section:

```
// In summary format:
lines.push(format!("observation files: {} ({} bytes)", report.observation_file_count, report.observation_total_size_bytes));
lines.push(format!("oldest file: {} days", report.observation_oldest_file_days));
lines.push(format!("retrospected features: {}", report.retrospected_feature_count));
if !report.observation_approaching_cleanup.is_empty() {
    lines.push(format!("approaching cleanup (>45 days): {}",
        report.observation_approaching_cleanup.join(", ")));
}
```

In the markdown format, add an observation section:

```
lines.push("## Observation Pipeline".to_string());
lines.push(format!("- Files: {}", report.observation_file_count));
lines.push(format!("- Total size: {} bytes", report.observation_total_size_bytes));
lines.push(format!("- Oldest file: {} days", report.observation_oldest_file_days));
lines.push(format!("- Retrospected features: {}", report.retrospected_feature_count));
if !report.observation_approaching_cleanup.is_empty() {
    lines.push(format!("- **Approaching cleanup**: {}",
        report.observation_approaching_cleanup.join(", ")));
}
```

In the JSON format, add observation fields to the JSON object.

## File: `crates/unimatrix-server/src/tools.rs` (modifications)

### context_status handler additions

In the existing context_status handler, after building the base StatusReport:

```
// Observation stats
let obs_dir = unimatrix_observe::files::observation_dir();
let obs_stats = tokio::task::spawn_blocking({
    let dir = obs_dir.clone();
    move || unimatrix_observe::scan_observation_stats(&dir)
}).await.unwrap()
.unwrap_or_else(|_| ObservationStats {
    file_count: 0,
    total_size_bytes: 0,
    oldest_file_age_days: 0,
    approaching_cleanup: vec![],
});

report.observation_file_count = obs_stats.file_count;
report.observation_total_size_bytes = obs_stats.total_size_bytes;
report.observation_oldest_file_days = obs_stats.oldest_file_age_days;
report.observation_approaching_cleanup = obs_stats.approaching_cleanup;

// Retrospected feature count from OBSERVATION_METRICS
let retrospected = tokio::task::spawn_blocking({
    let store = self.store.clone();
    move || store.list_all_metrics()
}).await.unwrap()
.unwrap_or_else(|_| vec![]);
report.retrospected_feature_count = retrospected.len() as u64;

// If maintain=true, also clean up old observation files
if maintain {
    let cleanup_dir = obs_dir.clone();
    tokio::task::spawn_blocking(move || {
        let sixty_days = 60 * 24 * 60 * 60;
        if let Ok(expired) = unimatrix_observe::identify_expired(&cleanup_dir, sixty_days) {
            for path in expired {
                let _ = std::fs::remove_file(path);
            }
        }
    }).await.unwrap();
}
```

## Existing Test Helpers

Update any `StatusReport` construction sites in test code to include the 5 new fields defaulted to 0/empty (R-14). The fields are:
- `observation_file_count: 0`
- `observation_total_size_bytes: 0`
- `observation_oldest_file_days: 0`
- `observation_approaching_cleanup: vec![]`
- `retrospected_feature_count: 0`

## Error Handling

- Observation dir missing -> default stats (0s)
- Store read failure -> default empty list
- Cleanup failures -> ignored (best-effort in maintain mode)

## Key Test Scenarios

- Status includes observation_file_count from synthetic files (AC-34)
- Status warns when files approach 60 days (AC-35)
- Status includes retrospected_feature_count from OBSERVATION_METRICS
- maintain=true triggers file cleanup (AC-33)
- All format modes (summary, markdown, json) include observation section (FR-11.2)
- Existing status tests compile with new fields (R-14)
