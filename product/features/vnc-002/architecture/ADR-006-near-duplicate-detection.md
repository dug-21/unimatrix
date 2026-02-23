## ADR-006: Near-Duplicate Detection at 0.92 Cosine Similarity Threshold

### Context

`context_store` must detect when a new entry is semantically near-identical to an existing entry, preventing knowledge base pollution with duplicates. This requires embedding the new entry and searching existing entries for high similarity.

Key questions:
1. **Threshold value**: How similar must entries be to count as duplicates?
2. **Response on duplicate**: Error or success with duplicate indicator?
3. **Which embedding to compare**: Title+content (same as the storage embedding) or content-only?
4. **Performance**: Embedding + search adds latency to every store call

### Decision

Threshold: **0.92 cosine similarity** (via dot product on L2-normalized vectors, which is what our HNSW index uses with DistDot). This was chosen based on empirical observation that entries above 0.92 are almost always expressing the same concept with minor wording differences, while entries between 0.85-0.92 are related but distinct.

Response: **Success with duplicate indicator**, not error. The `CallToolResult` contains the existing entry's ID, similarity score, and a duplicate indicator — formatted per the requested `format` parameter (summary/markdown/json). This lets the agent decide whether to proceed differently without forcing error handling for a non-error condition.

Embedding: Use the **same title+content embedding** that would be stored for the new entry. This is the embedding already computed for vector indexing, so no additional embedding call is needed.

Flow:
1. Embed the new entry's title+content (already needed for vector indexing)
2. Search HNSW with top_k=1 and the new embedding as query
3. If top result has similarity >= 0.92: fetch the existing entry, return duplicate response
4. If top result has similarity < 0.92 (or no results): proceed with insert

### Consequences

**Easier:**
- No extra embedding computation -- reuses the embedding already needed for indexing
- Agents get actionable information about the duplicate (ID, content preview, score)
- Non-error response simplifies agent logic -- no special error handling needed
- Threshold is a single constant, easy to tune later

**Harder:**
- Every `context_store` call pays for a vector search (top_k=1, fast but nonzero)
- 0.92 may be too aggressive for short entries (short text has less embedding signal) or too lenient for long entries. Tuning may be needed after real-world usage.
- No "force store" mechanism in vnc-002 -- if the agent wants to store despite a duplicate, it must modify the content slightly. vnc-003 could add a `force: true` parameter.
- Near-duplicate detection happens only at store time, not retroactively. Existing duplicates inserted before vnc-002 are not detected.
