## ADR-005: Briefing Graceful Degradation on Embed Not Ready

### Context

`context_briefing` has three components: convention lookup, duties lookup, and semantic search for task-relevant context. The first two use deterministic queries (no embeddings). The third requires the embedding model to be loaded.

During server startup, the embedding model loads lazily (EmbedServiceHandle state machine). If `context_briefing` is called before the model is ready, the semantic search component cannot execute.

### Decision

When the embedding model is not ready (Loading or Failed state), `context_briefing` returns a briefing with only the lookup components (conventions + duties) and omits the "Relevant Context" section. It does not return an error.

The response includes an indication that semantic search was unavailable (e.g., "Semantic search unavailable -- embedding model is loading" in markdown, `"search_available": false` in JSON).

This is different from `context_correct` and `context_store`, which require embeddings and fail with EmbedNotReady. The distinction: mutations MUST embed for near-duplicate detection and vector indexing, while `context_briefing` is a composite read that degrades gracefully.

### Consequences

**Easier:**
- Briefings are available immediately on server startup
- Agents get orientation (conventions + duties) even before embeddings are ready
- No retry logic needed on the agent side

**Harder:**
- Briefings during startup are less complete (missing task-relevant context)
- Agents may not realize they received a partial briefing (mitigated: response includes indicator)
