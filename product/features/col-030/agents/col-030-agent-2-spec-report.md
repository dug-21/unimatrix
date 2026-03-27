# Agent Report: col-030-agent-2-spec

## Task
Write the specification for col-030 Contradicts Collision Suppression (GitHub #395).

## Output
- `/workspaces/unimatrix/product/features/col-030/specification/SPECIFICATION.md`

## Key Decisions

1. **AC-12 added (parallel Vec invariant)**: SR-02 from SCOPE-RISK-ASSESSMENT.md elevated to a mandatory acceptance criterion requiring a single indexed pass. The SCOPE.md listed this only as a constraint; the spec promotes it to AC-12 because the failure mode (silent score-to-entry misalignment) is undetectable by tests that only check result order.

2. **AC-03 added (bidirectional Incoming direction)**: SCOPE.md AC-02 covered only the Outgoing case. A separate AC-03 added to explicitly verify the Incoming direction check, since `nli_detection.rs` writes unidirectionally and the Incoming case can only be caught if `Direction::Incoming` is explicitly exercised in a unit test.

3. **FR-14 (positive integration test) declared mandatory gate**: Per SR-05, the zero-regression eval gate (AC-06) gives no signal about suppression correctness. FR-14 and AC-07 are declared non-optional and non-substitutable by the eval gate.

4. **FR-15 (SR-07 test helper)**: `create_graph_edges_table` from `unimatrix-store` reflects pre-v13 schema only. Spec explicitly prohibits its use in `search.rs` integration tests and mandates `build_typed_relation_graph` with in-memory fixtures as the safe path.

5. **OQ-01 (file placement, SR-06)**: SCOPE.md Open Question 4 carried forward as the sole open question for the architect. Spec makes explicit that this must be resolved before the implementation brief is issued (gate-3b risk per entry #3580). Not resolved here because it requires line-count measurement, which is the architect's responsibility.

6. **AC-11 (observability log)**: SR-04 recommendation translated to a concrete acceptance criterion: `tracing::debug!` with both suppressed entry ID and contradicting entry ID. Spec records this is the sole operator escape hatch before #412 lands.

## Open Questions Surfaced
- **OQ-01**: `graph.rs` file placement for `suppress_contradicts`. Must be resolved by architect before implementation brief (see SPECIFICATION.md §Open Questions).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — entries #3616 and #3624 directly applicable (Step 10b insertion pattern and mandatory positive integration test gate for suppression features).
