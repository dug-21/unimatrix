# C4: UDS Path Hardening — Pseudocode

## Location
`crates/unimatrix-server/src/uds/listener.rs`

## Changes

### Modified: `handle_context_search`

In the ServiceSearchParams construction (around line 760-769):

```
// BEFORE:
let service_params = ServiceSearchParams {
    query: query.clone(),
    k,
    filters: None,
    similarity_floor: Some(SIMILARITY_FLOOR),
    confidence_floor: Some(CONFIDENCE_FLOOR),
    feature_tag: None,
    co_access_anchors: None,
    caller_agent_id: None,
}

// AFTER:
let service_params = ServiceSearchParams {
    query: query.clone(),
    k,
    filters: None,
    similarity_floor: Some(SIMILARITY_FLOOR),
    confidence_floor: Some(CONFIDENCE_FLOOR),
    feature_tag: None,
    co_access_anchors: None,
    caller_agent_id: None,
    retrieval_mode: RetrievalMode::Strict,  // NEW: UDS uses strict mode
}
```

### BriefingService injection history filtering

In the briefing assembly path (BriefingService), injection history entries that are deprecated should be excluded before payload assembly. This is handled by the SearchService strict mode when BriefingService calls `search()` with UDS parameters.

For injection history entries fetched by ID (BriefingParams.injection_history):

```
// In BriefingService::assemble, when processing injection_history:
// Filter out deprecated entries from injection history
if let Some(ref history) = params.injection_history:
    for entry in history:
        match entry_store.get(entry.entry_id).await:
            Ok(record):
                if record.status == Status::Deprecated || record.status == Status::Quarantined:
                    continue  // Skip deprecated/quarantined from injection history (AC-11)
                // ... existing processing ...
```

### Dead code cleanup

The `[deprecated]` indicator branch in response formatting becomes dead code since deprecated entries never reach the UDS response path under strict mode. The branch can be left as defensive code (no behavior change) or removed.

## Import Added

```rust
use crate::services::search::RetrievalMode;
```

## Key Design Points

- UDS always uses Strict mode — zero tolerance for deprecated/superseded entries (FR-4.1)
- Supersession injection still runs in Strict mode: Active successors are injected (they pass the Active filter)
- Empty results return `HookResponse::Entries { items: vec![], total_tokens: 0 }` — no fallback (FR-1.5)
- BriefingService injection history filtering is a separate concern from search mode (AC-11)
