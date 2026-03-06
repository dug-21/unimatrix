# C7: Penalty Constants — Test Plan

## Location
`crates/unimatrix-engine/src/confidence.rs` (unit tests in same file)

## Tests

### T-PC-01: DEPRECATED_PENALTY value (ADR-005, AC-02)
- Assert `DEPRECATED_PENALTY == 0.7`
- Assert `DEPRECATED_PENALTY > 0.0 && DEPRECATED_PENALTY < 1.0`

### T-PC-02: SUPERSEDED_PENALTY value (ADR-005, AC-03)
- Assert `SUPERSEDED_PENALTY == 0.5`
- Assert `SUPERSEDED_PENALTY > 0.0 && SUPERSEDED_PENALTY < 1.0`

### T-PC-03: SUPERSEDED_PENALTY < DEPRECATED_PENALTY (AC-03)
- Assert `SUPERSEDED_PENALTY < DEPRECATED_PENALTY` — superseded entries penalized more harshly

### T-PC-04: Penalties are independent of confidence formula (NFR-2.1)
- Assert `DEPRECATED_PENALTY + SUPERSEDED_PENALTY` does not appear in weight sum
- Verify weight sum invariant (0.92) unchanged

### T-CS-01: cosine_similarity — identical normalized vectors (R-11)
- Input: two identical L2-normalized vectors
- Expected: 1.0 (or very close, within epsilon)

### T-CS-02: cosine_similarity — orthogonal vectors (R-11)
- Input: [1,0,...] and [0,1,...]
- Expected: 0.0

### T-CS-03: cosine_similarity — zero vector (R-11, edge case)
- Input: non-zero vector and all-zeros vector
- Expected: 0.0 (not NaN, not panic)

### T-CS-04: cosine_similarity — mismatched dimensions (R-11)
- Input: 3-dim and 5-dim vectors
- Expected: 0.0

### T-CS-05: cosine_similarity — empty vectors (R-11)
- Input: two empty slices
- Expected: 0.0

### T-CS-06: cosine_similarity — L2-normalized inputs (R-11)
- Input: two known L2-normalized vectors with known angle
- Expected: cos(angle) within epsilon

### T-CS-07: cosine_similarity — result type is f64 (crt-005)
- Verify return type is f64
- Verify result is in [0.0, 1.0] for normalized inputs

### T-CS-08: cosine_similarity — result clamped (R-11 guard)
- Input: denormalized vectors that could produce value > 1.0
- Expected: result clamped to [0.0, 1.0]

## Risk Coverage

| Risk | Scenarios | Tests |
|------|-----------|-------|
| R-02 (penalty ranking) | Penalty constants correct | T-PC-01..04 |
| R-11 (denormalized vectors) | Zero, mismatch, denormalized | T-CS-03..05, T-CS-08 |
