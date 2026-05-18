# Agent Report: vnc-015-agent-4-ppr-expand

**Component**: Component 4 — PPR and graph_expand Expansion
**Feature**: vnc-015
**Branch**: feature/vnc-015

---

## Work Completed

Added `RelationType::RelatedTo` to the PPR positive type set and BFS expansion set per ADR-006.

### Files Modified

- `crates/unimatrix-engine/src/graph_ppr.rs`
- `crates/unimatrix-engine/src/graph_expand.rs`
- `crates/unimatrix-engine/src/graph_ppr_tests.rs`
- `crates/unimatrix-engine/src/graph_expand_tests.rs`

### Exact Changes

**graph_ppr.rs — Site A: `positive_out_degree_weight`**

Added fifth `edges_of_type` call after the existing `Informs` call:
```rust
for edge_ref in graph.edges_of_type(node_idx, RelationType::RelatedTo, Direction::Outgoing) {
    total += edge_ref.weight().weight as f64;
}
```

**graph_ppr.rs — Site B: `personalized_pagerank` inner loop**

Added fifth `edges_of_type` block after the existing `Informs` block:
```rust
for edge_ref in
    graph.edges_of_type(node_idx, RelationType::RelatedTo, Direction::Outgoing)
{
    // vnc-015 (ADR-006): RelatedTo added to positive type set at equal weight.
    neighbor_contribution +=
        outgoing_contribution(&current_scores, &edge_ref, out_degree, graph);
}
```

**graph_expand.rs — Site C: neighbors-gathering block**

Added fifth `edges_of_type` call after the existing `Prerequisite` call:
```rust
for edge_ref in
    graph.edges_of_type(node_idx, RelationType::RelatedTo, Direction::Outgoing)
{
    // vnc-015 (ADR-006): RelatedTo added to positive BFS set at equal weight.
    neighbors.push(graph.inner[edge_ref.target()]);
}
```

---

## Negative Check

`RelationType::Advances` and `RelationType::Motivates` are **ABSENT** from all positive sets in `graph_ppr.rs` and `graph_expand.rs` — CONFIRMED.

Grep result: no matches for `RelationType::Advances` or `RelationType::Motivates` in either file.

---

## Tests

**Result: 444 passed, 0 failed, 1 ignored**

Breakdown:
- 414 unit tests (lib)
- 14 pipeline_confidence integration tests
- 3 pipeline_regression integration tests
- 6 pipeline_retrieval integration tests
- 7 test_scenarios_unit integration tests

**New tests added (17 total across two test files):**

graph_ppr_tests.rs (8 new):
- `test_ppr_positive_types_include_related_to` — RelatedTo mass flows (R-11, AC-17)
- `test_ppr_related_to_weight_equals_existing_positive_types` — equal weight invariant (ADR-006)
- `test_ppr_advances_is_write_only_no_ppr_flow` — Advances negative check (R-11)
- `test_ppr_motivates_is_write_only_no_ppr_flow` — Motivates negative check (R-11)
- `test_ppr_existing_positive_types_still_flow` — regression for all 4 existing types (AC-04)
- `test_positive_out_degree_weight_includes_related_to_edge` — normalization site verified

graph_expand_tests.rs (6 new):
- `test_graph_expand_related_to_in_positive_bfs` — two-hop BFS traversal (AC-17)
- `test_graph_expand_related_to_unidirectional_fixture` — direction contract (pattern #4066)
- `test_graph_expand_advances_not_in_positive_bfs` — Advances negative check (R-11)
- `test_graph_expand_motivates_not_in_positive_bfs` — Motivates negative check (R-11)
- `test_graph_expand_existing_positive_types_still_expand` — regression for 4 existing types (AC-04)

---

## Commit

```
impl(ppr-expand): add RelatedTo to PPR and BFS positive type sets (vnc-015) (#595)
```
SHA: 96908a7d

---

## Issues / Blockers

None. Two pre-existing `clippy::collapsible_if` warnings in `auth.rs` and `event_queue.rs` were present before this change and are unrelated.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced pattern #3950 (4-site RelationType checklist), #3744 (PPR direction trap), #3992 (BFS expander architecture), #4431 (from_str wildcard arm ordering).
- Stored: entry #4434 via `context_correct` on #3950 — extended the 4-site checklist to include `graph_expand.rs` as the third PPR-positive site, and documented the silent normalization corruption trap when `positive_out_degree_weight` is missed (caught only by a weight-equality regression test, no compile or runtime signal).
