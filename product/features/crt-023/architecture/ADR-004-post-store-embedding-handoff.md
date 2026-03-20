## ADR-004: Post-Store Embedding Handoff — Move Into Fire-and-Forget Task

### Context

`StoreService::insert` computes (or receives) a `Vec<f32>` embedding for the new entry. After a successful insert, the fire-and-forget NLI detection task needs this embedding to query HNSW for the new entry's neighbors — without recomputing it.

The ownership question in async Rust: `StoreService::insert` is an `async fn` that completes before the fire-and-forget task is spawned. The embedding must be moved from the insert path into the spawned task. The insert path uses the embedding itself (for HNSW insert in Step 5 of the insert pipeline), so the move must happen after HNSW insertion.

**Insert pipeline order (from `store_ops.rs`):**
1. Gateway validate
2. Compute embedding (if not pre-supplied) → `Vec<f32>`
3. HNSW neighbor search (duplicate check, uses `embedding.clone()`)
4. SQL insert
5. HNSW insert (`vector_index.insert_hnsw_only(entry_id, data_id, &embedding)`)
6. Adaptation prototype update (uses adapted embedding, not raw)

After Step 5, the raw embedding is owned by `StoreService::insert` and is no longer needed by the insert pipeline itself. This is the hand-off point.

**OQ-02 scenarios:**

- **Happy path**: embedding was computed in Step 2 (or pre-supplied). After Step 5, the insert pipeline owns `embedding: Vec<f32>`. It can be moved into the spawned tokio task.
- **Duplicate detected (returns early)**: Step 3 detects a near-duplicate and returns early. No insert occurs, no NLI task should be spawned. The embedding is discarded. This is correct: the neighbor detection task is only meaningful for genuinely new entries.
- **Embedding failed**: If Step 2 fails (embed service not ready), `StoreService::insert` returns `Err(ServiceError::EmbeddingFailed(...))`. No insert occurs, no NLI task is spawned. This is correct.
- **HNSW insert fails**: Step 5 fails. The SQL entry exists but has no HNSW node. Spawning NLI detection would query for neighbors but find none (the entry is not in the HNSW graph yet). This is the most subtle case: the NLI task would silently find 0 neighbors and write no edges. This is acceptable — the NLI detection degrades gracefully.
- **NLI not ready**: `NliServiceHandle::get_provider()` returns `Err`. The fire-and-forget task exits immediately after checking. The embedding was moved for nothing, but this is a `Vec<f32>` clone (~1.5KB for 384-dim), not a meaningful cost.

**Ownership contract:**

```rust
// In StoreService::insert, after Step 5 (HNSW insert):
let insert_result = InsertResult {
    entry: record.clone(),
    duplicate_of: None,
    duplicate_similarity: None,
};

// Fire-and-forget: move embedding and entry_id into the task.
// NliServiceHandle, store, and vector_index are cloned from Arc.
if nli_is_configured {
    let embedding_for_nli = embedding; // move: insert pipeline is done with it
    let entry_id_for_nli = entry_id;
    let nli_handle = Arc::clone(&self.nli_handle);
    let store = Arc::clone(&self.store);
    let vector_index = Arc::clone(&self.vector_index);
    let nli_post_store_k = self.config.nli_post_store_k;
    let nli_entailment_threshold = self.config.nli_entailment_threshold;
    let nli_contradiction_threshold = self.config.nli_contradiction_threshold;
    let max_contradicts_per_tick = self.config.max_contradicts_per_tick;
    let rayon_pool = Arc::clone(&self.rayon_pool);

    tokio::spawn(async move {
        run_post_store_nli(
            embedding_for_nli,
            entry_id_for_nli,
            nli_handle,
            store,
            vector_index,
            rayon_pool,
            nli_post_store_k,
            nli_entailment_threshold,
            nli_contradiction_threshold,
            max_contradicts_per_tick,
        )
        .await;
    });
}

Ok(insert_result)
```

The key insight: `embedding` is moved into `embedding_for_nli` at the hand-off point. The `InsertResult` is constructed with the already-computed `record`; the embedding is not in `InsertResult` and does not need to be cloned for the return. The move is a rename, not a copy.

**Adapted vs raw embedding for HNSW query:**

`StoreService::insert` may produce an adapted (L2-normalized, category-projected) embedding in `normalized` for the HNSW insert. The post-store NLI task needs to query HNSW with the same embedding that was inserted. The `embedding` variable at the hand-off point is the adapted, normalized embedding — the same vector that was passed to `vector_index.insert_hnsw_only(...)`. Using this vector for HNSW neighbor search in the NLI task is correct: it searches the same embedding space.

**`nli_is_configured` guard:**

The fire-and-forget spawn is gated on `self.config.nli_enabled && self.nli_handle.is_ready_or_loading()`. If `nli_enabled = false`, no task is spawned and the embedding is simply dropped. If NLI is loading (not yet ready), the task is spawned anyway — it will check `get_provider()` inside `run_post_store_nli` and exit if NLI is not yet ready. This is acceptable; the task cost is a single async check and a channel message.

### Decision

**Move the embedding `Vec<f32>` into the fire-and-forget task immediately after the HNSW insert step.** The insert pipeline does not retain a reference to the embedding after this point. The task receives ownership of the vector; no clone is performed.

**The `run_post_store_nli` function is a standalone async function (not a method)** taking all required state as parameters. It checks `nli_handle.get_provider()` as its first step and returns immediately (logging `tracing::debug!`) if NLI is not ready. This keeps `StoreService` clean of inline NLI logic and makes the task independently testable.

**No embedding is stored in `InsertResult`.** The NLI task is fire-and-forget with no result returned to the MCP handler. The MCP response does not include NLI output.

**If embedding computation failed during insert, no NLI task is spawned.** The guard `if nli_is_configured` only applies when insert succeeded with a valid embedding. An `Err` return from `StoreService::insert` means the code never reaches the hand-off point.

**If duplicate detected, no NLI task is spawned.** The early return at the duplicate check point precedes the hand-off. Correct: the "new entry" does not actually exist; no edges should be written.

### Consequences

**Easier:**
- Zero-copy handoff: the embedding Vec is moved once, not cloned.
- Fire-and-forget task is self-contained: all needed state is passed as owned/Arced values.
- The task can be tested in isolation without going through the full insert pipeline.
- Insert latency is not affected: the `tokio::spawn` call is non-blocking.

**Harder:**
- `StoreService` gains a new field `nli_handle: Arc<NliServiceHandle>` and config fields for NLI thresholds. Its constructor and all test fixtures that construct `StoreService` must be updated.
- The hand-off point (after HNSW insert, before return) must be kept in sync if the insert pipeline order changes. A comment in `store_ops.rs` must note the NLI hand-off dependency.
- If HNSW insert fails but SQL insert succeeded, the NLI task runs but finds 0 neighbors. This is the correct silent-degradation behavior but may be surprising in tests. Test documentation should note this edge case.
