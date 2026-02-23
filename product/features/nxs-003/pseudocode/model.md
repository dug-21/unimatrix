# C3: Model Module -- Pseudocode

## Purpose

Define `EmbeddingModel` enum with 7 pre-configured sentence-transformer model variants and metadata accessors.

## File: `crates/unimatrix-embed/src/model.rs`

```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
ENUM EmbeddingModel:
    AllMiniLmL6V2           // default
    AllMiniLmL12V2
    MultiQaMiniLmL6
    ParaphraseMiniLmL6V2
    BgeSmallEnV15
    E5SmallV2
    GteSmall

IMPL Default for EmbeddingModel:
    fn default() -> Self:
        Self::AllMiniLmL6V2

IMPL EmbeddingModel:
    /// HuggingFace model repository ID
    pub fn model_id(&self) -> &'static str:
        MATCH self:
            AllMiniLmL6V2        => "sentence-transformers/all-MiniLM-L6-v2"
            AllMiniLmL12V2       => "sentence-transformers/all-MiniLM-L12-v2"
            MultiQaMiniLmL6      => "sentence-transformers/multi-qa-MiniLM-L6-cos-v1"
            ParaphraseMiniLmL6V2 => "sentence-transformers/paraphrase-MiniLM-L6-v2"
            BgeSmallEnV15        => "BAAI/bge-small-en-v1.5"
            E5SmallV2            => "intfloat/e5-small-v2"
            GteSmall             => "thenlper/gte-small"

    /// Output embedding dimension (always 384 for all catalog models)
    pub fn dimension(&self) -> usize:
        384

    /// Maximum input sequence length in word-piece tokens
    pub fn max_seq_length(&self) -> usize:
        MATCH self:
            AllMiniLmL6V2        => 256
            AllMiniLmL12V2       => 256
            MultiQaMiniLmL6      => 256
            ParaphraseMiniLmL6V2 => 256
            BgeSmallEnV15        => 512
            E5SmallV2            => 512
            GteSmall             => 512

    /// ONNX model filename within the repository
    pub fn onnx_filename(&self) -> &'static str:
        "model.onnx"

    /// Sanitized directory name for cache (slash -> underscore)
    pub fn cache_subdir(&self) -> String:
        self.model_id().replace('/', "_")
```

## Design Notes

- All models produce 384-d embeddings. `dimension()` returns a constant 384.
- `max_seq_length()` is 256 for MiniLM/Paraphrase models, 512 for BGE/E5/GTE.
- `onnx_filename()` is "model.onnx" for all models in the catalog.
- `cache_subdir()` sanitizes the model ID for filesystem use (e.g. "sentence-transformers/all-MiniLM-L6-v2" -> "sentence-transformers_all-MiniLM-L6-v2").
- Derives: Debug, Clone, Copy, PartialEq, Eq -- standard value-type derives.
