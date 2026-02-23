# C7: Provider Module -- Test Plan

## Tests

```
test_trait_object_safety_dyn_ref:
    // Compile-time test: function accepts &dyn EmbeddingProvider
    fn use_provider(p: &dyn EmbeddingProvider) -> usize {
        p.dimension()
    }
    provider = MockProvider::new(384)
    result = use_provider(&provider)
    ASSERT result == 384

test_trait_object_safety_box:
    // Compile-time test: Box<dyn EmbeddingProvider>
    fn use_boxed(p: Box<dyn EmbeddingProvider>) -> usize {
        p.dimension()
    }
    provider = Box::new(MockProvider::new(384)) as Box<dyn EmbeddingProvider>
    result = use_boxed(provider)
    ASSERT result == 384

test_trait_arc_dyn:
    // Arc<dyn EmbeddingProvider> for shared ownership
    provider: Arc<dyn EmbeddingProvider> = Arc::new(MockProvider::new(384))
    ASSERT provider.dimension() == 384
    cloned = provider.clone()
    ASSERT cloned.dimension() == 384

test_trait_all_methods_via_dyn:
    fn exercise(p: &dyn EmbeddingProvider) {
        // All four methods callable via dyn ref
        let _ = p.embed("test");
        let _ = p.embed_batch(&["a", "b"]);
        let _ = p.dimension();
        let _ = p.name();
    }
    provider = MockProvider::new(384)
    exercise(&provider)  // Compiles = object safety verified
```

## Risks Covered

- R-12: EmbeddingProvider trait object safety (AC-09).
- All four trait methods callable via &dyn, Box<dyn>, Arc<dyn>.
