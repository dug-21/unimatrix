## ADR-001: `graph_expand` as `#[path]` Submodule of `graph.rs`

### Context

Three placement options were considered for the `graph_expand` function:

**Option A — Inline in `search.rs`**: The expansion logic is small (BFS loop), and its
caller is in `search.rs`. Placing it inline avoids a new file.

**Option B — Inline in `graph.rs`**: `graph_expand` needs `TypedRelationGraph`, `edges_of_type`,
`RelationType`, `NodeIndex` — all defined in `graph.rs`. Co-location with its dependencies is
natural.

**Option C — New `graph_expand.rs` as `#[path]` submodule of `graph.rs`**: Mirrors the
`graph_ppr.rs` / `graph_suppression.rs` pattern established in crt-030 (ADR-001 crt-030,
entry #3731) and documented in pattern entry #3740.

**Option A** is wrong: `graph_expand` is a pure graph traversal function with no search policy
knowledge. Placing it in `search.rs` couples a pure graph algorithm to the search service,
preventing independent testing and reuse. The SR-01 boundary would be harder to audit — the
sole traversal enforcement point (`edges_of_type`) would be in a service file instead of a
graph module.

**Option B** is excluded by the 500-line file limit (CLAUDE.md rule). `graph.rs` is already
near-limit, and `graph_expand` + its inline unit tests would push it over.

**Option C** is the established pattern. It is the only option that preserves all three
invariants: (1) pure function with graph-module imports, (2) 500-line file limit respected,
(3) SR-01 boundary auditable within the graph module family.

### Decision

`graph_expand` lives in a new file `crates/unimatrix-engine/src/graph_expand.rs`, declared
as a `#[path]` submodule inside `graph.rs`:

```rust
#[path = "graph_expand.rs"]
mod graph_expand;
pub use graph_expand::graph_expand;
```

The function does NOT appear in `lib.rs`. It is accessible via `graph.rs`'s re-export.
If `graph_expand.rs` + inline unit tests exceeds 500 lines, tests split to
`graph_expand_tests.rs` following the `graph_ppr.rs` / `graph_ppr_tests.rs` pattern.

All traversal inside `graph_expand.rs` uses `edges_of_type()` exclusively. No direct
`.edges_directed()` or `.neighbors_directed()` calls are permitted (SR-01). This invariant
is stated in a module-level doc comment at the top of `graph_expand.rs`.

Related: ADR-001 crt-030 (entry #3731), pattern entry #3740.

### Consequences

- `graph.rs` remains the single entry point for all graph traversal functions.
- `graph_expand` is independently testable with hand-constructed `TypedRelationGraph` graphs.
- SR-01 enforcement (edges_of_type boundary) is auditable within the graph module family.
- Future graph traversal functions follow the same submodule pattern without debate.
- `search.rs` imports `graph_expand` via the existing `use unimatrix_engine::graph_expand;`
  import path (same pattern as `personalized_pagerank`).
