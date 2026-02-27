## ADR-004: Graph Compaction Atomicity

### Context

hnsw_rs does not support individual point deletion. When entries are re-embedded (via `context_correct` or embedding consistency checks), a new HNSW point is added but the old point remains as a stale routing node. Over time, stale nodes accumulate and degrade search quality. The only way to eliminate stale nodes is to rebuild the entire HNSW graph from scratch.

SR-03 identified the critical risk: if compaction mutates the existing index in place and fails midway (OOM, embed service timeout, panic in hnsw_rs), the old index is destroyed and search is broken until server restart.

Two design approaches were considered:

1. **In-place rebuild**: Drain the existing HNSW graph, rebuild. Fast (no allocation of a second graph), but catastrophic on failure.
2. **Build-new-then-swap**: Allocate a fresh HNSW graph, populate it completely, then atomically swap the old graph for the new one. Uses 2x memory during compaction, but the old graph is untouched until the swap succeeds.

SCOPE open question #2 asks whether compaction should re-embed from content (using the embed service) or read raw embeddings from the HNSW index. The hnsw_rs `Hnsw` struct does not expose a public API for retrieving raw vectors by data_id. The only reliable source of current embeddings is re-embedding from the entry's title + content via the embed service.

### Decision

Use **build-new-then-swap** with **re-embedding from content**.

The `VectorIndex::compact` method:

1. **Receives** `embeddings: Vec<(u64, Vec<f32>)>` -- pre-computed (entry_id, embedding) pairs from the caller. The caller (server crate) obtains these by reading active entries and calling `embed_service.embed_entries(...)`. This keeps VectorIndex independent of the embed service.

2. **Builds new graph**: Creates a fresh `Hnsw<'static, f32, DistDot>` with the same configuration (max_nb_connection, max_elements, max_layer, ef_construction). Inserts all provided embeddings, generating sequential data_ids starting from 0.

3. **Builds new IdMap**: Constructs a fresh `IdMap` from the insertions.

4. **Atomic swap**: Acquires write locks on `self.hnsw` and `self.id_map` simultaneously. Replaces both with the new versions. Resets `self.next_data_id` to the count of inserted embeddings.

5. **Updates VECTOR_MAP**: Writes all new `(entry_id, data_id)` mappings to the store in a single write transaction.

6. **Cleanup**: The old `Hnsw` and `IdMap` are dropped when the replaced values go out of scope.

**Failure handling at each step:**

- Step 2 (build) fails: Method returns error. Old graph untouched. No side effects.
- Step 4 (swap) fails (poisoned lock): Method panics propagation. Old graph is in an undefined state. This is a bug, not a runtime condition.
- Step 5 (VECTOR_MAP) fails: This is the critical window. The in-memory graph is already swapped, but VECTOR_MAP is stale. To handle this: the method performs VECTOR_MAP update *before* the in-memory swap. If VECTOR_MAP update fails, the method returns an error with no in-memory changes. If VECTOR_MAP succeeds, the in-memory swap proceeds (which cannot fail under non-poisoned locks).

**Revised sequence** (VECTOR_MAP first, then swap):

1. Build new HNSW graph and new IdMap (entirely in local variables).
2. Write all new `(entry_id, data_id)` mappings to VECTOR_MAP in a single write transaction.
3. If VECTOR_MAP write fails: return error, drop new graph, old graph untouched.
4. If VECTOR_MAP write succeeds: acquire write locks, swap in-memory graph and IdMap, reset next_data_id.

This ordering ensures that at no point is the in-memory state and VECTOR_MAP inconsistent. The worst case is a crash between steps 2 and 4: VECTOR_MAP has new data_ids but the in-memory graph still has the old ones. On restart, the persistence module rebuilds from VECTOR_MAP, which has the correct new mappings but no corresponding HNSW points. This is equivalent to an empty index that needs to be re-populated. The server already handles this gracefully (search returns empty results until entries are re-embedded).

**Embed service dependency**: The caller (context_status handler) is responsible for checking embed service availability before calling compact. If the embed service is not available, compaction is skipped entirely. The compact method itself does not interact with the embed service.

### Consequences

**Easier:**
- No risk of corrupting the active HNSW index during compaction
- The old graph remains functional for search during the entire build phase
- Re-embedding from content uses the current model, so embeddings are up-to-date
- VECTOR_MAP-first ordering eliminates the inconsistency window between in-memory and persistent state
- VectorIndex remains independent of the embed service (separation of concerns)

**Harder:**
- 2x memory during compaction (two HNSW graphs in memory simultaneously). At current scale (<1000 entries, ~384-dim f32 vectors), this is ~3 MB extra. Acceptable.
- Re-embedding all active entries is O(n) embed service calls. At current scale (<1000 entries), this is seconds. At 10K entries, this could be 5-10 seconds, blocking the `context_status` response. Mitigated by the stale ratio threshold (compaction only triggers when >10% stale).
- If a crash occurs between VECTOR_MAP write and in-memory swap (step 2 and step 4), the on-disk VECTOR_MAP has new data_ids that do not correspond to the old HNSW graph's data_ids. On restart, the persistence module will fail to find HNSW points for the new data_ids. The server handles this by returning empty search results until entries are re-embedded. This is a narrow crash window with graceful degradation.
