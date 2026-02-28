# Implementation Brief: crt-006 Adaptive Embedding

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-006/SCOPE.md |
| Scope Risk Assessment | product/features/crt-006/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-006/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-006/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-006/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-006/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| lora | pseudocode/lora.md | test-plan/lora.md |
| training | pseudocode/training.md | test-plan/training.md |
| regularization | pseudocode/regularization.md | test-plan/regularization.md |
| prototypes | pseudocode/prototypes.md | test-plan/prototypes.md |
| episodic | pseudocode/episodic.md | test-plan/episodic.md |
| persistence | pseudocode/persistence.md | test-plan/persistence.md |
| service | pseudocode/service.md | test-plan/service.md |
| server-integration | pseudocode/server-integration.md | test-plan/server-integration.md |

## Goal

Implement a 4-stage embedding adaptation pipeline (MicroLoRA -> Prototype Adjustment -> Episodic Augmentation -> L2 Normalization) that learns domain-specific adjustments to frozen ONNX embeddings using co-access pair signals. The pipeline lives in a new `unimatrix-adapt` crate and integrates into the server's write and read paths between embedding generation and vector indexing.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Matrix library | ndarray with pre-allocated buffers | ADR-001 | architecture/ADR-001-ndarray-for-matrix-ops.md |
| State serialization | bincode v2 with serde, version field + serde(default) | ADR-002 | architecture/ADR-002-bincode-state-persistence.md |
| Persistence strategy | Independent file (adaptation.state) alongside HNSW dump | ADR-003 | architecture/ADR-003-independent-persistence.md |
| Concurrency model | RwLock with atomic swap for weight updates | ADR-004 | architecture/ADR-004-rwlock-weight-access.md |
| Rank range | 2-16 configurable, default 4 | SCOPE.md | -- |
| Training buffer | Reservoir sampling, capacity 512, batch size 32 | SCOPE.md | -- |
| EWC++ decay | Online Fisher, alpha=0.95, lambda=0.5 | SCOPE.md | -- |
| Prototype bounds | Max 256, min entries 3, LRU eviction | SCOPE.md | -- |
| Temperature | tau=0.07 for InfoNCE | SCOPE.md | -- |
| LoRA+ ratio | B lr = 16 * A lr | SCOPE.md | -- |

## Files to Create

| Path | Summary |
|------|---------|
| `crates/unimatrix-adapt/Cargo.toml` | New crate: edition 2024, MSRV 1.89, deps: ndarray, bincode, serde, rand, unimatrix-store |
| `crates/unimatrix-adapt/src/lib.rs` | Crate root: `#![forbid(unsafe_code)]`, module declarations, public re-exports |
| `crates/unimatrix-adapt/src/lora.rs` | MicroLoRA struct, forward pass, backward pass, initialization (Xavier/near-zero) |
| `crates/unimatrix-adapt/src/training.rs` | TrainingReservoir, InfoNCE loss, batch training step orchestration |
| `crates/unimatrix-adapt/src/regularization.rs` | EwcState, online Fisher update, penalty computation, gradient contribution |
| `crates/unimatrix-adapt/src/prototypes.rs` | PrototypeManager, PrototypeKey, Prototype, soft pull, LRU eviction, cold start |
| `crates/unimatrix-adapt/src/episodic.rs` | Episodic augmentation for post-search result refinement |
| `crates/unimatrix-adapt/src/persistence.rs` | AdaptationState, save/load, version handling, graceful fallback |
| `crates/unimatrix-adapt/src/service.rs` | AdaptationService: public API orchestrating all components |
| `crates/unimatrix-adapt/src/config.rs` | AdaptConfig struct with all configurable parameters and defaults |

## Files to Modify

| Path | Summary |
|------|---------|
| `Cargo.toml` (workspace) | Add `crates/unimatrix-adapt` to workspace members |
| `crates/unimatrix-server/Cargo.toml` | Add dependency on `unimatrix-adapt` |
| `crates/unimatrix-server/src/server.rs` | Add `adapt_service: Arc<AdaptationService>` field to UnimatrixServer |
| `crates/unimatrix-server/src/tools.rs` | Insert adaptation step in context_store (write path) and context_search (read path) |
| `crates/unimatrix-server/src/main.rs` or startup | Load/save adaptation state alongside HNSW |
| `product/test/infra-001/suites/test_adaptation.py` | New integration test suite (10 tests) |
| `product/test/infra-001/suites/conftest.py` | Add `trained_server` fixture if needed |

## Data Structures

### AdaptConfig
```rust
pub struct AdaptConfig {
    pub rank: u8,           // 2-16, default 4
    pub dimension: u16,     // default 384
    pub scale: f32,         // LoRA scale factor, default 1.0
    pub lr_a: f32,          // Learning rate for A matrix, default 0.001
    pub lr_ratio: f32,      // B lr = lr_ratio * lr_a, default 16.0
    pub temperature: f32,   // InfoNCE tau, default 0.07
    pub batch_size: usize,  // Training batch size, default 32
    pub reservoir_capacity: usize, // default 512
    pub ewc_alpha: f32,     // EWC++ decay, default 0.95
    pub ewc_lambda: f32,    // EWC penalty weight, default 0.5
    pub max_prototypes: usize, // default 256
    pub min_prototype_entries: u32, // default 3
    pub pull_strength: f32, // Prototype pull, default 0.1
}
```

### MicroLoRA (internal)
```rust
pub struct MicroLoRA {
    weights_a: RwLock<Array2<f32>>,  // d x r
    weights_b: RwLock<Array2<f32>>,  // r x d
    scale: f32,
    config: LoraConfig,
    // Pre-allocated buffers for forward pass
    buf_down: RwLock<Array1<f32>>,   // r
    buf_up: RwLock<Array1<f32>>,     // d
}
```

### AdaptationState (serializable)
```rust
#[derive(Serialize, Deserialize)]
pub struct AdaptationState {
    pub version: u32,
    pub rank: u8,
    pub dimension: u16,
    pub scale: f32,
    pub weights_a: Vec<f32>,       // d * r flattened
    pub weights_b: Vec<f32>,       // r * d flattened
    pub fisher_diagonal: Vec<f32>, // 2 * d * r flattened
    pub reference_params: Vec<f32>,// 2 * d * r flattened
    pub prototypes: Vec<SerializedPrototype>,
    pub training_generation: u64,
    pub total_training_steps: u64,
    #[serde(default)]
    pub config: AdaptConfig,
}
```

### TrainingReservoir
```rust
pub struct TrainingReservoir {
    pairs: Vec<TrainingPair>,
    capacity: usize,
    total_seen: u64,
    rng: StdRng,
}

pub struct TrainingPair {
    pub entry_id_a: u64,
    pub entry_id_b: u64,
    pub count: u32,
}
```

## Function Signatures

### AdaptationService (public API)
```rust
impl AdaptationService {
    pub fn new(config: AdaptConfig) -> Self;
    pub fn adapt_embedding(&self, raw: &[f32], category: Option<&str>, topic: Option<&str>) -> Vec<f32>;
    pub fn record_training_pairs(&self, pairs: &[(u64, u64, u32)]);
    pub fn try_train_step(&self, embed_fn: &dyn Fn(u64) -> Option<Vec<f32>>);
    pub fn save_state(&self, path: &Path) -> Result<()>;
    pub fn load_state(&self, path: &Path) -> Result<()>;
    pub fn training_generation(&self) -> u64;
    pub fn state_size_bytes(&self) -> usize;
}
```

### MicroLoRA (internal)
```rust
impl MicroLoRA {
    pub fn new(config: LoraConfig) -> Self;
    pub fn forward(&self, input: &[f32]) -> Vec<f32>;
    pub fn backward(&self, input: &[f32], grad_output: &[f32]) -> (Array2<f32>, Array2<f32>);
    pub fn update_weights(&self, grad_a: &Array2<f32>, grad_b: &Array2<f32>, lr_a: f32, lr_b: f32);
    pub fn parameters_flat(&self) -> Vec<f32>;
}
```

### InfoNCE (internal)
```rust
pub fn infonce_loss(
    anchors: &[Array1<f32>],
    positives: &[Array1<f32>],
    temperature: f32,
) -> Result<f32>;

pub fn infonce_gradients(
    anchors: &[Array1<f32>],
    positives: &[Array1<f32>],
    temperature: f32,
) -> Result<Vec<Array1<f32>>>;
```

## Constraints

- Pure Rust: no PyTorch/tch-rs/candle/burn. ndarray is the only linear algebra dependency.
- f32 for all embedding operations. f64 scoring boundary (crt-005) unaffected.
- L2-normalized output required (reuse `unimatrix_embed::normalize::l2_normalize`).
- Object-safe: `AdaptationService` must be `Send + Sync` for `Arc` sharing.
- Fire-and-forget training: non-blocking, bounded time (< 50ms per step).
- No schema migration: adaptation state in separate file, no new redb tables.
- No changes to `EmbeddingProvider`, `EmbedService`, `VectorStore`, or `EntryStore` traits.
- Edition 2024, MSRV 1.89, `#![forbid(unsafe_code)]`.

## Dependencies

| Crate | Version | New? | Purpose |
|-------|---------|------|---------|
| ndarray | latest stable | Yes | Matrix operations |
| rand | latest stable | Yes (to unimatrix-adapt) | Xavier init, reservoir sampling |
| bincode | 2 | No (workspace) | State persistence |
| serde + derive | 1 | No (workspace) | Serialization |
| unimatrix-store | workspace | No | Shared types (CoAccessRecord) |

## NOT in Scope

- Changes to EmbeddingProvider trait
- Changes to EmbedService trait
- Changes to HNSW index configuration
- Changes to redb schema
- GPU acceleration
- Cross-project knowledge transfer (dsn-003)
- Auto-adaptive rank selection
- Per-agent adaptation profiles
- Quantization of adapted embeddings
- External ML framework dependencies

## Alignment Status

All checks PASS. No variances requiring approval. See ALIGNMENT-REPORT.md for details.
