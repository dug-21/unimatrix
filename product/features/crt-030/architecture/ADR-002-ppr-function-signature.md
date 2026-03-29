## ADR-002: personalized_pagerank() Function Signature and Algorithm Contract

### Context

The PPR function must be pure, synchronous, and deterministic. It takes a pre-built graph
and a caller-supplied personalization vector, runs power iteration, and returns a score map.
Several design questions need to be resolved in the signature:

1. **Seed input type**: Should the caller supply raw HNSW scores, or a pre-weighted
   personalization vector? If the function receives raw scores, it must know about
   `phase_affinity_score` — coupling the pure graph function to the search-pipeline concept
   of phase affinity. If the caller pre-weights and normalizes, the function stays pure.

2. **Normalization**: Should the function normalize the input vector to sum 1.0, or
   require normalized input? Normalizing inside the function hides caller errors.
   Requiring normalized input makes the caller responsible for the invariant, but the
   function can assert or guard against degenerate (zero-sum) input.

3. **Return type**: A `HashMap<u64, f64>` is the natural sparse score map. It can be
   large (all reachable nodes) before the caller applies `ppr_inclusion_threshold` and
   `ppr_max_expand`. Alternatively, the function could accept the threshold and cap as
   parameters, returning a pre-filtered sorted list. The pre-filter design leaks search
   policy into the pure function.

4. **Early exit**: Should the function exit early when convergence is detected (delta < ε)?
   Early exit improves performance but breaks determinism: the number of iterations
   executed depends on the input graph and personalization vector, making test assertions
   against exact scores impossible.

### Decision

The function signature is:

```rust
pub fn personalized_pagerank(
    graph: &TypedRelationGraph,
    seed_scores: &HashMap<u64, f64>,  // caller-normalized, sums to 1.0
    alpha: f64,                        // damping factor, in (0.0, 1.0)
    iterations: usize,                 // exact iteration count, no early exit
) -> HashMap<u64, f64>
```

The caller (Step 6d in search.rs) is responsible for:
1. Building `seed_scores` as `hnsw_score × phase_affinity_score` per HNSW candidate.
2. Normalizing `seed_scores` to sum 1.0.
3. Handling the zero-sum degenerate case (skip PPR, return empty map without calling).

The function is responsible for:
1. Running exactly `iterations` steps of power iteration.
2. Using node-ID-sorted accumulation each iteration (determinism).
3. Returning a `HashMap<u64, f64>` where values are the steady-state PPR scores.
4. Returning an empty map for empty graph or graph with no positive-edge neighbors.

Out-degree normalization: for each node, out-degree is computed as the count of
outgoing positive edges (Supports + CoAccess + Prerequisite). Edge weights from
`RelationEdge.weight as f64` are applied in the weighted transition probability.
Nodes with zero positive out-degree do not propagate forward (receive teleportation only).

The power iteration formula per node v at step t+1:
```
score[v][t+1] = (1 - alpha) * personalization[v]
              + alpha * Σ_{u: u→v ∈ positive_edges} (weight[u→v] / out_degree_weight[u]) * score[u][t]
```

where `personalization[v] = 0.0` for nodes not in `seed_scores`.

Post-condition: the returned map values sum approximately to 1.0 (within floating-point
error from power iteration). The caller does not rely on this invariant.

### Consequences

- The function is maximally pure: no knowledge of phase affinity, search policy, or config.
- Determinism is guaranteed: exact iteration count, no convergence-based exit.
- The caller bears the normalization burden, but this is simple arithmetic and already
  a single-location concern in Step 6d.
- The returned map may be large (one entry per reachable node). Filtering by
  `ppr_inclusion_threshold` and capping at `ppr_max_expand` is the caller's responsibility.
- Testability is high: the pure function can be tested with small hand-constructed graphs
  and exact expected values.
