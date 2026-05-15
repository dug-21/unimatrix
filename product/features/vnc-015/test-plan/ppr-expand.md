# Test Plan: PPR and graph_expand Expansion

**Component**: `crates/unimatrix-engine/src/graph_ppr.rs`, `graph_expand.rs`
**Architecture ref**: Component 4
**Risk coverage**: R-11
**AC coverage**: AC-04, AC-17

---

## Key Constraints

- Only `RelatedTo` is added to the positive type set in this feature (ADR-006).
- `Advances` and `Motivates` are write-only (Phase 2 deferral) — they must NOT appear in PPR
  or graph_expand positive sets.
- Test plan must include both a positive test (RelatedTo flows) and negative tests
  (Advances/Motivates do not flow). Pattern #4066 (BFS direction pair) applies.
- Weight factor for `RelatedTo` must equal the existing 4 positive types in PPR.

---

## Unit Test Expectations

### Location: `crates/unimatrix-engine/src/graph_ppr.rs`

#### test_ppr_positive_types_include_related_to (R-11, AC-17)
- Arrange: build a `TypedRelationGraph` with exactly one edge: `(A, B, RelatedTo)`
- Act: run `personalized_pagerank` seeded at A
- Assert: B's score is greater than its baseline (PPR mass flowed through the RelatedTo edge)
- Assert: specifically B.score > 0.0 (i.e., B is reachable from A via positive edge)

#### test_ppr_related_to_weight_equals_existing_positive_types (R-11)
- Arrange: build two TypedRelationGraphs:
  - Graph 1: `(A, B, Supports)` — existing positive type
  - Graph 2: `(A, B, RelatedTo)` — new positive type
- Act: run PPR on each graph with identical seed
- Assert: B's score in Graph 1 equals B's score in Graph 2 (equal weight)
- Note: confirms ADR-006 "equal weight as existing 4 positive types"

#### test_ppr_advances_is_write_only_no_ppr_flow (R-11, AC-17 negative)
- Arrange: build TypedRelationGraph with `(A, B, Advances)` edge
- Act: run PPR seeded at A
- Assert: B's score is NOT elevated above baseline (no mass flow through Advances)
- Note: Advances is write-only in this feature; Phase 2 deferral

#### test_ppr_motivates_is_write_only_no_ppr_flow (R-11, AC-17 negative)
- Arrange: build TypedRelationGraph with `(A, B, Motivates)` edge
- Act: run PPR seeded at A
- Assert: B's score is NOT elevated above baseline (no mass flow through Motivates)

#### test_ppr_write_only_variants_no_ppr_flow (R-11)
- For each of the 8 write-only variants: `Cites`, `Asserts`, `Mentions`, `Refutes`, `Tests`,
  `DerivedFrom`, `About`, plus `Advances` and `Motivates`
- Arrange: one edge of each type (A, B, <Variant>)
- Act: PPR seeded at A
- Assert: B score at baseline (no positive mass flow for write-only variants)
- This can be a single parameterized test or individual tests per variant

#### test_ppr_existing_positive_types_still_flow (AC-04 regression)
- Arrange: edges using all 4 existing positive types: `Supports`, `CoAccess`, `Prerequisite`, `Informs`
- Act: PPR for each
- Assert: target node scores elevated (no regression from adding RelatedTo)

### Location: `crates/unimatrix-engine/src/graph_expand.rs`

#### test_graph_expand_related_to_in_positive_bfs (AC-17)
- Arrange: build graph with `(A, B, RelatedTo)` and `(B, C, RelatedTo)` edges
- Act: call `graph_expand` from seed A
- Assert: both B and C are reachable via positive BFS traversal

#### test_graph_expand_related_to_unidirectional_fixture (AC-17, pattern #4066)
- Arrange: build graph with ONLY `(A, B, RelatedTo)` — deliberately unidirectional fixture
- Act: `graph_expand` from B
- Assert: A is NOT reached (confirms directionality — B→A is not a positive edge here)
- Note: pattern #4066 requires pairing positive-direction test with unidirectional fixture test

#### test_graph_expand_advances_not_in_positive_bfs (R-11, AC-17 negative)
- Arrange: graph with `(A, B, Advances)` edge only
- Act: `graph_expand` from A
- Assert: B NOT reachable via positive BFS (Advances is write-only, not in positive set)

#### test_graph_expand_motivates_not_in_positive_bfs (R-11, AC-17 negative)
- Arrange: graph with `(A, B, Motivates)` edge only
- Act: `graph_expand` from A
- Assert: B NOT reachable via positive BFS

#### test_graph_expand_existing_positive_types_still_expand (AC-04 regression)
- Arrange: edges using `Supports`, `CoAccess`, `Prerequisite`, `Informs`
- Act: `graph_expand` from source
- Assert: all targets reachable (no regression from adding RelatedTo to the positive BFS set)

---

## Code Review Gate (SR-01 Mitigation)

The following structural checks must be verified during Stage 3c:

1. `graph_ppr.rs` `positive_out_degree_weight`: contains `RelationType::RelatedTo` at equal
   weight to the other 4 positive types.
2. `graph_ppr.rs` `personalized_pagerank` edges_of_type calls: both include `RelationType::RelatedTo`.
3. `graph_expand.rs` outgoing traversal: `RelationType::RelatedTo` in the positive BFS set.
4. `graph_ppr.rs`: `RelationType::Advances` and `RelationType::Motivates` are ABSENT from all
   positive type lists.
5. `graph_expand.rs`: `RelationType::Advances` and `RelationType::Motivates` are ABSENT from
   the positive BFS set.

Use the grep commands from relation-type.md for automated verification.
