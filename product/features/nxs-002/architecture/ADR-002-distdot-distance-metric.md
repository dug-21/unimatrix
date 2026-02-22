## ADR-002: DistDot Distance Metric for Text Embeddings

### Context

The vector index must choose a distance metric for comparing text embeddings. The metric is fixed at index creation and cannot be changed without a full rebuild. The target embedding model (all-MiniLM-L6-v2) produces L2-normalized vectors (unit length).

Options:
- `DistCosine`: `1 - cos_sim(a, b)` with norm computation and sqrt. Universal but slower.
- `DistDot`: `1 - dot(a, b)` directly. Equivalent to DistCosine for normalized vectors, but skips norm computation.
- `DistL2`: Squared euclidean. Different ranking for non-normalized vectors, equivalent ranking for normalized.

### Decision

Use `DistDot` from anndists as the fixed distance metric.

For L2-normalized vectors: `DistCosine(a,b) = 1 - dot(a,b)/(||a||*||b||) = 1 - dot(a,b) = DistDot(a,b)`. The rankings are identical.

`DistDot` advantages:
- 2-3x faster per comparison (no norm computation, no sqrt).
- SIMD-accelerated via `simdeez_f` (AVX2/SSE2 on x86_64).
- Produces distances in range [0.0, 2.0] for normalized vectors (0 = identical, 2 = opposite).

Similarity conversion: `similarity = 1.0 - distance`. Range [0.0, 1.0] where 1.0 = identical, 0.0 = orthogonal, negative = opposite direction.

### Consequences

- **Easier**: 2-3x faster search (fewer FLOPs per distance computation).
- **Easier**: SIMD acceleration path already validated in spike research.
- **Easier**: Simple similarity formula (1.0 - distance).
- **Harder**: Requires input vectors to be L2-normalized. If nxs-003 produces non-normalized vectors, search quality degrades silently. Document this requirement clearly.
- **Harder**: Metric is fixed at index creation. Changing to DistCosine requires full rebuild. This is acceptable since there is no reason to change for normalized text embeddings.
