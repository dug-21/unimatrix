# C5: Pooling Module -- Test Plan

## Tests

```
test_mean_pool_hand_crafted_ac18:
    // AC-18: exact hand-crafted example from acceptance criteria
    token_embeddings = [1.0, 2.0, 3.0, 4.0, 0.0, 0.0]  // 3 tokens, dim 2
    attention_mask = [1, 1, 0]  // third token is padding
    result = mean_pool(&token_embeddings, &attention_mask, 1, 3, 2)
    ASSERT result.len() == 1
    ASSERT result[0].len() == 2
    // Expected: [(1+3)/2, (2+4)/2] = [2.0, 3.0]
    ASSERT (result[0][0] - 2.0).abs() < 1e-6
    ASSERT (result[0][1] - 3.0).abs() < 1e-6

test_mean_pool_all_tokens_active:
    // No padding -- all tokens contribute
    token_embeddings = [1.0, 0.0, 0.0, 1.0]  // 2 tokens, dim 2
    attention_mask = [1, 1]
    result = mean_pool(&token_embeddings, &attention_mask, 1, 2, 2)
    // Expected: [(1+0)/2, (0+1)/2] = [0.5, 0.5]
    ASSERT (result[0][0] - 0.5).abs() < 1e-6
    ASSERT (result[0][1] - 0.5).abs() < 1e-6

test_mean_pool_single_token:
    token_embeddings = [3.0, 7.0]  // 1 token, dim 2
    attention_mask = [1]
    result = mean_pool(&token_embeddings, &attention_mask, 1, 1, 2)
    ASSERT (result[0][0] - 3.0).abs() < 1e-6
    ASSERT (result[0][1] - 7.0).abs() < 1e-6

test_mean_pool_batch_of_two:
    // Two sequences, each 2 tokens, dim 2
    // Seq 1: [1,2], [3,4], mask [1,1]
    // Seq 2: [5,6], [0,0], mask [1,0]
    token_embeddings = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 0.0, 0.0]
    attention_mask = [1, 1, 1, 0]
    result = mean_pool(&token_embeddings, &attention_mask, 2, 2, 2)
    ASSERT result.len() == 2
    // Seq 1: [(1+3)/2, (2+4)/2] = [2.0, 3.0]
    ASSERT (result[0][0] - 2.0).abs() < 1e-6
    ASSERT (result[0][1] - 3.0).abs() < 1e-6
    // Seq 2: [5/1, 6/1] = [5.0, 6.0] (only first token is real)
    ASSERT (result[1][0] - 5.0).abs() < 1e-6
    ASSERT (result[1][1] - 6.0).abs() < 1e-6

test_mean_pool_all_masked:
    // All tokens masked (shouldn't happen in practice)
    token_embeddings = [1.0, 2.0, 3.0, 4.0]
    attention_mask = [0, 0]
    result = mean_pool(&token_embeddings, &attention_mask, 1, 2, 2)
    // Should not panic, returns near-zero (divided by 1e-9)
    ASSERT result.len() == 1
    ASSERT result[0].len() == 2

test_mean_pool_384d:
    // Realistic dimension
    seq_len = 5
    hidden_dim = 384
    // Fill with sequential values
    token_embeddings = (0..seq_len * hidden_dim).map(|i| i as f32).collect()
    attention_mask = vec![1_i64; seq_len]
    result = mean_pool(&token_embeddings, &attention_mask, 1, seq_len, hidden_dim)
    ASSERT result.len() == 1
    ASSERT result[0].len() == 384

test_mean_pool_padding_does_not_dilute:
    // Same real tokens but different padding should produce same pooled embedding
    // Seq with no padding: [1,2], [3,4], mask [1,1]
    // Seq with padding:    [1,2], [3,4], [0,0], mask [1,1,0]
    result_no_pad = mean_pool(&[1.0, 2.0, 3.0, 4.0], &[1, 1], 1, 2, 2)
    result_padded = mean_pool(&[1.0, 2.0, 3.0, 4.0, 0.0, 0.0], &[1, 1, 0], 1, 3, 2)
    ASSERT (result_no_pad[0][0] - result_padded[0][0]).abs() < 1e-6
    ASSERT (result_no_pad[0][1] - result_padded[0][1]).abs() < 1e-6
```

## Risks Covered

- R-02 (Critical): Mean pooling correctly applies attention mask.
- AC-18: Hand-crafted example validated.
- R-02 scenario 5: Divides by sum of attention mask, not sequence length.
- Edge case: All-zero mask handled without panic.
