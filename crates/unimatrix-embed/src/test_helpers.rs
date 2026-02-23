use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::error::Result;
use crate::normalize::l2_normalize;
use crate::provider::EmbeddingProvider;

/// Mock embedding provider for testing without ONNX model files.
///
/// Produces deterministic hash-based embeddings that are L2-normalized.
/// Same input always produces the same output. Different inputs produce
/// different outputs (with high probability).
pub struct MockProvider {
    pub dimension: usize,
}

impl MockProvider {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

impl EmbeddingProvider for MockProvider {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let seed = hasher.finish();

        let mut embedding = vec![0.0_f32; self.dimension];

        for (i, val) in embedding.iter_mut().enumerate() {
            // Mix seed with index to get per-dimension value
            let dim_hash = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(i as u64);
            // Map to [-1.0, 1.0] range
            *val = ((dim_hash as f32) / (u64::MAX as f32)) * 2.0 - 1.0;
        }

        l2_normalize(&mut embedding);

        Ok(embedding)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn name(&self) -> &str {
        "mock"
    }
}

/// Compute cosine similarity between two embeddings.
///
/// For L2-normalized vectors, this equals the dot product.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "embeddings must have same dimension");
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Assert that an embedding has the expected dimension.
///
/// Panics if the dimension does not match.
pub fn assert_dimension(embedding: &[f32], expected: usize) {
    assert_eq!(
        embedding.len(),
        expected,
        "expected dimension {expected}, got {}",
        embedding.len()
    );
}

/// Assert that an embedding is L2-normalized within the given tolerance.
///
/// Panics if `|norm - 1.0| >= tolerance`.
pub fn assert_normalized(embedding: &[f32], tolerance: f32) {
    let norm: f32 = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
    assert!(
        (norm - 1.0).abs() < tolerance,
        "expected L2 norm ~1.0, got {norm} (tolerance: {tolerance})"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_provider_embed_returns_384d() {
        let mock = MockProvider::new(384);
        let result = mock.embed("test text");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 384);
    }

    #[test]
    fn test_mock_provider_embed_normalized() {
        let mock = MockProvider::new(384);
        let embedding = mock.embed("test text").unwrap();
        let norm: f32 = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_mock_provider_deterministic() {
        let mock = MockProvider::new(384);
        let emb1 = mock.embed("same text").unwrap();
        let emb2 = mock.embed("same text").unwrap();
        assert_eq!(emb1, emb2);
    }

    #[test]
    fn test_mock_provider_different_texts_different_embeddings() {
        let mock = MockProvider::new(384);
        let emb1 = mock.embed("text one").unwrap();
        let emb2 = mock.embed("text two").unwrap();
        assert_ne!(emb1, emb2);
    }

    #[test]
    fn test_mock_provider_embed_batch() {
        let mock = MockProvider::new(384);
        let result = mock.embed_batch(&["a", "b", "c"]);
        assert!(result.is_ok());
        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 3);
        for emb in &embeddings {
            assert_eq!(emb.len(), 384);
        }
    }

    #[test]
    fn test_mock_provider_dimension() {
        let mock = MockProvider::new(384);
        assert_eq!(mock.dimension(), 384);
    }

    #[test]
    fn test_mock_provider_name() {
        let mock = MockProvider::new(384);
        assert_eq!(mock.name(), "mock");
    }

    #[test]
    fn test_mock_provider_implements_trait() {
        let mock = MockProvider::new(384);
        let provider: &dyn EmbeddingProvider = &mock;
        assert_eq!(provider.dimension(), 384);
        assert_eq!(provider.name(), "mock");
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_assert_dimension_pass() {
        let embedding = vec![1.0; 384];
        assert_dimension(&embedding, 384);
    }

    #[test]
    #[should_panic(expected = "expected dimension 384")]
    fn test_assert_dimension_fail() {
        let embedding = vec![1.0; 256];
        assert_dimension(&embedding, 384);
    }

    #[test]
    fn test_assert_normalized_pass() {
        let embedding = vec![0.6, 0.8]; // norm = 1.0
        assert_normalized(&embedding, 0.001);
    }

    #[test]
    #[should_panic(expected = "expected L2 norm")]
    fn test_assert_normalized_fail() {
        let embedding = vec![3.0, 4.0]; // norm = 5.0
        assert_normalized(&embedding, 0.001);
    }
}
