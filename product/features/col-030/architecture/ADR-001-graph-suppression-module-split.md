## ADR-001: graph_suppression.rs Module Split for suppress_contradicts

### Context

`suppress_contradicts` must be a pure function in the `unimatrix-engine` crate so it is
testable without server infrastructure. The natural home is `graph.rs`, where `TypedRelationGraph`,
`RelationType`, and `edges_of_type` are defined.

However, `graph.rs` is currently 587 lines. Adding `suppress_contradicts` (~30-50 lines
of function body plus doc comments) would bring the file to 617-637 lines, violating the
project-wide 500-line per-file convention (entry #161, rust-workspace.md). This is a
gate-3b risk if unresolved before delivery (entry #3580, SR-06 from SCOPE-RISK-ASSESSMENT.md).

Alternative considered: inline the suppression logic in `search.rs` (caller site). This
avoids the file-split complexity but breaks the testability requirement (AC-08) and
separates the graph logic from the graph module.

### Decision

`suppress_contradicts` is defined in a new sibling module `graph_suppression.rs`
(`crates/unimatrix-engine/src/graph_suppression.rs`) and re-exported from `graph.rs`:

```rust
// graph.rs
mod graph_suppression;
pub use graph_suppression::suppress_contradicts;
```

All callers use `unimatrix_engine::graph::suppress_contradicts`. The module boundary is
invisible to callers. Unit tests go in `graph_tests.rs` (consistent with the existing test
colocation pattern for `graph.rs` logic).

`graph_suppression.rs` imports `TypedRelationGraph`, `RelationType`, and `petgraph::Direction`
from the same crate — no new dependencies.

### Consequences

- `graph.rs` stays under 600 lines (suppression logic is ~30-50 lines in a separate file).
- `graph_suppression.rs` starts at ~50 lines — well under limit, with room for helpers.
- The public API is unchanged: `unimatrix_engine::graph::suppress_contradicts` is the
  single import path.
- Future graph filters (PPR candidate scoring, co-access suppression) can follow the same
  pattern, adding sibling modules re-exported from `graph.rs`.
- `lib.rs` does NOT need a new top-level `pub mod` entry — the module is private to `graph`.
