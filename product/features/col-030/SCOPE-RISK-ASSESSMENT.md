# Scope Risk Assessment: col-030

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `edges_of_type` has never been exercised for `RelationType::Contradicts` — the method exists but correctness on this edge type is unconfirmed until this feature runs | High | Med | Architect must confirm `edges_of_type` handles both Outgoing and Incoming directions for Contradicts before treating it as a black box |
| SR-02 | Parallel Vec invariant: `results_with_scores` and `final_scores` must be filtered in lockstep — any indexing bug produces silent score-to-entry misalignment, not a panic | High | Med | Implement mask application as a single indexed pass; do not filter the two Vecs with separate iterator chains |
| SR-03 | `graph.rs` is already at ~588 lines; suppression function (~30–50 lines) plus tests may push past the 500-line per-file limit, triggering a gate-3b violation (entry #3580) | Med | High | Architect must decide `graph.rs` vs `graph_suppression.rs` split before implementation begins; do not leave this to implementation judgment |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Always-on suppression with no config toggle means a defective Contradicts edge (false positive from NLI) silently drops a legitimate result — no operator escape hatch until #412 ships audit visibility | High | Med | Spec writer should define observable behavior when suppression fires: at minimum a log line at DEBUG level so operators can correlate missing results before #412 lands |
| SR-05 | Zero-regression eval gate validates only the no-Contradicts path (suppression is a no-op for all existing scenarios) — gate passage gives no signal about suppression correctness; a broken suppression function can pass gate and ship | High | High | Positive suppression path must be covered by integration tests in `search.rs`, not the eval gate; spec must make this mandatory, not optional |
| SR-06 | Open Question 4 (SCOPE.md §Open Questions) is unresolved at scope time: file placement decision deferred to implementation — this is a gate-3b risk per entry #3580 | Med | Med | Resolve file placement in architecture; do not carry it as an open question into delivery |

## Integration Risks

| Risk ID | Risk | Likelihood | Severity | Recommendation |
|---------|------|------------|----------|----------------|
| SR-07 | Test helper `create_graph_edges_table` is pre-v13 schema only (entry #3600) — new suppression integration tests in `search.rs` must not use it or they will fail against production schema | Med | Med | Spec must call out explicitly which test helpers are safe to use for GRAPH_EDGES setup in integration tests |
| SR-08 | Cold-start guard (`use_fallback = true`) relies on a flag set during tick rebuild — if the flag is ever unset before the graph is fully populated (race between tick completion and search request), suppression could query a partial graph | Low | Med | Architect should confirm the `use_fallback` flag transition is atomic with respect to the search hot path's read lock |

## Assumptions

- **SCOPE.md §Background Research / Edge direction**: Assumes NLI writes edges unidirectionally and never writes the reverse. If this assumption is wrong (e.g., a future NLI change adds bidirectional writes), bidirectional querying would double-suppress. Low likelihood given the code is settled, but the spec should note it as a stated invariant.
- **SCOPE.md §Eval Harness**: Assumes the zero-regression gate is sufficient for merge readiness. It is not sufficient for suppression behavior — only for non-regression. The spec must require positive suppression integration tests as a separate mandatory gate.
- **SCOPE.md §Search Pipeline Integration Point Step 10b**: Assumes `final_scores` and `results_with_scores` are still strictly parallel at Step 10b. If any earlier step (floors, NLI, etc.) ever de-syncs them, the mask application will corrupt output silently.

## Design Recommendations

- **SR-01**: Architect should add a micro-verification step: write a unit test that confirms `edges_of_type(Contradicts, Outgoing)` and `edges_of_type(Contradicts, Incoming)` both return correct neighbors on a hand-constructed graph before relying on the API in production code.
- **SR-02 + SR-05**: Spec writer should declare the parallel Vec mask application and the positive suppression integration test as non-negotiable acceptance criteria (not "should" but "must").
- **SR-03 + SR-06**: Architect resolves file placement and 500-line budget before issuing the implementation brief. Carrying Open Question 4 into delivery risks a gate-3b rejection.
- **SR-04**: Spec writer adds a DEBUG-level log line emitted when at least one entry is suppressed, including the suppressed entry ID and the contradicting entry ID. This is the minimum audit trail until #412 ships.
