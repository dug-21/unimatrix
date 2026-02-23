# C9: Test Infrastructure -- Test Plan

## AC-16: Test Infrastructure

The test infrastructure is verified by being used in all other test modules. If the helpers are broken, all other tests fail.

### Verification Checklist

```
TestVectorIndex::new() exists and works:
    Verified by: every test in index.rs, persistence.rs uses it

TestVectorIndex::with_config(config) exists:
    Verified by: config tests in index.rs

random_normalized_embedding(dim) returns correct dimension:
    Verified by: all insert tests pass dimension validation

random_normalized_embedding(dim) returns L2-normalized vector:
    // Explicit test
    test_random_embedding_normalized:
        for _ in 0..10:
            emb = random_normalized_embedding(384)
            ASSERT emb.len() == 384
            norm = emb.iter().map(|x| x * x).sum::<f32>().sqrt()
            ASSERT (norm - 1.0).abs() < 0.001  // L2-normalized

assert_search_contains works:
    Verified by: used in filtered search and self-search tests

assert_search_excludes works:
    Verified by: used in filtered search exclusion tests

assert_results_sorted works:
    Verified by: used in similarity score tests

seed_vectors works:
    Verified by: used in persistence, filtered search, and many other tests

test-support feature accessible:
    // Downstream crates can use:
    // [dev-dependencies]
    // unimatrix-vector = { path = "../unimatrix-vector", features = ["test-support"] }
    // Verified by: build succeeds with feature flag
```

### Explicit Tests (in test_helpers.rs or index.rs tests)

```
test_random_embedding_dimension:
    for dim in [128, 384, 768]:
        emb = random_normalized_embedding(dim)
        ASSERT emb.len() == dim

test_random_embedding_l2_normalized:
    emb = random_normalized_embedding(384)
    norm_sq = emb.iter().map(|x| x * x).sum::<f32>()
    ASSERT (norm_sq - 1.0).abs() < 0.001

test_random_embedding_not_all_zeros:
    emb = random_normalized_embedding(384)
    ASSERT emb.iter().any(|&x| x != 0.0)

test_random_embeddings_different:
    emb1 = random_normalized_embedding(384)
    emb2 = random_normalized_embedding(384)
    ASSERT emb1 != emb2  // extremely unlikely to be equal
```

## Risks Covered
None directly. C9 is quality infrastructure that enables all other testing.
