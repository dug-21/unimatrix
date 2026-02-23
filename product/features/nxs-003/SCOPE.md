# nxs-003: Embedding Pipeline

## Problem Statement

Unimatrix has a storage engine (nxs-001) that persists entries with metadata, and a vector index (nxs-002) that provides similarity search over pre-computed 384-dimensional embeddings using hnsw_rs with DistDot. However, there is no component that converts text into those embeddings. The `VectorIndex::insert(entry_id, embedding)` API requires a pre-computed `Vec<f32>`, but nothing in the system produces one.

Without an embedding pipeline, the write path is broken: entries can be stored but never made searchable via semantic similarity. The `context_search` tool (vnc-002) cannot function, and the core value proposition of Unimatrix -- delivering contextually relevant knowledge to agents -- remains unrealized.

nxs-003 bridges content authoring and vector search by providing local embedding generation via ONNX runtime with a pre-configured catalog of 384-d sentence-transformer models (default: all-MiniLM-L6-v2). The pipeline is built directly on `ort` + `tokenizers` (no fastembed wrapper) for full control over tokenization, pooling, normalization, and model lifecycle.

## Goals

1. Create a `unimatrix-embed` library crate that generates 384-dimensional normalized embeddings from text input, compatible with nxs-002's `VectorIndex` (DistDot metric).
2. Provide local embedding generation via raw `ort` (ONNX Runtime) + `tokenizers` (HuggingFace), with custom tokenization, mean pooling, and L2 normalization. No fastembed dependency. Default model: all-MiniLM-L6-v2.
3. Implement title+content concatenation strategy: embeddings are generated from `"{title}: {content}"` with configurable separator and truncation to the model's 256 word-piece token limit.
4. Provide batch embedding: accept multiple texts and process them efficiently in configurable batch sizes, using ONNX Runtime's native batched inference.
5. Provide a single-entry convenience API: `embed_entry(title, content) -> Vec<f32>` that concatenates, tokenizes, runs inference, and normalizes in one call.
6. Support a pre-configured catalog of 384-d sentence-transformer models, selectable at initialization: AllMiniLmL6V2 (default), AllMiniLmL12V2, MultiQaMiniLmL6, ParaphraseMiniLmL6V2, BgeSmallEnV15, E5SmallV2, GteSmall. Model is fixed for the lifetime of the provider instance.
7. Define an `EmbeddingProvider` trait that abstracts over provider implementations, enabling downstream consumers (vnc-001, vnc-002) to use any provider without code changes.
8. Manage ONNX model lifecycle: automatic download on first use (from HuggingFace Hub via `hf-hub` crate), local caching in `~/.cache/unimatrix/models/` (cross-platform via `dirs` crate), and model validation on load.
9. Normalize all output embeddings to unit length (L2 norm = 1.0) to ensure compatibility with nxs-002's DistDot metric, which requires pre-normalized vectors.
10. Expose a synchronous Rust API matching the nxs-001/nxs-002 pattern, suitable for wrapping with `spawn_blocking` in downstream async consumers.

## Non-Goals

- **No vector index operations.** This crate produces `Vec<f32>` embeddings. Inserting them into the hnsw_rs index is the caller's responsibility (via `VectorIndex::insert`).
- **No MCP tool exposure.** MCP tools (`context_store`, `context_search`) are vnc-001/vnc-002. This crate provides the embedding primitive.
- **No async API.** Matches nxs-001 and nxs-002's synchronous pattern. Async wrapping is the consumer's responsibility.
- **No custom model training or fine-tuning.** The crate uses a pre-trained ONNX model as-is. Fine-tuning is out of scope for the Nexus phase.
- **No GPU inference.** The default path uses CPU-only ONNX Runtime. GPU execution providers (CUDA, Metal) may be added as feature flags in a future iteration but are not part of this feature.
- **No streaming inference.** Embedding generation is a batch-in, batch-out operation. There is no streaming/incremental API.
- **No tokenizer training or vocabulary modification.** The crate uses the model's pre-trained tokenizer and vocabulary as-is.
- **No embedding storage.** Embeddings are ephemeral outputs. Persistence is handled by nxs-002 (in-memory hnsw_rs index + VECTOR_MAP in redb). If embeddings need to be recomputed (e.g., index rebuild), this crate re-generates them from source text.
- **No content preprocessing beyond concatenation.** No summarization, no keyword extraction, no chunking. The input is title+content; the output is one embedding per entry. Content chunking for large documents is a future concern.
- **No runtime model switching.** The model is configured at initialization and fixed for the lifetime of the provider instance. Switching models requires creating a new provider and rebuilding the vector index.
- **No API-based embedding providers.** The crate is local-first. API fallback (OpenAI, etc.) is deferred to a future feature if needed. All models run locally via ONNX Runtime.

## Background Research

### Prior Spike Research

**ASS-001 (hnsw_rs Capability Spike)** established the 384-dimension, DistDot requirement:
- all-MiniLM-L6-v2 produces 384-dimensional embeddings, compatible with hnsw_rs.
- DistDot requires L2-normalized vectors for correct similarity scoring (similarity = 1.0 - distance).
- Memory at 384d: ~1.8 MB / 1K entries, ~18 MB / 10K, ~183 MB / 100K.

**ASS-005 (Learning Model Assessment)** confirmed the metadata lifecycle approach:
- No custom neural networks needed for learning. Pre-trained embeddings (all-MiniLM-L6-v2) plus metadata formulas cover 95% of learning value.
- The embedding pipeline is a "use a pre-trained model" component, not a "train a custom model" component.

**nxs-002 (Vector Index)** defines the exact integration contract:
- `VectorIndex::insert(entry_id: u64, embedding: &[f32]) -> Result<()>` -- validates dimension == 384, validates no NaN/infinity.
- The caller (nxs-003 or higher-level code) computes the embedding, then passes it to nxs-002.
- nxs-002's `VectorConfig.dimension` is 384.

### Technical Landscape: ONNX Embedding in Rust

**ort crate (v2.0.0-rc.11)**: The Rust wrapper for ONNX Runtime 1.23.
- Production-ready, RC stability confirmed by ruvector production usage (rc.9/10).
- Provides Session-based inference API with `GraphOptimizationLevel::Level3`.
- Supports CPU execution provider by default, with optional GPU providers behind feature flags.
- Tensor creation via `Tensor::from_array((shape, data))`.

**tokenizers crate (HuggingFace)**: Rust tokenizer library.
- Loads `tokenizer.json` from model directory.
- Handles word-piece tokenization, padding, truncation.
- Produces `input_ids`, `attention_mask`, `token_type_ids` for ONNX input.

**hf-hub crate**: Model downloading from HuggingFace Hub.
- Handles authenticated and anonymous downloads.
- File-level caching with revision tracking.

**Decision: Use raw `ort` + `tokenizers` + `hf-hub` rather than fastembed-rs.** fastembed wraps these same crates but adds a dependency layer that risks edition 2024 compatibility issues and limits control over tokenization, pooling, and model lifecycle. The ruvector project (85+ crate Rust vector database) validates the raw approach in production -- their ONNX embedding pipeline uses the same stack with ~800 lines of code for tokenization, pooling, normalization, and model management. This gives us full control and avoids fastembed's exact-pinned `ort` version constraint.

### ruvector Prior Art

Research from ruvector's ONNX embedding implementation (see RESEARCH-ruvector.md):
- Validates raw `ort` + `tokenizers` approach in production Rust.
- Uses custom cache directory via `dirs` crate (not HuggingFace default).
- Pre-configured catalog of 8 models (7 at 384-d), all compatible with our VectorIndex.
- Mean pooling with attention mask, followed by L2 normalization.
- Configurable batch size (default 32), texts chunked into batches.
- `thiserror` 2.0 for typed error handling.
- ONNX session requires `&mut self` for inference (no concurrent inference on single session).

### Title+Content Concatenation Strategy

The all-MiniLM-L6-v2 model has a 256 word-piece token limit. For development knowledge entries:
- Title provides high-signal topic context (e.g., "JWT token validation convention").
- Content provides detailed information.
- Concatenation format: `"{title}: {content}"` -- colon separator matches common document titling conventions.
- Truncation: If the concatenated text exceeds the token limit, the tokenizer truncates from the end. This means very long content entries lose their tail, but title (high-signal) is preserved.
- For typical Unimatrix entries (title: 5-15 words, content: 50-200 words), the full text fits within the 256 token limit.

### Existing Crate Integration Surface

**unimatrix-store** (nxs-001):
- `EntryRecord { title, content, ... }` -- source text fields.
- `EntryRecord.embedding_dim: u16` -- set to 384 by nxs-002 on vector insertion.
- `Store::get(entry_id) -> Result<EntryRecord>` -- for retrieving text to embed.

**unimatrix-vector** (nxs-002):
- `VectorIndex::insert(entry_id: u64, embedding: &[f32]) -> Result<()>` -- the downstream consumer of embeddings.
- `VectorConfig { dimension: 384, ... }` -- dimension contract.
- Validates: dimension == 384, no NaN, no infinity.

### Crate Workspace Context

The Cargo workspace uses `edition = "2024"`, `rust-version = "1.89"`, `resolver = "3"`. The new crate will live at `crates/unimatrix-embed/`. It depends on `ort` for ONNX inference, `tokenizers` for text tokenization, `hf-hub` for model downloading, and `dirs` for cross-platform cache paths.

## Proposed Approach

### Crate Structure

Create a `unimatrix-embed` library crate within the existing Cargo workspace:

```
crates/unimatrix-embed/
  Cargo.toml
  src/
    lib.rs         -- Public re-exports, crate-level #![forbid(unsafe_code)]
    provider.rs    -- EmbeddingProvider trait definition
    model.rs       -- EmbeddingModel enum (pre-configured model catalog)
    onnx.rs        -- OnnxProvider (raw ort + tokenizers)
    pooling.rs     -- Mean pooling with attention mask
    config.rs      -- EmbedConfig, model paths, batch settings
    error.rs       -- EmbedError enum (thiserror 2.0)
    normalize.rs   -- L2 normalization utility
    text.rs        -- Title+content concatenation, truncation helpers
    download.rs    -- Model download + cache management (hf-hub + dirs)
```

### EmbeddingProvider Trait

```rust
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text string.
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed multiple texts in a batch.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// The output embedding dimension.
    fn dimension(&self) -> usize;

    /// The model name for identification.
    fn name(&self) -> &str;
}
```

### EmbeddingModel Catalog

Pre-configured 384-d sentence-transformer models, all compatible with nxs-002's VectorIndex:

| Variant | HuggingFace ID | Dim | Max Seq |
|---------|---------------|-----|---------|
| AllMiniLmL6V2 (default) | sentence-transformers/all-MiniLM-L6-v2 | 384 | 256 |
| AllMiniLmL12V2 | sentence-transformers/all-MiniLM-L12-v2 | 384 | 256 |
| MultiQaMiniLmL6 | sentence-transformers/multi-qa-MiniLM-L6-cos-v1 | 384 | 256 |
| ParaphraseMiniLmL6V2 | sentence-transformers/paraphrase-MiniLM-L6-v2 | 384 | 256 |
| BgeSmallEnV15 | BAAI/bge-small-en-v1.5 | 384 | 512 |
| E5SmallV2 | intfloat/e5-small-v2 | 384 | 512 |
| GteSmall | thenlper/gte-small | 384 | 512 |

Each variant encodes: HuggingFace model ID, dimension, max sequence length, and ONNX filename.

### OnnxProvider (Default Path)

Built on raw `ort` + `tokenizers`:
- Loads ONNX model via `ort::Session` with `GraphOptimizationLevel::Level3`.
- Tokenizes input via `tokenizers::Tokenizer` loaded from model's `tokenizer.json`.
- Mean pooling with attention mask applied after ONNX inference.
- L2 normalization applied after pooling.
- Model downloaded automatically on first use via `hf-hub`.
- Cached in `~/.cache/unimatrix/models/` (cross-platform via `dirs` crate).
- Batch size configurable (default: 32).
- Output is L2-normalized 384-d vectors.

### Convenience Functions

```rust
/// Concatenate title and content with the configured separator.
pub fn prepare_text(title: &str, content: &str, separator: &str) -> String;

/// Embed a single entry's text (title + content).
pub fn embed_entry(provider: &dyn EmbeddingProvider, title: &str, content: &str) -> Result<Vec<f32>>;

/// Embed a batch of entries.
pub fn embed_entries(provider: &dyn EmbeddingProvider, entries: &[(String, String)]) -> Result<Vec<Vec<f32>>>;
```

### Integration Flow

The full write path after nxs-003:

```
Caller (vnc-002 context_store)
  |
  | "Store this knowledge entry"
  |
  v
  Store::insert(NewEntry { title, content, ... })  -->  entry_id
  |
  | title + content
  v
  EmbeddingProvider::embed(prepare_text(title, content))  -->  Vec<f32> (384-d)
  |
  v
  VectorIndex::insert(entry_id, &embedding)  -->  hnsw_rs + VECTOR_MAP
```

## Acceptance Criteria

- AC-01: A `unimatrix-embed` library crate compiles within the Cargo workspace with `cargo build`.
- AC-02: `OnnxProvider::new(config)` loads the ONNX model and tokenizer, downloading from HuggingFace Hub on first use to `~/.cache/unimatrix/models/`.
- AC-03: `OnnxProvider::embed(text)` returns a `Vec<f32>` with exactly 384 elements for any non-empty input string.
- AC-04: `OnnxProvider::embed_batch(texts)` returns one 384-d embedding per input text. The number of output embeddings equals the number of input texts.
- AC-05: All output embeddings are L2-normalized: the L2 norm of each embedding is within tolerance of 1.0 (|norm - 1.0| < 0.001).
- AC-06: `prepare_text(title, content, ": ")` concatenates as `"{title}: {content}"`. Empty title produces just content. Empty content produces just title.
- AC-07: `embed_entry(provider, title, content)` returns a single 384-d normalized embedding for the concatenated title+content.
- AC-08: Embeddings for semantically similar texts have high cosine similarity (> 0.7). Embeddings for unrelated texts have low cosine similarity (< 0.3). Verified with known test pairs.
- AC-09: The `EmbeddingProvider` trait is object-safe and can be used as `&dyn EmbeddingProvider` or `Box<dyn EmbeddingProvider>`.
- AC-10: `OnnxProvider` is `Send + Sync`, shareable via `Arc<OnnxProvider>`.
- AC-11: Batch embedding of N texts produces the same embeddings (within floating-point tolerance) as embedding each text individually.
- AC-12: Empty string input returns an embedding (not an error) -- the model produces a valid vector for empty input.
- AC-13: `EmbedConfig` supports configuring: model selection (from catalog), model cache directory, batch size, and text separator for title+content concatenation.
- AC-14: All public API functions return typed `Result` errors (no panics). Error types cover model loading failures, inference errors, tokenization errors, and download failures.
- AC-15: `#![forbid(unsafe_code)]` at crate level.
- AC-16: `OnnxProvider::dimension()` returns 384 for all catalog models.
- AC-17: `EmbeddingModel` enum provides all 7 pre-configured 384-d models. Each variant encodes HuggingFace model ID, dimension, max sequence length, and ONNX filename.
- AC-18: Mean pooling correctly applies attention mask weighting -- masked (padding) tokens do not contribute to the pooled embedding.
- AC-19: Test infrastructure provides: helper functions for computing cosine similarity between embeddings, assertion helpers for dimension and normalization validation, and a mock provider for testing downstream consumers without ONNX model loading.

## Constraints

- **Rust edition 2024** (workspace setting).
- **Dependencies**: `ort` (2.0.0-rc.11) for ONNX inference, `tokenizers` for text tokenization, `hf-hub` for model downloading, `dirs` for cross-platform cache paths, `thiserror` 2.0 for error handling. No fastembed.
- **No unsafe code.** `#![forbid(unsafe_code)]` at crate level.
- **Fixed dimension: 384.** All catalog models produce 384-d embeddings. This is enforced to match nxs-002's `VectorConfig.dimension`.
- **Model selected at init, fixed for provider lifetime.** Switching models requires creating a new provider and rebuilding the vector index.
- **Token limits vary by model.** 256 word-piece tokens for MiniLM/Paraphrase models, 512 for BGE/E5/GTE. Truncation from the end preserves title (high-signal).
- **Model file size: ~90 MB.** The ONNX model is downloaded on first use. The crate does not embed the model in the binary.
- **No async runtime dependency.** The crate is synchronous. Matches nxs-001/nxs-002 pattern.
- **Network access required on first run.** The model is downloaded from HuggingFace Hub. Subsequent runs use the cached model. Offline-first: after first download, no network access needed.
- **Model cache: `~/.cache/unimatrix/models/`.** Cross-platform via `dirs` crate. Model files stored in sanitized subdirectories (e.g., `sentence-transformers_all-MiniLM-L6-v2/`).
- **ONNX session is `&mut self` for inference.** No concurrent inference on a single session. Thread safety achieved via `Mutex` or serialized access.

## Resolved Questions

- **OQ-01 → RESOLVED: Skip fastembed, use raw `ort` + `tokenizers`.** fastembed adds a dependency layer with potential edition 2024 complications. The raw approach is validated by ruvector in production (~800 lines for tokenization, pooling, normalization). Full control over the pipeline, no exact-pinned transitive dependencies.
- **OQ-02 → RESOLVED: Unimatrix-specific cache (`~/.cache/unimatrix/models/`).** Cross-platform via `dirs` crate. Matches ruvector's approach of custom cache directories. Avoids polluting/depending on HuggingFace's default cache layout.
- **OQ-03 → RESOLVED: No API fallback. Support multiple local 384-d models instead.** Seven pre-configured sentence-transformer models (all 384-d) provide variety without requiring network API calls. Local-first philosophy. API fallback deferred to future feature if needed.
- **OQ-04 → RESOLVED: Acceptable risk.** `ort` 2.0.0-rc.11 is stable enough for production use, validated by ruvector's use of rc.9/10 without issues.

## Tracking

https://github.com/dug-21/unimatrix/issues/5
