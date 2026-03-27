## ADR-003: Bidirectional Contradicts Query Required for Correctness

### Context

NLI detection (`nli_detection.rs`, lines 509-523) writes `Contradicts` edges unidirectionally:
`(source_id, neighbor_id, 'Contradicts')` only, always from the new entry toward its neighbor
at detection time. There is no reverse write.

Given entries A and B that are contradictory:
- If A was stored after B: the edge is `A → B` (Outgoing from A, Incoming to B)
- If B was stored after A: the edge is `B → A` (Outgoing from B, Incoming to A)

The direction is determined by the order of NLI detection, not by any semantic property of
A or B. At suppression time, neither the detection order nor the edge direction is known to
the suppression function.

This means querying only `Direction::Outgoing` from a higher-ranked entry misses cases where
the higher-ranked entry was stored first (and thus the edge was written Outgoing from the
lower-ranked entry toward the higher-ranked one).

### Decision

`suppress_contradicts` queries both `Direction::Outgoing` and `Direction::Incoming` for
each candidate entry against the `TypedRelationGraph`. The union of both direction sets is
used when checking whether a lower-ranked entry is a contradiction neighbor:

```rust
// For entry at result index i (higher rank):
let contradicts_neighbors: HashSet<u64> = outgoing_neighbors
    .union(&incoming_neighbors)
    .copied()
    .collect();
```

Any lower-ranked entry (j > i) whose ID appears in `contradicts_neighbors` is suppressed.

This is documented as a required invariant: if NLI ever adds bidirectional writes in the
future, the union query would produce duplicates in the HashSet — but HashSet membership
is idempotent, so suppression behavior would be unchanged (not broken).

### Consequences

- Both Outgoing and Incoming directions are checked, making suppression correct regardless
  of which order entries were detected by NLI.
- Two `edges_of_type` calls per candidate entry vs. one — negligible for k ≤ 20 with small
  Contradicts degree per node (typically < 3 per entry in practice).
- O(n * 2 * degree_c) traversal where n is result count and degree_c is Contradicts edge
  degree per node. For n=20, degree_c=3: 120 edge lookups per search — well within hot-path budget.
- The algorithm is correct even if the underlying NLI detection path changes to bidirectional
  writes in a future feature.
