## ADR-001: graph_ppr.rs as Submodule of graph.rs

### Context

The TypedRelationGraph, its edge types, and the `edges_of_type()` traversal boundary all live
in `graph.rs`. Two prior features have added graph-adjacent functions to this crate:

- `graph_suppression.rs` (col-030): `suppress_contradicts` — a pure function over
  `TypedRelationGraph` that detects Contradicts collisions in a result set. Declared as a
  `#[path]` submodule of `graph.rs` and re-exported.
- `graph_tests.rs`: Unit tests for `graph.rs` functions, split to respect the 500-line file
  limit.

The PPR function needs access to `TypedRelationGraph`, `RelationType`, `edges_of_type()`,
`NodeIndex`, and `RelationEdge`. These are all `pub(crate)` or `pub` on `graph.rs`.

Two structural options exist:
1. Add `personalized_pagerank` directly to `graph.rs`.
2. Add a new `graph_ppr.rs` submodule declared via `#[path]` in `graph.rs`, re-exported
   from there — the same pattern used by `graph_suppression.rs`.

Option 1 risks exceeding the 500-line file limit (graph.rs is already ~550 lines, and
PPR including tests will add ~300+ lines). Option 2 mirrors the existing pattern exactly
and keeps each file focused on a single responsibility.

### Decision

`graph_ppr.rs` is added as a `#[path = "graph_ppr.rs"] mod graph_ppr;` submodule of
`graph.rs`, with `pub use graph_ppr::personalized_pagerank;` re-exporting the function.
This mirrors the `graph_suppression.rs` / `suppress_contradicts` structure exactly.

`graph_ppr.rs` does NOT appear in `lib.rs`. It is only accessible through the `graph` module
re-export, consistent with ADR-001 col-030 (R-09).

If `graph_ppr.rs` + inline tests exceeds 500 lines, tests are split into
`graph_ppr_tests.rs` following the `graph.rs` / `graph_tests.rs` pattern.

### Consequences

- The PPR function is co-located architecturally with the graph module it depends on.
- `graph.rs` remains the single entry point for all graph traversal: callers import
  `unimatrix_engine::graph::personalized_pagerank`.
- The 500-line file limit is respected regardless of test volume.
- A future graph function (e.g., a Prerequisite-path finder) would follow the same pattern:
  new `graph_{concern}.rs` submodule, re-exported from `graph.rs`.
