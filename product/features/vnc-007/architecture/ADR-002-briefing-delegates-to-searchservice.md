## ADR-002: BriefingService Delegates to SearchService for Semantic Search

### Context

BriefingService needs semantic search capability (embed query, HNSW search, re-rank, feature boost, co-access boost) when `include_semantic=true`. Two approaches:

1. **Delegate to SearchService**: BriefingService calls `SearchService::search()` with appropriate params. Gets the full search pipeline including security gates (S1 warn-mode scan on query, S3 validation, S4 quarantine exclusion, S5 audit).

2. **Reimplement internally**: BriefingService uses EmbedServiceHandle, AsyncVectorStore, etc. directly. More control (e.g., fixed k=3) but duplicates the pipeline and misses security gates.

The current MCP `context_briefing` uses a bespoke embed/search path (not SearchService) with k=3 hardcoded. The question is whether to preserve that bespoke path or consolidate onto SearchService.

### Decision

BriefingService delegates to SearchService for semantic search. SearchService already supports configurable `k` via `ServiceSearchParams.k`, so BriefingService sets `k: 3` in its params. Feature boost and co-access boost are already handled by SearchService.

```rust
// Inside BriefingService::assemble(), when include_semantic=true:
let search_results = self.search.search(
    ServiceSearchParams {
        query: task.clone(),
        k: 3,
        filters: None,
        similarity_floor: None,
        confidence_floor: None,
        feature_tag: params.feature.clone(),
        co_access_anchors: Some(already_collected_ids),
        caller_agent_id: Some(audit_ctx.caller_id.clone()),
    },
    audit_ctx,
).await;
```

If SearchService returns an embedding error (EmbedNotReady), BriefingService sets `search_available = false` and continues with non-semantic sources only. This matches the current graceful degradation behavior (AC-28 from vnc-003).

### Consequences

- **Easier**: No duplication of embed/search/rank/boost logic. Security gates (S1-S5) come for free.
- **Easier**: Future SearchService improvements (e.g., better ranking) automatically benefit briefing.
- **Harder**: BriefingService cannot customize the search pipeline beyond what ServiceSearchParams exposes. If briefing needs a unique ranking strategy in the future, SearchService's interface would need extension.
- **Trade-off**: The current MCP briefing does co-access boost with briefing-specific anchor selection (top-3 results as anchors). SearchService's co-access boost uses the `co_access_anchors` param, which BriefingService can populate with already-collected entry IDs (conventions, injection entries). This is a slight behavioral change — anchors now include non-search entries — but it improves co-access relevance by connecting search results to the broader briefing context.
