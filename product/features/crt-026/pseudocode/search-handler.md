# crt-026: Component — `context_search` handler pre-resolution (`mcp/tools.rs`)

File: `crates/unimatrix-server/src/mcp/tools.rs`
Wave: 2 (depends on Wave 1: `session.rs` must have `get_category_histogram`; `search-params.md` must be implemented)

---

## Purpose

Pre-resolve the session category histogram in the `context_search` MCP handler before
constructing `ServiceSearchParams`, following the crt-025 SR-07 snapshot pattern. Thread
the pre-resolved histogram and `session_id` through `ServiceSearchParams` into
`SearchService::search`.

---

## Current State (Relevant Context)

The `context_search` handler currently has this structure (lines 284-360):

```
// 1. Identity + format + audit context → ctx
// 2. Capability check
// 3. Input validation (validate_search_params, validate_feature, validate_helpful)
// 3. Parse k
// 4. Build ServiceSearchParams and delegate to SearchService
//    let service_params = ServiceSearchParams { query, k, filters, ..., retrieval_mode }
//    → services.search.search(service_params, ...).await
// 5. Format response
// 6. Usage recording (fire-and-forget)
// 7. Query log recording (fire-and-forget)
```

The `ServiceSearchParams` construction (step 4) is currently the first `await` point in
the function (the `services.search.search(...).await`). The pre-resolution must be placed
before the `ServiceSearchParams { ... }` literal, before that `await`.

`ctx.audit_ctx.session_id` has type `Option<String>`. It is populated by `build_context`
(step 1), which reads from `params.session_id` (the MCP parameter field).

The `session_registry` is accessible as `self.session_registry`.

---

## Modification to `context_search` handler

### Step 4a: Pre-resolution block (INSERT before step 4)

Add after the `let k = validated_k(...)` line, before the `let service_params = ServiceSearchParams { ... }`:

```
// crt-026: Pre-resolve session histogram for histogram affinity boost (WA-2, ADR-002).
// Follows the crt-025 SR-07 snapshot pattern: session state is read once synchronously
// before any await point. This eliminates races with concurrent context_store calls
// in the same session (R-13).
//
// Maps is_empty() → None: the scoring loop treats None as cold-start (NFR-02).
// The None case covers: session_id absent, session unregistered, no prior stores.
let category_histogram: Option<HashMap<String, u32>> =
    ctx.audit_ctx.session_id.as_deref().and_then(|sid| {
        let h = self.session_registry.get_category_histogram(sid);
        if h.is_empty() { None } else { Some(h) }
    });
```

### Step 4b: Updated `ServiceSearchParams` construction

Replace the existing `ServiceSearchParams { ... }` literal with the extended version:

```
let service_params = ServiceSearchParams {
    query: params.query.clone(),
    k,
    filters: if params.topic.is_some() || params.category.is_some() || params.tags.is_some()
    {
        Some(QueryFilter {
            topic: params.topic.clone(),
            category: params.category.clone(),
            tags: params.tags.clone(),
            status: Some(Status::Active),
            time_range: None,
        })
    } else {
        None
    },
    similarity_floor: None,
    confidence_floor: None,
    feature_tag: params.feature.clone(),
    co_access_anchors: None,
    caller_agent_id: Some(ctx.agent_id.clone()),
    retrieval_mode: crate::services::RetrievalMode::Flexible,
    session_id: ctx.audit_ctx.session_id.clone(),           // crt-026: NEW
    category_histogram,                                      // crt-026: NEW (pre-resolved above)
};
```

---

## Pre-resolution Ordering Invariant

The `get_category_histogram` call must occur BEFORE the first `await` point in the function.
The first `await` is `services.search.search(service_params, ...).await`.

Correct ordering:
1. `build_context(...)` — contains an `.await` but this is in step 1, before session reads
   NOTE: `build_context` is an await but it does NOT read from `session_registry.category_counts`.
   The histogram snapshot must happen after `build_context` completes (so `ctx` is available)
   and before `services.search.search(...).await`.
2. `validated_k(params.k)` — synchronous
3. `get_category_histogram(sid)` — synchronous (Mutex, no await) ← PRE-RESOLUTION HERE
4. `ServiceSearchParams { ... }` — synchronous struct construction
5. `services.search.search(service_params, ...).await` ← first service await

This ordering is correct. The histogram snapshot is taken at step 3, after `ctx` provides
the `session_id`, and before the search service is invoked.

---

## Import Requirements

`HashMap` is already imported via `use std::collections::HashMap` in `mcp/tools.rs`.
No new imports needed for the pre-resolution block.

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `ctx.audit_ctx.session_id` is `None` | `and_then` on `None` → `category_histogram = None` |
| `session_id` is Some but session unregistered | `get_category_histogram` returns empty map → `is_empty()` → `None` |
| Session registered, no prior stores | `get_category_histogram` returns empty map → `None` |
| Session registered, has stores | `get_category_histogram` returns non-empty → `Some(histogram)` |
| `get_category_histogram` Mutex poison | Recovered in `session.rs`; returns empty map |

---

## Key Test Scenarios

See `test-plan/search-handler.md` for the full test plan. Key scenarios:

1. **AC-05**: Search with a session that has prior `context_store` calls. Assert
   `ServiceSearchParams.category_histogram` is `Some(non_empty_map)`.

2. **AC-05**: Search with `session_id = None`. Assert
   `ServiceSearchParams.category_histogram = None`.

3. **AC-08 / R-02 (gate blocker, end-to-end path)**: Search with no prior stores.
   Assert result order is identical to a call with no `session_id`.

4. **R-13 (ordering audit)**: Assert the `get_category_histogram` call appears before the
   first `.await` in `context_search`. This is a static code review check.

5. **Empty → None mapping**: Call `get_category_histogram` when session has zero stores.
   Assert `category_histogram = None` in the constructed `ServiceSearchParams`.
