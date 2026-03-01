# Pseudocode: observe-metrics

## Purpose

Compute a MetricVector from analyzed ObservationRecords and HotspotFindings.

## File: `crates/unimatrix-observe/src/metrics.rs`

### compute_metric_vector

```
pub fn compute_metric_vector(
    records: &[ObservationRecord],
    hotspots: &[HotspotFinding],
    computed_at: u64,
) -> MetricVector {
    let universal = compute_universal(records, hotspots);
    let phases = compute_phases(records);

    MetricVector {
        computed_at,
        universal,
        phases,
    }
}
```

### compute_universal

```
fn compute_universal(
    records: &[ObservationRecord],
    hotspots: &[HotspotFinding],
) -> UniversalMetrics {
    let mut m = UniversalMetrics::default();

    // Count tool calls (PreToolUse events)
    m.total_tool_calls = records.iter()
        .filter(|r| r.hook == HookType::PreToolUse)
        .count() as u64;

    // Total duration: max_ts - min_ts (in seconds)
    if let (Some(first), Some(last)) = (
        records.iter().map(|r| r.ts).min(),
        records.iter().map(|r| r.ts).max(),
    ) {
        m.total_duration_secs = (last - first) / 1000;  // millis to secs
    }

    // Session count: distinct session_ids
    let sessions: HashSet<&str> = records.iter()
        .map(|r| r.session_id.as_str())
        .collect();
    m.session_count = sessions.len() as u64;

    // Permission friction: count from records (Pre - Post per tool, sum positives)
    let mut pre_counts: HashMap<&str, u64> = HashMap::new();
    let mut post_counts: HashMap<&str, u64> = HashMap::new();
    for r in records {
        if let Some(tool) = &r.tool {
            match r.hook {
                HookType::PreToolUse => *pre_counts.entry(tool).or_default() += 1,
                HookType::PostToolUse => *post_counts.entry(tool).or_default() += 1,
                _ => {},
            }
        }
    }
    m.permission_friction_events = pre_counts.iter()
        .map(|(tool, &pre)| pre.saturating_sub(*post_counts.get(tool).unwrap_or(&0)))
        .sum();

    // Sleep workaround count
    m.sleep_workaround_count = records.iter()
        .filter(|r| r.tool.as_deref() == Some("Bash"))
        .filter(|r| {
            r.input.as_ref().map_or(false, |input| {
                let s = input_to_string(input);
                contains_sleep_command(&s)
            })
        })
        .count() as u64;

    // Bash for search count (using grep/find via Bash instead of Grep/Glob tools)
    m.bash_for_search_count = records.iter()
        .filter(|r| r.tool.as_deref() == Some("Bash"))
        .filter(|r| {
            r.input.as_ref().map_or(false, |input| {
                let s = input_to_string(input);
                contains_search_pattern(&s)
            })
        })
        .count() as u64;

    // Search miss rate: context_search calls that returned 0 results
    // Approximation: count search calls vs total tool calls
    // For now, set to 0.0 (requires response analysis not available in records)
    m.search_miss_rate = 0.0;

    // Context loaded: sum response_size from all PostToolUse records
    let total_response_bytes: u64 = records.iter()
        .filter(|r| r.hook == HookType::PostToolUse)
        .filter_map(|r| r.response_size)
        .sum();
    m.total_context_loaded_kb = total_response_bytes as f64 / 1024.0;

    // Coordinator respawn count: SubagentStart for coordinator-like agents
    m.coordinator_respawn_count = records.iter()
        .filter(|r| r.hook == HookType::SubagentStart)
        .filter(|r| r.tool.as_deref().map_or(false, |t|
            t.contains("scrum-master") || t.contains("coordinator")))
        .count() as u64;

    // Knowledge entries stored: context_store calls
    m.knowledge_entries_stored = records.iter()
        .filter(|r| r.hook == HookType::PreToolUse)
        .filter(|r| r.tool.as_deref().map_or(false, |t| t.contains("context_store")))
        .count() as u64;

    // Hotspot counts by category
    m.agent_hotspot_count = hotspots.iter()
        .filter(|h| h.category == HotspotCategory::Agent).count() as u64;
    m.friction_hotspot_count = hotspots.iter()
        .filter(|h| h.category == HotspotCategory::Friction).count() as u64;
    m.session_hotspot_count = hotspots.iter()
        .filter(|h| h.category == HotspotCategory::Session).count() as u64;
    m.scope_hotspot_count = hotspots.iter()
        .filter(|h| h.category == HotspotCategory::Scope).count() as u64;

    // Remaining fields left at default 0 (computed when more data patterns are available)
    // edit_bloat, parallel_call_rate, post_completion_work_pct, etc.
    // These require deeper analysis patterns not available in col-002 scope

    m
}
```

### compute_phases

```
fn compute_phases(records: &[ObservationRecord]) -> BTreeMap<String, PhaseMetrics> {
    // Extract phase from SubagentStart prompt_snippet (task subject prefix)
    // FR-07.3: split on first ":", trim prefix

    let mut phases: BTreeMap<String, Vec<&ObservationRecord>> = BTreeMap::new();
    let mut current_phase: Option<String> = None;

    for record in records {
        // Check SubagentStart records for phase transitions
        if record.hook == HookType::SubagentStart {
            if let Some(input) = &record.input {
                if let Some(phase) = extract_phase_name(input) {
                    current_phase = Some(phase);
                }
            }
        }

        if let Some(ref phase) = current_phase {
            phases.entry(phase.clone()).or_default().push(record);
        }
    }

    // Compute PhaseMetrics for each phase
    let mut result = BTreeMap::new();
    for (phase, records) in &phases {
        let tool_call_count = records.iter()
            .filter(|r| r.hook == HookType::PreToolUse)
            .count() as u64;

        let duration_secs = if let (Some(first), Some(last)) = (
            records.iter().map(|r| r.ts).min(),
            records.iter().map(|r| r.ts).max(),
        ) {
            (last - first) / 1000
        } else { 0 };

        result.insert(phase.clone(), PhaseMetrics {
            duration_secs,
            tool_call_count,
        });
    }

    result
}
```

### extract_phase_name (FR-07.3)

```
fn extract_phase_name(input: &serde_json::Value) -> Option<String> {
    let s = match input {
        Value::String(s) => s.as_str(),
        _ => return None,
    };

    // Split on first ":"
    let colon_pos = s.find(':')?;
    let prefix = s[..colon_pos].trim();

    if prefix.is_empty() {
        return None;  // Empty prefix -> no phase (R-11 scenario 4)
    }

    Some(prefix.to_string())
}
```

### Helper: input_to_string

```
fn input_to_string(input: &serde_json::Value) -> String {
    match input {
        Value::String(s) => s.clone(),
        Value::Object(map) => map.get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}
```

### Helper: contains_search_pattern

```
fn contains_search_pattern(s: &str) -> bool {
    // Detects using Bash for search instead of dedicated tools
    let patterns = ["grep ", "rg ", "find ", "ack "];
    patterns.iter().any(|p| s.contains(p))
}
```

Use `contains_sleep_command` from detection module (make it pub(crate) or duplicate).

## Error Handling

- No errors from metric computation -- uses defaults for missing data
- Division by zero protected (durations use saturating_sub)

## Key Test Scenarios

- Compute from synthetic records: both universal and phases populated (AC-15)
- Phase extraction: "3a: Pseudocode" -> phase "3a" (AC-16, R-11)
- Phase extraction: no colon -> no phase
- Phase extraction: "3b: Code: implement parser" -> phase "3b"
- Phase extraction: ": Just a description" -> None (empty prefix)
- Total tool calls counts only PreToolUse events
- Session count from distinct session_ids
- Duration from first to last record timestamp
- Hotspot counts match provided hotspots
- MetricVector includes computed_at timestamp (AC-27)
