# C5: MCP Filter Asymmetry Fix — Pseudocode

## Location
`crates/unimatrix-server/src/mcp/tools.rs`

## Changes

### Modified: `context_search` tool implementation

In the ServiceSearchParams construction (around line 276-298):

```
// BEFORE:
let service_params = ServiceSearchParams {
    query: params.query.clone(),
    k,
    filters: if params.topic.is_some()
        || params.category.is_some()
        || params.tags.is_some()
    {
        Some(QueryFilter {
            topic: params.topic.clone(),
            category: params.category.clone(),
            tags: params.tags.clone(),
            status: Some(Status::Active),
            time_range: None,
        })
    } else {
        None  // BUG: unfiltered HNSW — deprecated compete equally
    },
    similarity_floor: None,
    confidence_floor: None,
    feature_tag: params.feature.clone(),
    co_access_anchors: None,
    caller_agent_id: Some(ctx.agent_id.clone()),
}

// AFTER:
let service_params = ServiceSearchParams {
    query: params.query.clone(),
    k,
    filters: if params.topic.is_some()
        || params.category.is_some()
        || params.tags.is_some()
    {
        Some(QueryFilter {
            topic: params.topic.clone(),
            category: params.category.clone(),
            tags: params.tags.clone(),
            status: Some(Status::Active),
            time_range: None,
        })
    } else {
        None  // No metadata filter — HNSW returns all entries
              // Status handling now in SearchService Step 6a (Flexible mode)
    },
    similarity_floor: None,
    confidence_floor: None,
    feature_tag: params.feature.clone(),
    co_access_anchors: None,
    caller_agent_id: Some(ctx.agent_id.clone()),
    retrieval_mode: RetrievalMode::Flexible,  // NEW: always Flexible for MCP
}
```

The filter asymmetry is fixed by RetrievalMode::Flexible — even when `filters: None` (no topic/category/tags), the SearchService now applies status penalties in Step 6a. Previously, the `filters: None` path returned raw HNSW results with no status awareness.

When `filters` includes `status: Some(Status::Active)`, the HNSW pre-filter already excludes non-Active entries. The Flexible mode penalties then have no effect (all entries are Active). This is correct — no double penalty.

## Import Added

```rust
use crate::services::search::RetrievalMode;
```

## Key Design Points

- MCP always uses Flexible mode — deprecated visible but penalized (FR-6.1)
- No new MCP tool parameters added (NFR-4.1, AC-15)
- The existing `status: Some(Status::Active)` filter on the `filters.is_some()` branch means those results are already Active-only — Flexible mode penalties are redundant but harmless
- The `filters: None` branch (no metadata filters) is where the fix matters most: HNSW returns everything, and Flexible mode now penalizes deprecated entries
- Explicit `status: Deprecated` filter scenario (AC-14, AC-14b): This path is internal to SearchService, not exposed via MCP SearchParams (which has no status field)
