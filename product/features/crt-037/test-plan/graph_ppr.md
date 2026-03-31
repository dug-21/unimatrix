# crt-037 Test Plan: graph_ppr.rs (PPR Traversal Extension)

**Component**: `crates/unimatrix-engine/src/graph_ppr.rs`
**Nature of change**: Additive — fourth `edges_of_type(_, RelationType::Informs,
Direction::Outgoing)` call added in both `personalized_pagerank` inner loop and
`positive_out_degree_weight`. No other changes.
**Risks addressed**: R-02 (PPR direction), R-02 historical trap from entry #3896.

---

## Background: PPR Direction Semantics

`personalized_pagerank` implements a reverse random walk (transpose PPR). For an edge
`A → B` (source A informs target B), node A accumulates mass from B's score when B is
seeded. This uses `Direction::Outgoing`: iterate outgoing edges of A to find B, then
contribute B's current score to A's next-iteration score.

`Direction::Incoming` would have the opposite effect — A would accumulate mass only from
nodes that A points to, which is wrong for reverse PPR. Entry #3744 documents this as a
known trap. Entry #3896 documents the regression test trap: inserting only `B→A` and
seeding at B gives `A = 0.0`. Both forward (`A→B`) and reverse (`B→A`) edges must be
present in the test graph to exercise the correct code path.

---

## Unit Tests

### Positive PPR Mass Flow (R-02, AC-05)

**Test**: `test_personalized_pagerank_informs_edge_propagates_mass_to_lesson_node`
- Arrange:
  - Two nodes: node A (lesson-learned, index 0), node B (decision, index 1)
  - Add `Informs` edge A→B (weight > 0.0)
  - Add reverse edge B→A (required per entry #3896 — both edges needed)
  - Seed: `{B: 1.0}` (decision node seeded)
  - Config: default damping factor, sufficient iterations
- Act: `personalized_pagerank(graph, seed)` → `scores: HashMap<NodeIndex, f32>`
- Assert: `scores[node_A_index] > 0.0` — assert by the *specific lesson node index*, not
  by any-node non-zero — covers AC-05 (R-20 gate check 5)
- Note: if only A→B is inserted without B→A, the test will pass for wrong reasons; both
  edges must be present per the entry #3896 pattern

**Test**: `test_personalized_pagerank_decision_seed_reaches_only_lesson_via_informs`
- Arrange:
  - Three nodes: lesson A, decision B, unrelated C
  - `Informs` edge A→B only (no edges to/from C)
  - Both forward A→B and reverse B→A inserted
  - Seed at B
- Act: `personalized_pagerank(graph, seed)`
- Assert: `scores[A] > 0.0` AND `scores[C] == 0.0` (C unreachable — no inflation from
  unrelated nodes)

**Test**: `test_personalized_pagerank_informs_weight_influences_mass`
- Arrange:
  - Two nodes A and B with `Informs` edge A→B at weight `w1`
  - Two nodes C and D with `Supports` edge C→D at same weight `w1` (both edges + reverse)
  - Seed at B and D separately
- Act: PPR seeded at B → `scores_informs`; PPR seeded at D → `scores_supports`
- Assert: `scores_informs[A]` and `scores_supports[C]` are both > 0.0 and comparable in
  magnitude (verifies Informs participates in PPR mass pool with same mechanics as Supports)

### `positive_out_degree_weight` (AC-06)

**Test**: `test_positive_out_degree_weight_includes_informs_edge`
- Arrange: node X with exactly one outgoing `Informs` edge to Y at weight `0.6`; no other edges
- Act: `positive_out_degree_weight(graph, node_X_index)`
- Assert: return value equals `0.6` (or within f32 epsilon) — not zero — covers AC-06

**Test**: `test_positive_out_degree_weight_informs_adds_to_existing_positive_edges`
- Arrange: node X with one `Supports` edge (weight 0.8) and one `Informs` edge (weight 0.6)
- Act: `positive_out_degree_weight(graph, node_X_index)`
- Assert: return value equals `0.8 + 0.6 = 1.4` (within f32 epsilon) — `Informs` adds,
  not replaces

**Test**: `test_positive_out_degree_weight_supersedes_not_included`
- Arrange: node X with one `Supersedes` edge only (weight 1.0)
- Act: `positive_out_degree_weight(graph, node_X_index)`
- Assert: return value equals `0.0` — `Supersedes` is a penalty edge, not positive

### Direction Regression Guard (R-02 historical, C-14)

**Test**: `test_direction_outgoing_required_for_informs_mass_flow`
- This test documents the wrong-direction failure mode as a comment with expected behavior.
  The test itself uses `Direction::Outgoing` (correct). The comment states: "If
  `Direction::Incoming` were used here instead, `scores[A]` would be 0.0. See entry #3744."
- Assert: with `Direction::Outgoing`, `scores[A] > 0.0`

CI grep gate (also in OVERVIEW.md):
```bash
grep -n 'Direction::Incoming' crates/unimatrix-engine/src/graph_ppr.rs
# Must return empty after crt-037 changes
```

---

## Edge Cases

| Case | Expected |
|------|----------|
| Node with `Informs` edge but zero-weight | `positive_out_degree_weight = 0.0`; node not in PPR denominator effectively |
| Graph with only `Informs` edges, seed at leaf node with no out-edges | PPR converges to personalization vector; leaf gets residual from damping |
| `Informs` edge weight negative (shouldn't occur — `cosine * ppr_weight >= 0`) | Not exercised by crt-037 since weight is `cosine * nli_informs_ppr_weight`, both ≥ 0 |

---

## Acceptance Criteria Covered

| AC-ID | Test Name |
|-------|-----------|
| AC-05 | `test_personalized_pagerank_informs_edge_propagates_mass_to_lesson_node` |
| AC-06 | `test_positive_out_degree_weight_includes_informs_edge` |

### Regression Coverage

All existing `graph_ppr.rs` tests must continue passing unchanged after adding the fourth
`edges_of_type` call. No existing test should need modification — the change is purely
additive (new edge type iteration, does not remove or reorder existing iterations).
