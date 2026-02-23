# nxs-003: Embedding Pipeline — Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/nxs-003/SCOPE.md |
| Specification | product/features/nxs-003/specification/SPECIFICATION.md |
| Architecture | product/features/nxs-003/architecture/ARCHITECTURE.md |
| Risk Strategy | product/features/nxs-003/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nxs-003/ALIGNMENT-REPORT.md |
| ADR-001 | product/features/nxs-003/architecture/ADR-001-mutex-session-concurrency.md |
| ADR-002 | product/features/nxs-003/architecture/ADR-002-raw-ort-no-fastembed.md |
| ADR-003 | product/features/nxs-003/architecture/ADR-003-hf-hub-model-download.md |
| ADR-004 | product/features/nxs-003/architecture/ADR-004-custom-cache-directory.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| error | pseudocode/error.md | test-plan/error.md |
| config | pseudocode/config.md | test-plan/config.md |
| model | pseudocode/model.md | test-plan/model.md |
| normalize | pseudocode/normalize.md | test-plan/normalize.md |
| pooling | pseudocode/pooling.md | test-plan/pooling.md |
| text | pseudocode/text.md | test-plan/text.md |
| provider | pseudocode/provider.md | test-plan/provider.md |
| download | pseudocode/download.md | test-plan/download.md |
| onnx | pseudocode/onnx.md | test-plan/onnx.md |
| test-helpers | pseudocode/test-helpers.md | test-plan/test-helpers.md |
| lib | pseudocode/lib.md | test-plan/lib.md |

## Goal

Create a `unimatrix-embed` library crate that generates 384-dimensional L2-normalized embeddings from text using local ONNX Runtime inference, bridging content authoring (nxs-001 `Store::insert`) and vector search (nxs-002 `VectorIndex::insert`). The crate is built on raw `ort` + `tokenizers` + `hf-hub` (no fastembed), defines an `EmbeddingProvider` trait for provider abstraction, and ships an `OnnxProvider` with a catalog of 7 pre-configured sentence-transformer models, automatic model download/caching, title+content concatenation, mean pooling with attention mask, and configurable batch inference.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| ONNX session concurrency | `Mutex<ort::Session>` for thread-safe inference serialization. Tokenizer is lock-free (`&self` methods). | Architecture Section 3 | architecture/ADR-001-mutex-session-concurrency.md |
| No fastembed wrapper | Raw `ort` + `tokenizers` + `hf-hub` for full pipeline control, avoids edition 2024 risk and exact-pinned ort version constraint. | SCOPE OQ-01, Architecture | architecture/ADR-002-raw-ort-no-fastembed.md |
| Model download mechanism | `hf-hub` crate for HuggingFace Hub downloads. Handles repository structure, anonymous access, revision tracking. | Architecture Section 7 | architecture/ADR-003-hf-hub-model-download.md |
| Custom cache directory | `~/.cache/unimatrix/models/` via `dirs` crate. Model ID sanitized (slash→underscore) for subdirectory names. Configurable via `EmbedConfig.cache_dir`. | SCOPE OQ-02, Architecture | architecture/ADR-004-custom-cache-directory.md |
| No API-based providers | Local-first only. 7 pre-configured local 384-d models provide variety. API fallback deferred. | SCOPE OQ-03 | — |
| ort RC stability | `ort` 2.0.0-rc.11 acceptable. Validated by ruvector production use of rc.9/10. | SCOPE OQ-04 | — |
| thiserror 2.0 | First crate in workspace to use `thiserror`. Reduces error boilerplate vs manual impls in nxs-001/nxs-002. | SCOPE constraints, Specification OQ-1 | — |
| Error enum shape | Use Architecture's `ModelNotFound { path }` and `EmptyInput` variants. `ModelNotFound` is more specific than Specification's `ModelLoad(String)`. Drop `EmptyInput` unless batch_size=0 handling needs it. Resolve during implementation (see W2 in Alignment). | Alignment Report W2 | — |
| Mean pooling (not CLS) | Attention-masked mean pooling. Better sentence-level embeddings than CLS token extraction. Follows ruvector and sentence-transformer convention. | Specification Key Decision 3 | — |
| Synchronous API | No async runtime dependency. Matches nxs-001/nxs-002. Downstream wraps with `spawn_blocking`. | SCOPE constraints | — |
| Empty input is valid | Empty string returns a valid embedding (model processes CLS/SEP tokens). Simplifies caller logic. | AC-12, Specification Key Decision 5 | — |

## Files to Create

| Path | Summary |
|------|---------|
| `crates/unimatrix-embed/Cargo.toml` | Package metadata, dependencies (ort, tokenizers, hf-hub, dirs, thiserror), `test-support` feature flag |
| `crates/unimatrix-embed/src/lib.rs` | Crate root with `#![forbid(unsafe_code)]`, module declarations, public re-exports |
| `crates/unimatrix-embed/src/error.rs` | `EmbedError` enum (thiserror 2.0), `Result` type alias |
| `crates/unimatrix-embed/src/config.rs` | `EmbedConfig` struct with defaults (model, cache_dir, batch_size, separator) |
| `crates/unimatrix-embed/src/model.rs` | `EmbeddingModel` enum (7 variants) with metadata accessor methods |
| `crates/unimatrix-embed/src/normalize.rs` | `l2_normalize()` and `l2_normalized()` functions |
| `crates/unimatrix-embed/src/pooling.rs` | `mean_pool()` function for attention-masked mean pooling |
| `crates/unimatrix-embed/src/text.rs` | `prepare_text()`, `embed_entry()`, `embed_entries()` convenience functions |
| `crates/unimatrix-embed/src/provider.rs` | `EmbeddingProvider` trait definition (object-safe, Send + Sync) |
| `crates/unimatrix-embed/src/download.rs` | `ensure_model()` — download from HuggingFace Hub via hf-hub, cache management |
| `crates/unimatrix-embed/src/onnx.rs` | `OnnxProvider` struct — construction, tokenization, inference, pooling, normalization |
| `crates/unimatrix-embed/src/test_helpers.rs` | `MockProvider`, `cosine_similarity()`, `assert_dimension()`, `assert_normalized()` |

## Files to Modify

| Path | Summary |
|------|---------|
| `Cargo.toml` (workspace root) | Add `crates/unimatrix-embed` to workspace members (already uses glob `crates/*`) |

## Data Structures

### EmbeddingProvider (trait)

```rust
pub trait EmbeddingProvider: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
    fn name(&self) -> &str;
}
```

Object-safe. No generic methods, no `Self` in return position. Enables `&dyn EmbeddingProvider`, `Box<dyn EmbeddingProvider>`, `Arc<dyn EmbeddingProvider>`.

### EmbeddingModel (enum)

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
```

Methods: `model_id() -> &'static str`, `dimension() -> usize` (always 384), `max_seq_length() -> usize` (256 or 512), `onnx_filename() -> &'static str`. Implements `Default` (AllMiniLmL6V2).

### EmbedConfig (struct)

```rust
#[derive(Debug, Clone)]
pub struct EmbedConfig {
    pub model: EmbeddingModel,          // default: AllMiniLmL6V2
    pub cache_dir: Option<PathBuf>,     // default: None → ~/.cache/unimatrix/models/
    pub batch_size: usize,              // default: 32
    pub separator: String,              // default: ": "
}
```

### OnnxProvider (struct)

```rust
pub struct OnnxProvider {
    session: Mutex<ort::Session>,         // locked only during inference
    tokenizer: tokenizers::Tokenizer,     // lock-free, &self methods
    model: EmbeddingModel,
    config: EmbedConfig,
}
```

Must be `Send + Sync`. Implements `EmbeddingProvider`.

### EmbedError (enum)

```rust
#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    #[error("onnx runtime error: {0}")]
    OnnxRuntime(#[from] ort::Error),

    #[error("tokenizer error: {0}")]
    Tokenizer(String),

    #[error("model download failed: {0}")]
    Download(String),

    #[error("model not found: {path}")]
    ModelNotFound { path: PathBuf },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },
}
```

Implements `std::error::Error` and `Display` via thiserror 2.0. `From` impls for `ort::Error` and `std::io::Error`.

### MockProvider (struct, test-only)

```rust
#[cfg(any(test, feature = "test-support"))]
pub struct MockProvider {
    pub dimension: usize,
}
```

Deterministic hash-based embeddings. `embed(text)` returns L2-normalized 384-d vector derived from text hash. Available behind `test-support` feature.

## Function Signatures

### Core Provider

```rust
// onnx.rs
impl OnnxProvider {
    pub fn new(config: EmbedConfig) -> Result<Self>;
}

impl EmbeddingProvider for OnnxProvider {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
    fn name(&self) -> &str;
}
```

### Text Preparation & Convenience

```rust
// text.rs
pub fn prepare_text(title: &str, content: &str, separator: &str) -> String;
pub fn embed_entry(provider: &dyn EmbeddingProvider, title: &str, content: &str) -> Result<Vec<f32>>;
pub fn embed_entries(provider: &dyn EmbeddingProvider, entries: &[(String, String)]) -> Result<Vec<Vec<f32>>>;
```

### Normalization

```rust
// normalize.rs
pub fn l2_normalize(embedding: &mut Vec<f32>);
pub fn l2_normalized(embedding: &[f32]) -> Vec<f32>;
```

### Pooling

```rust
// pooling.rs
pub fn mean_pool(
    token_embeddings: &[f32],
    attention_mask: &[i64],
    batch_size: usize,
    seq_len: usize,
    hidden_dim: usize,
) -> Vec<Vec<f32>>;
```

### Download

```rust
// download.rs
pub fn ensure_model(model: EmbeddingModel, cache_dir: &Path) -> Result<PathBuf>;
```

### Test Helpers

```rust
// test_helpers.rs (behind test-support feature)
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32;
pub fn assert_dimension(embedding: &[f32], expected: usize);
pub fn assert_normalized(embedding: &[f32], tolerance: f32);
```

## Constraints

| Constraint | Source |
|------------|--------|
| Rust edition 2024, MSRV 1.89 | Workspace setting |
| `#![forbid(unsafe_code)]` | Project convention (nxs-001/nxs-002 precedent) |
| Synchronous API only — no async fn, no runtime dependency | nxs-001/nxs-002 pattern |
| Fixed output dimension: 384 for all catalog models | nxs-002 VectorConfig contract |
| Fixed model per provider lifetime — no runtime switching | SCOPE constraint |
| Network required on first run only — offline after cache populated | SCOPE constraint |
| ONNX Session requires `&mut self` for `run()` — Mutex required | ort crate API (ADR-001) |
| No fastembed dependency | SCOPE OQ-01 (ADR-002) |
| CPU-only inference — no CUDA/Metal execution providers | SCOPE non-goal |
| Model file ~90 MB — downloaded at runtime, not embedded in binary | SCOPE constraint |
| Token limits: 256 (MiniLM/Paraphrase) or 512 (BGE/E5/GTE) | Model architecture |
| L2 norm tolerance: \|norm - 1.0\| < 0.001 | AC-05, nxs-002 DistDot compatibility |
| No dependency on unimatrix-store or unimatrix-vector | Architecture decision (standalone crate) |

## Dependencies

### Runtime

| Crate | Version | Purpose |
|-------|---------|---------|
| `ort` | 2.0.0-rc.11 | ONNX Runtime inference (Session, Tensor). Feature: `download-binaries`. |
| `tokenizers` | 0.21 | HuggingFace tokenizer loading and encoding. Features: `onig`, `default-features = false`. |
| `hf-hub` | 0.4 | Model file download from HuggingFace Hub. |
| `dirs` | 6 | Cross-platform cache directory resolution. |
| `thiserror` | 2 | Error derive macros for `EmbedError`. |

### Dev

| Crate | Version | Purpose |
|-------|---------|---------|
| `approx` | 0.5 | Floating-point comparison in tests. |

### Workspace (no direct dependency)

| Crate | Relationship |
|-------|-------------|
| `unimatrix-store` (nxs-001) | Text source (`EntryRecord.title`, `.content`). Integration at caller level. |
| `unimatrix-vector` (nxs-002) | Embedding consumer (`VectorIndex::insert`). Integration at caller level. |

## NOT in Scope

- Vector index operations (nxs-002's concern)
- MCP tool exposure (vnc-001/vnc-002)
- Async API (sync only; consumers wrap with `spawn_blocking`)
- Custom model training or fine-tuning
- GPU inference (CPU-only; GPU deferred)
- Streaming inference
- Tokenizer training or vocabulary modification
- Embedding storage (embeddings are ephemeral; persistence is nxs-002's concern)
- Content preprocessing beyond concatenation (no summarization, chunking, keyword extraction)
- Runtime model switching
- API-based embedding providers (local-first only)
- Direct dependency on unimatrix-store or unimatrix-vector

## Alignment Status

**4 PASS, 2 WARN, 0 VARIANCE, 0 FAIL.** No variances requiring human approval.

- **W1 (Vision text)**: PRODUCT-VISION.md mentions "API-based fallback" but SCOPE.md resolved OQ-03 to exclude it. Human-approved scope decision. *Recommendation*: Update PRODUCT-VISION.md nxs-003 entry to reflect "local models only" resolution.
- **W2 (Error enum)**: Error enum variants differ between Architecture (`ModelNotFound { path }`, `EmptyInput`) and Specification (`ModelLoad(String)`, no `EmptyInput`). Minor doc inconsistency. *Recommendation*: Resolve during implementation — favor Architecture's `ModelNotFound { path }` for specificity; drop `EmptyInput` unless batch_size=0 handling requires it.

## Implementation Order

Components should be implemented in dependency order:

1. **error** — no internal dependencies; everything else uses `EmbedError` / `Result`
2. **config** + **model** — depend on error only; `EmbedConfig` and `EmbeddingModel` are pure data
3. **normalize** — depends on error only; standalone arithmetic
4. **pooling** — depends on error only; standalone arithmetic with attention mask
5. **text** — depends on provider trait (interface only)
6. **provider** — trait definition; depends on error
7. **download** — depends on model, config, error; uses hf-hub + dirs
8. **onnx** — depends on all above; the main implementation
9. **test-helpers** — depends on provider, normalize; gated behind `test-support`
10. **lib** — crate root; re-exports all public items

## Risk Hotspots (Test First)

From RISK-TEST-STRATEGY.md, ordered by priority:

1. **R-01: L2 normalization** (Critical) — If embeddings aren't normalized, every DistDot score in nxs-002 is wrong.
2. **R-02: Mean pooling attention mask** (Critical) — If padding tokens leak into pooling, batch embeddings silently diverge.
3. **R-03: Batch vs single consistency** (High) — AC-11 compliance. Primary signal that the entire pipeline is correct.
4. **R-04: ONNX model loading** (High) — Gate test; nothing else works without successful model load.
5. **R-09: Empty/degenerate input** (Medium) — High likelihood edge cases that downstream consumers hit immediately.
6. **R-08: Title+content concatenation** (Medium) — Fundamental to every embedding.

## Model Catalog Reference

| Variant | HuggingFace ID | Dim | Max Seq | ONNX File |
|---------|---------------|-----|---------|-----------|
| AllMiniLmL6V2 (default) | sentence-transformers/all-MiniLM-L6-v2 | 384 | 256 | model.onnx |
| AllMiniLmL12V2 | sentence-transformers/all-MiniLM-L12-v2 | 384 | 256 | model.onnx |
| MultiQaMiniLmL6 | sentence-transformers/multi-qa-MiniLM-L6-cos-v1 | 384 | 256 | model.onnx |
| ParaphraseMiniLmL6V2 | sentence-transformers/paraphrase-MiniLM-L6-v2 | 384 | 256 | model.onnx |
| BgeSmallEnV15 | BAAI/bge-small-en-v1.5 | 384 | 512 | model.onnx |
| E5SmallV2 | intfloat/e5-small-v2 | 384 | 512 | model.onnx |
| GteSmall | thenlper/gte-small | 384 | 512 | model.onnx |
