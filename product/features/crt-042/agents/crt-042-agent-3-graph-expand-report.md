# Agent Report: crt-042-agent-3-graph-expand

## Task

Implement `graph_expand` — pure BFS traversal of `TypedRelationGraph` returning entry IDs
reachable from a seed set within `depth` hops via positive edge types (crt-042, Component 1).

## Files Modified

- `crates/unimatrix-engine/src/graph_expand.rs` (CREATED — 179 lines)
- `crates/unimatrix-engine/src/graph_expand_tests.rs` (CREATED — 418 lines, split per NFR-09)
- `crates/unimatrix-engine/src/graph.rs` (MODIFIED — added `#[path]` submodule + re-export)

## Tests

**367 passed / 0 failed** (`cargo test --package unimatrix-engine`)

21 new unit tests covering all ACs from `test-plan/graph_expand.md`:
- AC-03: 4 tests (one per positive edge type)
- AC-04: backward edge does not surface
- AC-05/AC-06: two-hop chain at depth=2 and depth=1
- AC-07: Supersedes and Contradicts excluded (2 tests)
- AC-08: seed exclusion + self-loop (2 tests)
- AC-09: max_candidates early exit
- AC-10/AC-11/AC-12: empty seeds, empty graph, depth=0 (3 tests)
- R-11: bidirectional and triangular cycle termination (2 tests)
- R-13: determinism at budget boundary
- R-02: S1/S2 unidirectional failure mode (2 tests)
- R-17: S8 CoAccess unidirectional gap

AC-16 grep check PASSED: zero `.edges_directed()` or `.neighbors_directed()` calls in implementation code.

Full workspace: **no new failures** (all existing tests pass).

## Issues Encountered

**Depth-limit semantics bug (caught by tests, fixed before commit).**

Initial implementation followed the pseudocode literally: `can_expand_further` controlled
only whether neighbors were enqueued, but always added them to `result`. This caused
depth=1 to return 2-hop entries.

Root cause: the pseudocode's `can_expand_further` governs neighbor _expansion_, not
neighbor _discovery_. The correct semantics are: a node at `current_depth == depth`
does not process any neighbors at all — the `!can_expand_further` guard fires as a
`continue` before the neighbor loop. Nodes at the depth limit are already in `result`
(added by their parent at `current_depth - 1`); they simply do not expand further.

Fixed by moving the `!can_expand_further → continue` to guard the entire neighbor
processing loop, not just the enqueue step.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` — pattern #3740 (graph traversal submodule
  pattern), #3650 (TypedRelationGraph edges_of_type boundary), #3950 (extension checklist);
  decision #4054 (ADR-006 traversal direction), #4052 (ADR-004 config validation),
  #4050 (ADR-002 phase insertion). Results were directly applicable.
- Stored: entry #4071 "BFS depth-limit: guard entire neighbor loop with !can_expand_further, not just enqueue" via /uni-store-pattern
