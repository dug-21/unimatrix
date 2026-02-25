## ADR-002: f64 Intermediate Computation for Confidence Formula

### Context

The confidence formula involves mathematical operations that are sensitive to floating-point precision: Wilson score lower bound uses `z^2/(4n^2)` which produces very small values at large n, logarithmic transforms, and exponential decay. The final stored value is `confidence: f32` on EntryRecord. Scope risk SR-01 flagged potential f32 precision loss in Wilson score computation.

f32 has ~7 decimal digits of precision. f64 has ~15. The Wilson formula at n=10000: `z^2/(4n^2) = 3.8416 / 400000000 = 9.604e-9`. This value is representable in f32, but intermediate products during the computation may lose precision through accumulation.

### Decision

All component functions (`usage_score`, `freshness_score`, `helpfulness_score`, `correction_score`, `trust_score`, `base_score`) compute internally using f64. The composite `compute_confidence()` function sums weighted f64 components and casts the final result to f32 only at the return point.

The weight constants are declared as f32 (matching the stored type) but promoted to f64 for the weighted sum computation.

### Consequences

**Easier:**
- Wilson score is numerically stable at all realistic input ranges
- No precision-related bugs to debug
- Component function unit tests can assert exact f64 values without epsilon tolerance

**Harder:**
- Minor cognitive overhead: component functions return f64, stored field is f32
- The f64-to-f32 cast at the end could theoretically lose precision, but the result is in [0.0, 1.0] where f32 has full precision (no exponent scaling issues)
