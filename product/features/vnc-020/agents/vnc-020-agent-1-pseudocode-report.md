# Agent Report: vnc-020-agent-1-pseudocode

## Task

Produce per-component pseudocode files for vnc-020: three new `context_graph` modes
(inverse, filter, path) implemented as sibling modules, with dispatch and validation
expansion in `graph_read.rs` and tool description update in `tools.rs`.

## Files Produced

| File | Component |
|------|-----------|
| `product/features/vnc-020/pseudocode/OVERVIEW.md` | Cross-component overview |
| `product/features/vnc-020/pseudocode/graph_read.md` | Wave 1: types, dispatch, validation |
| `product/features/vnc-020/pseudocode/tools.md` | Wave 1: tool description update |
| `product/features/vnc-020/pseudocode/graph_read_inverse.md` | Wave 2: antijoin handler |
| `product/features/vnc-020/pseudocode/graph_read_filter.md` | Wave 2: correlated subquery handler |
| `product/features/vnc-020/pseudocode/graph_read_path.md` | Wave 2: BFS shortest-path handler |

## Components Covered

1. `graph_read.rs` (Wave 1, modified) — 8 new GraphParams fields, 4 new response types,
   3 module declarations, 3 dispatch arms, expanded `validate_no_unsupported_params`
   (3 new arms + depth rejection on 3 existing arms + 8-field rejections on 4 existing arms).

2. `tools.rs` (Wave 1, modified) — `CONTEXT_GRAPH_DESCRIPTION` constant extended with
   inverse, filter, and path mode sections including mandatory verbatim staleness
   disclosure for path mode.

3. `graph_read_inverse.rs` (Wave 2, new) — N-LEFT-JOIN antijoin SQL builder via sqlx
   QueryBuilder, AND semantics (ADR-003), returns InverseResponse.

4. `graph_read_filter.rs` (Wave 2, new) — parameterized correlated subquery with
   optional property and edge-count clauses, two independent subqueries when both
   min/max_edge_count are set (R-08), returns FilterResponse.

5. `graph_read_path.rs` (Wave 2, new) — path-carrying BFS over TypedRelationGraph
   (outgoing only), endpoint resolution via follow_to_current, visited set keyed on
   resolved ID (R-03), returns PathResponse.

## Open Questions

None — all SCOPE.md OQs are resolved per ARCHITECTURE.md. The following implementation
notes are flagged for the Wave 1 agent:

1. **graph_read.rs line budget**: Estimated ~467-578 lines post-expansion (compact vs.
   spaced formatting). The Wave 1 agent must count lines before committing; if >500,
   extract `validate_new_fields_rejected(params, mode)` helper to compress the
   8-field rejection blocks across the 4 existing arms. Detailed in graph_read.md
   §Line Budget.

2. **parse_relation_types / push_relation_type_list duplication**: Each Wave 2 module
   needs its own copy of these private helpers, OR one module exposes them as `pub(super)`
   for the others to import. The pseudocode notes both options; the implementation agent
   should choose Option A (private copies, no inter-module coupling) unless there is a
   strong reason for Option B.

3. **`#[path]` declarations cause compile failure if sibling files are absent**: Wave 2
   agents must create their `.rs` files immediately on spawn, even if initially empty
   (pattern #4509). This must be the first action of each Wave 2 agent.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 10 entries; directly applied:
  #4502 (ADR-001 module split), #4503 (ADR-002 GraphParams additions), #4504 (ADR-003
  AND semantics), #4505 (ADR-004 depth reuse), #4506 (ADR-005 path response format),
  #4507 (ADR-006 resolve_supersessions), all confirmed pseudocode design decisions.
- Queried: `context_search(pattern, "graph_read sibling module handler dispatch")` —
  found #4500 (context_graph new mode checklist), #3636 (`#[path]` attribute pattern),
  #4509 (stub-file-immediately pattern for parallel agents). All applied.
- Queried: `context_search(pattern, "sqlx push_bind dynamic IN clause")` — found #4058
  (push_bind SQL builder pattern) and #3442 (chunked IN-clause). Applied in both
  graph_read_inverse.md (alias-counter JOIN construction) and graph_read_filter.md
  (push_relation_type_list helper).
- Queried: `context_search(decision, "vnc-020 architectural decisions")` — confirmed
  #4507 (ADR-006), #4502 (ADR-001), #4506 (ADR-005) are all captured.
- Deviations from established patterns: none. All pseudocode follows the lock-acquire-
  clone-release pattern (#4493), `pub(super)` import path from graph_read_neighbors
  (established by graph_read_subgraph.rs), visited-set-on-resolved-ID (pattern #4494),
  and `push_bind` for dynamic SQL (pattern #4058).
