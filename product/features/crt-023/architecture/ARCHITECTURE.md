# Architecture: crt-023 — NLI + Cross-Encoder Re-ranking (W1-4)

## System Overview

crt-023 adds a second ONNX inference provider to Unimatrix: an NLI cross-encoder that classifies `(query, passage)` pairs into `{entailment, neutral, contradiction}` probabilities. This provider operates in two complementary modes that run on the existing `Arc<RayonPool>` from crt-022:

1. **Search re-ranking** (MCP path): After HNSW retrieves top-`nli_top_k` candidates, NLI re-scores each `(query, candidate)` pair and sorts by entailment score before truncation. Replaces the `rerank_score` composite formula when NLI is active.

2. **Post-store edge detection** (fire-and-forget): After `context_store` inserts a new entry, a background tokio task scores the entry against its HNSW neighbors and writes `Contradicts` / `Supports` edges to `GRAPH_EDGES` with `source='nli'`, `bootstrap_only=0`.

Additionally, a one-shot bootstrap promotion task runs on the first background tick after startup (when NLI is ready) to upgrade `bootstrap_only=1` Contradicts edges from W1-1 to NLI-confirmed edges.

The feature is gated by human-reviewed eval results from the W1-3 harness (nan-007). NLI absence, hash mismatch, or load failure never prevents server startup — graceful degradation to cosine similarity is the invariant.

### Placement in Unimatrix Architecture

```
                    ┌─────────────────────────────────────┐
                    │         MCP Server (rmcp)            │
                    │                                     │
  context_search ──►│  SearchService                      │
                    │    [embed] → [HNSW] → [NLI rerank]  │
                    │                           │          │
  context_store ───►│  StoreService             │          │
                    │    [embed] → [insert]     │          │
                    │    └──► fire-and-forget   │          │
                    │         NLI detection     │          │
                    └───────────────────────────┼──────────┘
                                                │
                    ┌───────────────────────────▼──────────┐
                    │         unimatrix-server infra        │
                    │  NliServiceHandle  EmbedServiceHandle │
                    │  (Loading→Ready|Failed→Retrying)      │
                    │                                       │
                    │  Arc<RayonPool> (shared, floor=6)     │
                    └───────────────────────────────────────┘
                                      │
                    ┌─────────────────▼─────────────────────┐
                    │         unimatrix-embed                │
                    │  NliProvider (Mutex<Session>+Tokenizer)│
                    │  OnnxProvider (existing)               │
                    │  NliModel enum                         │
                    │  CrossEncoderProvider trait            │
                    └────────────────────────────────────────┘
                                      │
                    ┌─────────────────▼─────────────────────┐
                    │         unimatrix-store                │
                    │  GRAPH_EDGES (schema v13, no change)  │
                    │  COUNTERS (bootstrap_nli_promotion_done)│
                    └────────────────────────────────────────┘
```

---

## Component Breakdown

### Component 1: `NliProvider` (`unimatrix-embed`)

**Responsibility**: ONNX cross-encoder inference. Takes `(query, passage)` string pairs, tokenizes them as a concatenated pair (cross-encoder input format), runs the ONNX model, applies softmax to 3-element logit output, returns `NliScores`.

**Key design constraints**:
- `Mutex<Session>` for inference serialization (ADR-001; see entry #67)
- `Tokenizer` outside the mutex for lock-free tokenization
- Per-side input truncation to 512 tokens / ~2000 chars enforced inside `NliProvider` before tokenization (security requirement, NFR-08)
- `Send + Sync` via `Mutex` + lock-free tokenizer
- `ort = "=2.0.0-rc.9"` pinned version (same as `OnnxProvider`)

**ONNX output format differs from `OnnxProvider`**:
- Embedding model output: `[batch_size, seq_len, hidden_dim]` → mean pool + L2 normalize
- Cross-encoder NLI output: `[batch_size, 3]` → softmax directly (no pooling step)

**Files**:
- `unimatrix-embed/src/cross_encoder.rs` — `CrossEncoderProvider` trait + `NliScores` struct + `NliProvider` impl
- `unimatrix-embed/src/model.rs` — `NliModel` enum added alongside `EmbeddingModel`
- `unimatrix-embed/src/download.rs` — `ensure_nli_model` following `ensure_model` pattern

### Component 2: `NliServiceHandle` (`unimatrix-server/src/infra/nli_handle.rs`)

**Responsibility**: Lazy-loading state machine for `NliProvider`. Mirrors `EmbedServiceHandle` exactly.

**State machine**:
```
Loading ──load_ok──► Ready
Loading ──load_err─► Failed
Failed  ──retry────► Loading  (backoff, up to MAX_RETRIES=3)
Ready   ──poison───► Failed   (detected on get_provider() when Mutex is poisoned)
```

**Key behaviors**:
- `get_provider()` returns `Ok(Arc<NliProvider>)` when Ready, `Err(NliNotReady)` when Loading/Retrying, `Err(NliFailed)` when retries exhausted
- SHA-256 hash verification before `Session::builder().commit_from_file()` (ADR-003)
- Hash mismatch → `Failed` + `tracing::error!` containing "security" and "hash mismatch"
- Model load uses `tokio::task::spawn_blocking` (not rayon pool — model loading is I/O + one-time CPU, not repeated inference)
- Config stored for retry attempts

### Component 3: Search Re-ranking Integration (`unimatrix-server/src/services/search.rs`)

**Responsibility**: Extend `SearchService` to run NLI re-ranking when the handle is Ready.

`SearchService` gains field `nli_handle: Arc<NliServiceHandle>`.

**Modified pipeline** (NLI active path):
```
embed
→ HNSW top-nli_top_k (was top-K; expanded candidate pool)
→ quarantine filter
→ status filter / penalty (score = base_score * status_penalty)
→ supersession injection
→ NLI batch score via rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)
→ sort by nli_scores.entailment DESCENDING
→ truncate to top-K
→ co-access boost
→ floors
```

**Fallback path** (NLI not ready, disabled, or timeout):
```
embed
→ HNSW top-K
→ quarantine filter
→ status filter / penalty
→ supersession injection
→ sort by rerank_score (existing path, unchanged)
→ co-access boost
→ truncate → floors
```

**NLI batch call**:
```rust
let provider = self.nli_handle.get_provider().await;
let nli_scores: Vec<NliScores> = match provider {
    Ok(p) => {
        let pairs: Vec<(&str, &str)> = candidates
            .iter()
            .map(|c| (query_text.as_str(), c.entry.content.as_str()))
            .collect();
        let p_clone = Arc::clone(&p);
        match self.rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, move || {
            p_clone.score_batch(&pairs)
        }).await {
            Ok(Ok(scores)) => scores,
            Ok(Err(_)) | Err(_) => return fallback_rerank_sort(candidates, confidence_weight),
        }
    }
    Err(_) => return fallback_rerank_sort(candidates, confidence_weight),
};
```

### Component 4: Post-Store NLI Detection (`unimatrix-server/src/services/store_ops.rs`)

**Responsibility**: After successful insert, fire-and-forget a tokio task that runs NLI on (new_entry, neighbor) pairs and writes edges to `GRAPH_EDGES`.

`StoreService` gains field `nli_handle: Arc<NliServiceHandle>` and reads `nli_post_store_k`, `nli_entailment_threshold`, `nli_contradiction_threshold`, `max_contradicts_per_tick` from `InferenceConfig`.

**Hand-off contract** (ADR-004): The `embedding: Vec<f32>` is moved into the spawned task after the HNSW insert step. No clone is needed.

**Post-store task pipeline**:
```
1. check nli_handle.get_provider() → exit if Err
2. vector_index.search(embedding, nli_post_store_k, EF_SEARCH) → neighbor_ids
3. store.get_batch(neighbor_ids) → neighbor entries
4. build pairs: (new_entry_text, neighbor_text) for each neighbor
5. rayon_pool.spawn(|| provider.score_batch(&pairs)) → Vec<NliScores>
   (no timeout — fire-and-forget background task)
6. for each (neighbor_id, scores):
   - if scores.entailment > nli_entailment_threshold → write Supports edge
   - if scores.contradiction > nli_contradiction_threshold → write Contradicts edge
   - circuit breaker: stop after max_contradicts_per_tick total edges written
7. each edge: store.write_pool_server() INSERT OR IGNORE with metadata JSON
```

**`run_post_store_nli` function signature**:
```rust
async fn run_post_store_nli(
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
    max_edges_per_call: usize,
)
```

### Component 5: Bootstrap Edge Promotion (`unimatrix-server/src/services/background.rs` or dedicated module)

**Responsibility**: One-shot task that promotes or deletes `bootstrap_only=1` Contradicts edges.

**Idempotency** (ADR-005): COUNTERS key `bootstrap_nli_promotion_done` = 1 when done. Checked before any GRAPH_EDGES query.

**Deferral** (FR-25): If NLI not ready at tick time → log info + return (no marker set). Re-attempts on next tick.

**Promotion logic**:
```
FOR EACH row in GRAPH_EDGES WHERE bootstrap_only=1 AND relation_type='Contradicts':
  score = nli.score_pair(source_entry_text, target_entry_text)
  IF score.contradiction > nli_contradiction_threshold:
    BEGIN TRANSACTION
      DELETE FROM graph_edges WHERE id = row.id
      INSERT OR IGNORE INTO graph_edges (..., source='nli', bootstrap_only=0, metadata=...)
    COMMIT
  ELSE:
    DELETE FROM graph_edges WHERE id = row.id
SET COUNTERS bootstrap_nli_promotion_done = 1
```

### Component 6: Config Extension (`unimatrix-server/src/infra/config.rs`)

Ten new fields in `InferenceConfig` (all `#[serde(default)]`):
- `nli_enabled: bool` (default true)
- `nli_model_name: Option<String>` (default None → resolves to "minilm2")
- `nli_model_path: Option<PathBuf>` (default None → uses cache dir)
- `nli_model_sha256: Option<String>` (default None)
- `nli_top_k: usize` (default 20, range [1,100])
- `nli_post_store_k: usize` (default 10, range [1,100])
- `nli_entailment_threshold: f32` (default 0.6, range (0.0, 1.0))
- `nli_contradiction_threshold: f32` (default 0.6, range (0.0, 1.0))
- `max_contradicts_per_tick: usize` (default 10, range [1,100]; per-call semantic per FR-22)
- `nli_auto_quarantine_threshold: f32` (default 0.85, range (0.0, 1.0), must be > `nli_contradiction_threshold`)

`InferenceConfig::validate()` extended with range checks for all ten fields plus the
cross-field invariant: `nli_auto_quarantine_threshold > nli_contradiction_threshold`.
Violation aborts startup with a structured error naming both fields (ADR-007).

Pool floor: when `nli_enabled = true`, `rayon_pool_size = rayon_pool_size.max(6).min(8)` (ADR-001 pool floor raise).

### Component 7: Eval Integration (`unimatrix-server/src/services/eval.rs` or eval module)

**Responsibility**: Fill the W1-4 stub in `EvalServiceLayer::from_profile()`.

When profile has `nli_enabled = true` and a resolvable model:
- Construct `NliServiceHandle`, call `start_loading()`
- Await readiness (up to 60s) before beginning scenario execution
- If `Failed` or timeout → skip profile with SKIPPED annotation (ADR-006)
- Wire `NliServiceHandle` into `SearchService` for this eval layer

### Component 8: Model Download CLI

`unimatrix model-download --nli [--nli-model minilm2|deberta]`

Downloads the selected NLI model via `hf-hub`, computes SHA-256, prints to stdout. Follows existing `ensure_model` pattern.

---

## Component Interactions

### Search Re-ranking Data Flow

```
MCP context_search
       │
       ▼
SearchService::search
  1. gateway.check_search_rate()
  2. embed_service.get_adapter() → EmbedAdapter
  3. rayon_pool.spawn_with_timeout → embed query → Vec<f32>
  4. vector_store.search(query_embedding, nli_top_k) → HNSW candidates
  5. filter: quarantine, status, supersession
  6. nli_handle.get_provider()
     ├── Err → fallback: rerank_score sort
     └── Ok(provider) →
           rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, || {
             provider.score_batch(pairs)
           })
           ├── Err(Timeout|Cancelled) → fallback: rerank_score sort
           └── Ok(scores) →
                 sort by nli_scores.entailment DESC
                 truncate to top-K
                 co-access boost
                 floors
       │
       ▼
SearchResults → MCP response (same schema as before)
```

### Post-Store NLI Detection Data Flow

```
MCP context_store
       │
       ▼
StoreService::insert
  1..5. [existing insert pipeline]
  5b. HNSW insert (embedding used here, then moved)
  6. tokio::spawn(run_post_store_nli(embedding, entry_id, ...))
     │ (fire-and-forget, non-blocking)
       ▼
  MCP response returned (insert complete)

run_post_store_nli (async, no MCP timeout)
  1. nli_handle.get_provider() → Err? return
  2. vector_index.search(embedding, nli_post_store_k)
  3. store.get_batch(neighbor_ids) → neighbor entries
  4. rayon_pool.spawn(|| provider.score_batch(pairs))  ← no timeout
  5. for (neighbor_id, scores) in results:
       edges_written = 0
       if scores.entailment > threshold:
         write_graph_edge(Supports, metadata)
         edges_written += 1
       if scores.contradiction > threshold && edges_written < max_edges_per_call:
         write_graph_edge(Contradicts, metadata)
         edges_written += 1
       if edges_written >= max_edges_per_call: break
  Each write: store.write_pool_server() INSERT OR IGNORE (not analytics queue — SR-02)
```

### GRAPH_EDGES Write Path

All NLI-confirmed edge writes use `store.write_pool_server()` directly. The `AnalyticsWrite::GraphEdge` variant is not used for NLI writes (its doc comment explicitly prohibits NLI-confirmed writes via the analytics queue). This is SR-02 and is documented in `analytics.rs`.

The write SQL:
```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at, created_by,
     source, bootstrap_only, metadata)
VALUES (?1, ?2, ?3, ?4, ?5, 'nli', 'nli', 0, ?6)
```

Where `metadata` = `'{"nli_entailment": <f32>, "nli_contradiction": <f32>}'`.

---

## Technology Decisions

| Decision | Choice | ADR |
|----------|--------|-----|
| NLI session concurrency | Single `Mutex<Session>` | ADR-001 |
| NLI sort key for search | Pure entailment replacement | ADR-002 |
| Model selection mechanism | Config string + hash pinning | ADR-003 |
| Embedding handoff to NLI task | Move after HNSW insert | ADR-004 |
| Bootstrap promotion idempotency | COUNTERS key `bootstrap_nli_promotion_done` | ADR-005 |
| Eval CLI missing model | Skip profile with SKIPPED annotation | ADR-006 |
| Pool floor when NLI enabled | Raise to 6 (from 4) | ADR-001 |
| NLI edge write path | `write_pool_server()` directly | SR-02 constraint (analytics.rs) |
| Primary NLI model | `cross-encoder/nli-MiniLM2-L6-H768` (~85MB) | ADR-003 |
| `ort` version | `=2.0.0-rc.9` pinned (no change) | Constraint C-01 |
| New crate | None; stays in `unimatrix-embed` + `unimatrix-server` | Non-goal |
| Schema migration | None; `GRAPH_EDGES` v13 used as-is | Constraint C-08 |

---

## Integration Points

### Existing Components Modified

| Component | Modification |
|-----------|-------------|
| `unimatrix-embed/src/model.rs` | Add `NliModel` enum |
| `unimatrix-embed/src/download.rs` | Add `ensure_nli_model` |
| `unimatrix-server/src/infra/config.rs` | Add 9 NLI fields to `InferenceConfig` |
| `unimatrix-server/src/services/search.rs` | Add `nli_handle` field; modified pipeline |
| `unimatrix-server/src/services/store_ops.rs` | Add `nli_handle` field; fire-and-forget spawn |
| `unimatrix-server/src/services/background.rs` | Add bootstrap promotion task call |
| `unimatrix-server/src/main.rs` / startup | Construct + wire `NliServiceHandle` |
| `EvalServiceLayer::from_profile()` | Fill W1-4 stub |

### New Files

| File | Purpose |
|------|---------|
| `unimatrix-embed/src/cross_encoder.rs` | `CrossEncoderProvider` trait, `NliScores`, `NliProvider` |
| `unimatrix-server/src/infra/nli_handle.rs` | `NliServiceHandle` state machine |
| `unimatrix-server/src/services/nli_detection.rs` | `run_post_store_nli`, `run_bootstrap_promotion` |

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `CrossEncoderProvider` trait | `fn score_pair(&self, query: &str, passage: &str) -> Result<NliScores>` | `unimatrix-embed/src/cross_encoder.rs` (new) |
| `CrossEncoderProvider` trait | `fn score_batch(&self, pairs: &[(&str, &str)]) -> Result<Vec<NliScores>>` | `unimatrix-embed/src/cross_encoder.rs` (new) |
| `NliScores` struct | `{ entailment: f32, neutral: f32, contradiction: f32 }` | `unimatrix-embed/src/cross_encoder.rs` (new) |
| `NliModel` enum | `NliMiniLM2L6H768`, `NliDebertaV3Small`; `from_config_name(&str) -> Option<Self>` | `unimatrix-embed/src/model.rs` (extended) |
| `NliModel::onnx_repo_path()` | `fn() -> &'static str` | `unimatrix-embed/src/model.rs` |
| `NliModel::onnx_filename()` | `fn() -> &'static str` (returns `"model.onnx"`) | `unimatrix-embed/src/model.rs` |
| `NliModel::cache_subdir()` | `fn() -> &'static str` | `unimatrix-embed/src/model.rs` |
| `NliServiceHandle::new()` | `fn() -> Arc<Self>` | `unimatrix-server/src/infra/nli_handle.rs` (new) |
| `NliServiceHandle::start_loading()` | `fn(self: &Arc<Self>, config: NliConfig)` | `unimatrix-server/src/infra/nli_handle.rs` (new) |
| `NliServiceHandle::get_provider()` | `async fn(&self) -> Result<Arc<NliProvider>, ServerError>` | `unimatrix-server/src/infra/nli_handle.rs` (new) |
| `ServerError::NliNotReady` | New error variant | `unimatrix-server/src/error.rs` (extended) |
| `ServerError::NliFailed(String)` | New error variant | `unimatrix-server/src/error.rs` (extended) |
| `InferenceConfig` new fields | 9 NLI config fields (see Component 6) | `unimatrix-server/src/infra/config.rs` (extended) |
| `run_post_store_nli` | `async fn(Vec<f32>, u64, String, Arc<NliServiceHandle>, ...) ` | `unimatrix-server/src/services/nli_detection.rs` (new) |
| `run_bootstrap_promotion` | `async fn(Arc<Store>, Arc<NliServiceHandle>, Arc<RayonPool>, &InferenceConfig)` | `unimatrix-server/src/services/nli_detection.rs` (new) |
| `GRAPH_EDGES` write (NLI path) | `INSERT OR IGNORE` with `source='nli'`, `bootstrap_only=0`, `metadata=JSON` | `unimatrix-store/src/db.rs` via `write_pool_server()` |
| `COUNTERS` key | `bootstrap_nli_promotion_done` (u64, 1 when done) | `unimatrix-store/src/counters.rs` via `set_counter` / `read_counter` |
| `RayonPool` floor (NLI active) | `rayon_pool_size.max(6).min(8)` applied at startup | `unimatrix-server/src/infra/config.rs` |

---

## Open Questions (Resolved)

**OQ-01: Pool sizing and session concurrency** — RESOLVED by ADR-001.
- Single `Mutex<Session>` for NLI (consistent with nxs-003 ADR-001, entry #67).
- At 20 pairs × 200ms worst case = ~4s hold time per search re-ranking call.
- With 2 concurrent search calls, second caller waits up to ~4s for the mutex; within `MCP_HANDLER_TIMEOUT`.
- Pool floor raised to 6 when `nli_enabled = true` (from current minimum of 4).
- Post-store task uses `rayon_pool.spawn()` (no timeout); `max_contradicts_per_tick` cap limits scope.

**OQ-02: Embedding handoff ownership contract** — RESOLVED by ADR-004.
- Embedding `Vec<f32>` is moved (not cloned) into the fire-and-forget task after the HNSW insert step.
- Insert pipeline uses the embedding for HNSW insert (step 5) and then relinquishes ownership.
- If embedding was not available (embedding failed during insert), no task is spawned.
- If duplicate detected (early return before HNSW insert), no task is spawned.

**OQ-03: Bootstrap promotion durable marker** — RESOLVED by ADR-005.
- COUNTERS table with `TEXT PRIMARY KEY` and `INTEGER` value is confirmed correct.
- Key `bootstrap_nli_promotion_done`, value 1 when done. `read_counter` returns 0 for missing rows (absence = not done).
- `set_counter` uses `INSERT OR REPLACE` — idempotent.
- Marker set inside the same write transaction as the last batch of edge operations.

**OQ-04: Eval CLI missing-model behavior** — RESOLVED by ADR-006.
- Skip the profile with a SKIPPED annotation in the eval report.
- `EvalServiceLayer` waits up to 60s for NLI readiness before beginning scenario execution.
- If timeout or Failed → profile SKIPPED. Baseline profile always runs.
- No `--skip-profile-on-missing-model` flag needed — skip is the default behavior.

**OQ-05: Deberta ONNX model availability** — RESOLVED by ADR-003.
- `NliDebertaV3Small` variant implemented unconditionally in the enum.
- ONNX availability must be verified at implementation time. If unavailable, 3-profile eval degrades to 2-profile; this is documented in the delivery report.
- `onnx_filename()` returns `"model.onnx"` as best-effort; implementer must confirm at download time.
- `cross-encoder/nli-MiniLM2-L6-H768` is the confirmed primary model.

---

## Key Design Decisions Summary

1. **Single `Mutex<Session>` for NLI** — Memory-bounded (~85MB), consistent with OnnxProvider pattern, adequate for Unimatrix's sequential MCP workload profile. Session pool rejected for complexity/memory cost.

2. **Pool floor raised to 6 when NLI is enabled** — NLI's multi-pair batched workload (up to 20 pairs per search call) requires headroom beyond the 4-thread floor established for embedding + contradiction scan. Pool floor is a conditional startup override, not a structural change.

3. **NLI entailment score replaces `rerank_score` entirely** — Clean semantics for a first iteration. Eval gate validates the replacement. Rollback is `nli_enabled = false`. Blended formula deferred to a follow-on feature if eval warrants.

4. **Config-string model selection + per-file hash pinning** — `nli_model_name = "minilm2"` or `"deberta"` enables model swap via config. `nli_model_sha256` binds the hash to the specific config file (one hash per deployment). Simple; no nested hash maps.

5. **Post-store embedding is moved, not cloned** — The raw `Vec<f32>` is transferred from the insert pipeline to the fire-and-forget task at the hand-off point after HNSW insert. Zero-copy ownership transfer in Rust.

6. **COUNTERS table for bootstrap promotion idempotency** — `bootstrap_nli_promotion_done = 1` is the durable completion marker. Existing `set_counter` / `read_counter` primitives handle the logic. No schema change.

7. **NLI edge writes use `write_pool_server()` directly** — `AnalyticsWrite::GraphEdge` is explicitly marked as shed-safe for bootstrap-origin only. NLI-confirmed edge writes are integrity writes and must bypass the analytics queue. This is already documented in `analytics.rs`.

8. **Eval missing-model behavior is skip-not-fail** — CI environments without cached models get partial results (baseline only) rather than hard failure. SKIPPED annotation makes the situation visible.

---

## Risk Mitigations

| Risk | Mitigation |
|------|-----------|
| SR-02: Pool saturation | Pool floor raised to 6; `spawn_with_timeout` fallback to cosine on timeout |
| SR-03: Single session serialization | Acceptable for Unimatrix's sequential workload; `MCP_HANDLER_TIMEOUT` fallback |
| SR-05: NLI score regression | Eval gate (AC-09) validates before feature is marked deliverable; `nli_enabled=false` rollback |
| SR-07: Bootstrap promotion non-idempotent | COUNTERS key marker in same transaction as edge operations (ADR-005) |
| SR-08: Eval CI missing model | Skip-profile behavior with SKIPPED annotation (ADR-006) |
| SR-09: Embedding handoff | Move semantics; no clone; fire-and-forget task owns the Vec (ADR-004) |
| SR-01: Deberta ONNX unavailable | Variant implemented; 2-profile eval documented as valid fallback |
