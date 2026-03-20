# crt-023 Pseudocode Overview: NLI + Cross-Encoder Re-ranking

## Feature Goal

Add an ONNX NLI cross-encoder to Unimatrix that scores `(query, passage)` pairs into
`{entailment, neutral, contradiction}` probabilities and uses those scores to: (1) re-rank
HNSW search candidates for `context_search`, replacing `rerank_score` when NLI is active; and
(2) write `Contradicts`/`Supports` edges to `GRAPH_EDGES` after each `context_store`.

NLI must never block server startup. Absence, hash mismatch, or load failure degrades gracefully
to the existing cosine + `rerank_score` path. All NLI inference runs on the shared rayon pool
(W1-2 contract — no tokio thread or spawn_blocking for inference).

---

## Component Map

| Component | File(s) | Build Phase |
|-----------|---------|-------------|
| NliProvider | `unimatrix-embed/src/cross_encoder.rs`, `model.rs` (extended), `download.rs` (extended) | Phase 1 |
| NliServiceHandle | `unimatrix-server/src/infra/nli_handle.rs` | Phase 1 |
| Config Extension | `infra/config.rs` | Phase 1 (prerequisite for all others) |
| Search Re-ranking | `services/search.rs` | Phase 2 |
| Post-Store NLI Detection | `services/nli_detection.rs` | Phase 2 |
| Bootstrap Edge Promotion | `services/nli_detection.rs` | Phase 2 |
| Auto-Quarantine Threshold | `background.rs` (background_tick auto-quarantine path) | Phase 2 |
| Eval Integration | `services/eval.rs` (stub fill-in) | Phase 3 |
| Model Download CLI | CLI model-download subcommand | Phase 3 |

**Sequencing constraint**: Config Extension must be implemented first. NliProvider and
NliServiceHandle may be built in parallel after Config. Search Re-ranking and Post-Store
Detection require both NliProvider and NliServiceHandle to be complete.

---

## Shared Types

All types in `unimatrix-embed/src/cross_encoder.rs` unless noted.

### `NliScores` (new struct)

```
struct NliScores {
    entailment:    f32,   // P(premise entails hypothesis); sort key for search re-ranking
    neutral:       f32,   // P(premise and hypothesis are unrelated)
    contradiction: f32,   // P(premise contradicts hypothesis); edge creation key
    // INVARIANT: entailment + neutral + contradiction ≈ 1.0 (within 1e-4)
    // Produced by softmax over 3-element ONNX logit output
}
```

### `CrossEncoderProvider` trait (new, `Send + Sync`)

```
trait CrossEncoderProvider: Send + Sync {
    fn score_pair(&self, query: &str, passage: &str) -> Result<NliScores>
    fn score_batch(&self, pairs: &[(&str, &str)]) -> Result<Vec<NliScores>>
    fn name(&self) -> &str
    // Both scoring methods are SYNCHRONOUS — called from rayon threads, not async
}
```

### `NliModel` enum (added to `unimatrix-embed/src/model.rs`)

```
enum NliModel {
    NliMiniLM2L6H768,   // "cross-encoder/nli-MiniLM2-L6-H768", ~85MB, primary
    NliDebertaV3Small,  // "cross-encoder/nli-deberta-v3-small", ~180MB, ONNX TBD
}
// from_config_name("minilm2") -> Some(NliMiniLM2L6H768)
// from_config_name("deberta") -> Some(NliDebertaV3Small)
// from_config_name(other) -> None (startup abort via validate())
```

### New `ServerError` variants (in `error.rs`)

```
ServerError::NliNotReady          // NliServiceHandle is Loading or Retrying
ServerError::NliFailed(String)    // NliServiceHandle exhausted retries or poisoned
// NliNotReady maps to ERROR_EMBED_NOT_READY (-32004) for callers
// NliFailed maps to ERROR_EMBED_NOT_READY (-32004) for callers
```

### NLI edge write contract (not a struct; fields for `GRAPH_EDGES` INSERT)

```
source_id:     u64    -- ID of premise entry
target_id:     u64    -- ID of hypothesis entry
relation_type: str    -- 'Contradicts' | 'Supports'
weight:        f32    -- NLI score for the relation type
created_by:    str    -- 'nli'
source:        str    -- 'nli'
bootstrap_only: i32   -- 0 (NLI-confirmed edges are never bootstrap-only)
metadata:      str    -- JSON: '{"nli_entailment": f32, "nli_contradiction": f32}'
// Write uses INSERT OR IGNORE on UNIQUE(source_id, target_id, relation_type)
// NEVER via AnalyticsWrite::GraphEdge — always store.write_pool_server() directly (SR-02)
```

---

## Data Flow Between Components

### Search Re-ranking (NLI active path)

```
context_search MCP call
  -> SearchService::search
       -> [embed query via rayon spawn_with_timeout]
       -> [HNSW search: expanded to nli_top_k candidates]
       -> [quarantine filter, status/penalty, supersession injection]
       -> nli_handle.get_provider()
            -> Err: fallback to rerank_score sort (unchanged)
            -> Ok(provider):
                 -> rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT) {
                      provider.score_batch(query_candidate_pairs)
                    }
                    -> Err(Timeout|Cancelled): fallback to rerank_score sort
                    -> Ok(nli_scores):
                         -> sort candidates by nli_scores[i].entailment DESC (stable sort)
                         -> truncate to top-K
                         -> co-access boost
                         -> floors
  -> SearchResults (same MCP response schema)
```

### Post-Store Detection (fire-and-forget)

```
context_store MCP call
  -> StoreService::insert
       -> [embed, duplicate check, SQL insert, HNSW insert]
       -- hand-off: move embedding Vec<f32> after HNSW insert (ADR-004) --
       -> if nli_enabled && nli_handle.is_ready_or_loading():
            tokio::spawn(run_post_store_nli(embedding, entry_id, ...))  [non-blocking]
  -> MCP response returned immediately

run_post_store_nli (async, no MCP timeout):
  -> nli_handle.get_provider() -> Err: return (debug log)
  -> vector_index.search(embedding, nli_post_store_k) -> neighbor_ids
  -> store.get_batch(neighbor_ids) -> neighbor entries
  -> rayon_pool.spawn(|| provider.score_batch(pairs)) [no timeout, W1-2]
  -> for (neighbor, scores) in zip(neighbors, nli_scores):
       if scores.entailment > nli_entailment_threshold: write Supports edge
       if scores.contradiction > nli_contradiction_threshold: write Contradicts edge
       if total_edges_written >= max_contradicts_per_tick: break (debug log)
  -> each edge: store.write_pool_server() INSERT OR IGNORE
```

### Bootstrap Promotion (one-shot, background tick)

```
background_tick -> maybe_run_bootstrap_promotion(store, nli_handle, rayon_pool, config)
  -> read_counter("bootstrap_nli_promotion_done") != 0: return (no-op)
  -> nli_handle.get_provider() -> Err: info log "deferred", return (no marker set)
  -> run_bootstrap_promotion(store, provider, rayon_pool, config):
       -> store.query_bootstrap_contradicts() -> rows (WHERE bootstrap_only=1 AND relation_type='Contradicts')
       -> fetch source/target texts for all rows (async DB reads)
       -- ALL inference dispatched as single rayon spawn (W1-2) --
       -> rayon_pool.spawn(|| provider.score_batch(all_pairs))
       -> for (row, score) in zip(rows, scores):
            BEGIN TRANSACTION
              DELETE FROM graph_edges WHERE id = row.id
              if score.contradiction > nli_contradiction_threshold:
                INSERT OR IGNORE INTO graph_edges (..., source='nli', bootstrap_only=0, metadata=...)
            COMMIT
       -> set_counter("bootstrap_nli_promotion_done", 1) -- inside final transaction
```

### Auto-Quarantine Threshold (background tick)

```
background_tick auto-quarantine path:
  -> for each entry with topology penalty:
       classify: is penalized ONLY by NLI-origin Contradicts edges?
         -> read edge.source from graph_edges WHERE source_id=entry.id AND relation_type='Contradicts'
         -> if ALL edges have source='nli' AND source_id matches entry:
              read nli_contradiction score from each edge's metadata JSON
              if ANY score < nli_auto_quarantine_threshold: DO NOT quarantine (higher bar)
         -> if ANY edge has source != 'nli': use existing auto-quarantine logic unchanged
```

---

## Sequencing Constraints

1. Config Extension must compile first — all other components import `InferenceConfig`.
2. `NliModel` enum (model.rs) and `NliScores` / `CrossEncoderProvider` (cross_encoder.rs) must
   exist before `NliProvider` can be implemented.
3. `NliProvider` must exist before `NliServiceHandle` can be implemented.
4. `NliServiceHandle` must exist before `SearchService` and `StoreService` can be extended.
5. `nli_detection.rs` (run_post_store_nli, run_bootstrap_promotion) may be developed alongside
   NliServiceHandle modifications to StoreService.
6. Eval integration depends on all other components being testable in isolation.
7. Model Download CLI depends on `ensure_nli_model` (download.rs extension).

---

## Critical Constraints (All Components)

- W1-2: Every `NliProvider::score_pair` / `score_batch` call across ALL paths must go via
  `rayon_pool.spawn()` or `rayon_pool.spawn_with_timeout()`. Never inline in async context.
  Never via `tokio::task::spawn_blocking`. This applies to search, post-store, and bootstrap.
- Pool floor: when `nli_enabled=true`, `rayon_pool_size = rayon_pool_size.max(6).min(8)`.
- NLI edge writes: always `store.write_pool_server()` directly. Never `AnalyticsWrite::GraphEdge`.
- No schema migration: `GRAPH_EDGES` schema v13 from crt-021 used as-is.
- Hash verification (ADR-003): if `nli_model_sha256` set, verify before `Session::builder()`.
- Input truncation (NFR-08): enforced in `NliProvider` before tokenization, not at call sites.
- Mutex poison (ADR-001): detect in `NliServiceHandle::get_provider()` via `try_lock()`.
