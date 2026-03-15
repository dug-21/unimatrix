/// Apply mean pooling to ONNX output with attention mask.
///
/// - `token_embeddings`: flat array, shape `[batch_size, seq_len, hidden_dim]`
/// - `attention_mask`: flat array, shape `[batch_size, seq_len]`, values 0 or 1
/// - `batch_size`: number of sequences in the batch
/// - `seq_len`: length of each sequence (padded to same length within batch)
/// - `hidden_dim`: embedding dimension (384)
///
/// For each sequence: `sum(token_embedding * mask) / sum(mask)`.
/// Masked (padding) tokens contribute zero to the pooled embedding.
pub fn mean_pool(
    token_embeddings: &[f32],
    attention_mask: &[i64],
    batch_size: usize,
    seq_len: usize,
    hidden_dim: usize,
) -> Vec<Vec<f32>> {
    let mut result = Vec::with_capacity(batch_size);

    for b in 0..batch_size {
        // Count real tokens for this sequence
        let mut mask_sum: f32 = 0.0;
        for t in 0..seq_len {
            mask_sum += attention_mask[b * seq_len + t] as f32;
        }

        // Guard against zero mask sum (all padding)
        if mask_sum < 1e-9 {
            mask_sum = 1e-9;
        }

        let mut pooled = vec![0.0_f32; hidden_dim];

        for t in 0..seq_len {
            let mask_val = attention_mask[b * seq_len + t] as f32;
            if mask_val > 0.0 {
                for (d, p) in pooled.iter_mut().enumerate().take(hidden_dim) {
                    let idx = b * seq_len * hidden_dim + t * hidden_dim + d;
                    *p += token_embeddings[idx] * mask_val;
                }
            }
        }

        // Divide by mask count to get mean
        for p in pooled.iter_mut().take(hidden_dim) {
            *p /= mask_sum;
        }

        result.push(pooled);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mean_pool_hand_crafted_ac18() {
        // AC-18: exact hand-crafted example
        // 3 tokens, dim 2: [[1,2], [3,4], [0,0]], mask [1,1,0]
        let token_embeddings = [1.0, 2.0, 3.0, 4.0, 0.0, 0.0];
        let attention_mask = [1_i64, 1, 0];
        let result = mean_pool(&token_embeddings, &attention_mask, 1, 3, 2);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 2);
        assert!((result[0][0] - 2.0).abs() < 1e-6);
        assert!((result[0][1] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_mean_pool_all_tokens_active() {
        let token_embeddings = [1.0, 0.0, 0.0, 1.0];
        let attention_mask = [1_i64, 1];
        let result = mean_pool(&token_embeddings, &attention_mask, 1, 2, 2);
        assert!((result[0][0] - 0.5).abs() < 1e-6);
        assert!((result[0][1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_mean_pool_single_token() {
        let token_embeddings = [3.0, 7.0];
        let attention_mask = [1_i64];
        let result = mean_pool(&token_embeddings, &attention_mask, 1, 1, 2);
        assert!((result[0][0] - 3.0).abs() < 1e-6);
        assert!((result[0][1] - 7.0).abs() < 1e-6);
    }

    #[test]
    fn test_mean_pool_batch_of_two() {
        // Seq 1: [1,2], [3,4], mask [1,1]
        // Seq 2: [5,6], [0,0], mask [1,0]
        let token_embeddings = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 0.0, 0.0];
        let attention_mask = [1_i64, 1, 1, 0];
        let result = mean_pool(&token_embeddings, &attention_mask, 2, 2, 2);
        assert_eq!(result.len(), 2);
        // Seq 1: [(1+3)/2, (2+4)/2] = [2.0, 3.0]
        assert!((result[0][0] - 2.0).abs() < 1e-6);
        assert!((result[0][1] - 3.0).abs() < 1e-6);
        // Seq 2: [5/1, 6/1] = [5.0, 6.0]
        assert!((result[1][0] - 5.0).abs() < 1e-6);
        assert!((result[1][1] - 6.0).abs() < 1e-6);
    }

    #[test]
    fn test_mean_pool_all_masked() {
        let token_embeddings = [1.0, 2.0, 3.0, 4.0];
        let attention_mask = [0_i64, 0];
        let result = mean_pool(&token_embeddings, &attention_mask, 1, 2, 2);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 2);
        // Should not panic; results near zero (divided by 1e-9)
    }

    #[test]
    fn test_mean_pool_384d() {
        let seq_len = 5;
        let hidden_dim = 384;
        let token_embeddings: Vec<f32> = (0..seq_len * hidden_dim).map(|i| i as f32).collect();
        let attention_mask = vec![1_i64; seq_len];
        let result = mean_pool(&token_embeddings, &attention_mask, 1, seq_len, hidden_dim);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 384);
    }

    #[test]
    fn test_mean_pool_padding_does_not_dilute() {
        // Same real tokens, different padding
        let result_no_pad = mean_pool(&[1.0, 2.0, 3.0, 4.0], &[1, 1], 1, 2, 2);
        let result_padded = mean_pool(&[1.0, 2.0, 3.0, 4.0, 0.0, 0.0], &[1, 1, 0], 1, 3, 2);
        assert!((result_no_pad[0][0] - result_padded[0][0]).abs() < 1e-6);
        assert!((result_no_pad[0][1] - result_padded[0][1]).abs() < 1e-6);
    }
}
