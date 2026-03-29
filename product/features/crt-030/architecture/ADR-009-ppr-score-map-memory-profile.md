## ADR-009: PPR Score Map Memory Profile — No Traversal Depth Cap Needed

### Context

SR-05 in the risk assessment identifies that the `personalized_pagerank` function iterates
over all reachable nodes in the graph. In a dense CoAccess graph (heavily bootstrapped from
the co_access table), the score map returned before `ppr_inclusion_threshold` filtering
could contain thousands of candidates.

The concern is:
1. **Memory**: A large `HashMap<u64, f64>` for the score map allocation.
2. **Filtering overhead**: Applying `ppr_inclusion_threshold` over a large map before
   selecting top-`ppr_max_expand` entries.

Two options:
1. **No traversal depth cap**: PPR score map is bounded by the total node count in the
   graph. Memory is O(N) where N is total graph nodes.
2. **Traversal depth cap**: Limit the number of hops from seed nodes. Stops accumulation
   when nodes are more than D hops from any seed. Reduces score map size in sparse-seed
   scenarios but adds implementation complexity, requires a separate parameter, and breaks
   the clean power-iteration formulation.

### Decision

No traversal depth cap. The PPR score map is allowed to contain all graph nodes.

**Memory analysis:**
- The score map type is `HashMap<u64, f64>` — 8 bytes per key + 8 bytes per value + HashMap
  overhead (~80 bytes per entry for a 50% load-factor HashMap). At 100K nodes: ~16 MB.
  At 10K nodes: ~1.6 MB.
- These allocations are per-search-call, short-lived, and immediately dropped after Step 6d.
- At current production scale (< 10K entries), the score map is < 2 MB — trivial.
- At 100K entries, 16 MB per search call is acceptable given the rayon offload threshold
  (ADR-008) moves the computation off the Tokio thread.

**Filtering overhead:**
- Applying `ppr_inclusion_threshold` to a 100K-entry HashMap is O(N) — one pass.
- Sorting the above-threshold entries by score descending and taking top-`ppr_max_expand`
  is O(M log M) where M is the above-threshold count. With a threshold of 0.05 and a
  well-seeded graph, M is typically O(10s to 100s), not O(N).
- Even in a pathological case (threshold = 0.001, M = 50K), sorting 50K f64 values is
  < 5 ms and is dominated by the power iteration cost.

**CoAccess density concern (from risk assessment):**
The assumption that CoAccess edge density is bounded by the `count >= 3` threshold in the
bootstrap path is reasonable at current scale. If production data shows unbounded CoAccess
density, the correct mitigation is a cap on CoAccess edge writes (a crt-029 concern), not
a traversal depth cap in PPR.

### Consequences

- The implementation is simpler: standard power iteration, no hop-count bookkeeping.
- Memory allocation is O(N) and bounded by total graph node count — well-understood and
  predictable.
- The threshold-and-cap mechanism (`ppr_inclusion_threshold` + `ppr_max_expand`) is the
  correct control surface for pool expansion. It does not limit the PPR score map size,
  but it limits the number of store fetches and pool additions — the operationally
  important constraint.
- A future traversal depth cap can be added as a separate `ppr_max_hop_depth: usize`
  config field if operational experience shows memory pressure at extreme CoAccess density.
  This does not require revisiting the core PPR algorithm.
