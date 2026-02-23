/// Normalize a vector to unit L2 norm in place.
///
/// If the norm is below `1e-12` (near-zero vector), leaves the vector unchanged
/// to avoid division by zero and noise amplification.
pub fn l2_normalize(embedding: &mut [f32]) {
    let norm_sq: f32 = embedding.iter().map(|v| v * v).sum();
    let norm = norm_sq.sqrt();

    if norm < 1e-12 {
        return;
    }

    for val in embedding.iter_mut() {
        *val /= norm;
    }
}

/// Normalize a vector to unit L2 norm, returning a new vector.
///
/// If the norm is below `1e-12` (near-zero vector), returns a copy unchanged.
pub fn l2_normalized(embedding: &[f32]) -> Vec<f32> {
    let mut result = embedding.to_vec();
    l2_normalize(&mut result);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2_normalize_known_vector() {
        let mut embedding = vec![3.0, 4.0];
        l2_normalize(&mut embedding);
        assert!((embedding[0] - 0.6).abs() < 1e-6);
        assert!((embedding[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_unit_vector() {
        let mut embedding = vec![1.0, 0.0, 0.0];
        l2_normalize(&mut embedding);
        assert!((embedding[0] - 1.0).abs() < 1e-6);
        assert!(embedding[1].abs() < 1e-6);
        assert!(embedding[2].abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_result_has_unit_norm() {
        let mut embedding = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        l2_normalize(&mut embedding);
        let norm: f32 = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_l2_normalize_near_zero_vector() {
        let mut embedding = vec![1e-13, 1e-13, 1e-13];
        let original = embedding.clone();
        l2_normalize(&mut embedding);
        assert_eq!(embedding, original);
    }

    #[test]
    fn test_l2_normalize_zero_vector() {
        let mut embedding = vec![0.0, 0.0, 0.0];
        l2_normalize(&mut embedding);
        assert_eq!(embedding, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_l2_normalize_384d() {
        let mut embedding: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
        l2_normalize(&mut embedding);
        let norm: f32 = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_l2_normalize_negative_values() {
        let mut embedding = vec![-3.0, 4.0, -5.0];
        l2_normalize(&mut embedding);
        let norm: f32 = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_l2_normalize_single_large_value() {
        let mut embedding = vec![0.0, 0.0, 1000.0, 0.0];
        l2_normalize(&mut embedding);
        assert!((embedding[2] - 1.0).abs() < 1e-6);
        assert!(embedding[0].abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_all_equal() {
        let n = 384;
        let mut embedding = vec![1.0_f32; n];
        l2_normalize(&mut embedding);
        let expected = 1.0 / (n as f32).sqrt();
        for v in &embedding {
            assert!((v - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn test_l2_normalized_returns_new_vector() {
        let original = vec![3.0, 4.0];
        let normalized = l2_normalized(&original);
        assert_eq!(original, vec![3.0, 4.0]);
        assert!((normalized[0] - 0.6).abs() < 1e-6);
        assert!((normalized[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_deterministic() {
        let mut embedding1 = vec![1.0, 2.0, 3.0];
        let mut embedding2 = vec![1.0, 2.0, 3.0];
        l2_normalize(&mut embedding1);
        l2_normalize(&mut embedding2);
        assert_eq!(embedding1, embedding2);
    }
}
