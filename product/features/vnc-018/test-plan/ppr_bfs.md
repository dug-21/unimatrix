# vnc-018 Test Plan: graph_ppr.rs + graph_expand.rs — Advances/Motivates Additions

## Component Scope

Targeted surgical additions to two existing files:

- `crates/unimatrix-server/src/graph_ppr.rs`
  - `personalized_pagerank`: add Advances + Motivates `edges_of_type` calls after RelatedTo block (~line 131–136)
  - `positive_out_degree_weight`: add Advances + Motivates `edges_of_type` calls (~line 203)
  
- `crates/unimatrix-server/src/graph_expand.rs`
  - BFS positive-type expansion: add Advances + Motivates `edges_of_type` calls after RelatedTo block (~line 144–148)

This completes ADR-006 (vnc-015) — the write-only deferral of Advances and Motivates
from W1B-1. Approximately 16 lines of changes across both files.

Also tested here: `node_index_for` accessor on `TypedRelationGraph` (ADR-008) — this
is the cross-crate BFS access method whose implementation is a prerequisite for depth>1
neighbors traversal.

---

## Unit Test Expectations

### `graph_ppr.rs` — Advances and Motivates in positive-type set

**Test: `test_ppr_positive_types_include_advances_and_motivates`** (AC-17, R-09)

```rust
// This test verifies that Advances and Motivates are present in the
// edge type sets iterated by personalized_pagerank and positive_out_degree_weight.
// Approach: construct a TypedRelationGraph with nodes connected by Advances and
// Motivates edges; run positive_out_degree_weight; assert the returned weight
// reflects the Advances/Motivates edges (i.e., out_degree > 0 when only
// Advances/Motivates edges are present).

// Arrange:
let mut graph = TypedRelationGraph::new();
let x = graph.add_node(42, "node_x_title");
let y = graph.add_node(43, "node_y_title");
graph.add_edge(x, y, RelationType::Advances, 1.0);

// Act:
let weight = positive_out_degree_weight(&graph, x);

// Assert: weight > 0.0 (Advances edge contributes to positive out-degree)
// If Advances were absent from the function, weight would be 0.0
// This is the definitive test that Advances is in the set.

// Repeat for RelationType::Motivates:
graph.add_edge(x, y, RelationType::Motivates, 1.0);
let weight_with_motivates = positive_out_degree_weight(&graph, x);
assert!(weight_with_motivates >= weight, "Motivates must add to positive out-degree");
```

**Test: `test_ppr_personalized_pagerank_includes_advances_motivates`** (AC-17)

```rust
// Arrange: three-node graph where X→Y (Advances), Y→Z (Motivates)
// Seed set: {X}
// Act: run personalized_pagerank(graph, &[x_idx], iterations=10)
// Assert: Z has non-zero score (score flows from X→Y via Advances, Y→Z via Motivates)
// If either type were absent, Z's score would be 0.0 because no path exists
// through the positive type set.

let scores = personalized_pagerank(&graph, &[x_idx], 10);
let z_score = scores[z_idx.index()];
assert!(z_score > 0.0, "Motivates must propagate PPR score; got {z_score}");
```

**Regression guard — existing PPR score assertions:**

Before shipping, audit `graph_ppr.rs` unit tests for hardcoded `assert_approx_eq!`
or similar score value assertions. Any test using a node with Advances or Motivates
edges that previously had no such edges will have a different out-degree normalization
after this change. Update those assertions if needed. Document any such updates in the
RISK-COVERAGE-REPORT.md.

---

### `graph_expand.rs` — BFS positive-type expansion includes Advances and Motivates

**Test: `test_graph_expand_follows_advances_edges`** (AC-18, R-09)

```rust
// Arrange: TypedRelationGraph with X→Y (Advances edge)
// Act: run graph_expand BFS from X
// Assert: Y appears in the expanded neighbor set

let mut graph = TypedRelationGraph::new();
let x = graph.add_node(100, "x");
let y = graph.add_node(101, "y");
graph.add_edge(x, y, RelationType::Advances, 1.0);

let neighbors = graph_expand(&graph, x, /* depth or one-hop */);
let neighbor_ids: Vec<u64> = neighbors.iter().map(|n| n.id).collect();
assert!(neighbor_ids.contains(&101), "Advances edge must be followed in BFS expansion");
```

**Test: `test_graph_expand_follows_motivates_edges`** (AC-18)

```rust
// Same structure with RelationType::Motivates
// Assert: Y appears in expanded neighbor set when connected via Motivates
```

**Test: `test_graph_expand_both_types_traversed_in_single_call`** (AC-18)

```rust
// Arrange: X→Y (Advances), X→Z (Motivates)
// Act: graph_expand from X
// Assert: both Y and Z in neighbors
// This tests that both types are iterated in the same expansion call —
// not just that each works independently.
```

---

### Regression: existing positive type tests

**Test: `test_ppr_existing_positive_types_still_work`** (R-09 regression guard)

```rust
// Verify RelatedTo still produces positive PPR scores after the addition.
// Construct a graph where X→Y via RelatedTo (the type that was present before ADR-006).
// Run PPR; assert non-zero score for Y.
// This guards against accidentally removing the existing RelatedTo behavior.
```

---

## Integration Test Expectations

The PPR and BFS additions are internal to the server's ranking pipeline and not directly
observable through individual MCP tool responses (PPR affects search re-ranking, not
`context_graph` responses). No new infra-001 Python tests are required for this component.

The additions are verified by:
1. Unit tests in this plan (AC-17, AC-18)
2. The existing `confidence` and `lifecycle` suites verifying search re-ranking is
   not broken by the normalization change (regression baseline)

If existing infra-001 `confidence` or `tools` tests assert specific confidence score
values that would shift due to PPR normalization changes, those are pre-existing tests
that may need updating — document in RISK-COVERAGE-REPORT.md.

---

## Module Doc Verification

The delivery agent must verify (code review, not a test):

- `graph_ppr.rs` module-level doc comment no longer says "Advances and Motivates
  write-only until Phase 2" — that note must be replaced with vnc-018 attribution
- `graph_expand.rs` same — the "deferred" note must be removed and vnc-018 credited

This is documentation correctness, not a functional test.

---

## Edge Cases

| Edge Case | Assertion |
|-----------|-----------|
| Node with only Advances edges (no RelatedTo) | positive_out_degree_weight > 0.0 |
| Node with Motivates edge pointing to self | No infinite loop; graceful handling |
| Graph with no Advances or Motivates edges | PPR and BFS behave identically to pre-vnc-018 (regression) |
| Node connected by both Advances and Motivates to same target | Single target with combined weight (not duplicated in BFS) |

---

## Risks Specifically Addressed in This Component

- R-09: PPR out-degree normalization shift — unit tests establish what the correct
  behavior IS, and the regression guard catches hardcoded score assertions that would
  otherwise silently pass with wrong values
- AC-17: `personalized_pagerank` and `positive_out_degree_weight` both include Advances/Motivates
- AC-18: `graph_expand` BFS follows Advances and Motivates edges
