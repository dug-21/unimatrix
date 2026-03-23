# Listener Dispatch — Pseudocode
# File: crates/unimatrix-server/src/uds/listener.rs

## Purpose

Two changes to `listener.rs`:

1. **`dispatch_request` ContextSearch arm**: Replace the hardcoded `"UserPromptSubmit"` literal
   in the `ObservationRow` `hook` field with `source.as_deref().unwrap_or("UserPromptSubmit")`.
2. **`handle_compact_payload` + `format_compaction_payload` migration**: Replace `BriefingService`
   call and `CompactionCategories` partitioning with `IndexBriefingService::index()` and flat
   indexed table output via `format_index_table`.

`handle_context_search()` is unchanged. The UDS listener accept loop is unchanged.

---

## Change 1: `dispatch_request` — Source Field Wiring

### Current ContextSearch arm (lines ~781-839)

```rust
HookRequest::ContextSearch {
    query,
    session_id,
    role: _,
    task: _,
    feature: _,
    k,
    max_tokens: _,
} => {
    // ...
    let obs = ObservationRow {
        // ...
        hook: "UserPromptSubmit".to_string(),  // ← hardcoded, WRONG for SubagentStart
        // ...
    };
```

### After: Destructure `source` and use it for the hook column

```rust
HookRequest::ContextSearch {
    query,
    session_id,
    role: _,
    task: _,
    feature: _,
    k,
    max_tokens: _,
    source,           // NEW: destructured from the variant
} => {
    // ... existing sanitize_session_id check unchanged ...
    // ... existing topic_signal extraction unchanged ...

    if let Some(ref sid) = session_id {
        if !query.is_empty() {
            if let Some(ref signal) = topic_signal {
                session_registry.record_topic_signal(sid, signal.clone(), unix_now_secs());
            }

            let truncated_input: String = query.chars().take(4096).collect();
            let obs = ObservationRow {
                session_id: sid.clone(),
                ts_millis: (unix_now_secs() as i64).saturating_mul(1000),
                // ADR-001 crt-027: use source field, default to "UserPromptSubmit"
                hook: source.as_deref().unwrap_or("UserPromptSubmit").to_string(),
                tool: None,
                input: Some(truncated_input),
                response_size: None,
                response_snippet: None,
                topic_signal: topic_signal.clone(),
            };

            let store_for_obs = Arc::clone(store);
            spawn_blocking_fire_and_forget(move || {
                if let Err(e) = insert_observation(&store_for_obs, &obs) {
                    tracing::error!(error = %e, "col-018: observation write failed");
                }
            });
        }
    }

    handle_context_search(query, session_id, k, store, session_registry, services).await
}
```

The `source` variable from destructuring is moved into the closure via `obs`. After the
`ObservationRow` construction, `source` is consumed. This is correct — it is not needed
after the observation is constructed.

Important: The observation comment should read `"col-018: observation write failed"` to
preserve the existing comment pattern. Update the comment from `"UserPromptSubmit observation
write failed"` to `"observation write failed"` since the source is no longer hardcoded.

---

## Change 2: `handle_compact_payload` — IndexBriefingService Migration

### Deleted items

- `CompactionCategories` struct (lines ~1122-1126) — **deleted entirely**
- `format_category_section()` function — **deleted**
- Budget constants: `DECISION_BUDGET_BYTES`, `INJECTION_BUDGET_BYTES`, `CONVENTION_BUDGET_BYTES`,
  `CONTEXT_BUDGET_BYTES` — **deleted** (only `MAX_COMPACTION_BYTES` is retained)

### New/modified: `handle_compact_payload`

**Existing signature** (unchanged):
```
async fn handle_compact_payload(
    session_id: &str,
    role: Option<String>,
    feature: Option<String>,
    token_limit: Option<u32>,
    session_registry: &SessionRegistry,
    services: &crate::services::ServiceLayer,
) -> HookResponse
```

**New body**:

```
async fn handle_compact_payload(
    session_id: &str,
    role: Option<String>,
    feature: Option<String>,
    token_limit: Option<u32>,
    session_registry: &SessionRegistry,
    services: &crate::services::ServiceLayer,
) -> HookResponse {
    // 1. Byte/token budget (unchanged)
    let max_bytes = match token_limit {
        Some(limit) => ((limit as usize) * 4).min(MAX_COMPACTION_BYTES),
        None => MAX_COMPACTION_BYTES,
    };

    // 2. Session state resolution (unchanged)
    let session_state = session_registry.get_state(session_id);
    let effective_role = session_state.as_ref().and_then(|s| s.role.clone()).or(role);
    let effective_feature = session_state
        .as_ref()
        .and_then(|s| s.feature.clone())
        .or(feature);
    let compaction_count = session_state.as_ref().map(|s| s.compaction_count).unwrap_or(0);

    // crt-026: category histogram (unchanged — retained for histogram block)
    let category_histogram = session_registry.get_category_histogram(session_id);

    // 3. Query derivation via shared helper (FR-11, AC-09, AC-10)
    // UDS path: session_state already held, NO SessionRegistry lookup for step 2
    let query = crate::services::index_briefing::derive_briefing_query(
        None,                          // task: None (no task param on CompactPayload)
        session_state.as_ref(),        // step 2: reads feature_cycle + topic_signals
        effective_feature.as_deref().unwrap_or(""),  // step 3: fallback topic
    );

    // 4. Build IndexBriefingParams
    let briefing_params = crate::services::index_briefing::IndexBriefingParams {
        query,
        k: 20,                              // default k (not from UNIMATRIX_BRIEFING_K)
        session_id: Some(session_id.to_string()),  // for WA-2 histogram boost
        max_tokens: Some(max_bytes / 4),    // approximate token budget
    };

    // 5. Build AuditContext (unchanged from current)
    let audit_ctx = crate::services::AuditContext {
        source: crate::services::AuditSource::Uds {
            uid: 0,
            pid: None,
            session_id: session_id.to_string(),
        },
        caller_id: "uds-compact".to_string(),
        session_id: Some(session_id.to_string()),
        feature_cycle: None,
    };

    // 6. Delegate to IndexBriefingService
    let entries = match services
        .briefing
        .index(briefing_params, &audit_ctx, None)
        .await
    {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("compact payload index failed: {e}");
            // FM-02: graceful degradation — return empty but valid BriefingContent
            return HookResponse::BriefingContent {
                content: String::new(),
                token_count: 0,
            };
        }
    };

    // 7. Format payload (updated signature accepting Vec<IndexEntry>)
    let content = format_compaction_payload(
        &entries,
        effective_role.as_deref(),
        effective_feature.as_deref(),
        compaction_count,
        max_bytes,
        &category_histogram,
    );

    // 8. Increment compaction count (unchanged)
    session_registry.increment_compaction(session_id);

    let token_count = content.as_ref().map(|c| (c.len() / 4) as u32).unwrap_or(0);

    HookResponse::BriefingContent {
        content: content.unwrap_or_default(),
        token_count,
    }
}
```

Imports required: Add `use crate::mcp::response::briefing::{IndexEntry, format_index_table};`
to `listener.rs` imports (or use the full path). Also add `use crate::services::index_briefing`.

---

## Change 3: `format_compaction_payload` — Rewritten

### New signature:

```
fn format_compaction_payload(
    entries: &[IndexEntry],
    role: Option<&str>,
    feature: Option<&str>,
    compaction_count: u32,
    max_bytes: usize,
    category_histogram: &HashMap<String, u32>,
) -> Option<String>
```

### New body:

```
fn format_compaction_payload(
    entries: &[IndexEntry],
    role: Option<&str>,
    feature: Option<&str>,
    compaction_count: u32,
    max_bytes: usize,
    category_histogram: &HashMap<String, u32>,
) -> Option<String> {
    // AC-18 part 1: if both entries and histogram are empty, return None
    if entries.is_empty() && category_histogram.is_empty() {
        return None;
    }

    let mut output = String::new();

    // Header (AC-18 part 2, format_payload_header_present)
    output.push_str("--- Unimatrix Compaction Context ---\n");

    // Session context block (format_payload_session_context)
    // Role, Feature, Compaction# lines — same as before
    let mut context_section = String::new();
    if let Some(r) = role {
        context_section.push_str(&format!("Role: {r}\n"));
    }
    if let Some(f) = feature {
        context_section.push_str(&format!("Feature: {f}\n"));
    }
    if compaction_count > 0 {
        context_section.push_str(&format!("Compaction: #{}\n", compaction_count + 1));
    }
    if !context_section.is_empty() {
        // Truncate context section to avoid overflow
        let context_budget = 800_usize.min(max_bytes.saturating_sub(output.len()));
        let truncated = truncate_utf8(&context_section, context_budget);
        output.push_str(truncated);
        output.push('\n');
    }

    // Flat indexed table (AC-08, AC-19, format_payload_sorted_by_confidence)
    // IndexBriefingService already sorts by fused score descending.
    // format_index_table produces the flat table; no section headers.
    if !entries.is_empty() {
        let table = format_index_table(entries);
        let remaining = max_bytes.saturating_sub(output.len());
        if !table.is_empty() && table.len() <= remaining {
            output.push_str(&table);
        } else if !table.is_empty() {
            // Budget enforcement: truncate table to fit (NFR-03, AC-16)
            // Drop lowest-ranked rows by taking only the rows that fit within budget.
            // Strategy: build table row-by-row and stop when budget is exceeded.
            // Simpler approach: truncate the string at UTF-8 boundary.
            // Note: truncating mid-table is acceptable per spec (rows dropped from end = lowest ranked)
            let truncated = truncate_utf8(&table, remaining);
            output.push_str(truncated);
        }
    }

    // Histogram block (AC-21, format_compact_payload_histogram_block_present_and_absent)
    // Same logic as before — preserved from existing format_compaction_payload
    if !category_histogram.is_empty() {
        let mut hist_entries: Vec<(&String, u32)> = category_histogram
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(cat, count)| (cat, *count))
            .collect();

        if !hist_entries.is_empty() {
            hist_entries.sort_by(|a, b| b.1.cmp(&a.1));
            hist_entries.truncate(5);  // top-5 cap

            let parts: Vec<String> = hist_entries
                .iter()
                .map(|(cat, count)| format!("{} \u{00d7} {}", cat, count))
                .collect();
            let summary_line = format!("Recent session activity: {}\n", parts.join(", "));

            let remaining = max_bytes.saturating_sub(output.len());
            if summary_line.len() <= remaining {
                output.push_str(&summary_line);
            }
        }
    }

    // Hard ceiling truncation (AC-16, format_payload_budget_enforcement)
    if output.len() > max_bytes {
        let truncated = truncate_utf8(&output, max_bytes);
        return Some(truncated.to_string());
    }

    // AC-18: if output is only the header (no entries and no histogram) return None
    // This handles the edge case where entries is empty but histogram was non-empty
    // and resulted in only the header being written with no histogram content.
    // Per AC-18: "when histogram is non-empty but entries are empty, return Some(...)"
    // So: if output has only the header and nothing else, we still return Some if histogram
    // was non-empty (the test asserts Some when histogram is non-empty, entries empty).
    // The current logic naturally handles this: histogram block will have been appended.

    Some(output)
}
```

### Edge case: budget row-by-row approach (preferred for AC-16)

The row-by-row approach for flat table budget is more correct than string truncation:

```
// Build the flat table within budget
let remaining = max_bytes.saturating_sub(output.len());
if !entries.is_empty() && remaining > 0 {
    // Option A: Use format_index_table on a budget-capped slice
    // Find how many rows fit within the remaining budget
    let header = build_index_table_header();  // produces the header + separator line
    if header.len() < remaining {
        output.push_str(&header);
        let mut row_budget = remaining.saturating_sub(header.len());
        for (row_num, entry) in entries.iter().enumerate() {
            let row = format_index_row(row_num + 1, entry);
            if row.len() <= row_budget {
                output.push_str(&row);
                row_budget = row_budget.saturating_sub(row.len());
            } else {
                break;  // lowest-ranked rows dropped first
            }
        }
    }
}
```

However, this requires `format_index_table` to expose a row-by-row API or for the
inline logic to duplicate the formatting. The simpler approach is to call
`format_index_table(entries)` on a row-count-capped slice:

```
// Find how many entries fit within max_bytes
let mut fitting_count = entries.len();
loop {
    let candidate = format_index_table(&entries[..fitting_count]);
    if candidate.len() <= remaining || fitting_count == 0 {
        output.push_str(&candidate);
        break;
    }
    fitting_count -= 1;
}
```

This approach calls `format_index_table` multiple times (O(k^2) worst case for k=20,
which is negligible). It correctly drops lowest-ranked rows first (entries are sorted
by descending confidence, so the last entries are lowest-ranked).

The implementation agent should choose whichever approach is cleaner in context.

---

## Imports Required in listener.rs

Add to the existing `use` block:

```rust
use crate::mcp::response::briefing::{IndexEntry, format_index_table, SNIPPET_CHARS};
use crate::services::index_briefing::{IndexBriefingParams, derive_briefing_query};
```

Or use full paths inline. The key constraint: `IndexEntry` and `format_index_table` are
defined in `mcp/response/briefing.rs` and must NOT be re-exported or redefined in `listener.rs`.

---

## Error Handling

- `IndexBriefingService::index()` returns `Result<Vec<IndexEntry>, ServiceError>`. On `Err`,
  `handle_compact_payload` returns `HookResponse::BriefingContent { content: String::new(), token_count: 0 }` (FM-02 graceful degradation). This matches the existing error path behavior.
- `format_compaction_payload` returns `Option<String>`. `None` is returned only when both
  entries is empty AND histogram is empty.
- Budget overflow is handled by truncation, never by panic.

---

## Key Test Scenarios

All in `listener.rs` `#[cfg(test)]` block.

### Observation Tagging (R-01, R-12)

**T-LD-01** `dispatch_context_search_source_subagentstart_tags_observation` (AC-05, R-12):
- Submit HookRequest::ContextSearch { source: Some("SubagentStart"), session_id: Some("s1"), query: "test", k: None, ... }
- Query observations table for session "s1"
- Assert: observation row has hook == "SubagentStart"

**T-LD-02** `dispatch_context_search_source_none_tags_userpromptsubmit` (AC-05):
- Submit HookRequest::ContextSearch { source: None, session_id: Some("s1"), query: "test query five words", k: None, ... }
- Assert: observation row has hook == "UserPromptSubmit"

**T-LD-03** `dispatch_context_search_source_absent_json_defaults_to_userpromptsubmit` (R-01, AC-05):
- Deserialize a JSON blob with source key absent: `{"type": "ContextSearch", "query": "hello world test now"}`
- Submit to dispatch_request
- Assert: observation hook == "UserPromptSubmit"

### format_compaction_payload (R-03 — all 11 named tests required)

**T-LD-04** `format_payload_empty_entries_returns_none` (AC-18, non-negotiable):
- Call: format_compaction_payload(&[], None, None, 0, 8000, &HashMap::new())
- Assert: returns None

**T-LD-05** `format_payload_header_present` (R-03):
- Call: format_compaction_payload(&[one_entry], None, None, 0, 8000, &HashMap::new())
- Assert: result.unwrap() starts with "--- Unimatrix Compaction Context ---\n"

**T-LD-06** `format_payload_sorted_by_confidence` (AC-19, non-negotiable):
- Input: entries = [IndexEntry { confidence: 0.3, id: 1, ... }, IndexEntry { confidence: 0.9, id: 2, ... }]
  (LOW confidence first in input, HIGH confidence second)
- Note: IndexBriefingService sorts by descending confidence before returning. However,
  format_compaction_payload itself must also respect the input order.
- The test should verify that entries are passed to format_index_table in the order
  they were received from IndexBriefingService (confidence descending).
- Assert: row 1 in output has confidence 0.90, row 2 has confidence 0.30

**T-LD-07** `format_payload_budget_enforcement` (AC-16, non-negotiable):
- Input: entries with large snippets, max_bytes = 200
- Assert: output.len() <= 200

**T-LD-08** `format_payload_multibyte_utf8` (AC-17, non-negotiable):
- Create an IndexEntry with snippet containing CJK chars: "\u{4e16}\u{754c}".repeat(200)
  truncated to SNIPPET_CHARS. The snippet itself is already truncated by IndexBriefingService.
- Pass to format_compaction_payload
- Assert: the output is valid UTF-8 (output.is_valid_utf8() or Rust strings are always valid UTF-8)
- Assert: truncation at max_bytes does not produce invalid UTF-8

**T-LD-09** `format_payload_session_context` (R-03):
- Call: format_compaction_payload(&[one_entry], Some("developer"), Some("crt-027"), 2, 8000, &HashMap::new())
- Assert: output contains "Role: developer\n"
- Assert: output contains "Feature: crt-027\n"
- Assert: output contains "Compaction: #3\n" (compaction_count + 1 = 2 + 1 = 3)

**T-LD-10** `format_payload_active_entries_only` (R-03):
- IndexBriefingService only returns Active entries — format_compaction_payload receives
  only Active entries. This test verifies no "[deprecated]" marker appears.
- Input: entries from a database with one Active + one Deprecated entry
- Assert: output does NOT contain "[deprecated]" or "[Deprecated]"
- Assert: output contains only the Active entry's id

**T-LD-11** `format_payload_entry_id_metadata` (R-03):
- Input: IndexEntry { id: 42, topic: "test", ... }
- Assert: output contains "42" (the id appears in the flat table id column)

**T-LD-12** `format_payload_token_limit_override` (AC-20):
- Call format_compaction_payload with max_bytes=400 and large entries
- Assert: output.len() <= 400

**T-LD-13** `test_compact_payload_histogram_block_present` (AC-21, non-negotiable):
- Input: category_histogram = {"decision": 3, "pattern": 2}
- Assert: output contains "Recent session activity:"
- Assert: output contains "decision"

**T-LD-14** `test_compact_payload_histogram_block_absent` (AC-21, non-negotiable):
- Input: category_histogram = HashMap::new()
- Assert: output does NOT contain "Recent session activity:"
