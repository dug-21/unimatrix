use crate::error::Result;

/// The core abstraction for embedding generation.
///
/// Object-safe: can be used as `&dyn EmbeddingProvider`, `Box<dyn EmbeddingProvider>`,
/// or `Arc<dyn EmbeddingProvider>`.
///
/// The `Send + Sync` supertrait bound enables sharing a provider across threads
/// via `Arc<dyn EmbeddingProvider>`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // Uses MockProvider from test_helpers -- but since test_helpers depends on provider,
    // we use a minimal inline mock here for compile-time checks.
    struct MinimalMock;

    impl EmbeddingProvider for MinimalMock {
        fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.0; 384])
        }
        fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.0; 384]).collect())
        }
        fn dimension(&self) -> usize {
            384
        }
        fn name(&self) -> &str {
            "minimal-mock"
        }
    }

    #[test]
    fn test_trait_object_safety_dyn_ref() {
        fn use_provider(p: &dyn EmbeddingProvider) -> usize {
            p.dimension()
        }
        let provider = MinimalMock;
        let result = use_provider(&provider);
        assert_eq!(result, 384);
    }

    #[test]
    fn test_trait_object_safety_box() {
        fn use_boxed(p: Box<dyn EmbeddingProvider>) -> usize {
            p.dimension()
        }
        let provider = Box::new(MinimalMock) as Box<dyn EmbeddingProvider>;
        let result = use_boxed(provider);
        assert_eq!(result, 384);
    }

    #[test]
    fn test_trait_arc_dyn() {
        let provider: Arc<dyn EmbeddingProvider> = Arc::new(MinimalMock);
        assert_eq!(provider.dimension(), 384);
        let cloned = provider.clone();
        assert_eq!(cloned.dimension(), 384);
    }

    #[test]
    fn test_trait_all_methods_via_dyn() {
        fn exercise(p: &dyn EmbeddingProvider) {
            let _ = p.embed("test");
            let _ = p.embed_batch(&["a", "b"]);
            let _ = p.dimension();
            let _ = p.name();
        }
        let provider = MinimalMock;
        exercise(&provider);
    }
}
