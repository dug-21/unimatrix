## ADR-002: ID-Based Fetch for Compaction Payload (No Embedding)

### Context

The PreCompact hook must construct a knowledge payload within a 50ms total latency budget. Three strategies were evaluated in ASS-014 research (access-pattern.md Section 6):

1. **Briefing-based** — embed a task query, HNSW search, format. ~15-20ms server-side. Requires ONNX runtime.
2. **Injection history replay** — fetch entries by ID from injection history. ~5-10ms server-side. No ONNX needed.
3. **Pre-computed snapshot** — serve a cached payload from memory. ~0ms server-side. Requires daemon or background updates.

### Decision

Use Strategy 2 (injection history replay via ID-based fetch) as the primary compaction strategy. The server fetches entries by their IDs from the session's injection history using `entry_store.get(id)`, sorts by priority (category then confidence), and formats within the token budget.

Fallback to a simplified briefing approach when no injection history exists: query entries by category ("decision", "convention") filtered by status (Active), sorted by confidence, truncated to budget. This fallback uses `entry_store.query()` — no embedding, no HNSW search.

Strategy 3 (pre-computed snapshot) is not implemented. The ID-based fetch is fast enough (~1ms per entry, ~20 entries = ~20ms total with overhead) and avoids the complexity of maintaining a materialized view that must be updated on every injection.

### Consequences

**Easier:**
- No ONNX runtime dependency at PreCompact time — the compaction handler works even if the embedding model failed to load
- Latency is predictable and bounded by the number of entries in injection history (~1ms per entry)
- Implementation is straightforward — no search pipeline duplication needed
- The fallback path (category-based query) uses existing `AsyncEntryStore::query()` API

**Harder:**
- The compaction payload can only re-inject entries that were previously injected — it cannot discover new entries added after the last injection
- If the entry store is slow (degraded disk, large database), ID-based fetch time grows linearly with entry count
- The fallback path produces less targeted results than a semantic search would (category-based vs. query-based)
