# crt-026: Component — `handle_context_search` + `handle_compact_payload` + `format_compaction_payload` (`uds/listener.rs`)

File: `crates/unimatrix-server/src/uds/listener.rs`
Wave: 2 (depends on Wave 1: `session.rs` must have `get_category_histogram`; `search-params.md` must be implemented)

---

## Purpose

Two changes in `uds/listener.rs`:

1. **`handle_context_search`**: Add histogram pre-resolution (same pattern as MCP path)
   after the existing `sanitize_session_id` check, before `ServiceSearchParams` construction.

2. **`handle_compact_payload` + `format_compaction_payload`**: Add `get_category_histogram`
   call after session_state resolution. Pass result to `format_compaction_payload`. Append
   histogram summary block when non-empty.

---

## Component 1: `handle_context_search` modifications

### Current structure (lines 942-994)

```
async fn handle_context_search(
    query: String,
    session_id: Option<String>,
    k: Option<u32>,
    store: &Arc<Store>,
    session_registry: &SessionRegistry,
    services: &crate::services::ServiceLayer,
) -> HookResponse {
    // 1. Parse k
    let k = k.map(|v| v as usize).unwrap_or(INJECTION_K);

    // 2. Build AuditContext (UDS transport)
    let audit_ctx = ...;

    // 3. Build ServiceSearchParams with UDS-specific floors
    let service_params = ServiceSearchParams {
        query, k, filters: None, similarity_floor, confidence_floor,
        feature_tag: None, co_access_anchors: None, caller_agent_id: None,
        retrieval_mode: Strict,
    };

    // 4. Delegate to SearchService
    services.search.search(service_params, ...).await
```

### Where to insert

The `sanitize_session_id` check fires in the caller (the main `handle_hook` dispatch block
at lines 796-803) BEFORE `handle_context_search` is called. By the time execution reaches
`handle_context_search`, the `session_id` string (if `Some`) is already validated.

The pre-resolution block goes after the `AuditContext` construction (step 2) and BEFORE
the `ServiceSearchParams` construction (step 3). This ensures the histogram is captured
synchronously before the first `.await` (the `services.search.search(...).await` at step 4).

### New block to insert (between steps 2 and 3)

```
// crt-026: Pre-resolve session histogram for histogram affinity boost (WA-2, ADR-002).
// Follows the crt-025 SR-07 snapshot pattern: session state is read once synchronously
// before any await point (R-13).
//
// session_id in this path comes from HookRequest::ContextSearch payload field (OQ-B confirmed).
// sanitize_session_id was already applied in the dispatch block at lines 796-803 before
// this function was called — no additional sanitization needed here.
//
// Maps is_empty() → None: cold-start path (category_histogram = None → boost = 0.0).
let category_histogram: Option<HashMap<String, u32>> =
    session_id.as_deref().and_then(|sid| {
        let h = session_registry.get_category_histogram(sid);
        if h.is_empty() { None } else { Some(h) }
    });
```

### Updated `ServiceSearchParams` construction

Replace the existing `let service_params = ServiceSearchParams { ... }` literal:

```
let service_params = crate::services::ServiceSearchParams {
    query: query.clone(),
    k,
    filters: None,                                                    // UDS doesn't pass metadata filters
    similarity_floor: Some(SIMILARITY_FLOOR),
    confidence_floor: Some(CONFIDENCE_FLOOR),
    feature_tag: None,
    co_access_anchors: None,
    caller_agent_id: None,
    retrieval_mode: crate::services::RetrievalMode::Strict,          // crt-010: UDS uses strict mode
    session_id: session_id.clone(),                                   // crt-026: NEW
    category_histogram,                                               // crt-026: NEW (pre-resolved above)
};
```

### Pre-resolution ordering invariant

```
handle_context_search called (sanitize_session_id already done by caller)
  → k parsing (sync)
  → AuditContext construction (sync)
  → get_category_histogram(sid) (sync: Mutex hold, no await)  ← SNAPSHOT HERE
  → ServiceSearchParams { ..., category_histogram }
  → services.search.search(service_params, ...).await          ← FIRST AWAIT
```

---

## Component 2: `handle_compact_payload` modifications

### Current structure (relevant portion, lines 1115-1240)

```
async fn handle_compact_payload(
    session_id: &str,
    role: Option<String>,
    feature: Option<String>,
    token_limit: Option<u32>,
    session_registry: &SessionRegistry,
    services: &crate::services::ServiceLayer,
) -> HookResponse {
    // 1. Byte/token budget
    // 2. Session state resolution
    //    let session_state = session_registry.get_state(session_id);
    //    let effective_role = session_state.as_ref().and_then(|s| s.role.clone()).or(role);
    //    let effective_feature = ...;
    //    let compaction_count = ...;
    // 3. Determine path
    // 4. Build AuditContext
    // 5. Build injection history
    // 6. Build BriefingParams
    // 7. Delegate to BriefingService
    // 8. Convert BriefingResult to CompactionCategories
    // 9. Format payload:
    //    let content = format_compaction_payload(&categories, role, feature, count, max_bytes);
    // 10. Increment compaction count
```

### Modification: extract category histogram after step 2

After step 2 (after `compaction_count` is resolved, before step 3 `has_injection_history`):

```
// crt-026: Extract category histogram for CompactPayload summary block (WA-2, FR-12).
// get_category_histogram returns a clone or empty map — no await needed (NFR-01, NFR-05).
let category_histogram = session_registry.get_category_histogram(session_id);
```

Note: This does NOT map to `None` here — `format_compaction_payload` receives the
`HashMap` directly and tests `is_empty()` internally.

### Modification: pass category_histogram to format_compaction_payload

Update the `format_compaction_payload` call in step 9:

```
let content = format_compaction_payload(
    &categories,
    effective_role.as_deref(),
    effective_feature.as_deref(),
    compaction_count,
    max_bytes,
    &category_histogram,    // crt-026: NEW parameter
);
```

---

## Component 3: `format_compaction_payload` modifications

### Signature change

```
fn format_compaction_payload(
    categories: &CompactionCategories,
    role: Option<&str>,
    feature: Option<&str>,
    compaction_count: u32,
    max_bytes: usize,
    category_histogram: &HashMap<String, u32>,    // crt-026: NEW parameter
) -> Option<String>
```

### Where to append the histogram block

The histogram summary block is appended AFTER the "Conventions section" (the last existing
section in the function), before the hard ceiling check. It consumes from the remaining
budget after conventions have been allocated.

Current end of function (before the hard ceiling check):
```rust
let _ = format_category_section(&mut output, "Conventions", &categories.conventions, convention_budget);
```

After that, append the histogram block:

```
// crt-026: Histogram summary block (WA-2, FR-12).
// Appended when the session histogram is non-empty.
// Format: "Recent session activity: decision × 3, pattern × 2"
// Rules: top-5 by count descending, counts > 0 only, omit when empty (R-10, AC-11).
// Fits within MAX_INJECTION_BYTES: < 100 bytes for typical sessions (< 20 categories).
if !category_histogram.is_empty() {
    // 1. Collect categories with count > 0 (all should be, but guard for safety)
    let mut entries: Vec<(&String, u32)> = category_histogram
        .iter()
        .filter(|(_, &count)| count > 0)
        .map(|(cat, &count)| (cat, count))
        .collect();

    if !entries.is_empty() {
        // 2. Sort by count descending (tiebreaking order is non-deterministic — acceptable per EC-04)
        entries.sort_by(|a, b| b.1.cmp(&a.1));

        // 3. Cap at top-5 (EC-07)
        entries.truncate(5);

        // 4. Format the line: "Recent session activity: decision × 3, pattern × 2"
        let parts: Vec<String> = entries
            .iter()
            .map(|(cat, count)| format!("{} \u{00d7} {}", cat, count))
            .collect();
        let summary_line = format!("Recent session activity: {}\n", parts.join(", "));

        // 5. Append only if within remaining budget
        let remaining = max_bytes.saturating_sub(output.len());
        if summary_line.len() <= remaining {
            output.push_str(&summary_line);
        }
        // If over budget, omit the block silently (MAX_INJECTION_BYTES is the hard limit)
    }
}
```

The `×` character is the Unicode MULTIPLICATION SIGN (U+00D7), matching the format
specified in the architecture: `"Recent session activity: decision × 3, pattern × 2"`.
Use `\u{00d7}` in the Rust string literal.

### Early-return guard: empty categories

The function currently returns `None` if all three existing category collections are empty:
```rust
if categories.decisions.is_empty()
    && categories.injections.is_empty()
    && categories.conventions.is_empty()
{
    return None;
}
```

This early return must NOT be changed to also check `category_histogram`. A session
with only a histogram summary (all categories empty in BriefingResult) would currently
return `None`, omitting the histogram. The guard must be adjusted to also allow
non-empty histogram through:

```
if categories.decisions.is_empty()
    && categories.injections.is_empty()
    && categories.conventions.is_empty()
    && category_histogram.is_empty()    // crt-026: also skip when histogram is empty
{
    return None;
}
```

This ensures: when only the histogram block has content, the function produces output.
When both categories AND histogram are empty, the function still returns `None`.

---

## Import Requirements

`HashMap<String, u32>` — `HashMap` is already in scope in `listener.rs` via
`use std::collections::HashMap`. No new imports needed.

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `category_histogram.is_empty()` in `handle_compact_payload` | Passed to `format_compaction_payload`; histogram block omitted |
| `category_histogram.is_empty()` in `format_compaction_payload` | `if !category_histogram.is_empty()` guard: block not appended |
| Summary line exceeds remaining budget | Appended only if `summary_line.len() <= remaining`; otherwise silently omitted |
| `handle_context_search`: `session_id` is `None` | `as_deref().and_then(...)` → `category_histogram = None` |
| `handle_context_search`: session unregistered | `get_category_histogram` returns empty map → `is_empty()` → `None` |

---

## Key Test Scenarios

See `test-plan/uds.md` for the full test plan. Key scenarios:

1. **AC-11 / R-10 (gate blocker)**: `format_compaction_payload` with non-empty histogram
   `{"decision": 3, "pattern": 2}` → assert output contains
   `"Recent session activity: decision × 3, pattern × 2"`.

2. **AC-11 / R-10 (gate blocker)**: `format_compaction_payload` with empty histogram →
   assert output does NOT contain `"Recent session activity"`.

3. **R-05 (gate blocker, integration test)**: Simulate UDS `HookRequest::ContextSearch`
   with a `session_id` that has a populated histogram. Verify `ServiceSearchParams.category_histogram`
   is `Some(non_empty_map)` when `SearchService::search` is invoked.

4. **R-10 top-5 cap**: Histogram with 6 categories → assert only 5 appear in output.

5. **EC-04 (tiebreaking)**: Histogram with two categories of equal count → assert both
   appear in output (order non-deterministic; test should accept either order).

6. **Empty histogram → None in `handle_context_search`**: Session registered but no stores
   → `category_histogram = None` in `ServiceSearchParams`.

7. **Pre-resolution ordering (R-13)**: Verify `get_category_histogram` call in
   `handle_context_search` appears before the first `.await` (static review check).

8. **Sanitize ordering (R-05 scenario 2)**: `sanitize_session_id` is called in the dispatch
   block (lines 796-803) before `handle_context_search` is entered — ordering is preserved
   by the existing dispatch structure; verify new code does not invert this.
