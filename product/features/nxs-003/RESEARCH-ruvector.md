# Research: ruvector ONNX Embedding Implementation

**Source:** github.com/ruvnet/ruvector
**Date:** 2026-02-23
**Purpose:** Inform nxs-003 open questions (OQ-01 through OQ-04) with prior art from a production Rust vector database.

## Repository Context

ruvector is a Rust monorepo (85+ crates) for a vector/graph database. Workspace edition 2021, MSRV 1.77. Their embedding pipeline spans three layers:

1. **`ruvector-core`** — Trait-based `EmbeddingProvider` with `HashEmbedding` (placeholder) and `ApiEmbedding` (OpenAI/Cohere/Voyage)
2. **`examples/onnx-embeddings/`** — Standalone ONNX pipeline using raw `ort` + `tokenizers` (isolated from main workspace)
3. **`ruvllm`** — LLM inference via `candle` + `tokenizers` + `hf-hub` (separate concern)

## Findings by Open Question

### OQ-01: fastembed Edition 2024 Compatibility

**Finding: ruvector does NOT use fastembed-rs.** Zero references in the entire repository.

They built their own pipeline from:
- `ort` crate (ONNX Runtime) for inference
- `tokenizers` crate (HuggingFace) for tokenization
- Custom pooling (Mean, CLS, Max, MeanSqrtLen, LastToken, WeightedMean)
- Custom L2 normalization

The ONNX embedding example is a **standalone package** with its own `[workspace]` in Cargo.toml, explicitly isolated from the main workspace. This avoids dependency conflicts.

**Implication for nxs-003:** The raw `ort` + `tokenizers` approach is well-trodden in production Rust. If fastembed causes edition 2024 issues, we have a validated fallback path. The trade-off: ~800 lines of code for tokenization, pooling, and normalization that fastembed would handle automatically.

### OQ-02: Model Cache Location

**Finding: Custom directory, NOT HuggingFace default.**

```rust
// from examples/onnx-embeddings/src/config.rs
fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ruvector")
        .join("onnx-models")
}
```

Resolves to:
- Linux: `~/.cache/ruvector/onnx-models/`
- macOS: `~/Library/Caches/ruvector/onnx-models/`
- Windows: `{FOLDERID_LocalAppData}/ruvector/onnx-models/`

Model download lifecycle:
1. Check if model exists in cache (sanitized model ID as subdirectory, e.g., `sentence-transformers_all-MiniLM-L6-v2/model.onnx`)
2. If not cached, download from HuggingFace via raw HTTPS (`reqwest`, not `hf-hub`)
3. Try root path first, then `onnx/` subfolder
4. Also download `tokenizer.json` and `config.json`
5. Progress bar via `indicatif` during download
6. No checksum verification for HuggingFace downloads

**Implication for nxs-003:** Confirms our instinct to use `~/.cache/unimatrix/models/` rather than the HuggingFace default. The `dirs` crate handles cross-platform cache paths cleanly. If we use fastembed, we'd need to override its default cache path; if raw ort, we control it directly.

### OQ-03: API Fallback Dimension Reduction

**Finding: No dimension reduction implemented.**

Their `ApiEmbedding` supports OpenAI, Cohere, and Voyage at **native dimensions**:
- OpenAI `text-embedding-3-small`: 1536d
- OpenAI `text-embedding-3-large`: 3072d
- Cohere `embed-english-v3.0`: 1024d
- Voyage `voyage-2`: 1024d

They do NOT use OpenAI's `dimensions` parameter to truncate to 384. No fallback chain between local and API providers exists — a single provider is chosen at construction time and fixed.

```rust
// Their trait — nearly identical to our proposed EmbeddingProvider
pub trait EmbeddingProvider: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn dimensions(&self) -> usize;
    fn name(&self) -> &str;
}
pub type BoxedEmbeddingProvider = Arc<dyn EmbeddingProvider>;
```

**Implication for nxs-003:** This question remains open — ruvector doesn't validate or invalidate the `dimensions: 384` approach with OpenAI. We need to verify quality independently. Their lack of a fallback chain is a gap we should fill: our `EmbeddingProvider` trait should make local-to-API switching seamless.

### OQ-04: ort Crate RC Stability

**Finding: Uses `ort 2.0.0-rc.9` (resolves to rc.10 in lockfile), no stability issues noted.**

```toml
# from examples/onnx-embeddings/Cargo.toml
ort = { version = "2.0.0-rc.9", features = ["download-binaries", "half"] }
```

Session creation pattern:
```rust
let mut builder = Session::builder()?;
builder = builder.with_optimization_level(GraphOptimizationLevel::Level3)?;
builder = builder.with_intra_threads(config.num_threads)?;
let session = builder.commit_from_file(path)?;
```

Tensor creation (ort 2.0 API):
```rust
let input_ids_tensor = Tensor::from_array((
    vec![batch_size, seq_length],
    input_ids.to_vec().into_boxed_slice(),
))?;
```

**Implication for nxs-003:** The ort 2.0 RC is stable enough for production use. If we go the raw `ort` route (OQ-01 fallback), the API surface is straightforward. fastembed pins `ort = "=2.0.0-rc.11"` (exact), which is slightly newer and equally stable.

## Additional Findings

### Pre-configured Model Catalog

8 models, 7 at 384d:

| Model | HuggingFace ID | Dim | Max Seq |
|-------|---------------|-----|---------|
| AllMiniLmL6V2 (default) | sentence-transformers/all-MiniLM-L6-v2 | 384 | 256 |
| AllMiniLmL12V2 | sentence-transformers/all-MiniLM-L12-v2 | 384 | 256 |
| AllMpnetBaseV2 | sentence-transformers/all-mpnet-base-v2 | 768 | 384 |
| MultiQaMiniLmL6 | sentence-transformers/multi-qa-MiniLM-L6-cos-v1 | 384 | 256 |
| ParaphraseMiniLmL6V2 | sentence-transformers/paraphrase-MiniLM-L6-v2 | 384 | 256 |
| BgeSmallEnV15 | BAAI/bge-small-en-v1.5 | 384 | 512 |
| E5SmallV2 | intfloat/e5-small-v2 | 384 | 512 |
| GteSmall | thenlper/gte-small | 384 | 512 |

All 384d models are compatible with our nxs-002 VectorIndex (DistDot, 384 dimensions).

### L2 Normalization

```rust
// from examples/onnx-embeddings/src/pooling.rs
pub fn normalize_vector(vec: &[f32]) -> Vec<f32> {
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-12 {
        vec.iter().map(|x| x / norm).collect()
    } else {
        vec.to_vec()
    }
}
```

Applied after pooling, controlled by a `normalize` boolean on the `Pooler` struct. All pretrained model configs set `normalize_output() -> true`.

### Batch Processing

- Configurable `batch_size` (default 32)
- Texts chunked into batches in `Embedder::embed()`
- Each batch tokenized together, padded to longest sequence in batch
- Parallel pooling via `rayon` (`par_iter()` over batch results)
- ONNX session requires `&mut self` (no concurrent inference on single session)

### Error Handling

Uses `thiserror` 2.0:
- `OnnxRuntime(#[from] ort::Error)`
- `Tokenizer(#[from] tokenizers::tokenizer::Error)`
- Specific variants: `ModelNotFound`, `TokenizerNotFound`, `InvalidModel`, `DimensionMismatch`, `EmptyInput`, `BatchSizeExceeded`, `SequenceTooLong`, `DownloadFailed`
- Helper methods: `is_recoverable()`, `is_config_error()`

### hnsw_rs Patch

They also patch hnsw_rs dependencies (rand 0.8 vs 0.9 for WASM), similar to our anndists patch:
```toml
[patch.crates-io]
hnsw_rs = { path = "./patches/hnsw_rs" }
```

### WASM Alternative

For WASM targets, they use `tract-onnx` 0.21 instead of `ort` (ort links native ONNX Runtime which cannot compile to WASM).

## Architectural Gap

The ONNX embedding pipeline (`examples/onnx-embeddings/`) does NOT implement the core `EmbeddingProvider` trait from `ruvector-core`. There is no bridge between the standalone ONNX crate and the main database's embedding abstraction. This is a design gap — the ONNX pipeline exists as a reference implementation rather than a first-class integration.

**Lesson for nxs-003:** Our `LocalProvider` must implement `EmbeddingProvider` from day one, ensuring the ONNX pipeline is a proper trait implementation, not a sidecar.

## Decision Impact Summary

| Question | ruvector Answer | nxs-003 Recommendation |
|----------|----------------|----------------------|
| OQ-01 | No fastembed; raw ort+tokenizers | Try fastembed first; raw ort is validated fallback |
| OQ-02 | Custom cache dir via `dirs` crate | Use `~/.cache/unimatrix/models/` |
| OQ-03 | No dimension reduction | Must validate independently |
| OQ-04 | ort 2.0.0-rc.9/10, stable | Acceptable risk; fastembed pins rc.11 |
