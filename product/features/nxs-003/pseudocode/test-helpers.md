# C10: Test Helpers Module -- Pseudocode

## Purpose

Mock provider and assertion helpers for testing. Available behind `test-support` feature flag and `#[cfg(test)]`.

## File: `crates/unimatrix-embed/src/test_helpers.rs`

```
USE std::collections::hash_map::DefaultHasher
USE std::hash::{Hash, Hasher}
USE crate::provider::EmbeddingProvider
USE crate::normalize::l2_normalize
USE crate::error::Result

/// Mock embedding provider for testing without ONNX model files.
/// Produces deterministic hash-based embeddings.
#[cfg(any(test, feature = "test-support"))]
pub struct MockProvider {
    pub dimension: usize,
}

#[cfg(any(test, feature = "test-support"))]
IMPL MockProvider:
    pub fn new(dimension: usize) -> Self:
        MockProvider { dimension }

#[cfg(any(test, feature = "test-support"))]
IMPL EmbeddingProvider for MockProvider:
    fn embed(&self, text: &str) -> Result<Vec<f32>>:
        // Deterministic: hash text, use hash to seed embedding values
        hasher = DefaultHasher::new()
        text.hash(&mut hasher)
        seed = hasher.finish()

        embedding = vec![0.0_f32; self.dimension]

        // Fill embedding deterministically from hash
        FOR i IN 0..self.dimension:
            // Mix seed with index to get per-dimension value
            dim_hash = seed.wrapping_mul(6364136223846793005).wrapping_add(i as u64)
            // Map to [-1.0, 1.0] range
            embedding[i] = ((dim_hash as f32) / (u64::MAX as f32)) * 2.0 - 1.0

        // L2 normalize
        l2_normalize(&mut embedding)

        Ok(embedding)

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>:
        texts.iter().map(|t| self.embed(t)).collect()

    fn dimension(&self) -> usize:
        self.dimension

    fn name(&self) -> &str:
        "mock"

/// Compute cosine similarity between two embeddings.
/// Assumes both are L2-normalized (returns dot product directly).
#[cfg(any(test, feature = "test-support"))]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32:
    assert_eq!(a.len(), b.len(), "embeddings must have same dimension")
    dot = 0.0_f32
    FOR i IN 0..a.len():
        dot += a[i] * b[i]
    dot

/// Assert that an embedding has the expected dimension.
/// Panics if the dimension is wrong.
#[cfg(any(test, feature = "test-support"))]
pub fn assert_dimension(embedding: &[f32], expected: usize):
    assert_eq!(embedding.len(), expected,
        "expected dimension {expected}, got {}", embedding.len())

/// Assert that an embedding is L2-normalized within tolerance.
/// Panics if |norm - 1.0| >= tolerance.
#[cfg(any(test, feature = "test-support"))]
pub fn assert_normalized(embedding: &[f32], tolerance: f32):
    norm_sq = 0.0_f32
    FOR val IN embedding:
        norm_sq += val * val
    norm = norm_sq.sqrt()

    assert!((norm - 1.0).abs() < tolerance,
        "expected L2 norm ~1.0, got {norm} (tolerance: {tolerance})")
```

## Design Notes

- `MockProvider` uses `DefaultHasher` for deterministic hashing. Same input always produces same output.
- The hash mixing strategy creates different values per dimension by combining seed with dimension index.
- All mock embeddings are L2-normalized, matching the real provider behavior.
- `cosine_similarity` computes dot product directly since inputs are assumed L2-normalized.
- `assert_dimension` and `assert_normalized` are panic-based (standard for test assertions).
- All helpers gated behind `#[cfg(any(test, feature = "test-support"))]`.
- AC-19: Test infrastructure requirements.
