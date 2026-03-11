# status-integration Pseudocode

## Purpose

Wire effectiveness computation into StatusService Phase 8 and extend StatusReport + format_status_report to render effectiveness data in all three output formats (summary, markdown, JSON). Touches two files in unimatrix-server.

## File: `crates/unimatrix-server/src/services/status.rs`

### Phase 8 in `compute_report`

Insert after Phase 7 (retrospected feature count) and before `Ok((report, active_entries))`.

```
// Phase 8: Effectiveness analysis (crt-018)
let store_for_eff = Arc::clone(&self.store);
let effectiveness = match tokio::task::spawn_blocking(move || {
    // Step 1: Get aggregates (4 SQL queries, 1 lock_conn)
    let aggregates = store_for_eff.compute_effectiveness_aggregates()?;

    // Step 2: Get entry metadata (1 SQL query)
    let entry_meta = store_for_eff.load_entry_classification_meta()?;

    Ok::<_, StoreError>((aggregates, entry_meta))
}).await {
    Ok(Ok((aggregates, entry_meta))) => {
        // Step 3: Build lookup from entry_id -> injection stats
        let stats_map: HashMap<u64, &EntryInjectionStats> =
            aggregates.entry_stats.iter()
                .map(|s| (s.entry_id, s))
                .collect();

        // Step 4: Classify each active entry
        let classifications: Vec<EntryEffectiveness> = entry_meta.iter().map(|meta| {
            // Look up injection stats; default to zero if entry has no injection_log rows
            let (inj_count, success, rework, abandoned) = match stats_map.get(&meta.entry_id) {
                Some(stats) => (stats.injection_count, stats.success_count,
                                stats.rework_count, stats.abandoned_count),
                None => (0, 0, 0, 0),
            };

            // Determine topic activity (ADR-002)
            // meta.topic is already "(unattributed)" for NULL/empty topics (handled by SQL)
            let topic_has_sessions = aggregates.active_topics.contains(&meta.topic);

            classify_entry(
                meta.entry_id,
                &meta.title,
                &meta.topic,
                &meta.trust_source,
                meta.helpful_count,
                meta.unhelpful_count,
                inj_count,
                success,
                rework,
                abandoned,
                topic_has_sessions,
                NOISY_TRUST_SOURCES,
            )
        }).collect();

        // Step 5: Build DataWindow from raw aggregates
        let data_window = DataWindow {
            session_count: aggregates.session_count,
            earliest_session_at: aggregates.earliest_session_at,
            latest_session_at: aggregates.latest_session_at,
        };

        // Step 6: Assemble report
        Some(build_report(classifications, &aggregates.calibration_rows, data_window))
    }
    Ok(Err(e)) => {
        // Store error: graceful degradation (R-11)
        // Log warning, set effectiveness = None
        tracing::warn!("Effectiveness query failed: {e}");
        None
    }
    Err(join_err) => {
        // spawn_blocking panic: graceful degradation (R-11)
        tracing::warn!("Effectiveness task panicked: {join_err}");
        None
    }
};

report.effectiveness = effectiveness;
```

Key implementation details:
- Follows existing Phase pattern: spawn_blocking, match on Ok/Err, set report field
- Graceful degradation on any failure: effectiveness = None (matches contradiction scan pattern, R-11)
- stats_map handles orphaned injection stats (entry deleted between two queries) by building map from stats and only iterating entry_meta
- entry_meta entries with no injection stats get zero counts -> classified as Unmatched or Effective
- No unwrap() on spawn_blocking result -- explicit match (R-11)

### Imports needed in status.rs

```
use unimatrix_engine::effectiveness::{
    classify_entry, build_report, DataWindow, EffectivenessReport,
    NOISY_TRUST_SOURCES,
};
use std::collections::HashMap;
// EntryInjectionStats -- used by reference from store types
```

## File: `crates/unimatrix-server/src/mcp/response/status.rs`

### StatusReport extension

Add field after the last existing field (coherence_by_source):

```
/// Effectiveness analysis results (None if no injection data or query failure).
pub effectiveness: Option<EffectivenessReport>,
```

Initialize to `None` in the StatusReport construction at the top of compute_report.

### StatusReportJson extension

Add to the StatusReportJson struct:

```
#[serde(skip_serializing_if = "Option::is_none")]
effectiveness: Option<EffectivenessReportJson>,
```

Define EffectivenessReportJson:

```
#[derive(Serialize)]
struct EffectivenessReportJson {
    by_category: Vec<CategoryCount>,
    by_source: Vec<SourceEffectivenessJson>,
    calibration_buckets: Vec<CalibrationBucketJson>,
    ineffective_entries: Vec<IneffectiveEntryJson>,   // top 10
    noisy_entries: Vec<NoisyEntryJson>,                // all
    unmatched_entries: Vec<UnmatchedEntryJson>,        // top 10
    data_window: DataWindowJson,
}

#[derive(Serialize)]
struct CategoryCount {
    category: String,           // "effective", "settled", etc.
    count: u32,
}

#[derive(Serialize)]
struct SourceEffectivenessJson {
    trust_source: String,
    effective: u32,
    settled: u32,
    unmatched: u32,
    ineffective: u32,
    noisy: u32,
    utility_ratio: f64,
}

#[derive(Serialize)]
struct CalibrationBucketJson {
    range_low: f64,
    range_high: f64,
    injection_count: u32,
    actual_success_rate: f64,
    expected_success_rate: f64,   // bucket midpoint: (lower + upper) / 2.0
}

#[derive(Serialize)]
struct IneffectiveEntryJson {
    entry_id: u64,
    title: String,
    injection_count: u32,
    success_rate: f64,
}

#[derive(Serialize)]
struct NoisyEntryJson {
    entry_id: u64,
    title: String,
}

#[derive(Serialize)]
struct UnmatchedEntryJson {
    entry_id: u64,
    title: String,
    topic: String,
}

#[derive(Serialize)]
struct DataWindowJson {
    session_count: u32,
    span_days: u64,             // computed from earliest/latest
}
```

### StatusReportJson From impl extension

In the `From<&StatusReport> for StatusReportJson` impl, add effectiveness mapping:

```
let effectiveness = report.effectiveness.as_ref().map(|eff| {
    let by_category = eff.by_category.iter().map(|(cat, count)| {
        CategoryCount {
            category: format!("{:?}", cat).to_lowercase(),  // "effective", "settled", etc.
            count: *count,
        }
    }).collect();

    let by_source = eff.by_source.iter().map(|s| SourceEffectivenessJson {
        trust_source: s.trust_source.clone(),
        effective: s.effective_count,
        settled: s.settled_count,
        unmatched: s.unmatched_count,
        ineffective: s.ineffective_count,
        noisy: s.noisy_count,
        utility_ratio: s.aggregate_utility,
    }).collect();

    let calibration_buckets = eff.calibration.iter().map(|b| CalibrationBucketJson {
        range_low: b.confidence_lower,
        range_high: b.confidence_upper,
        injection_count: b.entry_count,
        actual_success_rate: b.actual_success_rate,
        expected_success_rate: (b.confidence_lower + b.confidence_upper) / 2.0,
    }).collect();

    let ineffective_entries = eff.top_ineffective.iter().map(|e| IneffectiveEntryJson {
        entry_id: e.entry_id,
        title: e.title.clone(),
        injection_count: e.injection_count,
        success_rate: e.success_rate,
    }).collect();

    let noisy_entries = eff.noisy_entries.iter().map(|e| NoisyEntryJson {
        entry_id: e.entry_id,
        title: e.title.clone(),
    }).collect();

    let unmatched_entries = eff.unmatched_entries.iter().map(|e| UnmatchedEntryJson {
        entry_id: e.entry_id,
        title: e.title.clone(),
        topic: e.topic.clone(),
    }).collect();

    // Compute span_days from DataWindow timestamps
    let span_days = match (eff.data_window.earliest_session_at, eff.data_window.latest_session_at) {
        (Some(earliest), Some(latest)) if latest > earliest =>
            (latest - earliest) / 86400,   // seconds -> days
        _ => 0,
    };

    EffectivenessReportJson {
        by_category,
        by_source,
        calibration_buckets,
        ineffective_entries,
        noisy_entries,
        unmatched_entries,
        data_window: DataWindowJson {
            session_count: eff.data_window.session_count,
            span_days,
        },
    }
});

// Add to StatusReportJson construction:
// effectiveness,
```

### format_status_report: Summary format

Append effectiveness line after existing summary content. Add before the final `CallToolResult::success(...)`:

```
// Effectiveness line (FR-05)
match &report.effectiveness {
    Some(eff) => {
        // Count per category
        let counts: HashMap<_, _> = eff.by_category.iter().cloned().collect();
        let effective = counts.get(&Effective).unwrap_or(&0);
        let settled = counts.get(&Settled).unwrap_or(&0);
        let unmatched = counts.get(&Unmatched).unwrap_or(&0);
        let ineffective = counts.get(&Ineffective).unwrap_or(&0);
        let noisy = counts.get(&Noisy).unwrap_or(&0);

        text.push_str(&format!(
            "\nEffectiveness: {} effective, {} settled, {} unmatched, {} ineffective, {} noisy ({} sessions analyzed)",
            effective, settled, unmatched, ineffective, noisy,
            eff.data_window.session_count
        ));
    }
    None => {
        text.push_str("\nEffectiveness: no injection data");
    }
}
```

### format_status_report: Markdown format

Append `### Effectiveness Analysis` section after existing markdown sections:

```
// Effectiveness section (FR-06)
match &report.effectiveness {
    Some(eff) => {
        // Data window indicator
        let span = match (eff.data_window.earliest_session_at, eff.data_window.latest_session_at) {
            (Some(e), Some(l)) if l > e => format!("{} days", (l - e) / 86400),
            _ => "< 1 day".to_string(),
        };
        text.push_str(&format!(
            "\n### Effectiveness Analysis\n\nAnalysis covers {} sessions over {}.\n\n",
            eff.data_window.session_count, span
        ));

        // Category table
        text.push_str("| Category | Count | % of Active |\n|----------|-------|-------------|\n");
        let total: u32 = eff.by_category.iter().map(|(_, c)| c).sum();
        for (cat, count) in &eff.by_category {
            let pct = if total > 0 { (*count as f64 / total as f64) * 100.0 } else { 0.0 };
            text.push_str(&format!("| {:?} | {} | {:.1}% |\n", cat, count, pct));
        }

        // Per-source table
        text.push_str("\n| Source | Effective | Settled | Unmatched | Ineffective | Noisy | Utility |\n");
        text.push_str("|--------|-----------|---------|-----------|-------------|-------|---------|\n");
        for s in &eff.by_source {
            text.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {:.2} |\n",
                s.trust_source, s.effective_count, s.settled_count,
                s.unmatched_count, s.ineffective_count, s.noisy_count,
                s.aggregate_utility
            ));
        }

        // Calibration table
        text.push_str("\n| Confidence | Injections | Actual Success | Expected |\n");
        text.push_str("|------------|------------|----------------|----------|\n");
        for b in &eff.calibration {
            let expected = (b.confidence_lower + b.confidence_upper) / 2.0;
            text.push_str(&format!(
                "| {:.1}-{:.1} | {} | {:.2} | {:.2} |\n",
                b.confidence_lower, b.confidence_upper,
                b.entry_count, b.actual_success_rate, expected
            ));
        }

        // Top ineffective entries (R-12: up to 10 with entry_id, title)
        if !eff.top_ineffective.is_empty() {
            text.push_str("\n**Top Ineffective Entries:**\n\n");
            text.push_str("| ID | Title | Injections | Success Rate |\n");
            text.push_str("|----|-------|------------|-------------|\n");
            for e in &eff.top_ineffective {
                // R-12: sanitize title for markdown table (replace | with /)
                let safe_title = e.title.replace('|', "/");
                text.push_str(&format!(
                    "| {} | {} | {} | {:.2} |\n",
                    e.entry_id, safe_title, e.injection_count, e.success_rate
                ));
            }
        }

        // Noisy entries
        if !eff.noisy_entries.is_empty() {
            text.push_str("\n**Noisy Entries:**\n\n");
            text.push_str("| ID | Title |\n|----|-------|\n");
            for e in &eff.noisy_entries {
                let safe_title = e.title.replace('|', "/");
                text.push_str(&format!("| {} | {} |\n", e.entry_id, safe_title));
            }
        }

        // Unmatched entries (up to 10)
        if !eff.unmatched_entries.is_empty() {
            text.push_str("\n**Unmatched Entries:**\n\n");
            text.push_str("| ID | Title | Topic |\n|----|-------|-------|\n");
            for e in &eff.unmatched_entries {
                let safe_title = e.title.replace('|', "/");
                let safe_topic = e.topic.replace('|', "/");
                text.push_str(&format!("| {} | {} | {} |\n", e.entry_id, safe_title, safe_topic));
            }
        }
    }
    None => {
        text.push_str("\n### Effectiveness Analysis\n\nInsufficient injection data for analysis.\n");
    }
}
```

R-12 mitigation: Entry titles containing `|` are sanitized by replacing with `/` to prevent markdown table breakage. Newlines in titles would also break tables; replace `\n` with ` ` as well.

### Imports needed in response/status.rs

```
use unimatrix_engine::effectiveness::{
    EffectivenessReport, EffectivenessCategory, DataWindow,
};
```

## Error Handling

- Phase 8 errors (store or panic) -> `effectiveness = None` (graceful degradation per NFR-06, R-11)
- No new error types needed
- Pattern matches existing contradiction scan error handling in Phase 2

## Initialization

StatusReport construction (at top of compute_report) must initialize:
```
effectiveness: None,
```

This is set to `Some(report)` only if Phase 8 succeeds.

## Key Test Scenarios

1. **End-to-end (AC-15)**: Insert entries + injection_log + sessions -> call compute_report -> verify effectiveness field is Some with correct category counts
2. **All three formats (AC-08, AC-09, AC-10)**:
   - Summary contains "Effectiveness: N effective, ..." line
   - Markdown contains "### Effectiveness Analysis" section with tables
   - JSON contains "effectiveness" object, parseable, with correct structure
3. **No injection data (NFR-06)**: Empty injection_log -> Summary shows "no injection data", Markdown shows "Insufficient injection data", JSON omits effectiveness field
4. **JSON skip_serializing_if (R-08)**: When effectiveness is None, "effectiveness" key absent from JSON output
5. **Store error graceful degradation (R-11)**: Simulated store failure -> effectiveness = None, rest of report unaffected
6. **Markdown title sanitization (R-12)**: Entry with `|` in title -> table not broken
7. **Read-only verification (AC-13)**: Row counts before/after status call are identical
8. **DataWindow span_days computation**: 2 sessions 7 days apart -> span_days = 7
9. **Phase 8 independence**: Effectiveness computation does not depend on Phase 1-7 results (except that the report struct exists)
