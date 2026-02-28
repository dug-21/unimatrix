# Specification: crt-006 Adaptive Embedding

## Objective

Implement a 4-stage embedding adaptation pipeline (MicroLoRA, Prototype Adjustment, Episodic Augmentation, L2 Normalization) that learns domain-specific adjustments to frozen ONNX embeddings using co-access pair signals from crt-004. The pipeline sits between the embedding generation and vector indexing stages, improving retrieval precision as each project's knowledge base grows.

## Functional Requirements

### FR-01: MicroLoRA Forward Pass
The MicroLoRA engine accepts a 384d f32 input vector and produces a 384d f32 output vector via the formula `output = input + scale * (input @ A @ B)` where A is the down-projection matrix (384 x rank) and B is the up-projection matrix (rank x 384).

### FR-02: MicroLoRA Backward Pass
The MicroLoRA engine computes gradients of the loss with respect to matrices A and B, supporting SGD weight updates with separate learning rates per matrix (LoRA+).

### FR-03: MicroLoRA Initialization
Matrix A is initialized with Xavier normal distribution. Matrix B is initialized near-zero (scale 1e-4) so that the initial forward pass produces near-identity output (output approximately equals input).

### FR-04: InfoNCE Loss Computation
Given a batch of positive co-access pairs and within-batch negatives, compute contrastive loss with configurable temperature. Implementation uses log-sum-exp trick for numerical stability.

### FR-05: Training Reservoir
A fixed-capacity buffer accumulates co-access pairs via reservoir sampling. When the buffer reaches the configured batch size, a training step can be triggered. The reservoir provides uniform sampling over all observed pairs regardless of total pair count.

### FR-06: Batch Training Step
A training step samples a batch from the reservoir, computes forward passes for all entries in the batch, computes InfoNCE loss plus EWC++ penalty, computes gradients, and updates weights atomically.

### FR-07: EWC++ Online Regularization
The Fisher information diagonal and reference parameters are maintained as running exponential averages with configurable decay factor. The EWC penalty term is added to the total loss: `L_total = L_infonce + (lambda/2) * sum(F_i * (theta_i - theta*_i)^2)`.

### FR-08: Domain Prototypes
Online running-mean centroids are maintained per category and per topic. When adapting an embedding with known category/topic, a soft pull toward the nearest prototype is applied: `adjusted = adapted + alpha * (prototype - adapted)` where `alpha = pull_strength * cosine_sim(adapted, prototype)`.

### FR-09: Prototype Bounds
The prototype count is bounded by a configurable maximum (default 256). When the count exceeds the maximum, the least-recently-updated prototype is evicted. New categories/topics require a minimum entry count (default 3) before a prototype is established.

### FR-10: Episodic Augmentation
After search results are retrieved, entries with high co-access affinity to anchor results receive a small similarity score adjustment. This is query-time only and does not modify indexed vectors.

### FR-11: Write Path Integration
Every entry embedding operation in the server (context_store, context_correct, re-embedding during maintenance) passes through the adaptation pipeline between raw ONNX embedding and HNSW insertion.

### FR-12: Read Path Integration
Every query embedding operation in the server (context_search, context_briefing) passes through the adaptation pipeline between raw ONNX embedding and HNSW search.

### FR-13: Training Trigger
Training is triggered inline during co-access pair recording, using the same fire-and-forget pattern as existing usage recording. The training step is non-blocking.

### FR-14: Adaptation State Persistence
The full adaptation state (weights, Fisher diagonal, prototypes, configuration, metadata) is saved to a versioned binary file on shutdown and during debounced maintenance saves. The state is loaded at startup.

### FR-15: Graceful Degradation
If adaptation state is missing or corrupt at startup, the server starts with fresh identity adaptation (near-zero B matrix). Existing indexed vectors may be inconsistent, but the system remains functional.

### FR-16: Embedding Consistency Update
The crt-005 embedding consistency check compares entries against adapted re-embeddings (not raw ONNX re-embeddings). The check re-embeds via ONNX, then applies the current adaptation before comparing.

### FR-17: Training Generation Tracking
A monotonic generation counter increments on each successful training step. This counter is persisted with the adaptation state and is available via `context_status`.

## Non-Functional Requirements

### NFR-01: Forward Pass Latency
The adaptation forward pass (MicroLoRA + prototype pull) must complete in under 10 microseconds for rank <= 8 on a single core. No heap allocation during forward pass (pre-allocated buffers).

### NFR-02: Training Step Bounded Time
A single training step must complete within 50 milliseconds regardless of knowledge base size. The reservoir sampling ensures bounded input size.

### NFR-03: Adaptation State Size
Total adaptation state size must be O(rank * dimension + prototype_cap * dimension), not O(entries) or O(pairs). At rank=4, dimension=384, max prototypes=256: under 500KB.

### NFR-04: Memory-Bounded Training Buffer
The training reservoir has a configurable fixed capacity (default 512 pairs). Memory usage is bounded regardless of total co-access pair count.

### NFR-05: No External ML Frameworks
No PyTorch, tch-rs, candle, burn, or similar ML framework dependencies. ndarray is the only linear algebra dependency.

### NFR-06: f32 Embedding Precision
All adaptation computation uses f32, consistent with the ONNX embedding output and HNSW DistDot input. The f64 scoring boundary from crt-005 is unaffected.

### NFR-07: Thread Safety
`AdaptationService` must be `Send + Sync` for `Arc<AdaptationService>` usage in the async server.

### NFR-08: Edition 2024, MSRV 1.89
The new crate follows workspace conventions: edition 2024, rust-version 1.89, `#![forbid(unsafe_code)]`.

## Acceptance Criteria

| AC-ID | Description | Verification Method |
|-------|-------------|-------------------|
| AC-01 | `unimatrix-adapt` crate exists in workspace with `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89 | file-check: `crates/unimatrix-adapt/Cargo.toml` + grep for `forbid(unsafe_code)` |
| AC-02 | `MicroLoRA` struct with configurable rank (2-16, default 4) and dimension (default 384) | test: unit test validates configuration |
| AC-03 | MicroLoRA forward pass: `output = normalize(input + scale * (input @ A @ B))` | test: unit test with known inputs/outputs |
| AC-04 | MicroLoRA backward pass computes gradients for A and B | test: gradient correctness vs finite-difference approximation |
| AC-05 | Initialization: Xavier for A, near-zero for B | test: output approximately equals input on fresh model |
| AC-06 | LoRA+ learning rate ratio (default 16) | test: B matrix lr = ratio * A matrix lr after config |
| AC-07 | InfoNCE loss with log-sum-exp stability, temperature default 0.07 | test: loss computation matches reference, no NaN for extreme inputs |
| AC-08 | Training reservoir with configurable capacity (default 512), reservoir sampling | test: uniform sampling verified statistically over large input |
| AC-09 | Configurable batch size (default 32), graceful when fewer pairs available | test: partial batch training succeeds |
| AC-10 | Within-batch negative sampling | test: negatives sourced from other batch entries |
| AC-11 | EWC++ online Fisher diagonal, decay factor (default 0.95) | test: Fisher update follows exponential average formula |
| AC-12 | EWC penalty computation with lambda_ewc (default 0.5) | test: penalty value matches hand-computed expectation |
| AC-13 | Prototype centroids bounded (default max 256), minimum entry threshold (default 3) | test: prototypes not created below threshold, eviction at max |
| AC-14 | Prototype soft pull formula applied after MicroLoRA | test: output differs from MicroLoRA-only output in prototype direction |
| AC-15 | Prototype LRU eviction when count exceeds maximum | test: LRU eviction verified |
| AC-16 | Episodic augmentation for search result refinement | test: results with co-access affinity get score adjustment |
| AC-17 | All entry embedding operations use adaptation pipeline | test: integration test verifies store -> adapt -> index flow |
| AC-18 | Query embeddings adapted before HNSW search | test: integration test verifies query adaptation |
| AC-19 | Adaptation state persisted in versioned binary alongside HNSW dump | test: state file exists after shutdown |
| AC-20 | State file version field for forward-compatible evolution | test: load state with unknown fields succeeds via serde(default) |
| AC-21 | Training generation counter for staleness tracking | test: generation increments on successful training step |
| AC-22 | Training triggered inline during co-access recording | test: integration test verifies training fires after pair accumulation |
| AC-23 | Training time bounded regardless of KB size | test: training step < 50ms with full reservoir |
| AC-24 | Embedding consistency check uses adapted re-embeddings | test: consistency check with active adaptation returns valid score |
| AC-25 | Pre-allocated buffers for forward pass | test: no allocation observed during forward pass (benchmark) |
| AC-26 | No external ML framework dependencies | grep: Cargo.toml excludes torch, candle, burn |
| AC-27 | ndarray as dependency | file-check: Cargo.toml contains ndarray |
| AC-28 | Unit tests for all components | test: cargo test --lib passes in unimatrix-adapt |
| AC-29 | Integration tests for end-to-end flow | test: integration suite passes |
| AC-30 | Scale tests for 10K+ simulated entries | test: parameter counts and memory within bounds |
| AC-31 | Existing tests pass (no regressions) | test: full cargo test + integration suite pass |
| AC-32 | `#![forbid(unsafe_code)]` in all crates | grep: all lib.rs files |
| AC-33 | New test_adaptation.py integration suite | file-check: suite exists with tests |
| AC-34 | Adaptation state persistence across restart | test: integration test A-02 |
| AC-35 | Embedding consistency with adaptation active | test: integration test A-04 |
| AC-36 | Adapted search quality verification | test: integration test A-03 |
| AC-37 | Cold-start near-identity behavior | test: integration test A-01 |
| AC-38 | Volume suite unchanged with adaptation | test: volume suite passes |
| AC-39 | Smoke test covers adaptation path | test: A-01 marked as smoke |

## Domain Models

### Key Entities

- **AdaptationService**: Top-level orchestrator owning all adaptation state. One per server instance, shared via `Arc`.
- **MicroLoRA**: Low-rank weight matrices and forward/backward pass logic. Rank and dimension are fixed at creation.
- **TrainingReservoir**: Bounded buffer of co-access pairs for training. Uses reservoir sampling for uniform coverage.
- **EwcState**: Fisher information diagonal and reference parameters for regularization. Updated online after each training step.
- **PrototypeManager**: Collection of category/topic centroids with bounded count and LRU eviction.
- **AdaptationState**: Serializable snapshot of all learnable parameters and metadata. Persisted to disk.
- **AdaptConfig**: Immutable configuration (rank, dimension, scale, learning rates, temperatures, capacities, thresholds).

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| Raw embedding | 384d f32 vector produced by the frozen ONNX model, L2-normalized |
| Adapted embedding | 384d f32 vector after MicroLoRA + prototype adjustment, L2-normalized |
| Forward pass | MicroLoRA transformation: input + scale * (input @ A @ B) |
| Training step | One complete cycle: sample batch, compute loss, update weights |
| Training generation | Monotonic counter incremented on each successful training step |
| Reservoir sampling | Algorithm maintaining a uniform random sample of fixed size from a stream |
| Fisher diagonal | Diagonal approximation of the Fisher information matrix, used by EWC++ |
| Prototype | Running-mean centroid of adapted embeddings for a category or topic |
| Soft pull | Small adjustment pulling an adapted embedding toward its nearest prototype |
| Near-identity | MicroLoRA output approximately equals input (when B is near-zero) |
| Adaptation staleness | Indexed entries adapted with an older training generation than current |

## User Workflows

### Workflow 1: Normal Operation (Agent Perspective)
Agents interact with Unimatrix identically to before crt-006. The adaptation is transparent:
1. Agent calls `context_store(content, topic, category)` -- embedding is adapted before indexing
2. Agent calls `context_search(query)` -- query is adapted before HNSW search
3. Agent calls `context_status(check_embeddings=True)` -- consistency check uses adapted embeddings

### Workflow 2: Training Accumulation (Automatic)
1. As agents search, co-access pairs are generated and recorded (existing crt-004 flow)
2. Pairs are simultaneously added to the training reservoir
3. When the reservoir reaches batch size, a training step fires (fire-and-forget)
4. LoRA weights improve incrementally, improving subsequent search quality

### Workflow 3: Maintenance (Operator Perspective)
1. Operator (or agent) calls `context_status(maintain=True)`
2. Confidence refresh runs (existing)
3. Graph compaction runs if stale ratio exceeds threshold (existing)
4. Adaptation re-indexing: batch of stale-generation entries are re-adapted and re-indexed

### Workflow 4: Server Restart
1. Server loads redb, HNSW index, and adaptation state from disk
2. If adaptation state is missing/corrupt, fresh identity state is created
3. Training reservoir starts empty (repopulates via normal usage)
4. Learned weights carry forward -- search quality is preserved

## Constraints

- Pure Rust, no ML frameworks (ndarray permitted)
- f32 for embeddings, f64 for scoring (crt-005 boundary unchanged)
- L2-normalized output required (HNSW DistDot)
- Object-safe traits for `Arc<dyn ...>` usage
- Fire-and-forget training (non-blocking)
- No schema migration, no new redb tables
- Bounded memory: O(rank * dimension + prototype_cap * dimension)
- Edition 2024, MSRV 1.89, `#![forbid(unsafe_code)]`
- `EmbeddingProvider` trait unchanged
- Per-project isolation

## Dependencies

| Dependency | Source | Purpose |
|-----------|--------|---------|
| ndarray | crates.io | Matrix operations for LoRA forward/backward |
| bincode 2 | workspace | State serialization (existing workspace dep) |
| serde + derive | workspace | Serialization traits (existing workspace dep) |
| rand | crates.io | Xavier initialization, reservoir sampling |
| unimatrix-store | workspace | CoAccessRecord type, shared data types |

## NOT in Scope

- Changes to `EmbeddingProvider` trait
- Changes to `EmbedService` trait
- Changes to HNSW index configuration
- Changes to redb schema
- GPU acceleration
- Cross-project knowledge transfer
- Auto-adaptive rank selection
- Per-agent adaptation profiles
- Quantization of adapted embeddings
- PyTorch/tch-rs/candle/burn dependencies
