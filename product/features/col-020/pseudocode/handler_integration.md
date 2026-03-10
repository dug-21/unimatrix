# C6: Handler Integration (unimatrix-server/src/mcp/tools.rs)

## Purpose

Wire the new computation steps into the `context_retrospective` handler after the existing pipeline. Orchestrate data loading, computation, and report assembly. Each new step is best-effort: failure sets the report field to None and logs a warning.

## Location

Modify the `context_retrospective` method in `unimatrix-server/src/mcp/tools.rs`. New steps insert between the existing step 10f (lesson-learned write) and step 11 (audit), at approximately line 1231.

## New Steps (after existing pipeline)

The handler already has `attributed` (Vec<ObservationRecord>), `report` (mut RetrospectiveReport), `store` (Arc<Store>), and `feature_cycle` (String).

### Step 11: Compute Session Summaries (C1)

```
// Step 11: Session summaries (C1, best-effort)
match (|| -> std::result::Result<_, Box<dyn std::error::Error>> {
    let mut summaries = unimatrix_observe::compute_session_summaries(&attributed);

    // Enrich with outcome from SessionRecord
    let session_records = {
        let store = Arc::clone(&store);
        let fc = feature_cycle.clone();
        tokio::task::spawn_blocking(move || store.scan_sessions_by_feature(&fc))
            .await??
    };

    // Build session_id -> outcome map
    let outcome_map: HashMap<String, Option<String>> = session_records.iter()
        .map(|sr| (sr.session_id.clone(), sr.outcome.clone()))
        .collect();

    // Attach outcomes to summaries
    for summary in &mut summaries:
        if let Some(outcome) = outcome_map.get(&summary.session_id):
            summary.outcome = outcome.clone()

    Ok((summaries, session_records))
})() {
    Ok((summaries, session_records)) => {
        // Store for later use
        // session_records needed by steps 13, 14, 15
        // summaries needed by step 12
    }
    Err(e) => {
        tracing::warn!("col-020: session summaries failed: {e}");
        // All dependent steps (12-16) will also be skipped
    }
}
```

### Step 12: Context Reload Rate (C1)

```
// Step 12: Context reload percentage (C1, best-effort)
// Only if step 11 succeeded (summaries available)
let reload_pct = unimatrix_observe::compute_context_reload_pct(&summaries, &attributed);
report.context_reload_pct = Some(reload_pct);
```

### Step 13: Load Data for Knowledge Reuse (C4)

```
// Step 13: Knowledge reuse data loading (C4, best-effort)
match (|| -> std::result::Result<_, Box<dyn std::error::Error>> {
    let session_id_list: Vec<String> = session_records.iter()
        .map(|sr| sr.session_id.clone())
        .collect();
    let session_id_refs: Vec<&str> = session_id_list.iter()
        .map(|s| s.as_str())
        .collect();

    let store_c = Arc::clone(&store);
    let ids_for_ql = session_id_refs.clone();
    let query_logs = tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = ids_for_ql.iter().copied().collect();
        store_c.scan_query_log_by_sessions(&refs)
    }).await??;

    let store_c = Arc::clone(&store);
    let ids_for_il = session_id_refs.clone();
    let injection_logs = tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = ids_for_il.iter().copied().collect();
        store_c.scan_injection_log_by_sessions(&refs)
    }).await??;

    let store_c = Arc::clone(&store);
    let active_cats = tokio::task::spawn_blocking(move ||
        store_c.count_active_entries_by_category()
    ).await??;

    Ok((query_logs, injection_logs, active_cats))
})() {
    Ok((query_logs, injection_logs, active_cats)) => {
        // Proceed to knowledge reuse computation
    }
    Err(e) => {
        tracing::warn!("col-020: knowledge reuse data load failed: {e}");
    }
}
```

### Step 14: Compute Knowledge Reuse (C3)

```
// Step 14: Knowledge reuse computation (C3, best-effort)
// Algorithm documented in knowledge_reuse.md
match compute_knowledge_reuse_inline(
    &query_logs, &injection_logs, &active_cats, &store
) {
    Ok(reuse) => report.knowledge_reuse = Some(reuse),
    Err(e) => tracing::warn!("col-020: knowledge reuse computation failed: {e}"),
}
```

The inline computation follows the algorithm in knowledge_reuse.md. It may be extracted to a helper function within tools.rs for readability:

```
fn compute_knowledge_reuse_inline(
    query_logs: &[QueryLogRecord],
    injection_logs: &[InjectionLogRecord],
    active_cats: &HashMap<String, u64>,
    store: &Store,
) -> std::result::Result<KnowledgeReuse, Box<dyn std::error::Error>>
```

### Step 15: Count Rework Sessions

```
// Step 15: Rework session count (best-effort)
// Case-insensitive substring match per human override of FR-03.1
let rework_count = session_records.iter()
    .filter(|sr| {
        if let Some(outcome) = &sr.outcome:
            let lower = outcome.to_lowercase()
            lower.contains("result:rework") || lower.contains("result:failed")
        else:
            false
    })
    .count() as u64;

report.rework_session_count = Some(rework_count);
```

### Step 16: Attribution Metadata (ADR-003)

```
// Step 16: Attribution metadata
// attributed_session_count: sessions with direct feature_cycle match
// total_session_count: all discovered sessions

// Use discover_sessions_for_feature (ObservationSource trait) for total count
let store_for_discover = Arc::clone(&store);
let fc_for_discover = feature_cycle.clone();
match tokio::task::spawn_blocking(move || {
    use unimatrix_observe::ObservationSource;
    let source = crate::services::observation::SqlObservationSource::new(store_for_discover);
    source.discover_sessions_for_feature(&fc_for_discover)
}).await {
    Ok(Ok(discovered_ids)) => {
        let attributed_count = session_records.iter()
            .filter(|sr| sr.feature_cycle.as_deref() == Some(&feature_cycle))
            .count();
        let total_count = discovered_ids.len();

        report.attribution = Some(AttributionMetadata {
            attributed_session_count: attributed_count,
            total_session_count: total_count,
        });
    }
    Ok(Err(e)) => tracing::warn!("col-020: attribution metadata failed: {e}"),
    Err(e) => tracing::warn!("col-020: attribution metadata task failed: {e}"),
}
```

### Step 17: Update Topic Deliveries Counters (ADR-002)

```
// Step 17: Idempotent counter update (C4, best-effort)
// Compute totals from source data (MetricVector values)
let total_sessions = session_records.len() as i64;
let total_tool_calls = report.metrics.universal.total_tool_calls as i64;
let total_duration_secs = report.metrics.universal.total_duration_secs as i64;

let store_for_counters = Arc::clone(&store);
let topic_for_counters = feature_cycle.clone();
match tokio::task::spawn_blocking(move || {
    // Ensure record exists
    if store_for_counters.get_topic_delivery(&topic_for_counters)?.is_none() {
        use unimatrix_store::topic_deliveries::TopicDeliveryRecord;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        store_for_counters.upsert_topic_delivery(&TopicDeliveryRecord {
            topic: topic_for_counters.clone(),
            created_at: now,
            completed_at: None,
            status: "active".to_string(),
            github_issue: None,
            total_sessions: 0,
            total_tool_calls: 0,
            total_duration_secs: 0,
            phases_completed: None,
        })?;
    }
    store_for_counters.set_topic_delivery_counters(
        &topic_for_counters,
        total_sessions,
        total_tool_calls,
        total_duration_secs,
    )
}).await {
    Ok(Ok(())) => {},
    Ok(Err(e)) => tracing::warn!("col-020: counter update failed: {e}"),
    Err(e) => tracing::warn!("col-020: counter update task failed: {e}"),
}
```

### Assign Session Summaries to Report

```
report.session_summaries = Some(summaries);
```

## Cached Report Path Update

The cached report construction (~line 1100) must include the five new fields set to None:

```
let report = unimatrix_observe::RetrospectiveReport {
    // ... existing fields ...
    session_summaries: None,
    knowledge_reuse: None,
    rework_session_count: None,
    context_reload_pct: None,
    attribution: None,
};
```

## Structural Note: Best-Effort Wrapper

The implementation should use a clear pattern for each step. One approach is a local closure:

```rust
// Pattern for each new step:
let step_result: Option<T> = match step_computation() {
    Ok(value) => Some(value),
    Err(e) => {
        tracing::warn!("col-020: {step_name} failed: {e}");
        None
    }
};
```

If step 11 (session summaries + session records load) fails, steps 12-17 should be skipped entirely since they depend on session data. The implementation should use an early-return-from-block pattern or a nested match.

## Error Handling (R-14)

- Each new step is independently wrapped in error handling.
- Failure in any new step does NOT abort the retrospective.
- The existing pipeline output (hotspots, metrics, baselines, narratives, recommendations, lesson-learned) is always preserved.
- New steps that depend on earlier new steps (12-17 depend on 11) gracefully skip when the dependency failed.

## Key Test Scenarios

1. **Full integration (AC-01 through AC-16)**: Run context_retrospective with seeded observation data, session records, query_log, injection_log. Verify all new report fields populated.
2. **Empty topic (R-10, AC-14)**: Zero sessions returns cached/error path. New fields absent.
3. **New step failure isolation (R-14)**: Simulate Store error in knowledge reuse data load. Verify existing report fields intact, knowledge_reuse is None.
4. **Counter idempotency (R-05, AC-12)**: Run retrospective twice for same topic. Verify topic_deliveries counters identical after both runs.
5. **Counter creation**: Run retrospective for topic with no pre-existing topic_deliveries record. Verify record created and counters set.
6. **Rework counting (AC-09)**: Sessions with "result:rework" and "Result:Failed" outcomes both counted (case-insensitive).
7. **Attribution metadata (R-03)**: Topic with mixed attributed/unattributed sessions. Verify attributed_session_count < total_session_count.
8. **Session outcomes enrichment**: SessionSummary.outcome populated from SessionRecord.outcome.

## Open Question

**Attribution counting method**: The architecture says `attributed_session_count` comes from sessions with `feature_cycle` matching the topic. But `scan_sessions_by_feature` already filters by feature_cycle, so all returned records would be "attributed." The total_session_count should come from `discover_sessions_for_feature` which may include fallback-attributed sessions. Verify that `discover_sessions_for_feature` includes both direct and fallback sessions, or if the handler's own observation loading (which does fallback) should be the source of total count. The current pseudocode uses `discover_sessions_for_feature` for total and `scan_sessions_by_feature` results for attributed count -- this aligns with the architecture's description.
