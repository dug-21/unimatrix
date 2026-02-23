# C10: Test Helpers Module -- Test Plan

## Tests

```
test_mock_provider_embed_returns_384d:
    mock = MockProvider::new(384)
    result = mock.embed("test text")
    ASSERT result.is_ok()
    ASSERT result.unwrap().len() == 384

test_mock_provider_embed_normalized:
    mock = MockProvider::new(384)
    embedding = mock.embed("test text").unwrap()
    norm = embedding.iter().map(|v| v * v).sum::<f32>().sqrt()
    ASSERT (norm - 1.0).abs() < 0.001

test_mock_provider_deterministic:
    mock = MockProvider::new(384)
    emb1 = mock.embed("same text").unwrap()
    emb2 = mock.embed("same text").unwrap()
    ASSERT emb1 == emb2

test_mock_provider_different_texts_different_embeddings:
    mock = MockProvider::new(384)
    emb1 = mock.embed("text one").unwrap()
    emb2 = mock.embed("text two").unwrap()
    ASSERT emb1 != emb2

test_mock_provider_embed_batch:
    mock = MockProvider::new(384)
    result = mock.embed_batch(&["a", "b", "c"])
    ASSERT result.is_ok()
    embeddings = result.unwrap()
    ASSERT embeddings.len() == 3
    FOR emb IN &embeddings:
        ASSERT emb.len() == 384

test_mock_provider_dimension:
    mock = MockProvider::new(384)
    ASSERT mock.dimension() == 384

test_mock_provider_name:
    mock = MockProvider::new(384)
    ASSERT mock.name() == "mock"

test_mock_provider_implements_trait:
    // Object safety with MockProvider
    mock = MockProvider::new(384)
    provider: &dyn EmbeddingProvider = &mock
    ASSERT provider.dimension() == 384
    ASSERT provider.name() == "mock"

test_cosine_similarity_identical:
    a = vec![1.0, 0.0, 0.0]
    sim = cosine_similarity(&a, &a)
    ASSERT (sim - 1.0).abs() < 1e-6

test_cosine_similarity_orthogonal:
    a = vec![1.0, 0.0, 0.0]
    b = vec![0.0, 1.0, 0.0]
    sim = cosine_similarity(&a, &b)
    ASSERT sim.abs() < 1e-6

test_cosine_similarity_opposite:
    a = vec![1.0, 0.0]
    b = vec![-1.0, 0.0]
    sim = cosine_similarity(&a, &b)
    ASSERT (sim - (-1.0)).abs() < 1e-6

test_assert_dimension_pass:
    embedding = vec![1.0; 384]
    assert_dimension(&embedding, 384)  // should not panic

test_assert_dimension_fail:
    embedding = vec![1.0; 256]
    // Should panic
    result = std::panic::catch_unwind(|| assert_dimension(&embedding, 384))
    ASSERT result.is_err()

test_assert_normalized_pass:
    embedding = vec![0.6, 0.8]  // norm = 1.0
    assert_normalized(&embedding, 0.001)  // should not panic

test_assert_normalized_fail:
    embedding = vec![3.0, 4.0]  // norm = 5.0
    result = std::panic::catch_unwind(|| assert_normalized(&embedding, 0.001))
    ASSERT result.is_err()
```

## Risks Covered

- AC-19: MockProvider implements EmbeddingProvider, cosine_similarity correct, assertion helpers work.
- R-12: MockProvider used as &dyn EmbeddingProvider verifies object safety.
