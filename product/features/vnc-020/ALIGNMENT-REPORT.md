# Alignment Report: vnc-020

> Reviewed: 2026-05-20
> Artifacts reviewed:
>   - product/features/vnc-020/architecture/ARCHITECTURE.md
>   - product/features/vnc-020/specification/SPECIFICATION.md
>   - product/features/vnc-020/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/vnc-020/SCOPE.md
> Scope risk source: product/features/vnc-020/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly extends W1B-2 (context_graph tool, IN FLIGHT per vision). Completes typed graph traversal surface — core Wave 1B deliverable. |
| Milestone Fit | PASS | Correctly targets Wave 1B delivery. No Wave 2+ capability is introduced. Graph traversal completes before Wave 2 cloud deployment begins. |
| Scope Gaps | PASS | All SCOPE.md goals, acceptance criteria (AC-01 through AC-31), constraints (C1–C9), and open questions (OQ-01 through OQ-04) are addressed in the source documents. |
| Scope Additions | WARN | Architecture adds one behavior not in SCOPE.md: explicit rejection of `resolve_supersessions` when passed to `inverse` or `filter` modes (silently ignored per architecture vs. possible error per architecture open question). Minor — does not affect functional scope. |
| Architecture Consistency | PASS | Module split, rejection matrix, SQL patterns, BFS design, staleness disclosure, and SR disposition all consistent with SCOPE.md and SCOPE-RISK-ASSESSMENT.md. All SCOPE-RISK items addressed. |
| Risk Completeness | PASS | 14 risks registered. All SCOPE-RISK items traced. Critical risks R-01 through R-04 covered with concrete scenarios. Security risks SR-A, SR-B, SR-C documented. Integration risks IR-01 through IR-04 cover dynamic SQL, async BFS, file budget, and IN-clause binding. |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | None | All SCOPE.md goals, ACs, constraints, and OQs are addressed across the three source documents. |
| Addition | `resolve_supersessions` behavior on `inverse`/`filter` modes | SCOPE.md says these modes do not support `resolve_supersessions` (SQL-only, not applicable). ARCHITECTURE.md §Param/Mode Rejection Matrix notes the flag is "silently ignored" on these modes, then adds an Open Question to the spec writer: "may choose to explicitly reject it with an error message for clarity." This creates a small unresolved behavioral decision not present in SCOPE.md. Low risk — both valid outcomes are noted, and neither changes functional scope. Requires resolution at implementation. |
| Addition | `from_id == to_id` self-path behavior | RISK-TEST-STRATEGY.md R-12 edge case section defines expected behavior ("found: false — a self-path is not a valid traversal result") for `from_id == to_id`. SCOPE.md does not address this case. The behavior is not controversial but is not an explicitly approved specification decision. |
| Simplification | `direction` parameter for `path` mode deferred | SCOPE.md OQ-04 resolved: outgoing-only, no `direction` param. Architecture and specification carry this forward correctly. Rationale documented in ADR-004 (#4505) and SCOPE.md OQ-04. |

---

## Variances Requiring Approval

No VARIANCE or FAIL classifications identified.

The two WARN-level scope additions are minor and are documented below for human awareness. Neither requires architectural rework; both require a decision before the implementation agent begins.

### WARN-1: `resolve_supersessions` behavior on `inverse` and `filter` modes

**What**: SCOPE.md marks `resolve_supersessions` as not applicable to `inverse` and `filter` modes (SQL-only, live DB). ARCHITECTURE.md silently ignores the flag on these modes but leaves an open question for the spec writer to "explicitly reject it with an error message for clarity." SPECIFICATION.md carries the silent-ignore behavior in the Param/Mode Rejection Matrix note, but does not close the open question with a definitive AC.

**Why it matters**: Leaving this open means the implementation agent has two valid paths (silent ignore vs. validation error) with no spec mandate. A silent-ignore on a mis-typed flag is generally fine for SQL modes; a validation error would give callers clearer feedback. Neither path affects functional correctness, but the absence of a spec-level AC means the behavior is un-tested and un-gated.

**Recommendation**: Close this in the specification before delivery begins. Recommended resolution: silent ignore (consistent with the existing `agent_id` field behavior on all modes), documented in the Param/Mode Rejection Matrix notes column. If an explicit error is preferred, add an AC. Either choice is acceptable — the decision just needs to be made.

### WARN-2: `from_id == to_id` self-path behavior is spec'd in the risk strategy but not in the specification

**What**: RISK-TEST-STRATEGY.md R-12 declares that `from_id == to_id` returns `found: false` (not a zero-hop found). This is a behavioral decision, not just a test plan observation. The SPECIFICATION.md does not contain a corresponding FR or AC for this edge case.

**Why it matters**: Without a spec-level FR or AC, the implementation agent may choose `found: true, hops: [], length: 0` (technically a "path of length zero") instead. Both interpretations are defensible; only one is tested. A test for a behavior that the spec doesn't mandate is a test for an assumption, not a requirement.

**Recommendation**: Add one line to the SPECIFICATION.md path mode section: "When `from_id == to_id`, the response is `{ found: false, ... }`. A self-path is not a valid traversal result." Then add a corresponding unit test AC. This closes the gap at no implementation cost.

---

## Detailed Findings

### Vision Alignment

The product vision (Wave 1B) states:

> "W1B-2: `context_graph` tool — IN FLIGHT (#596–598). One MCP tool (14th) with seven modes... inverse+path+filter (#598)."

vnc-020 is the exact delivery of issue #598. The three modes (inverse, filter, path) complete the `context_graph` tool series as described in the vision. The vision explicitly deferred these modes from vnc-018 and vnc-019 — this feature closes that deferral.

The vision's stated goal for W1B-2 is "direct typed-edge traversal: dependency navigation, supersession chains, gap detection (entries missing expected incoming edges), and Goal → Decision → Outcome audit subgraphs in a single query." The `inverse` mode directly implements gap detection; `filter` mode implements structural health queries (stale goals, multi-advancement); `path` mode implements dependency navigation and Goal traceability. All three vision-stated use cases are addressed.

The vision non-negotiable "in-memory hot path: all analytics-derived search data cached in Arc<RwLock<_>> rebuilt by tick" is respected: `path` mode acquires a read lock, clones the graph, and releases before BFS — consistent with the tick-rebuild pattern documented throughout the vision. The staleness disclosure mandated by ADR-005/ADR-004 is present.

### Milestone Fit

vnc-020 sits squarely in Wave 1B, which "runs alongside Wave 2" per the vision dependency graph. The feature introduces:

- No Wave 2 capabilities (no HTTP transport, OAuth, container packaging)
- No Wave 3 capabilities (no GNN, GGUF, synthesis)
- No Wave 1A capabilities (no session conditioning, proactive injection)

The feature adds three query modes to an existing MCP tool (total count stays at 14). Schema version stays at 27 — no migration, no new tables. This is pure Wave 1B scope.

The vision's dependency graph shows W1B-2 completing before W2-1 deployment begins (or running in parallel). vnc-020 as designed does not create any ordering violations.

### Architecture Review

**Module split (ADR-001/#4502)**: Three sibling modules (`graph_read_inverse.rs`, `graph_read_filter.rs`, `graph_read_path.rs`) with dispatch and centralized validation in `graph_read.rs`. This follows the established `graph_read_neighbors.rs` / `graph_read_subgraph.rs` pattern from prior features. The 500-line budget constraint (SCOPE.md C5) is addressed with a projection of ~500 lines post-expansion, with all handler logic moved to sibling modules.

**Backward compatibility (ADR-002/#4503)**: All new `GraphParams` fields are `Option<T>`. The eight new fields and their mode owners are fully specified. No existing field is removed or retyped. Consistent with ADR-003 vnc-018.

**SQL injection surface**: The architecture explicitly rejects the ASS-057 Track B `where_clause: String` proposal on injection grounds (ADR-007/#4508). All filter clauses are built from typed parameters bound via sqlx. The RISK-TEST-STRATEGY.md SR-A mitigation (grep for string interpolation, fuzz extreme values) is appropriate.

**BFS design (path mode)**: The path-carrying frontier approach is justified for the graph bounds (3k nodes, 10k edges). Lock-acquire-clone-release pattern is consistent with neighbors and subgraph modes. `follow_to_current` reuse for endpoint and per-hop supersession resolution is correctly established (SR-05 resolution).

**Staleness disclosure**: The exact disclosure text is specified verbatim in the architecture and reproduced in the specification. This satisfies ADR-004 vnc-019 (#4493) precedent and SCOPE-RISK SR-01.

**One minor internal inconsistency**: The architecture's Param/Mode Rejection Matrix (§Param/Mode Rejection Matrix) marks `resolve_supersessions` on `inverse` and `filter` as `—` (no entry), while the Architecture's Staleness Disclosure section notes it is "silently ignored." The specification's matrix note says "No in-memory graph; SQL reads live DB regardless" but also doesn't mandate either behavior. This inconsistency is the source of WARN-1 above.

### Specification Review

All 31 SCOPE.md acceptance criteria (AC-01 through AC-31) are present in the specification with verification methods. All 9 SCOPE.md constraints (C1–C9) are mapped to spec requirements and verifiable checks. The parameter/mode rejection matrix is present and correctly marks all 8 new fields across all 7 modes. Wire formats for InverseResponse, FilterResponse, PathResponse (found/not-found) are specified. User workflows W1–W5 illustrate the primary use cases.

The specification contains two open questions (OQ-A1, OQ-A2) that were supposed to be resolved at the design phase per SCOPE.md ("All open questions resolved before design phase began"). OQ-A1 asks whether per-hop intermediate resolve_supersessions requires new infrastructure — this is in fact resolved in the architecture (SR-05 Resolution section documents the reuse of `follow_to_current`). OQ-A2 asks about the validation boundary between `graph_read.rs` and sibling modules — this is also resolved in the architecture (centralized validation in `graph_read.rs`, per-handler parameter validation in sibling modules). These open questions are stale artifacts in the spec; the architecture has answered both. The implementation agent should treat them as resolved.

The `from_id == to_id` edge case is addressed in the risk strategy but not in the specification (WARN-2 above).

### Risk Strategy Review

The risk strategy is thorough. All 8 SCOPE-RISK items are traced in the Scope Risk Traceability table. The 14 registered risks cover:

- 4 Critical risks (R-01 staleness disclosure, R-02 max_edge_count=0 boundary, R-03 BFS visited-set double-enqueue, R-04 rejection matrix completeness)
- 6 High risks (R-05 AND semantics, R-06 resolved ID in response, R-07 depth behavior change, R-08 dual correlated subqueries, R-09 no-path vs. not-in-snapshot, R-10 deprecated entry exclusion)
- 2 Medium risks (R-11 category-only filter, R-12 path length off-by-one)
- 2 Low risks (R-13 limit boundary, R-14 from_str wildcard ordering)

The Unimatrix knowledge references in the risk strategy are appropriate and specific:
- Pattern #4494 (visited-set keyed on resolved ID) → R-03
- Pattern #4497 (infallible handler signatures mask validation paths) → R-09
- Pattern #4058 (push_bind for dynamic SQL IN clauses) → IR-04
- Lesson #4473 (warn+continue masks failure-path tests) → R-09

Security risks SR-A (filter mode parameterization), SR-B (inverse alias construction), and SR-C (BFS cycle handling) are well-specified with concrete mitigation scenarios.

Integration risk IR-02 (async `follow_to_current` calls inside BFS loop) correctly identifies the bounded Store read concern under high deprecated-fraction graphs and is covered by a concrete unit test scenario.

The risk strategy includes a `from_id == to_id` edge case in the R-12 section with a definitive behavioral assertion ("found: false — a self-path is not a valid traversal result") that is not present in the specification. This is the source of WARN-2.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns — found 4 relevant entries:
  - #2298: Config key semantic divergence (alignment, vision tags) — not directly applicable to vnc-020
  - #3337: Architecture diagram informal headers diverge from spec (#4493 staleness pattern) — applicable; staleness disclosure text alignment between ARCHITECTURE.md and SPECIFICATION.md is correctly handled in vnc-020
  - #3742: Optional future branch in architecture must match scope intent — applicable; WARN-1 (resolve_supersessions ambiguity on SQL modes) is exactly this pattern: an architectural ambiguity that does not match a clear scope intent
  - #3426: Formatter features underestimate section-order regression — not directly applicable
- Stored: nothing novel to store — the two WARNs are feature-specific low-risk resolution gaps, not generalizable cross-feature patterns. The closest generalizable pattern (#3742: architectural ambiguity on deferred/N/A parameters) already exists.
