# C7: Penalty Constants — Pseudocode

## Location
`crates/unimatrix-engine/src/confidence.rs`

## Changes

Add two public constants after the existing `PROVENANCE_BOOST` constant:

```
pub const DEPRECATED_PENALTY: f64 = 0.7
pub const SUPERSEDED_PENALTY: f64 = 0.5
```

Add a pure function for cosine similarity:

```
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64:
    if a.len() != b.len() or a.is_empty():
        return 0.0

    dot_product = sum(a[i] * b[i] for i in 0..a.len()) as f64
    norm_a = sqrt(sum(a[i]^2 for i in 0..a.len())) as f64
    norm_b = sqrt(sum(b[i]^2 for i in 0..b.len())) as f64

    if norm_a == 0.0 or norm_b == 0.0:
        return 0.0

    result = dot_product / (norm_a * norm_b)
    return result.clamp(0.0, 1.0)
```

Note: Inputs are assumed L2-normalized (both query and stored embeddings are normalized). The clamp guards against floating-point edge cases. Return type is f64 for scoring pipeline precision (crt-005).

## Rationale

- Named constants avoid magic numbers at call sites (NFR-2.1, ADR-005)
- Cosine similarity is a pure function with no side effects
- Both are consumed by SearchService during re-rank (Step 7)
- Constants are NOT part of the confidence formula (crt-002 unchanged)
