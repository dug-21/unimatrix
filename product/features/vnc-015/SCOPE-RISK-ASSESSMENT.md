# Scope Risk Assessment: vnc-015

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | 10 new RelationType variants × 4 required sites = 40 coordinated changes; a missing `from_str()` arm causes silent row-drop at Pass 2b with no compile error (pattern #3950) | High | High | Spec must enumerate all 40 site-change targets explicitly; gate must grep-verify all 10 variants present in each of the 4 match arms before accepting |
| SR-02 | `write_graph_edge` returns `bool` via `rows_affected() > 0`, not `Ok(_) => true`; pseudocode written with wrong semantics caused Gate 3a rework in crt-040 (pattern #4041) | Med | High | Spec pseudocode must lead with the three-case contract table before any implementation body; edge-write loop error handling must key off the bool, not Result |
| SR-03 | Entry insert + edge writes are not in a single DB transaction; partial write (entry written, edge write fails) leaves the entry with no edges and no indication to the caller | Med | Med | Architect must decide: acceptable as infrastructure error (logged, not rolled back) as scoped, or wrapped in an explicit transaction boundary; document the partial-write blast radius |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `tools.rs` is already large; the 500-line file limit (SCOPE.md Constraints) may be breached by inlining edge-write logic in both `context_store` and `context_correct` handlers | Med | Med | Architect must verify current `tools.rs` line count before design; edge-write helper extraction is called out in SCOPE.md but the target module location is unspecified |
| SR-05 | The `DependencyOnDeprecated` rule is the first `DetectionRule` that requires injected Store data; if the constructor-injection interface is not made generic, future rules with the same need will diverge into ad-hoc patterns | Low | Med | Architect should define a typed injection interface (not just a Vec of pre-queried rows) so the pattern is reusable for rule #24+ |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | AC-16 fixes the asymmetric `query_contradicts_edges_for_entry` bug (WHERE target_id only → bidirectional). Any existing test that asserts the old asymmetric behavior will false-pass or false-fail after the fix; pattern #3650 confirms bidirectional Contradicts requires both Outgoing and Incoming calls | Med | High | Spec must audit existing `query_contradicts_edges_for_entry` test coverage before writing new tests; all existing callers of this function must be identified and verified they handle both directions |
| SR-07 | PPR expansion for `RelatedTo` (only) touches `positive_out_degree_weight` and BFS set; incorrect weight silently alters PPR mass flow; accidentally including `Advances`/`Motivates` is equally a defect since they are write-only in this feature | Med | Med | Architect confirms `RelatedTo` at equal weight to existing 4 positive types; Gate-3a negative check confirms `Advances`/`Motivates` absent from PPR/BFS |

## Assumptions

- **SCOPE.md §Background/RelationType enum**: Assumes `from_str()` is the sole guard in `build_typed_relation_graph` Pass 2b. If any additional guard exists (e.g., an allowlist), new variants also need updating there.
- **SCOPE.md §Proposed Approach**: Assumes "confidence floor validation" reads `entries.confidence` (stored value). If the live-recompute path is ever exercised instead, the validation adds a full recompute latency to every `context_store` call. The closed decision confirms stored value, but the spec must pin this explicitly.
- **SCOPE.md §Proposed Approach**: Assumes "if an edge write fails after entry insert, that is an infrastructure error" — i.e., partial-write is an accepted outcome. This assumption invalidates AC-07's atomicity guarantee for edge writes. Spec must clarify the failure contract.

## Design Recommendations

- **SR-01**: The spec writer should produce an explicit 10×4 checklist table (variant × site) as an AC. Gate-3a should grep-verify each cell. This is the single highest-probability implementation defect.
- **SR-02**: Pseudocode for the edge-write loop must open with the three-case contract table from pattern #4041 before writing any loop body.
- **SR-06**: The bidirectional Contradicts fix (AC-16) is a behavior change to an existing function. Architect should flag this as requiring its own test-update inventory — not just new tests.
- **SR-03/SR-05**: Both the partial-write blast radius and the injection interface generality are design decisions the architect must close before spec; leaving them open invites spec-level ambiguity.
