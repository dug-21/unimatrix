# Agent Report: vnc-018-agent-6-ppr-bfs

**Task**: Add `Advances` and `Motivates` to the positive relation type sets in `graph_ppr.rs` and `graph_expand.rs` (ADR-006 vnc-018).

## Files Modified

- `crates/unimatrix-engine/src/graph_ppr.rs`
- `crates/unimatrix-engine/src/graph_ppr_tests.rs`
- `crates/unimatrix-engine/src/graph_expand.rs`
- `crates/unimatrix-engine/src/graph_expand_tests.rs`

## Changes Made

### graph_ppr.rs

1. Module doc: replaced "Advances and Motivates intentionally NOT in positive type set — write-only until Phase 2" with vnc-018 attribution listing all 7 positive types.
2. `personalized_pagerank` function doc and inline comment: updated from 5 to 7 `edges_of_type` calls; added Advances and Motivates loops after the RelatedTo block; removed stale NOTE comment.
3. `positive_out_degree_weight` doc and implementation: updated from 5 to 7 types; added Advances and Motivates weight accumulation loops after RelatedTo; removed stale "intentionally absent" note.

### graph_expand.rs

1. Module doc: replaced deferral note with vnc-018 attribution.
2. `graph_expand` function doc: updated positive type list to include Advances and Motivates.
3. BFS neighbor collection loop: added Advances and Motivates `edges_of_type` calls after the RelatedTo block; updated inline comment from 5 to 7 calls; removed stale NOTE comment.

### graph_ppr_tests.rs

- **Removed** 2 stale negative tests (`test_ppr_advances_is_write_only_no_ppr_flow`, `test_ppr_motivates_is_write_only_no_ppr_flow`) that asserted zero PPR mass — now incorrect.
- **Added** 4 new tests (AC-17):
  - `test_ppr_positive_types_include_advances_and_motivates` — `positive_out_degree_weight` returns > 0 for Advances/Motivates-only nodes
  - `test_ppr_personalized_pagerank_includes_advances_motivates` — mass flows transitively through X→Y (Advances), Y→Z (Motivates)
  - `test_ppr_advances_propagates_ppr_flow` — Advances edge propagates in reverse-walk PPR
  - `test_ppr_motivates_propagates_ppr_flow` — Motivates edge propagates in reverse-walk PPR

### graph_expand_tests.rs

- **Removed** 2 stale negative tests (`test_graph_expand_advances_not_in_positive_bfs`, `test_graph_expand_motivates_not_in_positive_bfs`) — now assert wrong behavior.
- **Added** 3 new tests (AC-18):
  - `test_graph_expand_follows_advances_edges`
  - `test_graph_expand_follows_motivates_edges`
  - `test_graph_expand_both_types_traversed_in_single_call`

## Test Results

- `cargo test -p unimatrix-engine --lib`: 420 passed, 0 failed, 1 ignored
- `cargo test -p unimatrix-server`: all suites pass
- New Advances/Motivates tests: 7 new tests all pass

## Commit

`dc2b012b` — `impl(graph_ppr,graph_expand): add Advances and Motivates to PPR/BFS positive type sets (#608)`

## Issues

None. Regression risk (R-09): no existing test graphs included Advances or Motivates edges, so no hardcoded score assertions were broken by the normalization denominator change.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` (pattern, "personalized pagerank graph_ppr relation type expansion") — returned entry #4434 (three-site extension checklist); entry #3744 (Direction::Outgoing trap); entry #3896 (both-edges required trap). Applied #4434 checklist to confirm all 3 sites updated.
- Stored: entry #4483 "PPR/BFS RelationType promotion requires inverting stale write-only negative tests" via /uni-store-pattern — captures the trap that deferral-guard negative tests become incorrect assertions after type promotion and must be inverted, not deleted.
