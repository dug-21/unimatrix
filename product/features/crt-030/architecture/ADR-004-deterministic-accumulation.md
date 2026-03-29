## ADR-004: Deterministic Accumulation via Node-ID-Sorted Iteration

### Context

Power iteration accumulates contributions from neighbors into a new score vector. In Rust,
iterating over a `HashMap<u64, f64>` produces keys in an indeterminate order (randomized
hash seed per process). If contributions are accumulated in HashMap iteration order, the
result of floating-point summation depends on iteration order — and floating-point addition
is not associative. Two calls with the same inputs could produce different outputs.

This non-determinism would cause:
1. Flaky unit tests that occasionally fail due to order-dependent floating-point rounding.
2. Different search results for identical queries in different process runs.
3. Inability to write exact-value tests for multi-hop convergence cases (AC-16 test suite).

The SCOPE.md resolves this as a correctness constraint (AC-05): "Accumulation is sorted by
node ID each iteration."

Two options for enforcing sorted order:
1. **Sort HashMap keys before accumulation**: Collect node IDs into a `Vec<u64>`, sort
   ascending, iterate in sorted order. O(N log N) per iteration, plus a Vec allocation.
2. **Pre-compute sorted key list once**: Before the iteration loop, build a `Vec<u64>` of
   all node IDs in ascending order. Reuse across all iterations. O(N log N) once, O(N)
   allocation held for the duration of the call.

Option 2 is strictly better: same determinism guarantee, lower runtime cost.

A third option — using a `BTreeMap<u64, f64>` for the score maps — provides sorted
iteration implicitly but at O(log N) per insert/lookup vs O(1) for HashMap, and with
worse cache locality. At 10K–100K nodes this matters.

### Decision

A `Vec<u64>` of all node IDs in the graph is sorted ascending once before the iteration
loop. The inner accumulation loop iterates over this pre-sorted `Vec`, not over HashMap
keys directly. Score maps remain `HashMap<u64, f64>` for O(1) access during accumulation.

The sorted node list covers all nodes in `graph.node_index` — not just seed nodes. This
ensures that non-seed nodes (PPR candidates) are accumulated in a consistent order.

The pre-sorted Vec is constructed inside `personalized_pagerank` and is local to the
function call. It is not cached across calls — each call constructs its own sorted list.
This is acceptable: the call is already O(I × E_pos) and the O(N log N) sort is a
one-time cost.

Example structure (pseudocode):

```rust
let mut all_node_ids: Vec<u64> = graph.node_index.keys().copied().collect();
all_node_ids.sort_unstable();

for _ in 0..iterations {
    let mut next_scores = HashMap::with_capacity(current_scores.len());
    for &node_id in &all_node_ids {
        // accumulate teleportation + neighbor contributions
        // contributions visited in all_node_ids sorted order
    }
    current_scores = next_scores;
}
```

### Consequences

- All calls to `personalized_pagerank` with identical inputs produce identical outputs,
  regardless of process, thread, or HashMap hash seed.
- Unit tests can assert exact `f64` values for convergence cases.
- The one-time O(N log N) sort cost is paid per search call when PPR is active. At N=10K
  this is negligible (< 1 ms); at N=100K it is ~10 ms and is included in the RayonPool
  offload threshold calculation (ADR-008).
- `BTreeMap` is explicitly rejected to preserve O(1) HashMap access performance during
  the inner accumulation loop.
