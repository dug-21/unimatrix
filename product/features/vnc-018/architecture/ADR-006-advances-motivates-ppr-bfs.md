## ADR-006: Advances and Motivates Added to PPR and BFS Positive Type Sets

### Context

vnc-015 (W1B-1) added `Advances` and `Motivates` as two of the 10 new `RelationType`
variants. Both were intentionally excluded from PPR positive types and BFS expansion
in vnc-015, marked as "write-only until Phase 2" (vnc-015 ADR-006). The product
vision W1B-2 states these must be added as part of W1B-2. vnc-018 is the first W1B-2
delivery issue.

The current state:
- `graph_ppr.rs` module doc: "Advances and Motivates are intentionally NOT in the
  positive type set — write-only until Phase 2."
- `graph_expand.rs` module doc: same note.
- No `edges_of_type` calls for Advances or Motivates in either file.

PPR currently uses 5 positive types: Supports, CoAccess, Prerequisite, Informs,
RelatedTo (added by vnc-015). BFS uses the same 5. Adding Advances and Motivates
makes it 7 for both.

Semantic rationale:
- `Advances`: entry A advances goal/decision B — A is positively relevant to B's
  context. Agents searching near B should discover A. The direction (A advances B)
  means in PPR, outgoing Advances edges from A propagate mass toward B's neighborhood.
- `Motivates`: entry A motivates decision B — A provides justification for B.
  Structurally equivalent to Informs for PPR purposes.

Both types are semantically "positive signal" — they indicate that the source entry
is useful context for the target entry's domain. Including them in PPR and BFS
improves recall in goal-traceability and decision-dependency retrieval.

No evidence of a regression risk was identified in the scope assessment (SR-06 flags
the risk but notes ACs 17-18 cover it). The positive type set is additive — adding
types increases PPR mass flow, which can change rankings but not in a directionally
harmful way. Entries with many `Advances`/`Motivates` edges were likely already
highly connected via other positive types.

### Decision

Add `Advances` and `Motivates` to both PPR and BFS positive type sets in vnc-018.

**graph_ppr.rs changes** (two locations):

1. In `personalized_pagerank`, after the RelatedTo `edges_of_type` block (~line 131):
   ```rust
   for edge_ref in graph.edges_of_type(node_idx, RelationType::Advances, Direction::Outgoing) {
       neighbor_contribution += outgoing_contribution(&current_scores, &edge_ref, out_degree, graph);
   }
   for edge_ref in graph.edges_of_type(node_idx, RelationType::Motivates, Direction::Outgoing) {
       neighbor_contribution += outgoing_contribution(&current_scores, &edge_ref, out_degree, graph);
   }
   ```

2. In `positive_out_degree_weight`, after the RelatedTo block (~line 203):
   ```rust
   for edge_ref in graph.edges_of_type(node_idx, RelationType::Advances, Direction::Outgoing) {
       total += edge_ref.weight().weight;
   }
   for edge_ref in graph.edges_of_type(node_idx, RelationType::Motivates, Direction::Outgoing) {
       total += edge_ref.weight().weight;
   }
   ```

**graph_expand.rs change** (one location, after RelatedTo ~line 144):
```rust
for edge_ref in graph.edges_of_type(node_idx, RelationType::Advances, Direction::Outgoing) {
    neighbors.push(graph.inner[edge_ref.target()]);
}
for edge_ref in graph.edges_of_type(node_idx, RelationType::Motivates, Direction::Outgoing) {
    neighbors.push(graph.inner[edge_ref.target()]);
}
```

Module-level doc comment in both files: update to remove "write-only until Phase 2"
for Advances and Motivates; attribute the change to vnc-018.

Unit tests required (AC-17, AC-18):
- A test that constructs a `TypedRelationGraph` with Advances and Motivates edges and
  asserts that `personalized_pagerank` propagates mass through them.
- A test that asserts Advances and Motivates appear in the `graph_expand` output
  (BFS traversal returns nodes connected by these types).
- These tests are the regression baseline for SR-06: if a future change removes
  Advances/Motivates from the positive set, the tests catch it.

### Consequences

Easier: goal-traceability retrieval improves — agents querying near a Goal entry
will now discover entries that Advance or Motivate it via PPR. The test baseline
(AC-17, AC-18) protects against silent regression in search ranking.

Harder: PPR out-degree normalization changes for any node with Advances or Motivates
edges — the `positive_out_degree_weight` denominator increases, slightly reducing
the weight per individual edge for nodes that had many other positive edges. This is
expected and correct behavior (normalization prevents high-degree nodes from
dominating mass flow). The eval harness (W1-3) can measure the effect on a real
query set if the team wants quantitative confirmation before shipping.
