# crt-006: Adaptive Embedding

## Problem Statement

Unimatrix's embedding pipeline produces generic 384-dimensional vectors from a frozen ONNX model (all-MiniLM-L6-v2). These vectors capture general-purpose semantic similarity but have no awareness of any project's domain-specific usage patterns. When an architect searches for "error handling conventions," the ONNX model treats this the same way it would for any arbitrary text -- it has no knowledge that in this project, error handling conventions are semantically close to Result type patterns, logging conventions, and testing error paths, because those entries are consistently co-retrieved together.

The co-access signal from crt-004 already captures this domain knowledge: entries that are frequently retrieved together form natural clusters that reflect a project's actual conceptual topology. But this signal is only used for post-ranking boost (max +0.03 additive), not for improving the underlying embedding space. The embedding vectors themselves remain frozen in generic semantic space, so the fundamental similarity computation that drives HNSW candidate retrieval is domain-unaware.

This creates a precision ceiling: no matter how much post-ranking adjustment is applied (confidence re-ranking, co-access boosting), the initial candidate set from HNSW search is selected using generic embeddings. If two entries are domain-related but semantically dissimilar in generic embedding space (e.g., "ADR-002: additive confidence formula" and "Wilson score lower bound"), they may not appear in the same HNSW result set at all -- and no amount of post-ranking can recover candidates that were never retrieved.

This problem compounds at scale. As each project's knowledge base grows from hundreds to tens of thousands of entries, the density of the embedding space increases. Retrieving the right 5-10 results from 100K entries requires much finer discriminative power than from 200 entries. Generic embeddings produce increasingly noisy candidate sets as the knowledge base scales -- the distance between the 10th-best result and the 50th-best result shrinks, and domain-aware discrimination becomes the difference between useful and useless retrieval.

The problem is also multi-dimensional across projects. Each repository served by Unimatrix develops its own domain vocabulary, concept clusters, and retrieval patterns. A Rust systems project's notion of "similar" entries differs fundamentally from a web application's. The adaptation must be per-project, isolated, and independently evolving -- and the architecture must support managing adaptation state across many concurrent project instances.

## Goals

1. **Implement a MicroLoRA adaptation layer** that learns low-rank adjustments to frozen ONNX embeddings. The rank must be configurable (2-16, default 4) and adaptable based on knowledge base complexity. Forward pass: `output = input + scale * (input @ A @ B)` where A is the down-projection (dxr) and B is the up-projection (rxd). All computation in f32. Pure Rust implementation -- no external ML framework dependency.

2. **Implement InfoNCE contrastive loss** using co-access pairs from crt-004 as the training signal. Entries that are frequently co-retrieved get pulled closer in adapted embedding space. The training pipeline must handle scaling from tens of pairs to millions: configurable batch sizes, a negative sampling strategy that does not require the full pair set in memory, and a memory-bounded training buffer with reservoir sampling for large pair populations.

3. **Implement domain prototype adjustment** using online running-mean centroids per category/topic. Prototype management must handle growing category/topic counts across long-lived projects: bounded prototype count with merge/eviction for infrequently used prototypes, memory-efficient centroid storage, and gradual cold-start for new categories.

4. **Implement EWC++ (Elastic Weight Consolidation) regularization** to prevent catastrophic forgetting as the knowledge base evolves over months and years. The Fisher information diagonal must use online running averages (not per-task snapshots) to avoid unbounded memory growth. The regularization must remain effective across hundreds of training cycles without numerical drift.

5. **Implement episodic augmentation** as a post-search refinement stage. For result entries with high co-access affinity to anchor results, apply a small weighted interpolation with co-access partners' adapted embeddings. This is query-time only, not part of indexing.

6. **Create a new `unimatrix-adapt` crate** housing the adaptation pipeline. Clean dependency graph: depends on `unimatrix-store` (co-access data and entry metadata) but NOT on `unimatrix-embed` (frozen ONNX pipeline) or `unimatrix-vector` (HNSW index). The server orchestrates the flow.

7. **Integrate the adaptation pipeline into the server** so that all entry embedding operations (store, correct, re-embed) and query embedding operations produce adapted vectors. Queries and entries must live in the same adapted space.

8. **Update the crt-005 embedding consistency check** to compare entries against adapted re-embeddings rather than raw ONNX re-embeddings. The consistency threshold applies to the adapted vector comparison.

9. **Persist and load adaptation state** with a versioned binary format that supports forward-compatible evolution. Each project instance manages its own adaptation state file. State size must be bounded regardless of knowledge base size -- the adaptation parameters (LoRA weights, prototypes, Fisher diagonal) are fixed-size for a given rank, not proportional to entry count.

10. **Training is triggered inline during usage recording** -- the same fire-and-forget pattern used by existing pipelines. The training pathway must be non-blocking and bounded in execution time regardless of knowledge base size.

## Non-Goals

- **No PyTorch, tch-rs, candle, or burn dependency.** The adaptation pipeline uses pure Rust linear algebra. Even at higher ranks (8-16), the parameter count is modest enough for hand-written or ndarray-based operations.
- **No changes to the ONNX embedding model.** The frozen model in `unimatrix-embed` is not modified. MicroLoRA operates on the output vectors. The `EmbeddingProvider` trait is unchanged.
- **No changes to the HNSW index configuration.** `unimatrix-vector` continues using DistDot, 384 dimensions, same parameters. Adapted vectors are still 384d f32 L2-normalized.
- **No changes to the scoring pipeline precision.** The f64 scoring boundary from crt-005 is unaffected. Embedding adaptation operates in f32; scoring operates in f64.
- **No real-time online learning during search queries.** Training only occurs on the write path (co-access recording), not during search (read path).
- **No GPU acceleration.** At the parameter counts involved, CPU is sufficient. GPU would add deployment complexity with negligible benefit.
- **No full re-embedding of the knowledge base on every training step.** Existing indexed entries are re-adapted lazily or during maintenance.
- **No per-agent adaptation profiles.** Adaptation is per-project, shared across all agents within that project.
- **No cross-project knowledge transfer in this scope.** Each project's adaptation is isolated. Cross-project transfer learning is dsn-003 territory.
- **No quantization of adapted embeddings.** Adapted vectors remain f32. Quantization is a future optimization.

## Background Research

### Scale Scenarios

The architecture must be designed for these scenarios, not just current state:

| Scenario | Entries | Est. Co-Access Pairs | Est. Categories/Topics | Adaptation State Size |
|----------|---------|---------------------|----------------------|---------------------|
| Current (day 1) | ~180 | ~370 | ~10/15 | ~50KB |
| Small project (3 months) | 1K-2K | 5K-20K | 15-30 | ~80KB |
| Medium project (1 year) | 5K-15K | 50K-500K | 30-100 | ~150KB |
| Large project (multi-year) | 20K-100K | 500K-5M | 100-500 | ~300KB |
| Multi-project (10 repos) | 10x above | 10x above | Independent | 10x above |

Key insight: **Adaptation state size is O(rank * dimension + prototype_count * dimension + rank * dimension), NOT O(entries) or O(pairs).** MicroLoRA weights are fixed-size (determined by rank and dimension). Prototype centroids grow with categories/topics but are bounded by a configurable cap. Fisher diagonal matches LoRA parameter count. The only thing that grows with entry count is the training data volume -- and that is sampled, not stored.

### Co-Access Pair Scaling

Co-access pairs grow with usage, not quadratically with entry count. In practice:
- Each search returns 5-10 results, generating 10-45 pairs per search
- With dedup, actual new pairs per search average 2-5
- A busy project does 50-200 tool calls per day
- At scale: 100-1000 new pairs/day, stabilizing as the pair space saturates

At 500K pairs with 28 bytes each, the CO_ACCESS table is ~14MB -- well within redb's capability. But loading all pairs for training is wasteful. The training pipeline should sample training batches from the pair distribution, not load the full set.

Critical architectural implication: the current `top_co_access_pairs()` does a full table scan with in-memory sort. At 500K pairs, this is expensive. The training pipeline should use sampling strategies:
- **Reservoir sampling**: maintain a fixed-size buffer of recently seen pairs for training
- **Prioritized sampling**: weight sampling by pair count (more frequently co-accessed pairs are more important training signals)
- **Stratified sampling**: ensure training batches include pairs from diverse categories/topics, not just the most common

### MicroLoRA Rank Selection at Scale

The original LoRA paper (Hu et al. 2021) shows that even rank 1-4 captures most task-specific information for large language models. For 384d sentence embeddings, the information capacity is inherently lower than for LLM hidden states. However, as the knowledge base grows, the number of distinct domain-specific relationships increases:

| KB Size | Recommended Rank | Parameters | Forward Cost |
|---------|-----------------|------------|-------------|
| < 2K entries | 2-4 | 1.5K-3K | ~1.5K-3K FLOPs |
| 2K-20K entries | 4-8 | 3K-6K | ~3K-6K FLOPs |
| 20K-100K entries | 8-16 | 6K-12K | ~6K-12K FLOPs |

All of these are sub-microsecond on modern CPUs. The rank does not need to be auto-adaptive in this scope -- configurable per project is sufficient. Auto-rank selection (monitoring adaptation loss plateau and increasing rank) is a future enhancement.

LoRA+ research (Hayou et al. 2024) shows that using different learning rates for A and B matrices (with B having a higher learning rate, ratio ~16:1) significantly improves adaptation quality. This should be the default initialization strategy.

### InfoNCE at Scale

InfoNCE loss quality depends on the number of negative examples per positive pair. Research (Wu et al., IJCAI 2022 -- "Rethinking InfoNCE: How Many Negative Samples Do You Need?") shows:
- With in-batch negatives, effective negatives = batch_size - 1
- Quality improves logarithmically with negative count (diminishing returns past 64-128)
- At batch_size=64, InfoNCE performs well for embedding adaptation tasks
- At batch_size=256+, memory and compute costs increase without proportional quality gain

For Unimatrix's use case:
- **Small KB (< 2K entries):** batch_size=16-32, full pair buffer is manageable
- **Medium KB (2K-20K):** batch_size=32-64, sample from recent pairs
- **Large KB (20K+):** batch_size=64-128, reservoir sampling from pair stream

The batch size should be configurable and default to a value that works across scales (32). The training pipeline should handle the case where fewer pairs than batch_size are available (early knowledge base stages).

Numerical stability: the log-sum-exp trick is essential. Temperature tau=0.07 works well for sentence embeddings. With f32 computation, exp(sim/tau) can overflow for sim > 0.9 and tau=0.07 (exp(12.8) ~= 362,000), but the log-sum-exp trick avoids this by subtracting the max before exp.

### EWC++ at Scale: Long-Lived Continuous Learning

Standard EWC stores a Fisher diagonal snapshot per "task." For Unimatrix, tasks are continuous (every training batch), so per-task snapshots would grow without bound.

EWC++ (Chaudhry et al. 2018) uses online Fisher accumulation:
```
F_new = alpha * F_old + (1 - alpha) * F_batch
theta_star = alpha * theta_star_old + (1 - alpha) * theta_current
```

This keeps Fisher and reference parameters as single fixed-size vectors regardless of how many training batches have occurred. Alpha controls the exponential decay of old information.

At 3K parameters (rank=4), the Fisher diagonal is 3K f32 values = 12KB. At rank=16 (12K params), it is 48KB. Both are negligible.

Long-term concern: with alpha=0.95 and thousands of training steps, does the Fisher diagonal drift? The exponential averaging ensures that recent training information dominates, which is desirable -- we want the regularization to protect recent useful adaptations, not freeze the model in its day-1 state. The concern is numerical: after thousands of multiplications by 0.95, the contribution of early training is effectively zero. This is correct behavior for continuous learning.

### Domain Prototype Scaling

Prototypes are running-mean centroids per category and per topic. Current state: ~10 categories (decision, convention, pattern, etc.) and ~15 topics. At scale:

| Scenario | Categories | Topics | Prototypes | Memory |
|----------|-----------|--------|------------|--------|
| Current | ~10 | ~15 | ~25 | 25 * 384 * 4 = 38KB |
| Medium project | ~15 | ~100 | ~115 | 115 * 384 * 4 = 176KB |
| Large project | ~20 | ~500 | ~520 | 520 * 384 * 4 = 798KB |

Categories grow slowly (bounded by the allowlist). Topics can grow unboundedly as users create new knowledge areas. At 500+ prototypes, the soft-pull computation (find nearest prototype for each embedding) becomes a linear scan that is still fast (500 dot products of 384d vectors < 1ms).

However, many topics may have very few entries and unstable centroids. Design choices:
- **Minimum entry count**: require N >= 3 entries before a prototype is established (avoids noisy single-entry centroids)
- **Maximum prototype count**: cap at a configurable limit (default: 256), evicting least-recently-updated prototypes
- **Cold-start interpolation**: new categories/topics use the global centroid (mean of all entries) until enough data accumulates

### ndarray vs Alternatives at Scale

Key consideration: the matrix operations are small (384x16 at most), but they happen on every embedding (both write and read path). At high throughput (100+ embeddings/second during bulk operations), the allocation pattern matters more than peak FLOPS:

- **ndarray**: Heap-allocated, BLAS-backed for large matrices, good API for gradient computation. Overhead for small matrices is allocation, not computation. With `stack_new_axis` and views, intermediate allocations can be minimized.
- **nalgebra**: Stack-allocated small matrices, SIMD-optimized for known dimensions. Better for fixed-size small matrices. BUT: 384x16 is too large for stack allocation on many platforms.
- **Hand-written**: Zero allocation overhead if pre-allocated buffers are reused. But error-prone for gradient computation, and loses readability.

Recommendation: **ndarray with pre-allocated buffers**. Pre-allocate the output buffer and gradient accumulators at initialization time. This gives the API convenience of ndarray with the allocation efficiency of pre-allocation. The BLAS backend provides SIMD acceleration transparently.

At rank=4, the forward pass is two matmuls: [1x384] * [384x4] -> [1x4] and [1x4] * [4x384] -> [1x384]. These are ~3K FLOPs. Even without SIMD, this is < 1 microsecond. The concern is not computation speed but allocation churn during high-throughput embedding.

### Adaptation Staleness and Cold Start

When MicroLoRA weights update after training, all previously indexed embeddings become slightly stale (they were adapted with older weights). The staleness grows with the magnitude of weight updates.

Strategies at scale:
- **Lazy re-adaptation**: Re-adapt entries when they appear in search results. The search returns raw HNSW results; the server re-adapts the result entries with current weights before ranking. Cost: 5-10 MicroLoRA forward passes per search (microsecond-scale).
- **Maintenance re-adaptation**: During `context_status` with `maintain: true`, re-adapt and re-index a batch of entries (same pattern as confidence refresh). Batch size configurable (default: 100).
- **Delta tracking**: Track a "generation" counter on MicroLoRA weights. Each indexed entry records which generation it was adapted with. Entries with stale generation are prioritized for re-adaptation.

Cold start for new entries: entries added to an existing, well-adapted knowledge base should benefit from the current adaptation immediately. The pipeline applies current MicroLoRA weights at indexing time, so new entries enter the HNSW in the adapted space. No special cold-start handling needed for individual entries.

Cold start for new projects: a brand-new project has no co-access data and therefore no training signal. The adaptation pipeline returns near-identity transformations (because B is initialized near-zero). This is correct -- the pipeline adds no value and no harm until training data accumulates.

### Persistence and State Management

Each project's adaptation state must be independently persisted and versioned:

```
AdaptationState {
    version: u32,              // State format version for forward compat
    rank: u8,                  // LoRA rank used
    dimension: u16,            // Embedding dimension (384)
    scale: f32,                // LoRA scale factor
    weights_a: Vec<f32>,       // d * r values
    weights_b: Vec<f32>,       // r * d values
    fisher_diagonal: Vec<f32>, // d * r + r * d values
    reference_params: Vec<f32>,// d * r + r * d values (theta*)
    prototypes: Vec<Prototype>,// category/topic centroids
    training_generation: u64,  // Monotonic counter
    total_training_steps: u64, // Lifetime counter
    config: AdaptConfig,       // Frozen config at creation
}
```

Size analysis for rank=4, dimension=384:
- weights_a: 384*4*4 = 6,144 bytes
- weights_b: 4*384*4 = 6,144 bytes
- fisher_diagonal: 3,072*4 = 12,288 bytes
- reference_params: 3,072*4 = 12,288 bytes
- prototypes (256 max): 256 * (384*4 + metadata) ~= 400KB
- Total: ~440KB max, regardless of entry count

For rank=16: ~1.8MB. Still negligible.

Versioning: the `version` field enables forward-compatible state evolution. Unknown fields are ignored during deserialization (same pattern as EntryRecord's `serde(default)`). State file location: alongside the HNSW dump in the project's data directory.

Multi-project: each project gets its own state file. The server opens the state for the active project at startup. M7 (dsn) will manage the per-project data directory structure; crt-006 just needs to save/load from a provided path.

### Integration Architecture

The adaptation pipeline sits between embed and vector in the server's data flow:

**Write path (store/correct/re-embed):**
```
text -> unimatrix-embed (ONNX) -> raw 384d f32
     -> unimatrix-adapt (MicroLoRA + prototype) -> adapted 384d f32
     -> L2 normalize
     -> unimatrix-vector (HNSW insert)
```

**Read path (search/briefing query):**
```
query text -> unimatrix-embed (ONNX) -> raw 384d f32
           -> unimatrix-adapt (MicroLoRA + prototype only) -> adapted query f32
           -> L2 normalize
           -> unimatrix-vector (HNSW search) -> results
           -> [episodic augmentation on result embeddings, optional]
```

**Training path (inline with usage recording):**
```
co-access pairs recorded -> buffer in training reservoir
buffer >= batch_size -> sample batch from reservoir
                     -> look up raw embeddings for pair entries (from store or re-embed)
                     -> run MicroLoRA forward pass on each
                     -> compute InfoNCE loss + EWC penalty
                     -> compute gradients
                     -> update MicroLoRA weights (SGD with LoRA+ learning rates)
                     -> update prototype centroids for touched categories/topics
                     -> update Fisher diagonal (online EWC++)
                     -> increment training generation
                     -> persist state (debounced -- not every step)
```

### Existing Codebase Patterns

**Embedding pipeline** (`unimatrix-embed`): `OnnxProvider` implements `EmbeddingProvider` trait. Output is L2-normalized f32 vectors. The trait is object-safe (`Send + Sync`, `&dyn`). The adaptation layer does NOT modify this trait -- it operates on the output.

**Vector index** (`unimatrix-vector`): `VectorIndex` uses `Hnsw<f32, DistDot>`. Accepts 384d f32 vectors, validates dimension. Persistence via `dump`/`load` to disk. Adaptation state persistence should follow the same pattern (same directory, loaded alongside HNSW).

**Core traits** (`unimatrix-core`): `EmbedService` trait wraps embedding with `embed_entry(title, content) -> Vec<f32>`. The adaptation layer should integrate at the `EmbedAdapter` level or as a new `AdaptedEmbedService` that wraps `EmbedService` and applies adaptation.

**Server data flow** (`unimatrix-server`): Embeds queries via `spawn_blocking` on the `EmbedAdapter`. Searches via async wrappers on `VectorAdapter`. Co-access pairs recorded fire-and-forget. The adaptation training naturally fits the same fire-and-forget pattern.

**Co-access data** (`unimatrix-store`): `CO_ACCESS` table with `(u64, u64) -> CoAccessRecord`. `get_co_access_partners()` has a full table scan for reverse lookups (Scan 2 in `read.rs:244`). At scale, this scan becomes expensive. The training pipeline should avoid per-entry partner lookups and instead use batch-oriented sampling.

**Coherence gate** (`unimatrix-server/coherence.rs`): `embedding_consistency_score()` counts inconsistencies. The check in `contradiction.rs` re-embeds entries with raw ONNX. Must be updated to adapt after re-embedding.

## Proposed Approach

### Stage 1: Core Adaptation Layer

Build `unimatrix-adapt` crate with MicroLoRA, forward/backward passes, and configuration. Pre-allocate buffers for zero-allocation inference. Configurable rank with LoRA+ learning rate ratios.

### Stage 2: Training Pipeline

Implement InfoNCE loss with log-sum-exp stability. Training reservoir buffer with configurable capacity (default: 512 pairs). Reservoir sampling for uniform representation. Batch training with configurable batch size (default: 32). Negative sampling from within-batch entries.

### Stage 3: Regularization and Prototypes

EWC++ with online Fisher accumulation. Prototype centroids with bounded count, minimum entry threshold, and cold-start interpolation. Prototype eviction for stale topics.

### Stage 4: Episodic Augmentation

Post-search result augmentation for entries with high co-access affinity to anchor results.

### Stage 5: Integration and Persistence

Wire into server data flow. Versioned binary persistence. Embedding consistency check update. Generation tracking for adaptation staleness.

## Acceptance Criteria

- AC-01: `unimatrix-adapt` crate exists in workspace with `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89
- AC-02: `MicroLoRA` struct with configurable rank (2-16, default 4) and dimension (default 384)
- AC-03: MicroLoRA forward pass produces L2-normalized f32 output from f32 input: `output = normalize(input + scale * (input @ A @ B))`
- AC-04: MicroLoRA backward pass computes gradients with respect to A and B matrices
- AC-05: Initialization: Xavier for A, near-zero for B, ensuring near-identity output at start
- AC-06: LoRA+ learning rate ratio: B matrix learning rate = ratio * A matrix learning rate (default ratio: 16)
- AC-07: InfoNCE contrastive loss with log-sum-exp numerical stability, configurable temperature (default 0.07)
- AC-08: Training reservoir buffer with configurable capacity (default 512 pairs), reservoir sampling for uniform coverage
- AC-09: Configurable training batch size (default 32), graceful handling when fewer pairs than batch size available
- AC-10: Within-batch negative sampling -- other pairs in the batch provide negatives
- AC-11: EWC++ online Fisher diagonal with configurable decay factor (default alpha=0.95)
- AC-12: EWC penalty: `L_total = L_infonce + (lambda_ewc/2) * sum(F_i * (theta_i - theta*_i)^2)`, lambda_ewc configurable (default 0.5)
- AC-13: Prototype centroids per category and topic, bounded count (configurable max, default 256), minimum entry threshold (default 3)
- AC-14: Prototype soft pull: `alpha = 0.1 * similarity(adapted, prototype)`, applied after MicroLoRA
- AC-15: Prototype eviction: LRU eviction when prototype count exceeds maximum
- AC-16: Episodic augmentation for search result refinement (post-search, not during indexing)
- AC-17: All entry embedding operations in the server use the adaptation pipeline
- AC-18: Query embeddings adapted before HNSW search
- AC-19: Adaptation state persisted in versioned binary format alongside HNSW dump
- AC-20: State file version field for forward-compatible evolution
- AC-21: Training generation counter for staleness tracking
- AC-22: Training triggered inline during co-access pair recording (fire-and-forget)
- AC-23: Training execution time bounded regardless of knowledge base size
- AC-24: crt-005 embedding consistency check compares adapted re-embeddings, not raw ONNX
- AC-25: Pre-allocated buffers for forward pass to minimize allocation during high-throughput embedding
- AC-26: No new external ML framework dependencies
- AC-27: ndarray permitted as dependency for matrix operations
- AC-28: All new code has unit tests covering forward pass, loss computation, gradient descent, prototype management, serialization/deserialization, reservoir sampling
- AC-29: Integration tests verify end-to-end: embed -> adapt -> normalize -> index -> search with adapted vectors
- AC-30: Scale tests verify correct behavior with simulated 10K+ entry scenarios (parameter counts, memory bounds, training time bounds)
- AC-31: Existing integration tests continue to pass (no regressions across all 157 tests in 8 suites)
- AC-32: `#![forbid(unsafe_code)]` maintained in all crates
- AC-33: New `test_adaptation.py` integration test suite exercising adaptation-specific behavior through the MCP protocol
- AC-34: Adaptation state persistence verified across server restart (extension of L-12/E-13 pattern: store entries, build co-access pairs, trigger training, shutdown, restart, verify adapted search results are consistent)
- AC-35: Embedding consistency check (D-08 pattern) returns valid results when adaptation is active -- adapted re-embeddings compared, not raw ONNX re-embeddings
- AC-36: Search results with adapted embeddings return at least as relevant results as without adaptation when co-access training signal is available (adapted search quality)
- AC-37: Cold-start behavior verified: new project with zero co-access pairs produces search results equivalent to unadapted embeddings (near-identity MicroLoRA)
- AC-38: Volume suite behavior unchanged or improved with adaptation active (200-entry scale tests still pass within timeout)
- AC-39: Integration smoke tests extended to cover one critical adaptation path (minimum gate for crt-006 features)

## Integration Test Environment

### Existing Harness Overview

The integration test harness at `product/test/infra-001/` exercises the compiled `unimatrix-server` binary through the MCP JSON-RPC protocol over stdio -- the exact interface agents use. It currently has **157 tests across 8 suites**: protocol (13), tools (53), lifecycle (16), volume (11), security (15), confidence (13), contradiction (12), edge_cases (24).

The harness provides four fixtures:
- `server` (function-scoped): fresh DB per test, no state leakage
- `shared_server` (module-scoped): state accumulates, used by volume suite
- `populated_server` (function-scoped): 50 pre-loaded entries across 5 topics and 3 categories
- `admin_server` (function-scoped): server with admin agent context

### Impact Analysis: Which Suites Are Affected

crt-006 modifies every embedding operation (both write path and read path). This means **every existing test that stores, searches, or retrieves entries** now goes through the adaptation layer. The critical question is: do existing tests break or behave differently with adaptation active?

**Expected impact: none for fresh servers.** Every `server` fixture starts a fresh server with an empty database. With zero co-access pairs and zero training, the MicroLoRA weights produce near-identity transformations (B initialized near-zero). So adapted embeddings approximately equal raw ONNX embeddings. All existing function-scoped tests should pass unchanged.

**Potential impact for shared_server and populated_server:**
- `shared_server` accumulates state across a test module. If co-access pairs are generated during the module's test sequence, training could trigger mid-module. This is fine -- the tests should still pass because adaptation improves (not degrades) relevance.
- `populated_server` loads 50 entries but does not generate co-access pairs (it calls `context_store` 50 times, not `context_search`). No training signal, so adaptation is near-identity. No impact.

**Suites requiring specific attention:**

| Suite | Risk | Reason |
|-------|------|--------|
| `tools` (53) | Low | All function-scoped; no co-access pairs; adaptation is near-identity |
| `lifecycle` (16) | Medium | L-12 restart persistence must verify adaptation state persists; correction chains re-embed entries |
| `contradiction` (12) | Medium | D-08 embedding consistency check must use adapted re-embeddings |
| `confidence` (13) | Low | Confidence scoring is independent of embedding pipeline; search re-ranking uses HNSW results which are near-identical for fresh servers |
| `volume` (11) | Medium | 200 entries stored + 100 sequential searches on shared_server; co-access pairs WILL accumulate; training MAY trigger mid-suite; search accuracy assertions should still hold |
| `edge_cases` (24) | Low | E-13 restart persistence (same concern as L-12); rest are function-scoped |
| `protocol` (13) | None | MCP protocol mechanics, no embedding involvement |
| `security` (15) | None | Content scanning and capability checks, no embedding involvement |

### New Suite: `test_adaptation.py`

The adaptation pipeline introduces concepts with no home in existing suites: LoRA weight training, reservoir sampling, prototype management, EWC regularization, episodic augmentation, training generation tracking, and adaptation state persistence. A dedicated suite is warranted.

**Proposed tests for `test_adaptation.py`:**

1. **A-01: Cold-start search equivalence.** Store 10 entries, search without any co-access history. Verify results match expected semantic ordering (adaptation is near-identity).

2. **A-02: Adaptation state persists across restart.** Store entries, generate co-access pairs (via search), trigger training (enough pairs to fill batch), shutdown server, restart with same project dir, verify adaptation state loaded (search results should be consistent pre/post restart).

3. **A-03: Co-access training improves retrieval.** Store entries across 3-4 topics. Search repeatedly to generate co-access pairs within topics. After enough searches for training to trigger, verify that searches for topic-specific queries return entries from the correct topic more prominently. This is the core value proposition test.

4. **A-04: Embedding consistency check with adaptation.** Store entries, generate co-access pairs, trigger training. Run `context_status` with `check_embeddings=True`. Verify the consistency check completes and uses adapted re-embeddings (not raw ONNX). The consistency score should be valid (adapted embeddings should be self-consistent since re-embedding + re-adapting with same weights produces the same vector).

5. **A-05: Volume behavior with adaptation active.** Store 100+ entries with diverse topics, generate co-access signal through 50+ searches, verify search accuracy and status report completion. Must complete within volume suite timeout (120s).

6. **A-06: Adaptation does not crash on edge cases.** Unicode content, single-entry DB, empty search, minimal content -- all with adaptation layer active. Since adaptation is applied to every embedding, these edge cases from the existing edge_cases suite must also work through the adaptation path. This test specifically verifies 5-10 known edge cases with adaptation active after training has occurred.

7. **A-07: Training is non-blocking.** Store entries rapidly (100 sequential stores) while co-access pairs accumulate. Verify that tool responses are not delayed by training (response times stay within acceptable bounds -- no more than 2x baseline for store/search operations).

8. **A-08: Prototype management under diverse topics.** Store entries across 20+ distinct topics, each with 3+ entries. Verify that the server handles the prototype growth without errors or significant performance degradation.

9. **A-09: Correction chain with adaptation.** Store entry, correct it 3 times (creating a correction chain). Each correction re-embeds through the adaptation layer. Verify the final entry is searchable and the correction chain is intact.

10. **A-10: Status report with adaptation metadata.** After training has occurred, `context_status` should include adaptation-related information (training generation, adaptation state presence). This verifies the server exposes adaptation health through existing tools.

### Existing Test Extensions

Some existing tests need minor extensions rather than duplication:

**L-12 / E-13 (restart persistence):** The existing restart persistence tests verify that stored entries survive restart. crt-006 adds a dimension: adaptation state must also persist. Rather than modifying these tests (which validate pre-existing behavior), the new A-02 test covers adaptation persistence specifically.

**D-08 (embedding consistency check):** The existing test calls `context_status` with `check_embeddings=True` and verifies success. With crt-006, the consistency check must compare adapted re-embeddings. The existing D-08 test should continue to pass (the server internally switches to adapted comparison). The new A-04 test adds specific validation that the adapted consistency check is meaningful.

### Fixture Considerations

The new `test_adaptation.py` suite needs a fixture that has co-access training signal available. Options:

- **Extend `populated_server`:** Currently stores 50 entries but no searches. Could add searches to generate co-access pairs. However, modifying this fixture affects all tests that use it.
- **New `trained_server` fixture (recommended):** A function-scoped fixture that stores entries, performs searches to generate co-access pairs, and waits for at least one training batch to complete. This keeps the new fixture isolated from existing tests.

The `trained_server` fixture would:
1. Store 30+ entries across 5 topics
2. Perform 20+ topic-specific searches (generating co-access pairs within topics)
3. Yield the client for test use

The fixture does NOT need to verify training has occurred -- it just needs to generate enough signal. If training has not triggered yet (insufficient pairs), the tests must handle that gracefully (cold-start tests should pass either way).

### Integration with Feature Mapping Table

The USAGE-PROTOCOL.md feature mapping table should be extended to include crt-006:

```
| Adaptation (embedding layer) | `adaptation`, `tools`, `lifecycle`, `contradiction`, `volume` |
```

**Minimum gate requirement:** `pytest -m smoke` must pass. The smoke marker should be added to A-01 (cold-start equivalence) as the minimum crt-006 integration gate.

### Client API Considerations

The `UnimatrixClient` in `harness/client.py` already supports `context_status` with `check_embeddings=True`. No new client methods are required for the adaptation integration tests. The adaptation layer is transparent to the MCP protocol -- it operates server-side between embed and index. All existing client methods work unchanged.

If the status report gains adaptation-specific fields (training generation, adaptation state presence), the `parse_status_report` assertion helper may need extension to extract these new fields. This is a minor change to the harness, not a new tool or protocol change.

## Constraints

- **Pure Rust, no ML frameworks.** No PyTorch, tch-rs, candle, or burn. The parameter counts (3K-12K depending on rank) make this feasible. ndarray provides matrix operations; no BLAS backend required at this scale but transparent BLAS acceleration is available if needed.
- **f32 for embeddings, f64 for scoring.** Adapted embeddings remain f32. The f64 scoring boundary from crt-005 is unaffected.
- **L2-normalized output required.** HNSW uses DistDot on unit vectors. All adapted vectors must be L2-normalized. Reuse `l2_normalize` from `unimatrix-embed`.
- **Object-safe traits.** Any new traits must be object-safe for `Arc<dyn ...>` usage.
- **Fire-and-forget training.** Training must not block tool responses. Same `spawn_blocking` pattern as usage recording. Training time must be bounded (no full-table scans, no loading all pairs).
- **No schema migration.** Adaptation state lives in a separate file, not in redb tables. No new redb tables, no EntryRecord changes.
- **Bounded memory.** Adaptation state size is O(rank * dimension + prototype_cap * dimension), not O(entries) or O(pairs). Training buffer is bounded by reservoir capacity.
- **Embedding consistency coordination required.** The crt-005 coherence gate must compare adapted re-embeddings. This is not optional.
- **Test infrastructure is cumulative.** Build on existing test fixtures. New `test_adaptation.py` suite extends the harness; existing suites must continue passing.
- **Edition 2024, MSRV 1.89, `#![forbid(unsafe_code)]`.**
- **No changes to `EmbeddingProvider` trait.** Adaptation operates downstream of ONNX output.
- **Per-project isolation.** Adaptation state is per-project. No cross-project state sharing in this scope (that is dsn-003).
- **State persistence versioning.** The binary state format must include a version field for forward-compatible evolution. New fields can be added without breaking old state files.

## Open Questions

1. **ndarray vs hand-written matrix ops?** At rank 4-16 and dimension 384, ndarray with pre-allocated buffers provides the best balance of safety, performance, and readability. The BLAS backend activates transparently for larger matrices. Gradient computation in particular benefits from ndarray's broadcasting and view semantics. Recommend ndarray.

2. **Adaptation state file location?** Alongside the HNSW dump (option a). Both are vector-related persistent state, both are loaded at startup, both are per-project. This keeps the persistence contract simple: "everything in the data directory belongs together." The data directory path is the same one the server already manages.

3. **Re-adaptation strategy for existing entries?** Maintenance re-adaptation via `context_status` with `maintain: true`, batched like confidence refresh (100 entries per call). Additionally, lazy re-adaptation of search results at query time (re-adapt the 5-10 result entries with current weights before ranking -- microsecond cost). Both strategies together handle both background drift correction and real-time accuracy.

4. **Should episodic augmentation apply during indexing?** No. Episodic augmentation requires knowing the query context, which is only available at search time. Indexing uses MicroLoRA + prototype adjustment only. Episodic augmentation is a post-search result refinement step.

5. **Interaction with existing co-access boost.** Keep the +0.03 co-access boost unchanged initially. The adaptation pulls co-accessed entries closer in embedding space (improving HNSW recall), while the boost adjusts final ranking (improving precision). These are complementary mechanisms operating at different stages. If empirical evaluation shows double-counting, the boost weight can be reduced in a future iteration.

6. **Training data persistence across restarts?** The training reservoir buffer is in-memory and is lost on restart. This is acceptable -- the reservoir repopulates naturally as tool calls resume. The learned adaptation (LoRA weights, Fisher diagonal, prototypes) IS persisted and survives restarts. The concern is not data loss but convergence speed after restart, which is manageable since the weights carry the accumulated learning.

7. **Configurable vs auto-adaptive rank?** Start with configurable (per-project setting, default 4). Auto-rank selection (monitoring loss plateau, automatically increasing rank) is a meaningful enhancement but adds complexity. Recommend configuring rank as part of project setup, with documentation on when to increase it.

## Tracking

https://github.com/dug-21/unimatrix/issues/48
