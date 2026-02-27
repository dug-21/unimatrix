## ADR-003: Lambda Dimension Weighting Strategy

### Context

The composite coherence metric (lambda) combines four dimension scores into a single [0.0, 1.0] value:

1. **Confidence freshness**: Ratio of entries with non-stale confidence to total active entries.
2. **Graph quality**: 1.0 minus the HNSW stale node ratio.
3. **Embedding consistency**: 1.0 minus the ratio of inconsistent entries (opt-in check).
4. **Contradiction density**: 1.0 minus the quarantined-to-active ratio.

SCOPE open question #3 asks: what are appropriate dimension weights, and how should unavailable dimensions be handled?

Three sub-questions require decisions:

**Equal vs. unequal weights?** Confidence freshness and graph quality directly affect every search query (stale confidence causes systematic over-ranking; stale HNSW nodes degrade search quality). Embedding consistency and contradiction density are second-order effects that indicate model drift or knowledge quality issues but do not directly corrupt search results.

**Unavailable dimensions: default to 1.0 or exclude?** Embedding consistency requires `check_embeddings: true`, which is opt-in because it re-embeds all entries (expensive). SR-08 notes that defaulting to 1.0 inflates lambda for callers who never opt in, masking potential degradation.

**Fixed weights or configurable?** All thresholds in crt-005 are named constants. Weights should follow the same pattern.

### Decision

**Unequal weights reflecting impact on search quality:**

| Dimension | Weight | Rationale |
|---|---|---|
| Confidence freshness | 0.35 | Highest impact: stale confidence directly causes systematic over-ranking in every search query |
| Graph quality | 0.30 | High impact: stale HNSW nodes degrade search quality and increase latency |
| Contradiction density | 0.20 | Medium impact: quarantined entries indicate knowledge quality issues |
| Embedding consistency | 0.15 | Lower impact: detects model drift, but rarely changes between checks |

Weights sum to 1.0.

**Unavailable dimensions are excluded and weights re-normalized:**

When embedding consistency is not available (check_embeddings = false), the lambda computation uses only the three available dimensions with re-normalized weights:

- Confidence freshness: 0.35 / (1.0 - 0.15) = 0.4118
- Graph quality: 0.30 / 0.85 = 0.3529
- Contradiction density: 0.20 / 0.85 = 0.2353

This avoids inflating lambda by assuming healthy for unchecked dimensions. When a caller enables embedding checks, the fourth dimension is included at its configured weight.

The `embedding_consistency_score` field in StatusReport still returns 1.0 when checks are not performed (for display purposes), but the lambda computation excludes it from the weighted average.

**Weights are named constants in a struct:**

```rust
pub struct CoherenceWeights {
    pub confidence_freshness: f64,
    pub graph_quality: f64,
    pub embedding_consistency: f64,
    pub contradiction_density: f64,
}

pub const DEFAULT_WEIGHTS: CoherenceWeights = CoherenceWeights {
    confidence_freshness: 0.35,
    graph_quality: 0.30,
    embedding_consistency: 0.15,
    contradiction_density: 0.20,
};
```

The weight struct is passed to `compute_lambda`, allowing future configurability without changing the function signature.

### Consequences

**Easier:**
- Lambda accurately reflects search quality impact (not inflated by unchecked dimensions)
- Callers who never enable embedding checks get an honest lambda based on always-available signals
- Adding or removing dimensions in the future only requires adjusting the weight struct
- Weight values are transparent and documented, not buried in code

**Harder:**
- Re-normalization adds complexity to `compute_lambda` (but the function is pure and easily tested)
- Lambda values are not directly comparable between calls with and without embedding checks enabled (three-dimension lambda vs. four-dimension lambda)
- The weight values (0.35, 0.30, 0.20, 0.15) are informed judgment, not empirically derived. Future work may tune these based on observed correlation between dimension scores and actual search quality degradation.
