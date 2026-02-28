# Architecture: crt-006 Adaptive Embedding

## System Overview

crt-006 introduces a new `unimatrix-adapt` crate that sits between the frozen ONNX embedding pipeline (`unimatrix-embed`) and the HNSW vector index (`unimatrix-vector`). The adaptation layer transforms raw 384d f32 embeddings into domain-adapted 384d f32 embeddings using learned low-rank adjustments and domain prototypes. Training uses co-access pair signals from crt-004 via contrastive learning.

The adaptation is transparent to the rest of the system: all embeddings (both write-path and read-path) pass through the same adaptation pipeline, so entries and queries live in the same adapted embedding space. The `EmbeddingProvider` trait in `unimatrix-embed` is unchanged. The adaptation operates downstream on its output.

## Component Breakdown

### Component 1: MicroLoRA Engine (`unimatrix-adapt::lora`)

**Responsibility**: Low-rank adaptation of embedding vectors.

- `MicroLoRA` struct holding weight matrices A (d x r) and B (r x d), scale factor, and pre-allocated buffers
- Forward pass: `output = input + scale * (input @ A @ B)`, followed by L2 normalization
- Backward pass: gradient computation with respect to A and B matrices
- Initialization: Xavier normal for A, near-zero (1e-4 scale) for B (near-identity at start)
- LoRA+ learning rate support: B matrix gets a configurable ratio (default 16x) higher learning rate than A

**Data structures**:
- Weight storage: `ndarray::Array2<f32>` for A and B matrices
- Pre-allocated buffers: intermediate `Array1<f32>` for forward pass, gradient accumulators for backward pass
- Configuration: `LoraConfig { rank: u8, dimension: u16, scale: f32, lr_a: f32, lr_b: f32, lr_ratio: f32 }`

### Component 2: Training Pipeline (`unimatrix-adapt::training`)

**Responsibility**: InfoNCE contrastive loss, reservoir sampling, batch training orchestration.

- `TrainingReservoir` struct: fixed-capacity buffer of co-access pairs with reservoir sampling
- `InfoNCELoss` computation with log-sum-exp numerical stability
- `TrainingStep` orchestrator: sample batch from reservoir, compute forward passes, compute loss + EWC penalty, compute gradients, update weights
- Temperature parameter tau (default 0.07), configurable batch size (default 32)

**Data structures**:
- `TrainingPair { entry_id_a: u64, entry_id_b: u64, count: u32 }` -- co-access pair with frequency
- `TrainingReservoir { pairs: Vec<TrainingPair>, capacity: usize, total_seen: u64 }` -- reservoir sampling buffer
- `TrainingBatch { positive_pairs: Vec<(Vec<f32>, Vec<f32>)>, raw_embeddings: Vec<Vec<f32>> }` -- prepared batch with embeddings

### Component 3: EWC++ Regularization (`unimatrix-adapt::regularization`)

**Responsibility**: Prevent catastrophic forgetting via online Fisher information.

- `EwcState` struct: Fisher diagonal, reference parameters (theta*), decay factor alpha
- Online update: `F_new = alpha * F_old + (1-alpha) * F_batch`, `theta* = alpha * theta*_old + (1-alpha) * theta_current`
- Penalty computation: `(lambda_ewc/2) * sum(F_i * (theta_i - theta*_i)^2)`
- Gradient contribution: `lambda_ewc * F_i * (theta_i - theta*_i)`

**Data structures**:
- `EwcState { fisher: Array1<f32>, reference_params: Array1<f32>, alpha: f32, lambda: f32 }`
- Parameter vector is flattened: `[A.flatten(), B.flatten()]` of length `2 * rank * dimension`

### Component 4: Domain Prototypes (`unimatrix-adapt::prototypes`)

**Responsibility**: Online running-mean centroids per category/topic with soft pull.

- `PrototypeManager` struct: bounded collection of prototypes with LRU eviction
- Prototype update: online running mean when entries are embedded
- Soft pull: `adjusted = adapted + alpha * (prototype - adapted)` where `alpha = pull_strength * cosine_sim(adapted, prototype)`
- Cold start: new categories/topics skip pull until minimum entry count threshold (default 3)

**Data structures**:
- `Prototype { key: PrototypeKey, centroid: Array1<f32>, entry_count: u32, last_updated: u64 }`
- `PrototypeKey` enum: `Category(String)` or `Topic(String)`
- `PrototypeManager { prototypes: HashMap<PrototypeKey, Prototype>, max_count: usize, min_entries: u32, pull_strength: f32 }`

### Component 5: Episodic Augmentation (`unimatrix-adapt::episodic`)

**Responsibility**: Post-search result refinement using co-access affinity.

- Query-time only -- not part of indexing
- For top-k results, check co-access affinity with anchor results
- Apply small weighted interpolation with co-access partners' adapted embeddings
- This adjusts the final similarity scores, not the indexed vectors

### Component 6: Adaptation State Persistence (`unimatrix-adapt::persistence`)

**Responsibility**: Versioned binary serialization/deserialization of full adaptation state.

- `AdaptationState` struct: all learnable parameters + configuration + metadata
- Versioned format with bincode v2 serialization (matches project convention)
- `save(path)` and `load(path)` with graceful fallback to identity on missing/corrupt state
- Version field for forward-compatible evolution (unknown fields skipped via `serde(default)`)

### Component 7: Adaptation Service (`unimatrix-adapt::service`)

**Responsibility**: Public API that the server calls. Orchestrates the pipeline.

- `AdaptationService` struct: owns MicroLoRA, prototypes, EWC state, training reservoir, and persistence
- `adapt_embedding(raw: &[f32], category: Option<&str>, topic: Option<&str>) -> Vec<f32>` -- full forward pipeline
- `record_training_pairs(pairs: &[(u64, u64, u32)])` -- add pairs to reservoir
- `try_train_step(embed_fn: &dyn Fn(u64) -> Option<Vec<f32>>)` -- attempt training if reservoir has enough pairs
- `save_state(path)` / `load_state(path)` -- persistence delegation
- `training_generation() -> u64` -- current generation counter
- Must be `Send + Sync` for Arc sharing across async tasks

## Component Interactions

```
Server (unimatrix-server)
  |
  |-- Write path: context_store / context_correct
  |     1. embed_service.embed_entry(title, content) -> raw Vec<f32>
  |     2. adapt_service.adapt_embedding(raw, category, topic) -> adapted Vec<f32>
  |     3. l2_normalize(&mut adapted)
  |     4. vector_store.insert(entry_id, &adapted)
  |
  |-- Read path: context_search / context_briefing
  |     1. embed_service.embed_entry("", query) -> raw Vec<f32>
  |     2. adapt_service.adapt_embedding(raw, None, None) -> adapted query Vec<f32>
  |     3. l2_normalize(&mut adapted)
  |     4. vector_store.search(adapted, top_k, ef_search) -> results
  |     5. [optional: episodic augmentation on results]
  |     6. re-rank, co-access boost, trim
  |
  |-- Training path: inline with co-access recording
  |     1. coaccess::generate_pairs(entry_ids) -> pairs
  |     2. store.record_co_access_pairs(pairs) [existing]
  |     3. adapt_service.record_training_pairs(pairs_with_counts)
  |     4. adapt_service.try_train_step(|id| re_embed(id)) [fire-and-forget]
  |
  |-- Coherence path: context_status with check_embeddings
  |     1. For sampled entries: re-embed via embed_service
  |     2. Re-adapt via adapt_service.adapt_embedding(re_embedded)
  |     3. Compare adapted re-embedding to stored embedding
  |     4. Count inconsistencies -> embedding_consistency_score()
  |
  |-- Maintenance path: context_status with maintain=true
  |     1. Confidence refresh [existing]
  |     2. Graph compaction [existing]
  |     3. Adaptation re-indexing: batch re-adapt stale entries

unimatrix-adapt (NEW CRATE)
  |-- lora: MicroLoRA forward/backward
  |-- training: InfoNCE + reservoir + batch orchestration
  |-- regularization: EWC++ state + penalty
  |-- prototypes: PrototypeManager + soft pull
  |-- episodic: post-search augmentation
  |-- persistence: AdaptationState save/load
  |-- service: AdaptationService (public API)
```

## Technology Decisions

- **ndarray for matrix operations** (ADR-001): Heap-allocated, BLAS-transparent, good broadcasting API. Pre-allocated buffers minimize allocation churn on the hot path.
- **bincode v2 for state persistence** (ADR-002): Matches project convention (redb uses bincode v2 via serde). Version field + `serde(default)` for forward compatibility.
- **Independent persistence** (ADR-003): Adaptation state persists in its own file alongside HNSW dump. Server handles missing adaptation state gracefully (identity transform).
- **RwLock for weight access** (ADR-004): Readers (forward pass) do not block each other. Writer (training step) acquires exclusive lock briefly for weight swap. Atomic swap pattern: compute new weights fully, then swap under write lock.

## Integration Points

### Dependency Graph

```
unimatrix-adapt
  depends on: unimatrix-store (co-access data types, CoAccessRecord)
  does NOT depend on: unimatrix-embed, unimatrix-vector, unimatrix-core

unimatrix-server
  depends on: unimatrix-adapt (new), unimatrix-embed, unimatrix-vector, unimatrix-core, unimatrix-store
```

The `unimatrix-adapt` crate depends only on `unimatrix-store` for shared data types (`CoAccessRecord` for pair counts). It does NOT depend on `unimatrix-embed` (the frozen ONNX pipeline) or `unimatrix-vector` (the HNSW index). The server orchestrates the data flow between all crates.

### Existing Components Modified

1. **unimatrix-server/tools.rs**: Search and store paths gain adaptation step between embed and vector insert/search.
2. **unimatrix-server/server.rs**: `UnimatrixServer` gains `adapt_service: Arc<AdaptationService>` field.
3. **unimatrix-server/coherence.rs** (no code change): Embedding consistency check receives adapted re-embeddings from the server's orchestration code, not raw ONNX output.
4. **unimatrix-server persistence** (main.rs or startup): Load adaptation state from disk alongside HNSW load. Save adaptation state alongside HNSW dump on shutdown.

### Existing Components NOT Modified

- `unimatrix-embed`: No changes. `EmbeddingProvider` trait unchanged. `OnnxProvider` unchanged.
- `unimatrix-vector`: No changes. `VectorIndex` receives adapted vectors (still 384d f32 L2-normalized).
- `unimatrix-core`: No trait changes. `EmbedService` trait unchanged. The adaptation wraps around the service at the server level, not at the trait level.
- `unimatrix-store`: No schema changes. No new tables. Adaptation state lives in a separate file.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `AdaptationService::new(config: AdaptConfig) -> Self` | Constructor | `unimatrix-adapt::service` |
| `AdaptationService::adapt_embedding(raw: &[f32], category: Option<&str>, topic: Option<&str>) -> Vec<f32>` | Forward pass | `unimatrix-adapt::service` |
| `AdaptationService::record_training_pairs(pairs: &[(u64, u64, u32)])` | Pair ingestion | `unimatrix-adapt::service` |
| `AdaptationService::try_train_step(embed_fn: &dyn Fn(u64) -> Option<Vec<f32>>)` | Training trigger | `unimatrix-adapt::service` |
| `AdaptationService::save_state(path: &Path) -> Result<()>` | Persistence | `unimatrix-adapt::service` |
| `AdaptationService::load_state(path: &Path) -> Result<()>` | Persistence | `unimatrix-adapt::service` |
| `AdaptationService::training_generation() -> u64` | Metadata | `unimatrix-adapt::service` |
| `AdaptConfig` | Configuration struct | `unimatrix-adapt::service` |
| `l2_normalize(&mut [f32])` | Re-exported from unimatrix-embed | `unimatrix-embed::normalize` |
| `UnimatrixServer.adapt_service` | New field: `Arc<AdaptationService>` | `unimatrix-server::server` |

## Data Flow Diagram

### Write Path (context_store)

```
Agent calls context_store("content", "topic", "category")
  |
  v
[1] embed_service.embed_entry(title, content)
  |  -> raw: Vec<f32> (384d, L2-normalized from ONNX)
  v
[2] adapt_service.adapt_embedding(&raw, Some(category), Some(topic))
  |  -> MicroLoRA forward: raw + scale * (raw @ A @ B)
  |  -> Prototype soft pull (if category/topic prototype exists)
  |  -> adapted: Vec<f32> (384d)
  v
[3] l2_normalize(&mut adapted)
  |  -> adapted: Vec<f32> (384d, L2-normalized)
  v
[4] vector_store.insert(entry_id, &adapted)
  |  -> HNSW index updated
```

### Training Path (inline with co-access recording)

```
Agent calls context_search("query") -> results [ids: 1, 5, 3, 8, 2]
  |
  v
[1] coaccess::generate_pairs([1,5,3,8,2]) -> [(1,5), (1,3), (1,8), ...]
  |
  v
[2] store.record_co_access_pairs(pairs) [existing, unchanged]
  |
  v
[3] adapt_service.record_training_pairs(pairs_with_counts)
  |  -> reservoir.add(pairs) -- reservoir sampling
  |
  v
[4] if reservoir.len() >= batch_size:
  |    adapt_service.try_train_step(|id| {
  |      let entry = store.get(id)?;
  |      embed_service.embed_entry(&entry.title, &entry.content).ok()
  |    })
  |    -> Sample batch from reservoir
  |    -> Embed each entry (raw ONNX -> adapt with current weights)
  |    -> Compute InfoNCE loss + EWC penalty
  |    -> Compute gradients for A and B
  |    -> Update weights atomically (compute new, swap under write lock)
  |    -> Update Fisher diagonal (EWC++)
  |    -> Update prototypes for touched categories/topics
  |    -> Increment training_generation
  |    -> Debounced save_state()
```

## Error Handling

- **NaN/Inf guards**: Every loss computation and gradient update checks for NaN/Inf. If detected, the training step is aborted (weights are not updated). Logged as warning.
- **Missing adaptation state**: On startup, if state file does not exist, create fresh state (identity transform). No error.
- **Corrupt adaptation state**: On startup, if state file fails to deserialize, log error, create fresh state. Existing indexed vectors will be inconsistent with fresh adaptation until maintenance re-indexes.
- **Training failure**: Any training step failure (embedding lookup fails, loss computation error) is silently swallowed (fire-and-forget pattern). Weights remain at their pre-step values.
- **Dimension mismatch**: `adapt_embedding` validates input dimension matches configured dimension. Returns error on mismatch (programmer error, not runtime).
