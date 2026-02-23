# C4: Normalize Module -- Test Plan

## Tests

```
test_l2_normalize_known_vector:
    embedding = vec![3.0, 4.0]
    l2_normalize(&mut embedding)
    // norm = 5.0, result = [0.6, 0.8]
    ASSERT (embedding[0] - 0.6).abs() < 1e-6
    ASSERT (embedding[1] - 0.8).abs() < 1e-6

test_l2_normalize_unit_vector:
    embedding = vec![1.0, 0.0, 0.0]
    l2_normalize(&mut embedding)
    ASSERT (embedding[0] - 1.0).abs() < 1e-6
    ASSERT embedding[1].abs() < 1e-6
    ASSERT embedding[2].abs() < 1e-6

test_l2_normalize_result_has_unit_norm:
    embedding = vec![1.0, 2.0, 3.0, 4.0, 5.0]
    l2_normalize(&mut embedding)
    norm_sq = embedding.iter().map(|v| v * v).sum::<f32>()
    ASSERT (norm_sq.sqrt() - 1.0).abs() < 0.001

test_l2_normalize_near_zero_vector:
    // Near-zero vector: should NOT normalize (avoid noise amplification)
    embedding = vec![1e-13, 1e-13, 1e-13]
    original = embedding.clone()
    l2_normalize(&mut embedding)
    ASSERT embedding == original  // unchanged

test_l2_normalize_zero_vector:
    embedding = vec![0.0, 0.0, 0.0]
    l2_normalize(&mut embedding)
    ASSERT embedding == vec![0.0, 0.0, 0.0]  // unchanged

test_l2_normalize_384d:
    // Realistic 384-d vector
    embedding = (0..384).map(|i| (i as f32) * 0.01).collect::<Vec<_>>()
    l2_normalize(&mut embedding)
    norm_sq = embedding.iter().map(|v| v * v).sum::<f32>()
    ASSERT (norm_sq.sqrt() - 1.0).abs() < 0.001

test_l2_normalize_negative_values:
    embedding = vec![-3.0, 4.0, -5.0]
    l2_normalize(&mut embedding)
    norm_sq = embedding.iter().map(|v| v * v).sum::<f32>()
    ASSERT (norm_sq.sqrt() - 1.0).abs() < 0.001

test_l2_normalize_single_large_value:
    embedding = vec![0.0, 0.0, 1000.0, 0.0]
    l2_normalize(&mut embedding)
    ASSERT (embedding[2] - 1.0).abs() < 1e-6  // should be [0, 0, 1, 0]
    ASSERT embedding[0].abs() < 1e-6

test_l2_normalize_all_equal:
    n = 384
    val = 1.0_f32
    embedding = vec![val; n]
    l2_normalize(&mut embedding)
    expected = 1.0 / (n as f32).sqrt()
    FOR v IN &embedding:
        ASSERT (v - expected).abs() < 1e-6

test_l2_normalized_returns_new_vector:
    original = vec![3.0, 4.0]
    normalized = l2_normalized(&original)
    // Original unchanged
    ASSERT original == vec![3.0, 4.0]
    // Result normalized
    ASSERT (normalized[0] - 0.6).abs() < 1e-6
    ASSERT (normalized[1] - 0.8).abs() < 1e-6

test_l2_normalize_deterministic:
    embedding1 = vec![1.0, 2.0, 3.0]
    embedding2 = vec![1.0, 2.0, 3.0]
    l2_normalize(&mut embedding1)
    l2_normalize(&mut embedding2)
    ASSERT embedding1 == embedding2
```

## Risks Covered

- R-01 (Critical): L2 normalization produces unit vectors. Tolerance < 0.001.
- R-01 scenario 5: Near-zero vector handling (no NaN, no infinity).
- R-01 scenario 6: Single large value produces valid unit vector.
- R-01 scenario 7: Deterministic output.
- AC-05: Normalization tolerance verified.
