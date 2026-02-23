# nxs-003: Embedding Pipeline -- Specification

**Feature**: nxs-003 (Nexus Phase)
**Status**: Specification
**Source**: SCOPE.md, PRODUCT-VISION.md, RESEARCH-ruvector.md, nxs-001/nxs-002 integration surface

---

## 1. Objective

Create a `unimatrix-embed` library crate that generates 384-dimensional L2-normalized embeddings from text using raw `ort` (ONNX Runtime) + `tokenizers` (HuggingFace) + `hf-hub`, bridging content authoring (unimatrix-store) and vector search (unimatrix-vector). The crate defines an `EmbeddingProvider` trait for provider abstraction and ships an `OnnxProvider` implementation with a pre-configured catalog of 7 sentence-transformer models, automatic model download/caching, title+content concatenation, mean pooling with attention mask, and configurable batch inference.

---

## 2. Functional Requirements

### FR-01: Crate Setup

**FR-01.1: Library Crate**
A `unimatrix-embed` library crate exists at `crates/unimatrix-embed/` within the Cargo workspace. It compiles with `cargo build --workspace`. Edition 2024, MSRV 1.89.

**FR-01.2: Crate Attributes**
The crate root declares `#![forbid(unsafe_code)]`.

**FR-01.3: Module Structure**
```
crates/unimatrix-embed/
  Cargo.toml
  src/
    lib.rs         -- Crate root, #![forbid(unsafe_code)], public re-exports
    provider.rs    -- EmbeddingProvider trait definition
    model.rs       -- EmbeddingModel enum (pre-configured model catalog)
    onnx.rs        -- OnnxProvider implementation
    pooling.rs     -- Mean pooling with attention mask
    config.rs      -- EmbedConfig struct
    error.rs       -- EmbedError enum (thiserror 2.0)
    normalize.rs   -- L2 normalization utility
    text.rs        -- Title+content concatenation, text preparation
    download.rs    -- Model download + cache management (hf-hub + dirs)
```

### FR-02: EmbeddingProvider Trait

**FR-02.1: Trait Definition**
```rust
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text string into a fixed-dimension vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed multiple texts in a batch. Returns one embedding per input text.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// The output embedding dimension (384 for all catalog models).
    fn dimension(&self) -> usize;

    /// The model name for identification/logging.
    fn name(&self) -> &str;
}
```

**FR-02.2: Object Safety**
The trait is object-safe: it can be used as `&dyn EmbeddingProvider`, `Box<dyn EmbeddingProvider>`, and `Arc<dyn EmbeddingProvider>`. No generic methods, no `Self` in return position.

**FR-02.3: Send + Sync Bound**
The `Send + Sync` supertrait bound enables sharing a provider across threads via `Arc<dyn EmbeddingProvider>`.

### FR-03: EmbeddingModel Catalog

**FR-03.1: Model Enum**
```rust
pub enum EmbeddingModel {
    AllMiniLmL6V2,       // default
    AllMiniLmL12V2,
    MultiQaMiniLmL6,
    ParaphraseMiniLmL6V2,
    BgeSmallEnV15,
    E5SmallV2,
    GteSmall,
}
```

**FR-03.2: Model Metadata**
Each variant encodes four properties, accessible via methods on the enum:

| Variant | HuggingFace ID | Dim | Max Seq | ONNX Filename |
|---------|---------------|-----|---------|---------------|
| AllMiniLmL6V2 (default) | sentence-transformers/all-MiniLM-L6-v2 | 384 | 256 | model.onnx |
| AllMiniLmL12V2 | sentence-transformers/all-MiniLM-L12-v2 | 384 | 256 | model.onnx |
| MultiQaMiniLmL6 | sentence-transformers/multi-qa-MiniLM-L6-cos-v1 | 384 | 256 | model.onnx |
| ParaphraseMiniLmL6V2 | sentence-transformers/paraphrase-MiniLM-L6-v2 | 384 | 256 | model.onnx |
| BgeSmallEnV15 | BAAI/bge-small-en-v1.5 | 384 | 512 | model.onnx |
| E5SmallV2 | intfloat/e5-small-v2 | 384 | 512 | model.onnx |
| GteSmall | thenlper/gte-small | 384 | 512 | model.onnx |

**FR-03.3: Accessor Methods**
```rust
impl EmbeddingModel {
    pub fn model_id(&self) -> &str;       // HuggingFace model ID
    pub fn dimension(&self) -> usize;      // Always 384
    pub fn max_seq_length(&self) -> usize; // 256 or 512
    pub fn onnx_filename(&self) -> &str;   // "model.onnx"
}
```

**FR-03.4: Default**
`EmbeddingModel::default()` returns `AllMiniLmL6V2`.

### FR-04: EmbedConfig

**FR-04.1: Config Struct**
```rust
pub struct EmbedConfig {
    pub model: EmbeddingModel,
    pub cache_dir: Option<PathBuf>,
    pub batch_size: usize,
    pub separator: String,
}
```

**FR-04.2: Defaults**
- `model`: `EmbeddingModel::AllMiniLmL6V2`
- `cache_dir`: `None` (resolved to `~/.cache/unimatrix/models/` at runtime via `dirs` crate)
- `batch_size`: 32
- `separator`: `": "` (colon-space)

**FR-04.3: Cache Directory Resolution**
When `cache_dir` is `None`, the runtime default is:
- Linux: `~/.cache/unimatrix/models/`
- macOS: `~/Library/Caches/unimatrix/models/`
- Windows: `{FOLDERID_LocalAppData}/unimatrix/models/`

Resolved via `dirs::cache_dir().join("unimatrix").join("models")`. If `dirs::cache_dir()` returns `None`, fall back to `PathBuf::from(".unimatrix/models")` (current directory).

### FR-05: OnnxProvider

**FR-05.1: Construction**
`OnnxProvider::new(config: EmbedConfig) -> Result<OnnxProvider>`:
1. Resolve cache directory from config.
2. Determine model subdirectory: sanitize model ID by replacing `/` with `_` (e.g., `sentence-transformers_all-MiniLM-L6-v2/`).
3. If model files not cached locally, download from HuggingFace Hub (FR-07).
4. Load the ONNX model into an `ort::Session` with `GraphOptimizationLevel::Level3`.
5. Load the tokenizer from `tokenizer.json` in the model directory.
6. Store config, session, and tokenizer. Return `Ok(OnnxProvider)`.

**FR-05.2: Thread Safety**
`OnnxProvider` is `Send + Sync`. The `ort::Session` requires `&mut self` for inference. Thread safety is achieved by wrapping the session in a `Mutex<Session>`. The `Mutex` is acquired for each `embed`/`embed_batch` call and released immediately after inference completes.

**FR-05.3: embed() Implementation**
`OnnxProvider::embed(&self, text: &str) -> Result<Vec<f32>>`:
1. Acquire session lock.
2. Tokenize the input text (FR-06).
3. Create ONNX input tensors: `input_ids`, `attention_mask`, `token_type_ids`.
4. Run inference on the session.
5. Extract the output tensor (shape: `[1, seq_len, 384]`).
6. Apply mean pooling with attention mask (FR-08).
7. Apply L2 normalization (FR-09).
8. Release session lock.
9. Return the 384-d normalized vector.

**FR-05.4: embed_batch() Implementation**
`OnnxProvider::embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>`:
1. Chunk the input texts into batches of `config.batch_size`.
2. For each batch:
   a. Tokenize all texts in the batch together, padding to the longest sequence.
   b. Acquire session lock.
   c. Create batched ONNX input tensors (shape: `[batch_size, max_seq_len]`).
   d. Run batched inference.
   e. Release session lock.
   f. For each text in the batch, extract the embedding slice from the output tensor.
   g. Apply mean pooling with per-text attention mask.
   h. Apply L2 normalization.
3. Collect all embeddings in input order.
4. Return `Vec<Vec<f32>>` with exactly one embedding per input text.

**FR-05.5: dimension() and name()**
- `dimension()` returns the model's dimension (384 for all catalog models).
- `name()` returns the HuggingFace model ID string.

**FR-05.6: EmbeddingProvider Implementation**
`OnnxProvider` implements the `EmbeddingProvider` trait. All four trait methods delegate to the implementations described above.

### FR-06: Tokenization

**FR-06.1: Tokenizer Loading**
The tokenizer is loaded from `tokenizer.json` in the model's cache directory. Loaded once during `OnnxProvider::new()` and reused for all inference calls.

**FR-06.2: Tokenization Parameters**
- Truncation: enabled, max length = model's `max_seq_length` (256 or 512 depending on model).
- Padding: enabled for batch tokenization; pad to longest sequence in the batch. Padding direction: right.
- Special tokens: added (CLS, SEP) as required by the model's tokenizer config.

**FR-06.3: Tokenizer Output**
Tokenization produces three parallel arrays per text:
- `input_ids: Vec<u32>` -- token IDs.
- `attention_mask: Vec<u32>` -- 1 for real tokens, 0 for padding.
- `token_type_ids: Vec<u32>` -- 0 for all tokens (single-sentence input).

For batch tokenization, these are flattened into contiguous arrays with shape `[batch_size, seq_len]` for ONNX tensor creation.

**FR-06.4: Empty Input Handling**
An empty string (`""`) is a valid input. The tokenizer produces at minimum the special tokens (e.g., `[CLS]`, `[SEP]`), resulting in a valid embedding.

### FR-07: Model Download and Caching

**FR-07.1: Download Trigger**
On `OnnxProvider::new()`, if the model's ONNX file and `tokenizer.json` are not present in the cache directory, download them from HuggingFace Hub.

**FR-07.2: Download Mechanism**
Use the `hf-hub` crate's `Api` to download model files. Required files per model:
- `model.onnx` (or the filename from `EmbeddingModel::onnx_filename()`)
- `tokenizer.json`

**FR-07.3: Cache Layout**
```
~/.cache/unimatrix/models/
  sentence-transformers_all-MiniLM-L6-v2/
    model.onnx
    tokenizer.json
  BAAI_bge-small-en-v1.5/
    model.onnx
    tokenizer.json
  ...
```

Model ID sanitization: replace `/` with `_` in the HuggingFace model ID to create the subdirectory name.

**FR-07.4: Offline After First Download**
After the initial download, no network access is required. The cached files are used directly.

**FR-07.5: Download Failure**
If the download fails (network error, model not found, etc.), `OnnxProvider::new()` returns `EmbedError::Download` with a descriptive message.

**FR-07.6: Model Validation**
After download (or on cache hit), validate that:
- The ONNX file exists and is non-empty.
- The `tokenizer.json` file exists and is non-empty.
If validation fails, return `EmbedError::ModelLoad` with a descriptive message.

### FR-08: Mean Pooling

**FR-08.1: Pooling Algorithm**
Given the ONNX model output tensor (shape `[batch_size, seq_len, dim]`) and the attention mask (shape `[batch_size, seq_len]`):

For each text in the batch:
1. Extract the token embeddings (shape `[seq_len, dim]`) and attention mask (shape `[seq_len]`).
2. For each dimension `d` in `0..dim`:
   - `sum = 0.0`
   - `count = 0.0`
   - For each token `t` in `0..seq_len`:
     - `sum += token_embeddings[t][d] * attention_mask[t] as f32`
     - `count += attention_mask[t] as f32`
   - `pooled[d] = sum / count.max(1e-9)` (avoid division by zero)
3. Return `pooled` as the embedding for this text.

**FR-08.2: Attention Mask Effect**
Padding tokens (attention_mask = 0) do not contribute to the pooled embedding. Only real tokens are averaged. This ensures padding does not distort the embedding.

### FR-09: L2 Normalization

**FR-09.1: Normalization Algorithm**
```
norm = sqrt(sum(x[i]^2 for i in 0..dim))
if norm > 1e-12:
    for i in 0..dim: x[i] = x[i] / norm
else:
    // Near-zero vector: return as-is (avoid division by near-zero)
```

**FR-09.2: Output Guarantee**
All output embeddings have L2 norm within tolerance of 1.0: `|norm - 1.0| < 0.001`. This is required for compatibility with nxs-002's DistDot metric.

**FR-09.3: Application Order**
Normalization is applied after mean pooling (FR-08), before returning the embedding to the caller.

### FR-10: Text Preparation

**FR-10.1: prepare_text Function**
```rust
pub fn prepare_text(title: &str, content: &str, separator: &str) -> String
```

Concatenation rules:
- Both non-empty: `"{title}{separator}{content}"` (e.g., `"JWT validation: Always verify the exp claim"`)
- Title empty: content only (no separator prefix)
- Content empty: title only (no separator suffix)
- Both empty: empty string `""`

**FR-10.2: Truncation**
The tokenizer handles truncation to the model's max sequence length (FR-06.2). `prepare_text` does not truncate; it produces the full concatenated string. The tokenizer truncates from the end during tokenization, preserving the title (which appears at the start and carries high-signal topic context).

### FR-11: Convenience Functions

**FR-11.1: embed_entry**
```rust
pub fn embed_entry(
    provider: &dyn EmbeddingProvider,
    title: &str,
    content: &str,
) -> Result<Vec<f32>>
```
Equivalent to `provider.embed(&prepare_text(title, content, ": "))`. Uses the default separator.

**FR-11.2: embed_entries**
```rust
pub fn embed_entries(
    provider: &dyn EmbeddingProvider,
    entries: &[(String, String)],
) -> Result<Vec<Vec<f32>>>
```
Prepares texts with `prepare_text` using the default separator, then calls `provider.embed_batch` on the prepared texts. Returns one embedding per entry in the same order.

### FR-12: Error Handling

**FR-12.1: Error Enum**
```rust
#[derive(Debug)]
pub enum EmbedError {
    /// ONNX Runtime error (session creation, inference).
    OnnxRuntime(ort::Error),

    /// Tokenizer error (loading, encoding).
    Tokenizer(String),

    /// Model file download failed.
    Download(String),

    /// Model loading failed (file missing, empty, corrupt).
    ModelLoad(String),

    /// Embedding dimension does not match expected value.
    DimensionMismatch { expected: usize, got: usize },

    /// I/O error (cache directory creation, file operations).
    Io(std::io::Error),
}
```

**FR-12.2: Error Traits**
`EmbedError` implements `std::fmt::Display` and `std::error::Error`. Each variant produces a descriptive message indicating the failure context.

**FR-12.3: From Implementations**
- `From<ort::Error> for EmbedError` -> `EmbedError::OnnxRuntime`
- `From<std::io::Error> for EmbedError` -> `EmbedError::Io`

**FR-12.4: No Panics**
All public functions return `Result<T, EmbedError>`. No panics, no `unwrap()`, no `expect()` in non-test code.

**FR-12.5: Result Alias**
```rust
pub type Result<T> = std::result::Result<T, EmbedError>;
```

### FR-13: Test Infrastructure

**FR-13.1: MockProvider**
A `MockProvider` struct that implements `EmbeddingProvider` for testing downstream consumers without requiring ONNX model loading:
```rust
pub struct MockProvider {
    dimension: usize,
}
```
- `embed()`: Returns a deterministic 384-d vector derived from the input text (e.g., hash-based) that is L2-normalized.
- `embed_batch()`: Calls `embed()` for each text.
- `dimension()`: Returns the configured dimension.
- `name()`: Returns `"mock"`.

The `MockProvider` is available behind `#[cfg(any(test, feature = "test-support"))]`.

**FR-13.2: Assertion Helpers**
Test helper functions, also behind the `test-support` feature flag:
- `assert_dimension(embedding: &[f32], expected: usize)` -- asserts `embedding.len() == expected`.
- `assert_normalized(embedding: &[f32], tolerance: f32)` -- asserts L2 norm within tolerance of 1.0.
- `cosine_similarity(a: &[f32], b: &[f32]) -> f32` -- computes cosine similarity between two embeddings.

---

## 3. Non-Functional Requirements

### NFR-01: Performance

| Operation | Target | Basis |
|-----------|--------|-------|
| Single text embedding | < 50 ms | ONNX Runtime CPU, GraphOptLevel3, 384-d model |
| Batch embedding (32 texts) | < 500 ms | Batched ONNX inference amortizes session overhead |
| Model load (from cache) | < 3 s | ONNX session initialization + tokenizer parse |
| Mean pooling + normalization | < 1 ms per embedding | Pure arithmetic on 384 floats |

These are design targets for typical development workstation hardware, not hard SLA guarantees. First-run model download time depends on network speed and is excluded.

### NFR-02: Memory

- ONNX Runtime session: ~90 MB resident (model loaded into memory).
- Tokenizer: < 5 MB (vocabulary + merge rules).
- Per-embedding transient allocation: ~1.5 KB (384 floats = 1,536 bytes).
- Batch of 32: ~50 KB transient (32 * 1.5 KB per embedding + tokenization buffers).
- Total steady-state: ~100 MB for one loaded model. Acceptable for local-first, single-model operation.

### NFR-03: Safety

- `#![forbid(unsafe_code)]` at crate level. No unsafe blocks.
- All public functions return `Result<T, EmbedError>`. No panics.
- Thread safety via `Mutex<Session>` for ONNX inference serialization.

### NFR-04: Compatibility

- Rust edition 2024, MSRV 1.89 (workspace setting).
- `ort` 2.0.0-rc.11 for ONNX Runtime.
- `tokenizers` (HuggingFace) for text tokenization.
- `hf-hub` for model download.
- `dirs` for cross-platform cache paths.
- `thiserror` 2.0 for error derives.
- Output dimension: 384 for all catalog models. Compatible with nxs-002 `VectorConfig { dimension: 384 }`.
- Output normalization: L2 norm = 1.0. Compatible with nxs-002 DistDot metric.

### NFR-05: No Async Runtime Dependency

The crate is synchronous. No dependency on tokio, async-std, or any async runtime. Matches nxs-001 and nxs-002 pattern. Downstream async consumers wrap with `spawn_blocking`.

### NFR-06: Network Isolation

After first model download, the crate operates fully offline. No network calls during `embed()`, `embed_batch()`, or any inference operation. Network access is limited to `OnnxProvider::new()` when model files are not cached.

---

## 4. Acceptance Criteria

Restated from SCOPE.md with verification methods.

### Workspace and Compilation

**AC-01**: A `unimatrix-embed` library crate compiles within the Cargo workspace with `cargo build`.
- **Verification**: `cargo build --workspace` succeeds with zero errors. `Cargo.toml` at workspace root includes `unimatrix-embed` as a member. Crate is edition 2024.

### Provider Construction and Model Loading

**AC-02**: `OnnxProvider::new(config)` loads the ONNX model and tokenizer, downloading from HuggingFace Hub on first use to `~/.cache/unimatrix/models/`.
- **Verification**: Integration test creates `OnnxProvider` with default config. On first run, model files appear in the cache directory. On second run, no download occurs (verified by checking file modification times are unchanged). Test asserts `OnnxProvider` is ready for inference after construction.

### Single Embedding

**AC-03**: `OnnxProvider::embed(text)` returns a `Vec<f32>` with exactly 384 elements for any non-empty input string.
- **Verification**: Integration test embeds three distinct strings ("hello world", a long paragraph, a single word). Each result has `.len() == 384`. All elements are finite floats (no NaN, no infinity).

### Batch Embedding

**AC-04**: `OnnxProvider::embed_batch(texts)` returns one 384-d embedding per input text. The number of output embeddings equals the number of input texts.
- **Verification**: Integration test embeds a batch of 5 texts. Result has `.len() == 5`. Each inner vec has `.len() == 384`.

### Normalization

**AC-05**: All output embeddings are L2-normalized: the L2 norm of each embedding is within tolerance of 1.0 (`|norm - 1.0| < 0.001`).
- **Verification**: Integration test embeds 10 varied texts (short, long, empty, unicode). For each embedding, compute L2 norm and assert `(norm - 1.0).abs() < 0.001`.

### Text Preparation

**AC-06**: `prepare_text(title, content, ": ")` concatenates as `"{title}: {content}"`. Empty title produces just content. Empty content produces just title.
- **Verification**: Unit tests:
  - `prepare_text("JWT", "Validate exp", ": ")` == `"JWT: Validate exp"`
  - `prepare_text("", "content only", ": ")` == `"content only"`
  - `prepare_text("title only", "", ": ")` == `"title only"`
  - `prepare_text("", "", ": ")` == `""`

### Convenience API

**AC-07**: `embed_entry(provider, title, content)` returns a single 384-d normalized embedding for the concatenated title+content.
- **Verification**: Integration test: `embed_entry(provider, "Auth", "Use JWT")` returns a 384-d embedding. Assert dimension and normalization. Assert the result equals `provider.embed(&prepare_text("Auth", "Use JWT", ": "))`.

### Semantic Quality

**AC-08**: Embeddings for semantically similar texts have high cosine similarity (> 0.7). Embeddings for unrelated texts have low cosine similarity (< 0.3). Verified with known test pairs.
- **Verification**: Integration test with known pairs:
  - Similar: ("Rust error handling best practices", "How to handle errors in Rust") -> similarity > 0.7
  - Dissimilar: ("Rust error handling", "Recipe for chocolate cake") -> similarity < 0.3
  - Computed via `cosine_similarity` helper.

### Object Safety

**AC-09**: The `EmbeddingProvider` trait is object-safe and can be used as `&dyn EmbeddingProvider` or `Box<dyn EmbeddingProvider>`.
- **Verification**: Compile-time test: a function accepts `&dyn EmbeddingProvider` and calls all four trait methods. A second function accepts `Box<dyn EmbeddingProvider>`. Both compile without error.

### Thread Safety

**AC-10**: `OnnxProvider` is `Send + Sync`, shareable via `Arc<OnnxProvider>`.
- **Verification**: Compile-time assert: `fn assert_send_sync<T: Send + Sync>() {}; assert_send_sync::<OnnxProvider>();`. Integration test: wrap `OnnxProvider` in `Arc`, clone to two threads, embed from both, assert both get valid results.

### Batch Consistency

**AC-11**: Batch embedding of N texts produces the same embeddings (within floating-point tolerance) as embedding each text individually.
- **Verification**: Integration test: embed 5 texts individually via `embed()`, then batch via `embed_batch()`. For each pair, assert `(individual[i] - batch[i]).abs() < 1e-5` element-wise.

### Empty Input

**AC-12**: Empty string input returns an embedding (not an error) -- the model produces a valid vector for empty input.
- **Verification**: Integration test: `provider.embed("")` returns `Ok(embedding)` with 384 elements, L2 norm within tolerance of 1.0.

### Configuration

**AC-13**: `EmbedConfig` supports configuring: model selection (from catalog), model cache directory, batch size, and text separator for title+content concatenation.
- **Verification**: Unit test: create `EmbedConfig` with non-default values for all four fields. Assert each field is set correctly. Integration test: create `OnnxProvider` with a custom `cache_dir` (temp directory); model files appear in the custom directory.

### Error Handling

**AC-14**: All public API functions return typed `Result` errors (no panics). Error types cover model loading failures, inference errors, tokenization errors, and download failures.
- **Verification**: Unit tests construct each `EmbedError` variant and verify `Display` output. Integration test: attempt `OnnxProvider::new()` with a non-existent cache directory that cannot be created (read-only path) -- returns `EmbedError::Io`. Attempt to load a corrupt/empty model file -- returns `EmbedError::ModelLoad`.

### Unsafe Code Prohibition

**AC-15**: `#![forbid(unsafe_code)]` at crate level.
- **Verification**: Compiler enforcement. Any unsafe block in the crate causes a compile error.

### Dimension Accessor

**AC-16**: `OnnxProvider::dimension()` returns 384 for all catalog models.
- **Verification**: Unit test: for each `EmbeddingModel` variant, assert `model.dimension() == 384`. Integration test (if feasible with multiple models): assert `provider.dimension() == 384`.

### Model Catalog

**AC-17**: `EmbeddingModel` enum provides all 7 pre-configured 384-d models. Each variant encodes HuggingFace model ID, dimension, max sequence length, and ONNX filename.
- **Verification**: Unit test iterates all 7 variants, asserts each has a non-empty `model_id()`, `dimension() == 384`, `max_seq_length()` is either 256 or 512, and `onnx_filename()` is non-empty.

### Mean Pooling Correctness

**AC-18**: Mean pooling correctly applies attention mask weighting -- masked (padding) tokens do not contribute to the pooled embedding.
- **Verification**: Unit test with a hand-crafted example:
  - Token embeddings: `[[1.0, 2.0], [3.0, 4.0], [0.0, 0.0]]` (3 tokens, dim 2)
  - Attention mask: `[1, 1, 0]` (third token is padding)
  - Expected pooled: `[(1.0+3.0)/2, (2.0+4.0)/2] = [2.0, 3.0]`
  - Assert the pooling function output matches expected.

### Test Infrastructure

**AC-19**: Test infrastructure provides: helper functions for computing cosine similarity between embeddings, assertion helpers for dimension and normalization validation, and a mock provider for testing downstream consumers without ONNX model loading.
- **Verification**: Unit test: `MockProvider` implements `EmbeddingProvider`, `embed()` returns a 384-d normalized vector, `embed_batch()` returns one per text. `cosine_similarity` computes correct values for known vectors. `assert_dimension` and `assert_normalized` pass for valid embeddings and panic for invalid ones.

---

## 5. Domain Models

### 5.1 Core Types

#### EmbeddingProvider (trait)
The central abstraction for embedding generation. Decouples downstream consumers (vnc-001, vnc-002) from the specific inference backend. Enables mock implementations for testing.

#### OnnxProvider (struct)
The production implementation of `EmbeddingProvider`. Wraps an ONNX Runtime session and HuggingFace tokenizer. Handles tokenization, inference, pooling, and normalization internally.

#### EmbeddingModel (enum)
A pre-configured catalog of 7 sentence-transformer models. Each variant is a complete model specification (HuggingFace ID, dimension, max sequence length, ONNX filename). The model is selected at `EmbedConfig` construction time and fixed for the lifetime of the provider.

#### EmbedConfig (struct)
Configuration for provider construction. Includes model selection, cache directory override, batch size, and text separator.

#### EmbedError (enum)
Typed error for all failure modes: ONNX runtime errors, tokenizer errors, download failures, model load failures, dimension mismatches, and I/O errors.

#### MockProvider (struct, test-only)
A deterministic `EmbeddingProvider` implementation that produces hash-based normalized embeddings without requiring ONNX model files. Available behind the `test-support` feature flag.

### 5.2 Relationships

```
EmbeddingProvider (trait)
    ^                ^
    |                |
OnnxProvider    MockProvider (test-only)
    |
    | uses
    v
EmbeddingModel ─── EmbedConfig
    |
    | specifies download from
    v
HuggingFace Hub ──> Local Cache (~/.cache/unimatrix/models/)
    |
    | loads
    v
ort::Session + tokenizers::Tokenizer
    |
    | produces
    v
Vec<f32> (384-d, L2-normalized)
    |
    | consumed by
    v
VectorIndex::insert(entry_id, &embedding)  [nxs-002]
```

### 5.3 Integration Surface

**Upstream (text source):**
- `EntryRecord.title` and `EntryRecord.content` from unimatrix-store (nxs-001) provide the input text.

**Downstream (vector consumer):**
- `VectorIndex::insert(entry_id: u64, embedding: &[f32])` from unimatrix-vector (nxs-002) consumes the output.
- nxs-002 validates: `embedding.len() == 384`, no NaN, no infinity.

**Full write path (after nxs-003):**
```
Store::insert(NewEntry { title, content, ... })  -->  entry_id
    |
    v
prepare_text(title, content, ": ")  -->  text
    |
    v
EmbeddingProvider::embed(text)  -->  Vec<f32> (384-d, normalized)
    |
    v
VectorIndex::insert(entry_id, &embedding)  -->  hnsw_rs + VECTOR_MAP
```

### 5.4 Ubiquitous Language

| Term | Definition |
|------|-----------|
| **Embedding** | A 384-dimension float vector representing the semantic content of a text. L2-normalized to unit length. |
| **Provider** | An implementation of `EmbeddingProvider` that converts text to embeddings. |
| **OnnxProvider** | The production provider using ONNX Runtime for local inference. |
| **MockProvider** | A test-only provider that generates deterministic embeddings without model files. |
| **Model catalog** | The 7 pre-configured sentence-transformer models in `EmbeddingModel`. |
| **Mean pooling** | Averaging token embeddings weighted by the attention mask to produce a single sentence embedding. |
| **L2 normalization** | Scaling a vector to unit length (norm = 1.0). Required for DistDot compatibility. |
| **Attention mask** | A binary vector indicating real tokens (1) vs. padding tokens (0). Used in mean pooling. |
| **Text preparation** | Concatenating title and content with a separator before tokenization. |
| **Token limit** | The maximum number of word-piece tokens a model accepts (256 or 512). Enforced by the tokenizer via truncation. |
| **Cache directory** | Local filesystem path where downloaded model files are stored (`~/.cache/unimatrix/models/`). |

---

## 6. User Workflows

### Workflow 1: First-Time Provider Setup

1. Caller creates `EmbedConfig::default()` (or with custom settings).
2. Caller calls `OnnxProvider::new(config)`.
3. Provider checks cache directory for model files.
4. Model files not found: downloads from HuggingFace Hub (~90 MB).
5. Loads ONNX session and tokenizer.
6. Provider is ready for embedding generation.

### Workflow 2: Subsequent Provider Setup (Cached)

1. Caller creates `EmbedConfig::default()`.
2. Caller calls `OnnxProvider::new(config)`.
3. Provider finds cached model files in `~/.cache/unimatrix/models/`.
4. Loads ONNX session and tokenizer from cache. No network access.
5. Provider is ready.

### Workflow 3: Embed a Single Entry

1. Caller has `title` and `content` strings (from `EntryRecord` or user input).
2. Caller calls `embed_entry(&provider, title, content)`.
3. Internally: `prepare_text(title, content, ": ")` -> concatenated text.
4. Internally: `provider.embed(text)` -> tokenize, infer, pool, normalize.
5. Returns 384-d normalized `Vec<f32>`.
6. Caller passes embedding to `VectorIndex::insert(entry_id, &embedding)`.

### Workflow 4: Batch Embed Multiple Entries

1. Caller has a list of `(title, content)` pairs (e.g., during initial import or index rebuild).
2. Caller calls `embed_entries(&provider, &entries)`.
3. Internally: prepare all texts, then `provider.embed_batch(&texts)`.
4. Batch inference processes texts in chunks of `config.batch_size` (default 32).
5. Returns `Vec<Vec<f32>>` with one embedding per entry.
6. Caller inserts each into `VectorIndex`.

### Workflow 5: Index Rebuild (Re-embedding)

1. Vector index is lost or corrupt (crash recovery per nxs-002 Workflow 7).
2. Caller creates a new empty `VectorIndex`.
3. Caller iterates all entries from `Store` that had vector mappings.
4. For each entry: `embed_entry(&provider, entry.title, entry.content)`.
5. `VectorIndex::insert(entry.id, &embedding)`.
6. Index is restored.

### Workflow 6: Downstream Testing (Mock Provider)

1. Test code creates `MockProvider { dimension: 384 }`.
2. `MockProvider::embed("text")` returns a deterministic 384-d normalized vector.
3. Test verifies downstream logic (e.g., vnc-002 tool integration) without needing ONNX model files or network access.

---

## 7. Constraints

| Constraint | Source | Impact |
|------------|--------|--------|
| Rust edition 2024, MSRV 1.89 | Workspace setting | Cargo.toml inherits from workspace |
| `#![forbid(unsafe_code)]` | Project convention (nxs-001, nxs-002 precedent) | Compiler-enforced |
| Synchronous API only | nxs-001/nxs-002 pattern | No async fn, no runtime dependency |
| Fixed dimension: 384 | nxs-002 VectorConfig contract | All catalog models must produce 384-d output |
| Fixed model per provider lifetime | SCOPE.md constraint | No runtime model switching |
| Network required on first run only | Offline-first design | Model downloaded once, cached locally |
| ONNX session is `&mut self` | ort crate API | Requires Mutex for Send + Sync |
| No fastembed dependency | SCOPE.md resolved OQ-01 | Raw ort + tokenizers + hf-hub |
| CPU-only inference | SCOPE.md non-goal (GPU deferred) | No CUDA/Metal execution providers |
| Model file size ~90 MB | Pre-trained ONNX model | Not embedded in binary; downloaded at runtime |
| Token limits per model | Model architecture | 256 (MiniLM/Paraphrase) or 512 (BGE/E5/GTE) |

---

## 8. Dependencies

### Runtime Dependencies

| Dependency | Version | Purpose |
|-----------|---------|---------|
| `ort` | 2.0.0-rc.11 | ONNX Runtime inference (Session, Tensor) |
| `tokenizers` | latest compatible | HuggingFace tokenizer (tokenizer.json loading, encoding) |
| `hf-hub` | latest compatible | Model file download from HuggingFace Hub |
| `dirs` | latest compatible | Cross-platform cache directory resolution |
| `thiserror` | 2.x | Derive macros for EmbedError |

### Dev Dependencies

| Dependency | Version | Purpose |
|-----------|---------|---------|
| `tempfile` | 3.x | Temporary directories for test cache |

### Workspace Dependencies

| Dependency | Relationship | Purpose |
|-----------|-------------|---------|
| `unimatrix-store` | Not a direct dependency | Text source (EntryRecord.title, .content). Integration is at the caller level, not a crate dependency. |
| `unimatrix-vector` | Not a direct dependency | Embedding consumer (VectorIndex::insert). Integration is at the caller level. |

Note: `unimatrix-embed` is a standalone crate. It does not depend on `unimatrix-store` or `unimatrix-vector`. The integration between the three crates happens in the caller (vnc-001/vnc-002 or a future orchestration crate). This maintains clean dependency boundaries.

---

## 9. NOT in Scope

- **Vector index operations.** Producing `Vec<f32>` is nxs-003's job. Inserting into hnsw_rs is the caller's responsibility via nxs-002.
- **MCP tool exposure.** MCP tools (`context_store`, `context_search`) are vnc-001/vnc-002.
- **Async API.** Synchronous only. Async wrapping is the consumer's responsibility.
- **Custom model training or fine-tuning.** Pre-trained models only.
- **GPU inference.** CPU-only. GPU execution providers deferred to future iteration.
- **Streaming inference.** Batch-in, batch-out only.
- **Tokenizer training or vocabulary modification.** Uses pre-trained tokenizer as-is.
- **Embedding storage.** Embeddings are ephemeral outputs. Persistence is nxs-002's concern.
- **Content preprocessing beyond concatenation.** No summarization, chunking, or keyword extraction.
- **Runtime model switching.** Model fixed at provider construction.
- **API-based embedding providers.** Local-first only. OpenAI/Cohere fallback deferred.
- **Direct dependency on unimatrix-store or unimatrix-vector.** Integration is at the caller level.

---

## 10. Open Questions

**OQ-1: thiserror vs manual Error impl.**
nxs-001 and nxs-002 use manual `Display` + `Error` implementations. SCOPE.md specifies `thiserror` 2.0. The specification follows SCOPE.md and uses `thiserror` 2.0 for `EmbedError`. This is a minor divergence from the manual pattern in prior crates. If the architect prefers consistency, `thiserror` can be replaced with manual impls.
**Recommendation**: Use `thiserror` 2.0 as specified in SCOPE.md. It reduces boilerplate and is widely adopted.

**OQ-2: ort feature flags.**
The `ort` crate has several feature flags (`download-binaries`, `half`, etc.). The exact feature set needs to be determined during implementation. At minimum, `download-binaries` is likely needed for the ONNX Runtime shared library.
**Recommendation**: Determine exact ort features during architecture/implementation. The specification does not prescribe specific feature flags.

**OQ-3: MockProvider determinism strategy.**
The specification says MockProvider produces "deterministic hash-based" embeddings. The exact hashing strategy (e.g., hash the input string, use bytes to seed a sequence of floats, then normalize) is an implementation detail. The key requirement is: same input -> same output, different inputs -> different outputs (with high probability), and all outputs are normalized.
**Recommendation**: Architect decides the hashing approach. A simple strategy: fill the vector with zeros, then set a few dimensions based on a hash of the input, then normalize.

---

## 11. Key Specification Decisions

1. **Standalone crate with no store/vector dependency.** `unimatrix-embed` does not import `unimatrix-store` or `unimatrix-vector`. The three crates connect at the caller level. This keeps dependency boundaries clean and allows each crate to evolve independently.

2. **Mutex for thread safety.** The ort `Session` requires `&mut self` for inference. `OnnxProvider` wraps it in a `Mutex` to satisfy the `Send + Sync` requirement. This serializes inference calls but allows the provider to be shared across threads via `Arc`.

3. **Mean pooling (not CLS pooling).** Following the ruvector precedent and sentence-transformer convention, mean pooling with attention mask is used rather than CLS token extraction. Mean pooling produces better embeddings for sentence-level tasks.

4. **Normalization tolerance: 0.001.** The L2 norm guarantee allows for floating-point imprecision. This matches nxs-002's validation that rejects NaN/infinity but accepts minor float variance.

5. **Empty string is valid input (AC-12).** Rather than erroring on empty input, the model processes the special tokens (CLS, SEP) and produces a valid embedding. This simplifies caller logic — no need to special-case empty content.

6. **Default separator is `": "` (colon-space).** Matches common document titling conventions. The separator is configurable via `EmbedConfig` but the convenience functions `embed_entry` and `embed_entries` use the default.

7. **No rayon parallelism in v1.** Unlike ruvector which uses `rayon::par_iter` for batch pooling, nxs-003 keeps pooling sequential. At 384 dimensions and batch size 32, the pooling overhead is negligible compared to ONNX inference time. Parallelism adds a dependency and complexity without meaningful benefit at this scale.

8. **Cache directory uses dirs crate, not hf-hub default.** The cache lives at `~/.cache/unimatrix/models/` rather than HuggingFace's default `~/.cache/huggingface/`. This gives Unimatrix full control over cache layout and avoids interference with other HuggingFace tools.

9. **Seven models, all 384-d.** The catalog excludes 768-d models (e.g., all-mpnet-base-v2) because nxs-002's VectorConfig is fixed at 384 dimensions. Including non-384-d models would create a runtime error when the caller passes the embedding to VectorIndex.

10. **thiserror 2.0 for error handling.** This is the first crate in the workspace to use thiserror. It reduces error boilerplate compared to the manual approach in nxs-001/nxs-002. The architect may choose to align all crates in a future cleanup.
