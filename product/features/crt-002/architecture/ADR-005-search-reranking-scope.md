## ADR-005: Search Re-ranking Scope (context_search Only)

### Context

crt-002 introduces confidence-based re-ranking of search results. Scope risk SR-04 identified ambiguity about which retrieval tools receive re-ranking. The human clarified (Phase 1b approval):

- `context_search`: Non-deterministic, similarity-based. Re-ranking applies.
- `context_lookup`: Deterministic metadata query. No similarity scores. Ordering unchanged.
- `context_get`: Single entry by ID. No ranking.
- `context_briefing`: Internal search component gets re-ranked (via `context_search` code path). Lookup/get paths remain deterministic.

### Decision

Re-ranking applies ONLY to `context_search` results. The formula is:

```
final_score = SEARCH_SIMILARITY_WEIGHT * similarity + (1 - SEARCH_SIMILARITY_WEIGHT) * confidence
```

Where `SEARCH_SIMILARITY_WEIGHT = 0.85`. The re-ranking step operates on the existing top-k candidates returned by the HNSW index -- it does not change the vector search itself.

For `context_briefing`, the internal search component calls the same code path as `context_search`, so re-ranking applies to that component naturally. The lookup and get components within briefing are unaffected.

### Consequences

**Easier:**
- Deterministic paths (`context_lookup`, `context_get`) remain deterministic
- Agents relying on consistent `context_lookup` ordering are not surprised
- Implementation is localized to one tool handler

**Harder:**
- `context_search` results may change ordering over time as confidence evolves (SR-09)
- Agents that depended on exact `context_search` ordering (unlikely -- semantic search is inherently approximate) may see different results
