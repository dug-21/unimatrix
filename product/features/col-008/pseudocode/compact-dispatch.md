# Pseudocode: compact-dispatch

## Purpose

Handle CompactPayload requests in the UDS dispatcher. Construct a prioritized knowledge payload from the session's injection history (primary path) or from category-based queries (fallback path), formatted within the token budget per ADR-003.

## Changes to uds_listener.rs

### Constants

```
/// Total byte budget for compaction payload (~2000 tokens).
const MAX_COMPACTION_BYTES: usize = 8000;

/// Soft cap for decision entries (~400 tokens).
const DECISION_BUDGET_BYTES: usize = 1600;

/// Soft cap for re-injected entries (~600 tokens).
const INJECTION_BUDGET_BYTES: usize = 2400;

/// Soft cap for convention entries (~400 tokens).
const CONVENTION_BUDGET_BYTES: usize = 1600;

/// Soft cap for session context section (~200 tokens).
const CONTEXT_BUDGET_BYTES: usize = 800;
```

### dispatch_request() -- CompactPayload Arm

```
HookRequest::CompactPayload { session_id, injected_entry_ids, role, feature, token_limit } =>
    handle_compact_payload(
        session_id,
        injected_entry_ids,
        role,
        feature,
        token_limit,
        entry_store,
        session_registry,
    ).await
```

### handle_compact_payload() -- Main Handler

```
async fn handle_compact_payload(
    session_id: String,
    injected_entry_ids: Vec<u64>,
    role: Option<String>,
    feature: Option<String>,
    token_limit: Option<u32>,
    entry_store: &AsyncEntryStore<StoreAdapter>,
    session_registry: &SessionRegistry,
) -> HookResponse:

    // Determine byte budget
    let max_bytes = match token_limit {
        Some(limit) => (limit as usize) * 4,  // 4 bytes per token
        None => MAX_COMPACTION_BYTES,
    };
    let max_bytes = max_bytes.min(MAX_COMPACTION_BYTES);  // Never exceed hard ceiling

    // Get session state
    let session_state = session_registry.get_state(&session_id);

    // Determine role/feature: prefer session state, fall back to request fields
    let effective_role = session_state.as_ref().and_then(|s| s.role.clone()).or(role);
    let effective_feature = session_state.as_ref().and_then(|s| s.feature.clone()).or(feature);
    let compaction_count = session_state.as_ref().map(|s| s.compaction_count).unwrap_or(0);

    // Choose primary vs fallback path
    let has_injection_history = session_state
        .as_ref()
        .map(|s| !s.injection_history.is_empty())
        .unwrap_or(false);

    let entries_by_category = if has_injection_history {
        primary_path(session_state.as_ref().unwrap(), entry_store).await
    } else {
        fallback_path(effective_feature.as_deref(), entry_store).await
    };

    // Format payload
    let content = format_compaction_payload(
        &entries_by_category,
        effective_role.as_deref(),
        effective_feature.as_deref(),
        compaction_count,
        max_bytes,
    );

    // Increment compaction count
    session_registry.increment_compaction(&session_id);

    // Return response
    let token_count = content.as_ref().map(|c| (c.len() / 4) as u32).unwrap_or(0);
    HookResponse::BriefingContent {
        content: content.unwrap_or_default(),
        token_count,
    }
```

### primary_path() -- Fetch From Injection History

```
struct CompactionCategories {
    decisions: Vec<(EntryRecord, f64)>,     // (entry, confidence)
    injections: Vec<(EntryRecord, f64)>,    // non-decision, non-convention
    conventions: Vec<(EntryRecord, f64)>,
}

async fn primary_path(session: &SessionState, entry_store) -> CompactionCategories:
    // Deduplicate injection history: keep highest confidence per entry_id
    let mut best_confidence: HashMap<u64, f64> = HashMap::new();
    for record in &session.injection_history:
        let entry = best_confidence.entry(record.entry_id).or_insert(0.0);
        if record.confidence > *entry:
            *entry = record.confidence;

    // Fetch entries by ID
    let mut decisions = Vec::new();
    let mut injections = Vec::new();
    let mut conventions = Vec::new();

    for (entry_id, confidence) in &best_confidence:
        match entry_store.get(*entry_id).await:
            Ok(entry) =>
                // Skip quarantined entries (FR-03.2)
                if entry.status == Status::Quarantined:
                    continue
                // Partition by category
                match entry.category.as_str():
                    "decision" => decisions.push((entry, *confidence))
                    "convention" => conventions.push((entry, *confidence))
                    _ => injections.push((entry, *confidence))
            Err(_) =>
                // Skip entries that no longer exist (R-11)
                continue

    // Sort each group by confidence descending
    decisions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal));
    injections.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal));
    conventions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal));

    CompactionCategories { decisions, injections, conventions }
```

### fallback_path() -- Category-Based Query

```
async fn fallback_path(feature: Option<&str>, entry_store) -> CompactionCategories:
    // Query active decisions
    let mut decisions: Vec<(EntryRecord, f64)> = match entry_store.query_by_category("decision").await:
        Ok(entries) => entries
            .into_iter()
            .filter(|e| e.status == Status::Active)
            .map(|e| { let c = e.confidence; (e, c) })
            .collect(),
        Err(_) => Vec::new(),
    };

    // If feature tag available, prefer feature-specific decisions
    if let Some(feat) = feature:
        let feature_decisions: Vec<_> = decisions
            .iter()
            .filter(|(e, _)| e.tags.iter().any(|t| t == feat))
            .cloned()
            .collect();
        if !feature_decisions.is_empty():
            // Prepend feature-specific, then general
            let general: Vec<_> = decisions
                .into_iter()
                .filter(|(e, _)| !e.tags.iter().any(|t| t == feat))
                .collect();
            decisions = feature_decisions;
            decisions.extend(general);

    // Query active conventions
    let conventions: Vec<(EntryRecord, f64)> = match entry_store.query_by_category("convention").await:
        Ok(entries) => entries
            .into_iter()
            .filter(|e| e.status == Status::Active)
            .map(|e| { let c = e.confidence; (e, c) })
            .collect(),
        Err(_) => Vec::new(),
    };

    // Sort by confidence descending
    decisions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal));
    let mut conventions_sorted = conventions;
    conventions_sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Equal));

    CompactionCategories {
        decisions,
        injections: Vec::new(),  // No injection history in fallback
        conventions: conventions_sorted,
    }
```

### format_compaction_payload() -- Budget Allocation

```
fn format_compaction_payload(
    categories: &CompactionCategories,
    role: Option<&str>,
    feature: Option<&str>,
    compaction_count: u32,
    max_bytes: usize,
) -> Option<String>:

    // Check if we have anything to format
    if categories.decisions.is_empty()
        && categories.injections.is_empty()
        && categories.conventions.is_empty():
        return None

    let mut output = String::new();
    let mut bytes_used = 0;

    // 1. Header
    let header = "--- Unimatrix Compaction Context ---\n";
    output.push_str(header);
    bytes_used = output.len();

    // 2. Session context section (budget: CONTEXT_BUDGET_BYTES)
    let context_budget = CONTEXT_BUDGET_BYTES.min(max_bytes - bytes_used);
    let mut context_section = String::new();
    if let Some(r) = role:
        context_section.push_str(&format!("Role: {r}\n"));
    if let Some(f) = feature:
        context_section.push_str(&format!("Feature: {f}\n"));
    if compaction_count > 0:
        context_section.push_str(&format!("Compaction: #{}\n", compaction_count + 1));
    if !context_section.is_empty():
        let truncated = truncate_utf8(&context_section, context_budget);
        output.push_str(truncated);
        output.push('\n');
    bytes_used = output.len();

    // 3. Decisions section (budget: DECISION_BUDGET_BYTES + unused from context)
    let remaining = max_bytes - bytes_used;
    let decision_budget = DECISION_BUDGET_BYTES.min(remaining);
    bytes_used += format_category_section(
        &mut output, "Decisions", &categories.decisions, decision_budget
    );

    // 4. Injections section (budget: INJECTION_BUDGET_BYTES + unused from decisions)
    let remaining = max_bytes - bytes_used;
    let injection_budget = INJECTION_BUDGET_BYTES.min(remaining);
    bytes_used += format_category_section(
        &mut output, "Key Context", &categories.injections, injection_budget
    );

    // 5. Conventions section (budget: CONVENTION_BUDGET_BYTES + unused from injections)
    let remaining = max_bytes - bytes_used;
    let convention_budget = CONVENTION_BUDGET_BYTES.min(remaining);
    format_category_section(
        &mut output, "Conventions", &categories.conventions, convention_budget
    );

    // Hard ceiling check
    if output.len() > max_bytes:
        output = truncate_utf8(&output, max_bytes).to_string();

    Some(output)


fn format_category_section(
    output: &mut String,
    section_name: &str,
    entries: &[(EntryRecord, f64)],
    budget: usize,
) -> usize:
    if entries.is_empty() || budget < 50:
        return 0

    let start_len = output.len();
    let section_header = format!("\n## {section_name}\n");
    if section_header.len() > budget:
        return 0
    output.push_str(&section_header);

    for (entry, confidence) in entries:
        let confidence_pct = (confidence * 100.0) as u32;
        let status_indicator = if entry.status == Status::Deprecated { " [deprecated]" } else { "" };
        let block = format!(
            "[{}]{} ({}% confidence)\n{}\n<!-- id:{} -->\n\n",
            entry.title, status_indicator, confidence_pct, entry.content, entry.id
        );

        let current_section_bytes = output.len() - start_len;
        let projected = current_section_bytes + block.len();
        if projected <= budget:
            output.push_str(&block);
        else:
            let remaining = budget.saturating_sub(current_section_bytes);
            if remaining < 100:
                break
            let truncated = truncate_utf8(&block, remaining);
            output.push_str(truncated);
            break

    output.len() - start_len  // bytes consumed by this section
```

### truncate_utf8() -- Reuse from hook.rs

The `truncate_utf8` function in hook.rs is private. Either:
- Make it a shared utility (in a common module), or
- Duplicate it in uds_listener.rs (simpler, follows existing no-shared pattern from ADR-001)

Recommend: duplicate in uds_listener.rs (2 lines of logic, not worth a new module).

## Error Handling

- entry_store.get() fails: skip that entry, continue with others (R-11)
- All entry fetches fail: return BriefingContent with empty content (FR-03.9)
- entry_store.query_by_category() fails: treat as empty result
- Mutex poisoning in SessionRegistry: recovered via into_inner()
- token_limit overflow protection: min(requested, MAX_COMPACTION_BYTES)

## Key Test Scenarios

1. CompactPayload with injection history: returns BriefingContent with entries sorted by category priority
2. CompactPayload without injection history: returns fallback entries
3. CompactPayload for unknown session: returns fallback entries
4. Budget enforcement: total output <= MAX_COMPACTION_BYTES
5. Multi-byte UTF-8 content truncated at char boundary
6. Quarantined entries excluded from payload
7. Deprecated entries included with indicator
8. Empty injection history + empty knowledge base: returns empty BriefingContent
9. Category budget rollover: unused decision budget goes to injections
10. Single entry exceeding category budget: truncated at char boundary
11. token_limit override: respects provided limit
12. increment_compaction called after formatting
13. CompactPayload latency benchmark: p95 < 15ms for 20 entries
