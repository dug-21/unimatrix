# Post-Store NLI Detection — Pseudocode

**Files**:
- `crates/unimatrix-server/src/services/nli_detection.rs` (new — contains `run_post_store_nli`)
- `crates/unimatrix-server/src/services/store_ops.rs` (modified — fire-and-forget spawn after HNSW insert)
- `crates/unimatrix-server/src/services/mod.rs` (modified — expose `nli_detection` module)

**Purpose**: After a successful `context_store` inserts a new entry into HNSW, a fire-and-forget
tokio task scores `(new_entry_text, neighbor_text)` pairs and writes `Contradicts`/`Supports`
edges to `GRAPH_EDGES` with `source='nli'`. Uses the embedding already computed during insert
(zero-copy move, ADR-004). Does not block the MCP response.

**Critical constraint (W1-2)**: The `score_batch` call runs via `rayon_pool.spawn()` (no
timeout — this is a background task). Never inline in async context. Never via spawn_blocking.

---

## `StoreService` Extension

### Struct changes

Add two fields to `StoreService`:

```
// Add to StoreService struct:

/// crt-023: NLI handle for post-store edge detection (ADR-004).
pub(crate) nli_handle: Arc<NliServiceHandle>,

/// crt-023: NLI inference config fields (snapshot at construction time).
/// Separate struct to avoid passing full InferenceConfig through service layer.
pub(crate) nli_cfg: NliStoreConfig,
```

```
/// NLI-related config fields needed by StoreService (snapshot at startup).
/// Allows StoreService to be cloned without copying the full InferenceConfig.
pub(crate) struct NliStoreConfig {
    pub enabled:                  bool,
    pub nli_post_store_k:         usize,
    pub nli_entailment_threshold: f32,
    pub nli_contradiction_threshold: f32,
    pub max_contradicts_per_tick: usize, // per-call cap (FR-22, AC-23)
}
```

### `StoreService::new` Extension

Add `nli_handle` and `nli_cfg` parameters to the existing `new` function.

### Fire-and-forget spawn after HNSW insert

Modify `StoreService::insert` after the HNSW insert step (Step 5 in existing code):

```
// Existing Step 5: HNSW insert
if !embedding.is_empty() {
    self.vector_index.insert_hnsw_only(entry_id, data_id, &embedding)
        // ... existing error handling (non-fatal, logs warn) ...
}

// crt-023: NLI post-store detection hand-off (ADR-004).
// INVARIANT: embedding is moved here. Do not reference `embedding` after this block.
// This is the only consumer of the raw Vec<f32> after HNSW insert.
// COMMENT REQUIRED IN CODE: "NLI hand-off point: embedding moves into fire-and-forget task.
//  Step ordering must not change without reviewing ADR-004."
if self.nli_cfg.enabled && self.nli_handle.is_ready_or_loading() {
    let embedding_for_nli = embedding   // move: insert pipeline is done with this Vec<f32>
    let entry_text_for_nli = record.content.clone()   // clone: record is used for return value

    let nli_handle  = Arc::clone(&self.nli_handle)
    let store       = Arc::clone(&self.store)
    let vector_index = Arc::clone(&self.vector_index)
    let rayon_pool  = Arc::clone(&self.rayon_pool)
    let nli_cfg     = self.nli_cfg.clone()

    tokio::spawn(async move {
        run_post_store_nli(
            embedding_for_nli,
            entry_id,
            entry_text_for_nli,
            nli_handle,
            store,
            vector_index,
            rayon_pool,
            nli_cfg.nli_post_store_k,
            nli_cfg.nli_entailment_threshold,
            nli_cfg.nli_contradiction_threshold,
            nli_cfg.max_contradicts_per_tick,
        ).await
    });
    // tokio::spawn returns immediately — does not block the MCP response (NFR-02)
}
// Return MCP response
Ok(InsertResult { entry: record, duplicate_of: None, duplicate_similarity: None })
```

**Edge cases for the spawn guard** (ADR-004):
- `nli_enabled = false`: spawn guard fails (`nli_cfg.enabled = false`); no task spawned.
- Duplicate detected (early return before HNSW): code never reaches this block.
- Embedding failed during Step 2: `StoreService::insert` returns `Err` before reaching here.
- HNSW insert failed: task is still spawned; `run_post_store_nli` will find 0 HNSW neighbors,
  write no edges, and return silently (R-08 — intentional silent degradation).
- `embedding.is_empty()` (no embedding computed): guard `!embedding.is_empty()` prevents spawn.

---

## `run_post_store_nli` (in nli_detection.rs)

```
/// Fire-and-forget async function. Spawned via tokio::spawn after context_store HNSW insert.
/// No timeout — this is a background task. MCP response is already returned.
///
/// W1-2 contract: score_batch runs via rayon_pool.spawn() (no timeout).
/// Never inline in async context. Never via spawn_blocking.
///
/// NOTE: max_edges_per_call is named "max_contradicts_per_tick" in config for compatibility
/// (FR-22, AC-23). Its semantic unit is per context_store call (not per background tick).
pub async fn run_post_store_nli(
    embedding: Vec<f32>,
    new_entry_id: u64,
    new_entry_text: String,
    nli_handle: Arc<NliServiceHandle>,
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    rayon_pool: Arc<RayonPool>,
    nli_post_store_k: usize,
    nli_entailment_threshold: f32,
    nli_contradiction_threshold: f32,
    max_edges_per_call: usize,  // config: max_contradicts_per_tick
)
    // Step 1: Get NLI provider. Exit immediately if not ready (no blocking wait).
    let provider = match nli_handle.get_provider().await:
        Ok(p)  -> p
        Err(_) ->
            tracing::debug!("post-store NLI skipped: NLI not ready for entry {}", new_entry_id)
            return

    // Step 2: Check embedding is non-empty (defensive guard, R-07)
    if embedding.is_empty():
        tracing::warn!("post-store NLI skipped: empty embedding for entry {}", new_entry_id)
        return

    // Step 3: Query HNSW for nearest neighbors (async, on tokio thread — this is DB/index I/O)
    // EF_SEARCH matches the constant used in SearchService.
    const EF_SEARCH: usize = 32
    let neighbor_results = match vector_index.search_hnsw(&embedding, nli_post_store_k, EF_SEARCH).await:
        Ok(results) -> results
        Err(e)      ->
            tracing::warn!(entry_id=new_entry_id, error=%e, "post-store NLI: HNSW search failed")
            return

    // Filter out the new entry itself from neighbors (it may appear if HNSW indexed immediately)
    let neighbor_ids: Vec<u64> = neighbor_results.iter()
        .filter(|r| r.entry_id != new_entry_id)
        .map(|r| r.entry_id)
        .collect()

    if neighbor_ids.is_empty():
        tracing::debug!("post-store NLI: no neighbors found for entry {}", new_entry_id)
        return

    // Step 4: Fetch neighbor entry texts (async DB reads, on tokio thread)
    let mut neighbor_texts: Vec<(u64, String)> = Vec::with_capacity(neighbor_ids.len())
    for id in &neighbor_ids:
        match store.get(*id).await:
            Ok(entry)  -> neighbor_texts.push((*id, entry.content))
            Err(e)     -> tracing::debug!(id=id, error=%e, "post-store NLI: failed to fetch neighbor")
            // Skip unreachable neighbors; continue with the rest

    if neighbor_texts.is_empty():
        return

    // Step 5: Build pairs for batch scoring.
    // new_entry_text is the "premise"; neighbor text is the "hypothesis" (SNLI convention).
    let pairs_owned: Vec<(String, String)> = neighbor_texts.iter()
        .map(|(_, text)| (new_entry_text.clone(), text.clone()))
        .collect()

    // Step 6: Dispatch to rayon pool (W1-2 contract — no timeout for background task).
    let provider_clone = Arc::clone(&provider)
    let nli_result = rayon_pool.spawn(move || {
        let pairs: Vec<(&str, &str)> = pairs_owned.iter()
            .map(|(q, p)| (q.as_str(), p.as_str()))
            .collect()
        provider_clone.score_batch(&pairs)
    }).await

    let nli_scores = match nli_result:
        Ok(Ok(scores)) -> scores
        Ok(Err(e))     ->
            tracing::warn!(entry_id=new_entry_id, error=%e, "post-store NLI: score_batch failed")
            return   // FR-21: do not propagate; log at warn and exit
        Err(rayon_err) ->
            // RayonError::Cancelled = rayon panic (session poisoned or other panic)
            tracing::warn!(entry_id=new_entry_id, error=%rayon_err, "post-store NLI: rayon task cancelled (panic?)")
            return   // FR-21: clean exit on rayon panic

    // Step 7: Write edges. Cap at max_edges_per_call (FR-22, R-09, AC-13).
    // Cap counts BOTH Supports AND Contradicts edges combined (not just Contradicts).
    // NOTE: This is named max_contradicts_per_tick in config for compatibility; semantic is per-call.
    let mut edges_written: usize = 0
    let now = current_timestamp_secs()

    for (idx, (neighbor_id, _)) in neighbor_texts.iter().enumerate():
        if edges_written >= max_edges_per_call:
            let remaining = neighbor_texts.len() - idx
            tracing::debug!(
                entry_id=new_entry_id,
                cap=max_edges_per_call,
                dropped=remaining,
                "post-store NLI: edge cap reached; dropping remaining pairs"
            )
            break

        if idx >= nli_scores.len():
            break  // defensive: score count mismatch (should not happen)

        let scores = &nli_scores[idx]
        let metadata = format_nli_metadata(scores)

        // Write Supports edge if entailment exceeds threshold (strict >)
        if scores.entailment > nli_entailment_threshold:
            let wrote = write_nli_edge(
                &store, new_entry_id, *neighbor_id, "Supports",
                scores.entailment, now, &metadata
            ).await
            if wrote { edges_written += 1 }
            if edges_written >= max_edges_per_call: continue  // recheck after writing

        // Write Contradicts edge if contradiction exceeds threshold (strict >)
        if scores.contradiction > nli_contradiction_threshold:
            let wrote = write_nli_edge(
                &store, new_entry_id, *neighbor_id, "Contradicts",
                scores.contradiction, now, &metadata
            ).await
            if wrote { edges_written += 1 }

    tracing::debug!(
        entry_id=new_entry_id,
        edges_written=edges_written,
        neighbors_scored=nli_scores.len(),
        "post-store NLI detection complete"
    )
```

---

## `write_nli_edge` (private helper in nli_detection.rs)

```
/// Write a single NLI-confirmed graph edge via write_pool_server() (SR-02).
/// Uses INSERT OR IGNORE for idempotency on UNIQUE(source_id, target_id, relation_type).
/// Returns true if the insert succeeded (edge written or already existed).
async fn write_nli_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,   // "Supports" or "Contradicts"
    weight: f32,
    created_at: u64,
    metadata: &str,        // JSON string: '{"nli_entailment": f32, "nli_contradiction": f32}'
) -> bool
    // SQL (from ARCHITECTURE.md integration surface):
    // INSERT OR IGNORE INTO graph_edges
    //     (source_id, target_id, relation_type, weight, created_at, created_by,
    //      source, bootstrap_only, metadata)
    // VALUES (?1, ?2, ?3, ?4, ?5, 'nli', 'nli', 0, ?6)

    let result = store.write_pool_server().execute(
        "INSERT OR IGNORE INTO graph_edges \
         (source_id, target_id, relation_type, weight, created_at, created_by, \
          source, bootstrap_only, metadata) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'nli', 'nli', 0, ?6)",
        params![source_id, target_id, relation_type, weight as f64, created_at, metadata]
    ).await

    match result:
        Ok(_)  -> true
        Err(e) ->
            // R-16: write pool contention or SQLite busy; log at warn, do NOT propagate
            tracing::warn!(
                source_id=source_id, target_id=target_id, relation_type=relation_type,
                error=%e, "post-store NLI: failed to write graph edge"
            )
            false
```

---

## `format_nli_metadata` (private helper in nli_detection.rs)

```
/// Serialize NLI scores to the required GRAPH_EDGES metadata JSON format.
/// Uses serde_json::to_string to prevent SQL injection via string concatenation (security note).
fn format_nli_metadata(scores: &NliScores) -> String
    // Output: '{"nli_entailment": 0.85, "nli_contradiction": 0.07}'
    // Required fields per AC-11: nli_entailment and nli_contradiction (f32).
    serde_json::json!({
        "nli_entailment":    scores.entailment,
        "nli_contradiction": scores.contradiction,
    }).to_string()
    // Note: serde_json::to_string is used, NOT string concatenation (RISK-TEST-STRATEGY security).
```

---

## services/mod.rs Extension

```
// Add to services/mod.rs:
pub(crate) mod nli_detection;
```

---

## Error Handling Summary

| Failure | Log Level | Behavior |
|---------|-----------|----------|
| `get_provider()` → Err | debug | Return; no edges written |
| Empty embedding | warn | Return; no edges written |
| HNSW search fails | warn | Return; no edges written |
| Neighbor fetch fails (one entry) | debug | Skip that neighbor; continue |
| `score_batch` → Err | warn | Return; no edges written for this store call |
| Rayon task cancelled (panic) | warn | Return; NliProvider may be poisoned; next `get_provider()` call on NliServiceHandle detects poison (R-13) |
| `write_pool_server()` fails | warn | Log and continue; edge dropped; other edges may still write |
| Edge cap reached | debug | Stop writing; log dropped count |

**FR-21**: Rayon panics must not propagate to the tokio runtime. `RayonError::Cancelled`
is caught and logged; the task exits cleanly.

---

## Key Test Scenarios

1. **AC-10 / Contradicts edge written**: Store entry with contradictory content; mock provider returns `contradiction=0.9`; assert `GRAPH_EDGES` has row with `source='nli'`, `bootstrap_only=0`, `relation_type='Contradicts'`.
2. **AC-10 / Supports edge written**: Store entry with entailing content; mock provider returns `entailment=0.9`; assert `GRAPH_EDGES` has `Supports` edge.
3. **AC-11 / metadata format**: Assert `metadata` column contains valid JSON with keys `nli_entailment` and `nli_contradiction`.
4. **AC-13 / circuit breaker**: Set `max_contradicts_per_tick = 2`; mock 5 neighbors all scoring above both thresholds; assert exactly 2 edges written (R-09: cap is total, not per type).
5. **R-09 / cap counts both types**: Arrange 3 Supports + 3 Contradicts candidates, cap=3; assert 3 total edges (not 6).
6. **R-07 / embedding hand-off**: Integration test; await fire-and-forget task (via polling GRAPH_EDGES); assert NLI was called with non-empty embedding.
7. **R-07 / empty embedding guard**: `run_post_store_nli` with empty `Vec<f32>` returns without calling provider.
8. **R-08 / HNSW fail degradation**: Mock HNSW returning error; task exits without error propagation; `context_store` MCP response is unaffected.
9. **FR-21 / rayon panic**: Mock provider that panics inside score_batch; assert MCP handler is unaffected; assert task exits cleanly (no panic propagation to tokio).
10. **Duplicate detection guard**: When `context_store` returns a duplicate result (early return), assert NO tokio::spawn was called.
11. **R-16 / write pool contention**: 5 concurrent context_store calls; assert all expected edges eventually written (no silent drop due to SQLite busy).
