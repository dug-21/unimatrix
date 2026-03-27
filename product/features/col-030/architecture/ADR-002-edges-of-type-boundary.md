## ADR-002: edges_of_type as the Sole Traversal Boundary for suppress_contradicts

### Context

`TypedRelationGraph::edges_of_type` is documented as the SR-01 filter boundary — "All
traversal in `graph_penalty`, `find_terminal_active`, `dfs_active_reachable`, and
`bfs_chain_depth` MUST call this method. Direct calls to `.edges_directed()` or
`.neighbors_directed()` are prohibited at those sites."

For col-030, `suppress_contradicts` is a new traversal site. Two implementation approaches
exist:

1. **Call `edges_of_type` with `RelationType::Contradicts`** — uses the established boundary,
   filters by type inside the method, returns typed `EdgeReference` iterators.
2. **Call `self.inner.edges_directed()` directly and filter inline** — bypasses the boundary,
   introduces a second traversal pattern inconsistent with the established SR-01 mitigation.

Option 2 also carries correctness risk: `edges_of_type` has never been exercised for
`RelationType::Contradicts` (SR-01 in SCOPE-RISK-ASSESSMENT.md), and a second direct call
site would not exercise the boundary method for this edge type — leaving the method unproven
for Contradicts.

### Decision

`suppress_contradicts` calls `edges_of_type` exclusively — once with `Direction::Outgoing`
and once with `Direction::Incoming` — for each non-suppressed entry in `result_ids`. No
direct `.edges_directed()` or `.neighbors_directed()` calls appear in `graph_suppression.rs`.

```rust
// Both directions required — NLI writes edges unidirectionally (see ADR-003)
let out_neighbors: HashSet<u64> = graph
    .edges_of_type(node_idx, RelationType::Contradicts, Direction::Outgoing)
    .map(|e| *graph.inner[e.target()])
    .collect();
let in_neighbors: HashSet<u64> = graph
    .edges_of_type(node_idx, RelationType::Contradicts, Direction::Incoming)
    .map(|e| *graph.inner[e.source()])
    .collect();
```

The unit tests in `graph_tests.rs` (both Outgoing and Incoming cases) verify that
`edges_of_type` returns correct results for `RelationType::Contradicts` — exercising the
boundary for this edge type for the first time (resolving SR-01).

### Consequences

- SR-01 boundary is maintained: a single method is the only graph traversal path.
- `edges_of_type` is confirmed correct for `RelationType::Contradicts` via unit tests.
- Adding future edge types to suppression logic requires only changing the `RelationType`
  argument — no structural change.
- `suppress_contradicts` is decoupled from petgraph internals; if the underlying graph
  library changes, only `edges_of_type` needs updating.
