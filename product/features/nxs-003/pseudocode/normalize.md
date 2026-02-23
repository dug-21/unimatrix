# C4: Normalize Module -- Pseudocode

## Purpose

L2 normalization functions for embedding vectors. Critical for nxs-002 DistDot compatibility.

## File: `crates/unimatrix-embed/src/normalize.rs`

```
/// Normalize a vector to unit L2 norm in place.
/// If norm is below 1e-12 (near-zero vector), leaves vector unchanged.
pub fn l2_normalize(embedding: &mut Vec<f32>):
    norm_sq = 0.0_f32
    FOR val IN embedding.iter():
        norm_sq += val * val

    norm = norm_sq.sqrt()

    IF norm < 1e-12:
        // Near-zero vector: avoid division by zero.
        // Return as-is (zero vector is safe for DistDot -- dot product = 0).
        return

    FOR val IN embedding.iter_mut():
        *val /= norm

/// Normalize a vector to unit L2 norm, returning a new vector.
/// If norm is below 1e-12 (near-zero vector), returns copy unchanged.
pub fn l2_normalized(embedding: &[f32]) -> Vec<f32>:
    result = embedding.to_vec()
    l2_normalize(&mut result)
    result
```

## Design Notes

- Threshold of 1e-12 prevents division by near-zero norms that would amplify noise.
- Near-zero vectors are returned unchanged (effectively zero vectors) rather than returning an error, to satisfy AC-12 (empty string returns valid embedding).
- The mutable version (`l2_normalize`) is used in the hot path (onnx.rs) to avoid allocation.
- The immutable version (`l2_normalized`) is for convenience/test use.
- R-01 critical risk: All output embeddings must satisfy |norm - 1.0| < 0.001.
