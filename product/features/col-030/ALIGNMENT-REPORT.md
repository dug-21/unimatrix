# Alignment Report: col-030

> Reviewed: 2026-03-27
> Artifacts reviewed:
>   - product/features/col-030/architecture/ARCHITECTURE.md
>   - product/features/col-030/specification/SPECIFICATION.md
>   - product/features/col-030/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope sources: product/features/col-030/SCOPE.md
>                product/features/col-030/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances the "knowledge integrity" and "trustworthy retrieval" core vision |
| Milestone Fit | PASS | Correctly positioned as Wave 1A stepping stone; no future-milestone capability introduced |
| Scope Gaps | PASS | All SCOPE.md goals, non-goals, constraints, and acceptance criteria are represented in the source docs |
| Scope Additions | WARN | Architecture adds a chain-suppression behaviour case (rank-0 → rank-2 → rank-3) not described in SCOPE.md §Suppression Logic; minor |
| Architecture Consistency | WARN | Architecture resolves SCOPE.md Open Question 4 (file placement) but the specification carries OQ-01 as "Unresolved. Assigned to architect." — the two documents are out of sync |
| Risk Completeness | PASS | RISK-TEST-STRATEGY covers all SCOPE-RISK-ASSESSMENT risks (SR-01 through SR-08) and adds 5 new delivery-specific risks with full scenario coverage |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `graph_tests.rs` test case count | SCOPE.md lists 6 test cases for `suppress_contradicts`; SPECIFICATION.md (FR-13) expands to 8 cases. Rationale: FR-13 adds a "Contradicts edge between rank-2 and rank-3 only" case and splits the chain case for clarity. Both are safe additions that strengthen correctness coverage — no scope concern. |
| Addition | Chain-suppression unit test case | Architecture §Component 3 unit test 4 states "rank-0 contradicts rank-2, rank-2 contradicts rank-3 → both suppressed". SCOPE.md §Suppression Logic defines the algorithm but does not enumerate this test case explicitly. The logic derivation is correct and consistent with SCOPE.md's algorithm. WARN classification because an implicit test-case expansion should be traced. |
| Simplification | `final_scores` shadow vs `let mut` | Architecture makes explicit that `final_scores` at line 893 is `let` and the Step 10b implementation must shadow it with `let final_scores = new_fs`. SCOPE.md does not address this binding detail. Architecture is correctly filling an implementation gap, not adding scope. Acceptable. |
| Gap | None identified | All SCOPE.md goals (1–5), acceptance criteria (AC-01 through AC-10), constraints, and non-goals are present and traceable across the three source documents. |

---

## Variances Requiring Approval

### WARN-01: Specification carries OQ-01 as "Unresolved" while Architecture has resolved it

**What**: SPECIFICATION.md §Open Questions states:

> OQ-01 — File placement (SR-06, FR-12) ... *Status*: Unresolved. Assigned to architect.

ARCHITECTURE.md §File Placement Decision states:

> Decision: `graph_suppression.rs` as a sibling module. (ADR-001)

FR-12 in the specification itself reads "If ... the function must be placed in a new sibling module ... This placement decision must be resolved before implementation begins." The architect has resolved it. The specification's OQ-01 status block was not updated to reflect that resolution.

**Why it matters**: An implementation agent reading only SPECIFICATION.md (the document they are most likely to follow as a delivery contract) sees an unresolved open question on file placement. This is the exact scenario SCOPE-RISK-ASSESSMENT SR-06 flagged as a gate-3b risk. If the delivery agent treats OQ-01 as genuinely open and makes their own placement choice (e.g., inline in `graph.rs`), it will produce a gate-3b violation because `graph.rs` is already at 587 lines.

**Recommendation**: Before the implementation brief is issued, update SPECIFICATION.md §Open Questions OQ-01 to:

> *Status*: Resolved by architecture (ADR-001). `suppress_contradicts` placed in `graph_suppression.rs`, re-exported from `graph.rs`. See ARCHITECTURE.md §File Placement Decision.

This is a one-line edit with no functional change to the spec. The delivery coordinator should make this update before spawning the implementation agent.

---

### WARN-02: `graph_tests.rs` line-count risk not surfaced in architecture document

**What**: RISK-TEST-STRATEGY R-01 identifies `graph_tests.rs` at 1,068 lines — already more than double the 500-line convention — and classifies this as Critical/High. It mandates that new `suppress_contradicts` unit tests must live in a separate `graph_suppression_tests.rs` (or inline in `graph_suppression.rs` under `#[cfg(test)]`), not appended to the existing file.

ARCHITECTURE.md §Test Coverage Strategy states:

> Tests for `suppress_contradicts` using hand-constructed `TypedRelationGraph` instances ... go in `graph_tests.rs` to keep test colocation consistent with the existing pattern.

This directly contradicts R-01's mandatory requirement. The architecture document sends the unit tests to a file that is already at 1,068 lines; the risk strategy says they must not go there.

**Why it matters**: If the implementation brief follows ARCHITECTURE.md, the implementation agent will append tests to `graph_tests.rs`, producing a 1,148+ line file. Gate-3b checks file size and will reject the PR. This is classified as Critical/High in the risk strategy and was documented as a primary source of rework in nan-009 (entry #3580).

**Recommendation**: Architecture §Test Coverage Strategy must be updated before the implementation brief is issued. Replace "go in `graph_tests.rs`" with a directive that unit tests for `suppress_contradicts` belong in `graph_suppression.rs` under `#[cfg(test)]`, or in a new `graph_suppression_tests.rs` sibling. The implementation brief must include this as an explicit, unambiguous placement instruction.

---

## Detailed Findings

### Vision Alignment

col-030 is tightly aligned with the product vision on multiple dimensions.

The vision core statement: "Unimatrix is a self-learning knowledge integrity engine ... makes it trustworthy, correctable, and ever-improving. It delivers the right knowledge at the right time."

col-030 directly serves "trustworthy" and "delivers the right knowledge": when an agent receives contradictory entries in a single response with no signal that a conflict exists, the engine is delivering knowledge that is neither trustworthy nor right. Suppressing the lower-ranked collision member makes every search response internally consistent.

The vision story section states: "A typed knowledge graph formalizes relationships — not just what agents retrieve together, but why: support, contradiction, supersession, dependency." col-030 is the first feature that acts on `Contradicts` edges at retrieval time — it transforms a graph capability (W1-1, COMPLETE) into a user-visible behavior. This is the natural next step in the vision's intelligence pipeline progression.

The vision §The Critical Gaps / Intelligence & Confidence table shows: "Co-access and contradiction never formalized as graph edges — Fixed (W1-1)." col-030 closes the downstream half of this gap: the edges are now written (W1-4), persisted (W1-1), and — with col-030 — finally acted on during retrieval.

The vision's "intelligence pipeline is the core of the platform" framing requires that every signal the pipeline captures be used. Contradicts edges that sit in `GRAPH_EDGES` but are never consulted at query time represent unused signal. col-030 closes this gap without modifying the scoring formula — it is a pure retrieval correctness measure, not a scoring change.

No vision principles are contradicted. No future-wave capabilities (W3-1 GNN, W2-x deployment) are pre-implemented. No shortcuts were taken against the vision's emphasis on integrity.

### Milestone Fit

col-030 is correctly positioned as a Wave 1A stepping stone. The vision dependency graph places PPR (#398) downstream of this feature, and the scope explicitly calls col-030 a "predecessor stepping stone before PPR (#398)."

The vision's Wave 1A items (WA-0 through WA-5, ASS-029) are all listed in the dependency graph. col-030 does not appear by name in the vision — it is a feature-level decomposition of the W1-1 payoff, below the vision's level of abstraction. This is expected and correct. The feature is not jumping ahead to Wave 2 or Wave 3 capabilities.

The zero-regression eval gate (NFR-02, AC-06) is correctly referenced against the W1-3 eval harness infrastructure. The scope calls this out as using "the existing `--distribution_change false` profile path" — no new eval infrastructure is invented or required.

Milestone fit: PASS.

### Architecture Review

The architecture document is thorough, technically correct, and well-aligned with both SCOPE.md and the vision. Key strengths:

- All four SCOPE.md open questions that were resolvable are resolved (Open Questions 4, 5, edge direction, `use_fallback` atomicity).
- SR-01 through SR-08 from SCOPE-RISK-ASSESSMENT are all addressed with explicit decisions (ADR-001 through ADR-005).
- The `TypedGraphState` atomicity analysis (SR-08) is correctly resolved: `use_fallback` and `typed_graph` are cloned under the same read lock at Step 6, preventing any torn-read race.
- The parallel Vec invariant (SR-02) is addressed with an explicit code-level contract, not just a principle statement.
- The integration surface table is complete and precise.

One internal inconsistency (WARN-02 above): §Test Coverage Strategy directs unit tests to `graph_tests.rs` (currently 1,068 lines), contradicting the risk strategy's Critical/High finding.

One external inconsistency (WARN-01 above): the architecture resolves OQ-01 but the specification's OQ-01 status block was not updated.

The architecture correctly avoids:
- Touching the scoring formula (non-goal in SCOPE.md).
- Adding config toggles (explicitly prohibited in SCOPE.md Constraints and SCOPE-RISK-ASSESSMENT SR-04).
- Applying suppression to `context_lookup` or `context_get` (non-goal in SCOPE.md).
- Introducing new crates (Constraint in SCOPE.md).
- Requiring schema changes (Constraint in SCOPE.md).

### Specification Review

The specification is comprehensive and directly traceable to SCOPE.md. FR-01 through FR-15, NFR-01 through NFR-07, and AC-01 through AC-12 cover all SCOPE.md acceptance criteria (AC-01 through AC-10) with appropriate expansions.

FR-13 expands SCOPE.md's six test cases to eight. The additions are:
1. A "Contradicts edge between rank-2 and rank-3 only" case (rank-0 and rank-1 unaffected) — tests that suppression is correctly scoped to conflicting pairs, not applied globally.
2. Splitting the "Incoming direction" case (AC-03) as a standalone required case — this is the most important test for catching the bidirectional omission failure mode (R-05).

Both additions are safe and conservative — they prevent delivery-agent errors that SCOPE.md's original test case list would not catch.

The specification carries one unfixed open question: OQ-01 (see WARN-01). All other open questions from SCOPE.md are correctly resolved and marked resolved in the specification.

NFR-05 (observability floor: DEBUG log with both suppressed entry ID and contradicting entry ID) is correctly derived from SCOPE-RISK-ASSESSMENT SR-04 and extended in the right direction. The specification does not weaken SR-04's recommendation.

AC-12 (parallel Vec single-pass requirement) is a specification-level strengthening of SCOPE.md's SR-02 constraint. This is correct alignment, not scope addition.

### Risk Strategy Review

The RISK-TEST-STRATEGY is the strongest of the three source documents. It:

- Traces all eight SCOPE-RISK-ASSESSMENT risks (SR-01 through SR-08) to architecture decisions and test scenarios via the Scope Risk Traceability table.
- Adds 13 delivery-specific risks (R-01 through R-13) that are below the scope-risk level of abstraction but important for gate compliance.
- Correctly identifies R-01 (`graph_tests.rs` at 1,068 lines) as Critical/High — the most likely source of gate-3b rejection.
- Correctly identifies R-13 (eval gate passage mistaken for suppression correctness) as Med/High — a discipline failure that has historically caused features to ship with silent broken behavior.

The risk strategy's finding on R-01 (WARN-02 above) is the document's most important contribution. It catches a real gap in the architecture document's test placement guidance. The risk strategy's recommendation (tests in `graph_suppression.rs` under `#[cfg(test)]` or in `graph_suppression_tests.rs`) is the correct resolution.

No risks in SCOPE-RISK-ASSESSMENT are omitted from the risk strategy. The risk strategy's additional findings (R-02 through R-13) are all within scope — they are delivery-agent failure modes, not scope expansions.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `vision alignment scope review variance pattern` — returned entries #2298 (config key semantic divergence), #3426 (formatter regression risk), #2964 (signal fusion pattern). None directly applicable to the vision alignment review process; no prior vision alignment pattern entries found.
- Stored: nothing novel to store — the variances found (spec/architecture sync gap; test-placement contradiction) are col-030-specific. However, the pattern "architecture resolves open questions but specification document is not updated to reflect them, creating a delivery-agent ambiguity trap" recurs broadly. If this pattern appears again in a future feature, it warrants a stored pattern entry.
