# C5: Pooling Module -- Pseudocode

## Purpose

Mean pooling with attention mask. Converts ONNX model output (per-token embeddings) into sentence-level embeddings.

## File: `crates/unimatrix-embed/src/pooling.rs`

```
/// Apply mean pooling to ONNX output with attention mask.
///
/// token_embeddings: flat array, shape [batch_size, seq_len, hidden_dim]
/// attention_mask: flat array, shape [batch_size, seq_len], values 0 or 1
/// batch_size: number of sequences
/// seq_len: length of each sequence (padded to same length)
/// hidden_dim: embedding dimension (384)
///
/// Returns: Vec<Vec<f32>> with batch_size entries, each of length hidden_dim.
///
/// For each sequence: sum(token_embedding * mask) / sum(mask)
/// Masked (padding) tokens contribute zero.
pub fn mean_pool(
    token_embeddings: &[f32],
    attention_mask: &[i64],
    batch_size: usize,
    seq_len: usize,
    hidden_dim: usize,
) -> Vec<Vec<f32>>:

    result = Vec with capacity batch_size

    FOR b IN 0..batch_size:
        // Count real tokens for this sequence
        mask_sum = 0.0_f32
        FOR t IN 0..seq_len:
            mask_sum += attention_mask[b * seq_len + t] as f32

        // Guard against zero mask sum (all padding -- shouldn't happen but safety)
        IF mask_sum < 1e-9:
            mask_sum = 1e-9

        pooled = vec![0.0_f32; hidden_dim]

        FOR t IN 0..seq_len:
            mask_val = attention_mask[b * seq_len + t] as f32
            IF mask_val > 0.0:
                FOR d IN 0..hidden_dim:
                    idx = b * seq_len * hidden_dim + t * hidden_dim + d
                    pooled[d] += token_embeddings[idx] * mask_val

        // Divide by mask count to get mean
        FOR d IN 0..hidden_dim:
            pooled[d] /= mask_sum

        result.push(pooled)

    return result
```

## Design Notes

- Attention mask values are i64 (matching ONNX tensor type) but logically 0 or 1.
- The mask_sum guard (1e-9 minimum) prevents division by zero if all tokens are masked.
- Indexing into the flat arrays uses `b * seq_len * hidden_dim + t * hidden_dim + d` for embeddings, `b * seq_len + t` for mask.
- R-02 critical risk: If padding tokens contribute to pooling, batch embeddings diverge from single embeddings.
- The function is pure arithmetic with no external dependencies.
