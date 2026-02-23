# nxs-003: Embedding Pipeline -- Architecture

## System Overview

The Embedding Pipeline is a library crate (`unimatrix-embed`) that converts text into 384-dimensional normalized embeddings using local ONNX Runtime inference. It bridges the gap between content authoring (nxs-001 `Store::insert`) and vector search (nxs-002 `VectorIndex::insert`) -- without this crate, entries can be stored but never made semantically searchable.

The crate is built on raw `ort` + `tokenizers` + `hf-hub` (no fastembed wrapper), giving full control over tokenization, pooling, normalization, and model lifecycle. It defines an `EmbeddingProvider` trait for abstraction and an `OnnxProvider` implementation as the default path.

This crate is synchronous (matching nxs-001 and nxs-002), has no async runtime dependency, and is designed to be wrapped by downstream consumers via `spawn_blocking` with `Arc<OnnxProvider>`.

## System Context

```
                    Downstream Consumers

  vnc-002 MCP Tools            vnc-001 MCP Server
  (context_store,              (async wrapping,
   context_search)              spawn_blocking)
        |                           |
        |  embed before store       |
        |  embed before search      |
        v                           v
  +---------------------------------------------+
  |         unimatrix-embed (this crate)         |
  |                                              |
  |  +----------+  +-----------+  +-----------+  |
  |  | provider |  |   onnx    |  |   text    |  |
  |  |          |  |           |  |           |  |
  |  |Embedding |  |OnnxProvdr |  |prepare_   |  |
  |  |Provider  |  |Session    |  |text       |  |
  |  |trait     |  |Tokenizer  |  |embed_entry|  |
  |  +----------+  +-----------+  +-----------+  |
  |  +----------+  +-----------+  +-----------+  |
  |  |  model   |  | pooling   |  | normalize |  |
  |  |          |  |           |  |           |  |
  |  |Embedding |  |mean_pool  |  |l2_norm    |  |
  |  |Model enum|  |attn_mask  |  |           |  |
  |  +----------+  +-----------+  +-----------+  |
  |  +----------+  +-----------+                 |
  |  | download |  |  error    |                 |
  |  |          |  |           |                 |
  |  |hf-hub    |  |EmbedError |                 |
  |  |cache mgmt|  |           |                 |
  |  +----------+  +-----------+                 |
  |                                              |
  |          ort + tokenizers + hf-hub           |
  +---------------------------------------------+
        |                           |
        v                           v
  unimatrix-store              unimatrix-vector
  (EntryRecord.title,          (VectorIndex::insert
   EntryRecord.content)          requires Vec<f32>)
```

## Component Breakdown

### 1. Provider Module (`provider`)

Defines the `EmbeddingProvider` trait -- the public abstraction over embedding implementations.

**Primary Type: `EmbeddingProvider`**

```rust
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text string into a fixed-dimension vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed multiple texts in a batch. Returns one embedding per input.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// The output embedding dimension (always 384 for catalog models).
    fn dimension(&self) -> usize;

    /// Human-readable model name for identification/logging.
    fn name(&self) -> &str;
}
```

The trait is object-safe (`&dyn EmbeddingProvider`, `Box<dyn EmbeddingProvider>`). The `Send + Sync` bound enables `Arc<dyn EmbeddingProvider>` sharing across threads.

### 2. Model Module (`model`)

Pre-configured catalog of 384-d sentence-transformer models.

**Primary Type: `EmbeddingModel`**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingModel {
    AllMiniLmL6V2,        // default
    AllMiniLmL12V2,
    MultiQaMiniLmL6,
    ParaphraseMiniLmL6V2,
    BgeSmallEnV15,
    E5SmallV2,
    GteSmall,
}

impl EmbeddingModel {
    /// HuggingFace model repository ID.
    pub fn model_id(&self) -> &'static str;

    /// Output embedding dimension (384 for all variants).
    pub fn dimension(&self) -> usize;

    /// Maximum input sequence length in word-piece tokens.
    pub fn max_seq_length(&self) -> usize;

    /// ONNX model filename within the repository.
    pub fn onnx_filename(&self) -> &'static str;
}

impl Default for EmbeddingModel {
    fn default() -> Self { Self::AllMiniLmL6V2 }
}
```

| Variant | HuggingFace ID | Dim | Max Seq | ONNX File |
|---------|---------------|-----|---------|-----------|
| AllMiniLmL6V2 (default) | sentence-transformers/all-MiniLM-L6-v2 | 384 | 256 | model.onnx |
| AllMiniLmL12V2 | sentence-transformers/all-MiniLM-L12-v2 | 384 | 256 | model.onnx |
| MultiQaMiniLmL6 | sentence-transformers/multi-qa-MiniLM-L6-cos-v1 | 384 | 256 | model.onnx |
| ParaphraseMiniLmL6V2 | sentence-transformers/paraphrase-MiniLM-L6-v2 | 384 | 256 | model.onnx |
| BgeSmallEnV15 | BAAI/bge-small-en-v1.5 | 384 | 512 | model.onnx |
| E5SmallV2 | intfloat/e5-small-v2 | 384 | 512 | model.onnx |
| GteSmall | thenlper/gte-small | 384 | 512 | model.onnx |

All models produce 384-d embeddings, ensuring compatibility with nxs-002's `VectorConfig.dimension = 384`.

### 3. ONNX Provider Module (`onnx`)

The concrete `EmbeddingProvider` implementation using raw `ort` + `tokenizers`.

**Primary Type: `OnnxProvider`**

```rust
pub struct OnnxProvider {
    session: Mutex<ort::Session>,
    tokenizer: tokenizers::Tokenizer,
    model: EmbeddingModel,
    config: EmbedConfig,
}
```

**Construction:**

```rust
impl OnnxProvider {
    /// Create a new OnnxProvider. Downloads the model on first use.
    pub fn new(config: EmbedConfig) -> Result<Self>;
}
```

Construction flow:
1. Resolve model cache directory (`config.cache_dir` or default `~/.cache/unimatrix/models/`).
2. Compute model subdirectory by sanitizing the HuggingFace model ID (e.g., `sentence-transformers/all-MiniLM-L6-v2` → `sentence-transformers_all-MiniLM-L6-v2/`).
3. Check if `model.onnx` and `tokenizer.json` exist in the cache.
4. If not cached, download via `hf-hub` crate's `Api` (anonymous, no token required for public models).
5. Load `tokenizer.json` via `tokenizers::Tokenizer::from_file()`.
6. Configure tokenizer: enable truncation to `model.max_seq_length()`, enable padding to longest in batch.
7. Build `ort::Session` with `GraphOptimizationLevel::Level3` from the cached `model.onnx`.
8. Return `OnnxProvider` ready for inference.

**Inference flow (single text):**
1. Tokenize text → `input_ids`, `attention_mask`, `token_type_ids`.
2. Create ONNX tensors from token arrays (shape: `[1, seq_len]`).
3. Run ONNX session inference → raw output tensor (shape: `[1, seq_len, 384]`).
4. Mean pool with attention mask → `[384]`.
5. L2 normalize → `[384]` with unit norm.
6. Return `Vec<f32>`.

**Inference flow (batch):**
1. Split input texts into chunks of `config.batch_size` (default: 32).
2. For each chunk:
   a. Tokenize all texts together (padded to longest in batch).
   b. Create ONNX tensors (shape: `[batch_size, seq_len]`).
   c. Run single ONNX session inference → `[batch_size, seq_len, 384]`.
   d. Mean pool each sequence with its attention mask → `[batch_size, 384]`.
   e. L2 normalize each → `[batch_size, 384]`.
3. Collect and return all embeddings.

**Concurrency:** The `ort::Session` requires `&mut self` for `run()` (mutable borrow). Wrapping in `Mutex<Session>` provides safe serialized access. See [ADR-001](ADR-001-mutex-session-concurrency.md). The `Tokenizer` is stateless for encoding operations and is safe to call concurrently.

### 4. Pooling Module (`pooling`)

Mean pooling with attention mask weighting.

```rust
/// Apply mean pooling to ONNX output with attention mask.
///
/// - `token_embeddings`: shape `[seq_len, hidden_dim]` for single, or
///   `[batch_size, seq_len, hidden_dim]` for batch.
/// - `attention_mask`: shape `[seq_len]` or `[batch_size, seq_len]`.
///   Values are 0 (padding) or 1 (real token).
///
/// For each sequence: sum(token_embedding * mask) / sum(mask).
/// Masked (padding) tokens contribute zero to the sum.
pub fn mean_pool(
    token_embeddings: &[f32],
    attention_mask: &[i64],
    batch_size: usize,
    seq_len: usize,
    hidden_dim: usize,
) -> Vec<Vec<f32>>;
```

The attention mask is critical: without it, padding tokens dilute the embedding. The mask ensures only real tokens contribute to the pooled representation.

### 5. Normalize Module (`normalize`)

L2 normalization to unit length.

```rust
/// Normalize a vector to unit L2 norm.
///
/// If the input norm is below `1e-12` (near-zero vector), returns
/// the input unchanged to avoid division by zero.
pub fn l2_normalize(embedding: &mut Vec<f32>);

/// Normalize a vector to unit L2 norm, returning a new vector.
pub fn l2_normalized(embedding: &[f32]) -> Vec<f32>;
```

L2 normalization is required for nxs-002's DistDot metric. For normalized vectors, dot product equals cosine similarity, and DistDot computes `1.0 - dot_product` as the distance. Without normalization, similarity scores are meaningless.

### 6. Text Module (`text`)

Title+content concatenation and convenience functions.

```rust
/// Concatenate title and content with the given separator.
///
/// - If title is empty, returns content only.
/// - If content is empty, returns title only.
/// - If both are empty, returns empty string.
pub fn prepare_text(title: &str, content: &str, separator: &str) -> String;

/// Embed a single entry's text fields using the given provider.
///
/// Concatenates title and content with the default separator (": "),
/// then calls `provider.embed()`.
pub fn embed_entry(
    provider: &dyn EmbeddingProvider,
    title: &str,
    content: &str,
) -> Result<Vec<f32>>;

/// Embed a batch of entry text fields.
///
/// Each entry is a `(title, content)` pair. Concatenates each pair,
/// then calls `provider.embed_batch()`.
pub fn embed_entries(
    provider: &dyn EmbeddingProvider,
    entries: &[(String, String)],
) -> Result<Vec<Vec<f32>>>;
```

The default separator `": "` matches common document titling conventions. For example: `"JWT validation convention: Always validate JWT tokens server-side..."`.

### 7. Download Module (`download`)

Model download and cache management via `hf-hub` + `dirs`.

```rust
/// Ensure model files exist in the cache directory.
///
/// Downloads from HuggingFace Hub if not already cached.
/// Returns the path to the model directory.
pub fn ensure_model(
    model: EmbeddingModel,
    cache_dir: &Path,
) -> Result<PathBuf>;
```

**Cache layout:**

```
~/.cache/unimatrix/models/
└── sentence-transformers_all-MiniLM-L6-v2/
    ├── model.onnx        (~90 MB)
    └── tokenizer.json    (~700 KB)
```

**Default cache directory resolution (cross-platform via `dirs` crate):**
- Linux: `~/.cache/unimatrix/models/`
- macOS: `~/Library/Caches/unimatrix/models/`
- Windows: `{FOLDERID_LocalAppData}\unimatrix\models\`

The `hf-hub` crate's `Api` handles anonymous downloads from public HuggingFace repositories. Files are downloaded to a temporary location and renamed atomically to prevent partial downloads in the cache.

### 8. Config Module (`config`)

Configuration for the embedding pipeline.

```rust
#[derive(Debug, Clone)]
pub struct EmbedConfig {
    /// Model to use. Default: AllMiniLmL6V2.
    pub model: EmbeddingModel,

    /// Cache directory for model files.
    /// Default: platform-specific via `dirs` crate.
    pub cache_dir: Option<PathBuf>,

    /// Maximum batch size for `embed_batch`. Default: 32.
    pub batch_size: usize,

    /// Separator for title+content concatenation. Default: ": ".
    pub separator: String,
}

impl Default for EmbedConfig {
    fn default() -> Self {
        Self {
            model: EmbeddingModel::default(),
            cache_dir: None,
            batch_size: 32,
            separator: ": ".to_string(),
        }
    }
}
```

When `cache_dir` is `None`, the default platform cache directory is resolved at construction time.

### 9. Error Module (`error`)

Typed error enum using `thiserror` 2.0.

```rust
#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    /// ONNX Runtime error (session creation, inference).
    #[error("onnx runtime error: {0}")]
    OnnxRuntime(#[from] ort::Error),

    /// Tokenizer error (loading, encoding).
    #[error("tokenizer error: {0}")]
    Tokenizer(String),

    /// Model download failed.
    #[error("model download failed: {0}")]
    Download(String),

    /// Model file not found in cache after download.
    #[error("model not found: {path}")]
    ModelNotFound { path: PathBuf },

    /// I/O error (file operations, cache directory).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Embedding dimension mismatch (unexpected output from model).
    #[error("dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },

    /// Empty batch input.
    #[error("empty input: {0}")]
    EmptyInput(String),
}
```

`EmbedError` implements `std::error::Error` and `Display`. The `From` impls for `ort::Error` and `std::io::Error` enable ergonomic `?` propagation.

Note: `tokenizers::Error` is converted to `EmbedError::Tokenizer(String)` via `map_err` rather than a `From` impl, because the tokenizers crate's error type may not implement `Send` in all versions, and string conversion avoids tight coupling to its internal error representation.

### 10. Lib Module (`lib`)

Crate root with public re-exports and `#![forbid(unsafe_code)]`.

```rust
#![forbid(unsafe_code)]

mod provider;
mod model;
mod onnx;
mod pooling;
mod normalize;
mod text;
mod download;
mod config;
mod error;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

pub use provider::EmbeddingProvider;
pub use model::EmbeddingModel;
pub use onnx::OnnxProvider;
pub use config::EmbedConfig;
pub use error::{EmbedError, Result};
pub use text::{prepare_text, embed_entry, embed_entries};
pub use normalize::{l2_normalize, l2_normalized};
```

### 11. Test Helpers Module (`test_helpers`)

Available behind `test-support` feature flag.

```rust
/// A mock embedding provider for testing downstream consumers
/// without requiring ONNX model loading.
pub struct MockProvider {
    pub dimension: usize,
}

impl EmbeddingProvider for MockProvider {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Returns a deterministic embedding based on text hash.
        // NOT a real embedding -- for testing plumbing only.
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    fn dimension(&self) -> usize { self.dimension }
    fn name(&self) -> &str { "mock" }
}

/// Compute cosine similarity between two embeddings.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32;

/// Assert that an embedding has the expected dimension.
pub fn assert_dimension(embedding: &[f32], expected: usize);

/// Assert that an embedding is L2-normalized (norm ≈ 1.0).
pub fn assert_normalized(embedding: &[f32], tolerance: f32);
```

The `MockProvider` uses a deterministic hash-based approach: it hashes the input text and uses the hash to seed a pseudo-random embedding, then L2-normalizes. This ensures:
- Same text always produces the same embedding.
- Different texts produce different embeddings.
- All embeddings are properly normalized.
- No ONNX model or network required.

## Component Interactions

### Single-Text Embedding Flow

```
Caller              OnnxProvider         Tokenizer        ONNX Session    Pooling    Normalize
  |                      |                   |                |              |           |
  | embed(text)          |                   |                |              |           |
  |--------------------->|                   |                |              |           |
  |                      | encode(text)      |                |              |           |
  |                      |------------------>|                |              |           |
  |                      | Encoding {        |                |              |           |
  |                      |   input_ids,      |                |              |           |
  |                      |   attention_mask,  |               |              |           |
  |                      |   token_type_ids   |               |              |           |
  |                      | }                 |                |              |           |
  |                      |<------------------|                |              |           |
  |                      |                   |                |              |           |
  |                      | build tensors [1, seq_len]         |              |           |
  |                      | lock session                       |              |           |
  |                      | run(inputs)       |                |              |           |
  |                      |-------------------------------------->|           |           |
  |                      | output [1, seq_len, 384]           |              |           |
  |                      |<--------------------------------------|           |           |
  |                      | unlock session    |                |              |           |
  |                      |                   |                |              |           |
  |                      | mean_pool(output, attention_mask)  |              |           |
  |                      |-------------------------------------------------->|           |
  |                      | [384]             |                |              |           |
  |                      |<--------------------------------------------------|           |
  |                      |                   |                |              |           |
  |                      | l2_normalize(pooled)               |              |           |
  |                      |------------------------------------------------------------->|
  |                      | [384] unit norm   |                |              |           |
  |                      |<-------------------------------------------------------------|
  |                      |                   |                |              |           |
  | Ok(Vec<f32>)         |                   |                |              |           |
  |<---------------------|                   |                |              |           |
```

### Batch Embedding Flow

```
Caller              OnnxProvider         Tokenizer        ONNX Session    Pooling
  |                      |                   |                |              |
  | embed_batch(texts)   |                   |                |              |
  |--------------------->|                   |                |              |
  |                      |                   |                |              |
  |                      | chunk texts into batches of config.batch_size     |
  |                      |                   |                |              |
  |                      | for each chunk:   |                |              |
  |                      |   encode_batch()  |                |              |
  |                      |------------------>|                |              |
  |                      |   [batch, seq]    |                |              |
  |                      |<------------------|                |              |
  |                      |                   |                |              |
  |                      |   lock session    |                |              |
  |                      |   run(tensors)    |                |              |
  |                      |-------------------------------------->|           |
  |                      |   [batch, seq, 384]                |              |
  |                      |<--------------------------------------|           |
  |                      |   unlock session  |                |              |
  |                      |                   |                |              |
  |                      |   mean_pool per sequence           |              |
  |                      |-------------------------------------------------->|
  |                      |   [batch, 384]    |                |              |
  |                      |<--------------------------------------------------|
  |                      |                   |                |              |
  |                      |   l2_normalize each                |              |
  |                      |                   |                |              |
  |                      | collect all embeddings             |              |
  |                      |                   |                |              |
  | Ok(Vec<Vec<f32>>)    |                   |                |              |
  |<---------------------|                   |                |              |
```

### Full Write Path (Downstream Integration)

After nxs-003, the complete write path for a knowledge entry:

```
vnc-002 context_store
  |
  | title + content + metadata
  v
Store::insert(NewEntry { title, content, ... })   --> entry_id (u64)
  |
  | title, content
  v
prepare_text(title, content, ": ")                --> combined text
  |
  v
OnnxProvider::embed(combined_text)                --> Vec<f32> (384-d, normalized)
  |
  v
VectorIndex::insert(entry_id, &embedding)         --> hnsw_rs + VECTOR_MAP
```

### Full Search Path (Downstream Integration)

```
vnc-002 context_search
  |
  | query text
  v
OnnxProvider::embed(query)                         --> Vec<f32> (384-d, normalized)
  |
  v
VectorIndex::search(&query_embedding, top_k, ef)  --> Vec<SearchResult>
  |
  | for each result: entry_id
  v
Store::get(entry_id)                               --> EntryRecord (hydration)
```

## Technology Decisions

| Technology | Decision | Rationale | ADR |
|------------|----------|-----------|-----|
| Mutex\<Session\> | Concurrency model for ONNX session | `ort::Session::run()` requires `&mut self`. Mutex serializes inference calls. Tokenizer is stateless and needs no lock. | [ADR-001](ADR-001-mutex-session-concurrency.md) |
| Raw ort + tokenizers | No fastembed wrapper | Full control over tokenization, pooling, normalization. Avoids fastembed's exact-pinned ort version and edition 2024 risk. Validated by ruvector in production. | [ADR-002](ADR-002-raw-ort-no-fastembed.md) |
| hf-hub for downloads | Model acquisition strategy | Handles HuggingFace repository structure, revision tracking, anonymous access. More robust than raw HTTP downloads. | [ADR-003](ADR-003-hf-hub-model-download.md) |
| Custom cache dir | Model storage location | `~/.cache/unimatrix/models/` via `dirs` crate. Avoids depending on HuggingFace's default cache layout. Cross-platform. | [ADR-004](ADR-004-custom-cache-directory.md) |
| Synchronous API | Concurrency boundary | Matches nxs-001/nxs-002 pattern. No tokio dependency. Consumers wrap with `spawn_blocking`. | nxs-001 ADR-004 |

## Cargo Workspace Integration

```
/                               Cargo workspace root
+-- Cargo.toml                  [workspace] members = ["crates/*"]
+-- crates/
|   +-- unimatrix-store/        nxs-001: storage engine (existing)
|   +-- unimatrix-vector/       nxs-002: vector index (existing)
|   +-- unimatrix-embed/        nxs-003: embedding pipeline (this crate)
|       +-- Cargo.toml
|       +-- src/
|           +-- lib.rs          Public re-exports, #![forbid(unsafe_code)]
|           +-- provider.rs     EmbeddingProvider trait
|           +-- model.rs        EmbeddingModel enum (7 variants)
|           +-- onnx.rs         OnnxProvider (ort + tokenizers)
|           +-- pooling.rs      Mean pooling with attention mask
|           +-- normalize.rs    L2 normalization
|           +-- text.rs         prepare_text, embed_entry, embed_entries
|           +-- download.rs     hf-hub download + cache management
|           +-- config.rs       EmbedConfig
|           +-- error.rs        EmbedError enum
|           +-- test_helpers.rs MockProvider, cosine_similarity, assertions
```

**unimatrix-embed Cargo.toml:**

```toml
[package]
name = "unimatrix-embed"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[features]
test-support = []

[dependencies]
ort = { version = "2.0.0-rc.11", features = ["download-binaries"] }
tokenizers = { version = "0.21", default-features = false, features = ["onig"] }
hf-hub = "0.4"
dirs = "6"
thiserror = "2"

[dev-dependencies]
approx = "0.5"
```

**Notes on dependencies:**
- `ort` v2.0.0-rc.11: Matches fastembed's pinned version and ruvector's validated rc.9/10 lineage. The `download-binaries` feature auto-downloads ONNX Runtime shared libraries on first build.
- `tokenizers` v0.21: HuggingFace tokenizers with `onig` feature for regex tokenization. `default-features = false` avoids pulling in a full HTTP client.
- `hf-hub` v0.4: HuggingFace Hub client for model downloading.
- `dirs` v6: Cross-platform standard directory paths.
- `thiserror` v2: Derive macros for `Error` trait implementation.

This crate does NOT depend on `unimatrix-store` or `unimatrix-vector`. It produces `Vec<f32>` embeddings; the caller is responsible for passing them to `VectorIndex::insert`. This keeps the dependency graph clean:

```
unimatrix-store  <--  unimatrix-vector
                                           (no dependency between embed and store/vector)
unimatrix-embed  (standalone)
```

Downstream consumers (vnc-001, vnc-002) depend on all three crates and coordinate the flow.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `EmbeddingProvider::embed(&self, text: &str) -> Result<Vec<f32>>` | Trait method | unimatrix-embed provider.rs |
| `EmbeddingProvider::embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>` | Trait method | unimatrix-embed provider.rs |
| `EmbeddingProvider::dimension(&self) -> usize` | Trait method | unimatrix-embed provider.rs |
| `EmbeddingProvider::name(&self) -> &str` | Trait method | unimatrix-embed provider.rs |
| `OnnxProvider::new(config: EmbedConfig) -> Result<Self>` | Constructor | unimatrix-embed onnx.rs |
| `prepare_text(title: &str, content: &str, separator: &str) -> String` | Free function | unimatrix-embed text.rs |
| `embed_entry(provider: &dyn EmbeddingProvider, title: &str, content: &str) -> Result<Vec<f32>>` | Free function | unimatrix-embed text.rs |
| `embed_entries(provider: &dyn EmbeddingProvider, entries: &[(String, String)]) -> Result<Vec<Vec<f32>>>` | Free function | unimatrix-embed text.rs |
| `l2_normalize(embedding: &mut Vec<f32>)` | Free function | unimatrix-embed normalize.rs |
| `l2_normalized(embedding: &[f32]) -> Vec<f32>` | Free function | unimatrix-embed normalize.rs |
| `EmbedConfig { model, cache_dir, batch_size, separator }` | Config struct | unimatrix-embed config.rs |
| `EmbeddingModel::AllMiniLmL6V2` (+ 6 variants) | Enum | unimatrix-embed model.rs |
| `EmbedError` | Error enum | unimatrix-embed error.rs |

### Upstream Integration (consumed by this crate)

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `ort::Session::builder()` | ONNX session factory | ort crate |
| `ort::Session::run(inputs)` | ONNX inference | ort crate |
| `tokenizers::Tokenizer::from_file(path)` | Tokenizer loading | tokenizers crate |
| `tokenizers::Tokenizer::encode(text, add_special_tokens)` | Single tokenization | tokenizers crate |
| `tokenizers::Tokenizer::encode_batch(texts, add_special_tokens)` | Batch tokenization | tokenizers crate |
| `hf_hub::api::sync::Api::model(model_id)` | Model repo handle | hf-hub crate |
| `hf_hub::api::sync::ApiRepo::get(filename)` | File download | hf-hub crate |
| `dirs::cache_dir()` | Platform cache path | dirs crate |

### Downstream Integration (consumed by future crates)

| Consumer | How it uses unimatrix-embed |
|----------|---------------------------|
| vnc-002 (`context_store`) | Calls `embed_entry(provider, title, content)` after `Store::insert`, then `VectorIndex::insert(entry_id, &embedding)`. |
| vnc-002 (`context_search`) | Calls `provider.embed(query)` to convert search query to vector, then `VectorIndex::search(&embedding, top_k, ef)`. |
| vnc-002 (near-dup detection) | Calls `provider.embed(text)` before insert, then `VectorIndex::search(&embedding, 1, ef)` to check if similarity >= 0.92. |
| vnc-001 (async wrapping) | Wraps `Arc<OnnxProvider>` with `spawn_blocking` for async context. |
| Future: index rebuild | Iterates all entries via `Store`, calls `embed_entries(provider, entries)`, then batch-inserts into `VectorIndex`. |

## Error Handling Strategy

1. **No panics in public API.** Every public function returns `Result<T, EmbedError>`.
2. **Model loading errors are clear.** `EmbedError::ModelNotFound`, `EmbedError::Download`, and `EmbedError::OnnxRuntime` cover distinct failure modes during initialization.
3. **Inference errors propagate.** ONNX runtime errors during `Session::run` propagate as `EmbedError::OnnxRuntime`. Tokenizer errors propagate as `EmbedError::Tokenizer`.
4. **Empty input is valid.** An empty string produces a valid (if semantically meaningless) embedding. This matches AC-12 and avoids edge-case panics.
5. **Network errors surface during download.** First-use model download may fail due to network issues. The error is `EmbedError::Download` with a descriptive message. After successful download, no network access is needed.
6. **Dimension validation on output.** After inference, the output dimension is validated against the expected 384. A mismatch (which would indicate a model/config bug) produces `EmbedError::DimensionMismatch`.

## Concurrency Model

**OnnxProvider sharing pattern:**

`OnnxProvider` is `Send + Sync` (Session behind Mutex, Tokenizer is Send + Sync). Downstream consumers share via `Arc<OnnxProvider>`:

```rust
let provider: Arc<OnnxProvider> = Arc::new(OnnxProvider::new(config)?);

// In async context:
let p = provider.clone();
let embedding = tokio::task::spawn_blocking(move || {
    p.embed("some text")
}).await??;
```

**Inference serialization:**

The Mutex on the ONNX Session serializes inference calls. This is acceptable because:
- Embedding generation is CPU-bound (~5-20ms per text for MiniLM models).
- Concurrent callers queue at the Mutex, not in the async runtime.
- Batch embedding amortizes the lock: one lock acquisition for N texts.
- For higher throughput, downstream consumers can create multiple `OnnxProvider` instances (each loads its own ONNX session).

**Tokenizer is lock-free:**

`tokenizers::Tokenizer::encode()` and `encode_batch()` take `&self` and are safe for concurrent use. The tokenizer is NOT behind a lock. Only the ONNX Session needs Mutex protection.

## Open Questions

1. **tokenizers crate `onig` vs `esaxx` feature.** The `onig` feature provides regex-based tokenization used by most sentence-transformer models. Verify during implementation that the `onig` feature builds cleanly with edition 2024 and doesn't pull in problematic native dependencies. Fallback: use `esaxx` or default features if `onig` fails.

2. **ONNX output tensor extraction.** The exact API for extracting the output tensor from `ort::Session::run()` depends on the ort 2.0 API. The shape is expected to be `[batch_size, seq_len, hidden_dim]` with the output name `last_hidden_state` or index 0. Verify the output tensor name/index during implementation against the actual model files.

3. **hf-hub cache atomicity.** The `hf-hub` crate handles its own download caching. Verify that concurrent `OnnxProvider::new()` calls (rare, but possible during tests) don't corrupt the cache. If `hf-hub` uses atomic writes internally, this is a non-issue.
