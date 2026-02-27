# Pseudocode: status-extension (server crate)

## Purpose

Extend context_status to compute and display outcome statistics: total outcomes, breakdown by workflow type and result, and top feature cycles.

## Changes

### response.rs: StatusReport

Add 4 new fields after `stale_pairs_cleaned`:

```rust
pub struct StatusReport {
    // ... existing fields ...
    /// Total outcome entries.
    pub total_outcomes: u64,
    /// Outcome count by workflow type (from type: tag).
    pub outcomes_by_type: Vec<(String, u64)>,
    /// Outcome count by result (from result: tag).
    pub outcomes_by_result: Vec<(String, u64)>,
    /// Top feature cycles by outcome count.
    pub outcomes_by_feature_cycle: Vec<(String, u64)>,
}
```

### tools.rs: context_status handler

In the spawn_blocking read transaction block (step 5), after existing stats computation (after step 5d, before step 5e), add outcome stats:

```
// 5d2. Outcome statistics
let mut total_outcomes = 0u64;
let mut outcomes_by_type: BTreeMap<String, u64> = BTreeMap::new();
let mut outcomes_by_result: BTreeMap<String, u64> = BTreeMap::new();
let mut outcomes_by_feature_cycle: BTreeMap<String, u64> = BTreeMap::new();

// Scan CATEGORY_INDEX for "outcome" entries
let outcome_range = cat_table.range::<(&str, u64)>(("outcome", 0u64)..=("outcome", u64::MAX))
    .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;

for item in outcome_range {
    let (key, _) = item.map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
    let (_cat, entry_id) = key.value();
    total_outcomes += 1;

    // Read the entry record to extract tags
    if let Some(entry_guard) = entries_table.get(entry_id)
        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))? {
        let record = deserialize_entry(entry_guard.value())
            .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

        // Extract type: and result: tags
        for tag in &record.tags {
            if let Some((key, value)) = tag.split_once(':') {
                match key {
                    "type" => { *outcomes_by_type.entry(value.to_string()).or_insert(0) += 1; }
                    "result" => { *outcomes_by_result.entry(value.to_string()).or_insert(0) += 1; }
                    _ => {}
                }
            }
        }

        // Track feature_cycle
        if !record.feature_cycle.is_empty() {
            *outcomes_by_feature_cycle.entry(record.feature_cycle.clone()).or_insert(0) += 1;
        }
    }
}

// Sort feature cycles by count descending, take top 10
let mut fc_sorted: Vec<(String, u64)> = outcomes_by_feature_cycle.into_iter().collect();
fc_sorted.sort_by(|a, b| b.1.cmp(&a.1));
fc_sorted.truncate(10);
```

In step 5e (build StatusReport), add the new fields:

```
Ok(StatusReport {
    // ... existing fields ...
    total_outcomes,
    outcomes_by_type: outcomes_by_type.into_iter().collect(),
    outcomes_by_result: outcomes_by_result.into_iter().collect(),
    outcomes_by_feature_cycle: fc_sorted,
})
```

### response.rs: format_status_report

#### Summary format

Append after co-access line:

```
if report.total_outcomes > 0 {
    text.push_str(&format!(
        "\nOutcomes: {} total",
        report.total_outcomes,
    ));
}
```

#### Markdown format

Add new section after co-access patterns:

```
if report.total_outcomes > 0 || !report.outcomes_by_type.is_empty() {
    text.push_str("\n### Outcome Statistics\n\n");
    text.push_str(&format!("- Total outcomes: {}\n", report.total_outcomes));

    if !report.outcomes_by_type.is_empty() {
        text.push_str("\n#### By Workflow Type\n");
        text.push_str("| Type | Count |\n|------|-------|\n");
        for (type_name, count) in &report.outcomes_by_type {
            text.push_str(&format!("| {} | {} |\n", type_name, count));
        }
    }

    if !report.outcomes_by_result.is_empty() {
        text.push_str("\n#### By Result\n");
        text.push_str("| Result | Count |\n|--------|-------|\n");
        for (result_name, count) in &report.outcomes_by_result {
            text.push_str(&format!("| {} | {} |\n", result_name, count));
        }
    }

    if !report.outcomes_by_feature_cycle.is_empty() {
        text.push_str("\n#### Top Feature Cycles\n");
        text.push_str("| Feature Cycle | Outcomes |\n|--------------|----------|\n");
        for (fc, count) in &report.outcomes_by_feature_cycle {
            text.push_str(&format!("| {} | {} |\n", fc, count));
        }
    }
}
```

#### JSON format

Add outcomes object:

```
if report.total_outcomes > 0 || !report.outcomes_by_type.is_empty() {
    let type_dist: serde_json::Value = report.outcomes_by_type.iter()
        .map(|(k, v)| (k.clone(), serde_json::json!(v)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();
    let result_dist: serde_json::Value = report.outcomes_by_result.iter()
        .map(|(k, v)| (k.clone(), serde_json::json!(v)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();
    let fc_list: Vec<serde_json::Value> = report.outcomes_by_feature_cycle.iter()
        .map(|(fc, count)| serde_json::json!({"feature_cycle": fc, "count": count}))
        .collect();

    obj["outcomes"] = serde_json::json!({
        "total": report.total_outcomes,
        "by_type": type_dist,
        "by_result": result_dist,
        "top_feature_cycles": fc_list,
    });
}
```

## Invariants

- Outcome stats computed within the same read transaction as other stats (consistent snapshot)
- Empty database: total_outcomes = 0, all Vecs empty -- not errors
- Feature cycles sorted by count descending, limited to top 10
- Non-outcome entries are not counted even if they have type: or result: tags
- OUTCOME_INDEX scan is separate from CATEGORY_INDEX scan to get different data from each
