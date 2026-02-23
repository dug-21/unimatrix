# C7: Provider Module -- Pseudocode

## Purpose

Define the `EmbeddingProvider` trait -- the public abstraction over embedding implementations.

## File: `crates/unimatrix-embed/src/provider.rs`

```
USE crate::error::Result

/// The core abstraction for embedding generation.
///
/// Object-safe: can be used as &dyn EmbeddingProvider, Box<dyn EmbeddingProvider>,
/// Arc<dyn EmbeddingProvider>.
///
/// Send + Sync: enables sharing across threads via Arc.
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text string into a fixed-dimension vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed multiple texts in a batch. Returns one embedding per input.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// The output embedding dimension (384 for all catalog models).
    fn dimension(&self) -> usize;

    /// Human-readable model name for identification/logging.
    fn name(&self) -> &str;
}
```

## Design Notes

- No generic methods, no `Self` in return position -- ensures object safety (AC-09).
- `Send + Sync` supertrait bound enables `Arc<dyn EmbeddingProvider>` (AC-10).
- `embed_batch` takes `&[&str]` rather than `&[String]` for flexibility.
- `name()` returns `&str` (not `String`) to avoid allocation on each call.
- R-12: Object safety is a medium risk -- verified by compile-time tests.
