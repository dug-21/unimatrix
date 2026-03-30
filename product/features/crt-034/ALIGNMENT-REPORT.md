# Alignment Report: crt-034

> Reviewed: 2026-03-30
> Artifacts reviewed:
>   - product/features/crt-034/architecture/ARCHITECTURE.md
>   - product/features/crt-034/specification/SPECIFICATION.md
>   - product/features/crt-034/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-034/SCOPE.md
> Scope risk source: product/features/crt-034/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly repairs a frozen co-access signal gap; supports the PPR-based intelligence pipeline |
| Milestone Fit | PASS | Solidly in Wave 1 / Wave 1A territory — keeping the typed graph current is a prerequisite for W3-1 |
| Scope Gaps | PASS | All SCOPE.md goals, non-goals, constraints, and ACs addressed in source docs |
| Scope Additions | WARN | Architecture introduces an SR-05 first-tick detectability mechanism (warn on tick < N) that is implied by SCOPE-RISK-ASSESSMENT.md but is not in SCOPE.md; scope treats SR-05 as a deployment/ordering control |
| Architecture Consistency | PASS | Architecture, specification, and scope are internally consistent; one open question noted below |
| Risk Completeness | PASS | Risk-test strategy covers all 6 scope risks and adds 7 implementation-level risks with full test scenario coverage |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | SR-05 early-tick warn! mechanism | SCOPE.md §Constraints states "no error, no warning" for silent signal loss; SCOPE-RISK-ASSESSMENT.md SR-05 recommends a defensive log; Architecture §SR-05 formalizes it as `warn!` when `qualifying_count == 0 AND current_tick < N`. SCOPE.md does not explicitly authorize this mechanism. |
| Simplification | Migration constant unification (AC-07) | SCOPE.md says "No changes to the v12→v13 migration"; spec AC-07 requires removing or aliasing `CO_ACCESS_BOOTSTRAP_MIN_COUNT`. The spec qualifies this as "removed and replaced or set equal to" (no code behavior change, only constant deduplication). Acceptable — the migration SQL is untouched; only a private symbol is potentially redirected. |

---

## Variances Requiring Approval

### WARN-01: SR-05 First-Tick Warn! Mechanism Added Beyond Scope

**What**: SCOPE.md §Constraints lists "No changes to co_access write paths or the co-access staleness cleanup logic" and in §Known Limitation describes the #409 race as a risk managed at the GH milestone level, not by code. The architecture (§SR-05) and the risk-test strategy (R-06, scenarios covering four warn! quadrants) define a runtime mechanism — `warn!` logged when `qualifying_count == 0 AND current_tick < PROMOTION_EARLY_RUN_WARN_TICKS` — that was not in SCOPE.md but was recommended in SCOPE-RISK-ASSESSMENT.md SR-05.

**Why it matters**: This is a scope addition — behavior the human did not explicitly request. The mechanism is benign (log-only, no data impact), but it requires a per-process tick counter and changes the observable logging contract. The SCOPE-RISK-ASSESSMENT.md recommendation is advisory; it does not constitute approval.

**Recommendation**: Accept. The mechanism is low-risk, directly addresses a High-severity integration risk (silent signal loss from #409 race), and was explicitly recommended by the scope risk assessment. Document as an approved addition in a follow-up SCOPE.md annotation, or accept during delivery gate review.

---

## Detailed Findings

### Vision Alignment

The product vision (Intelligence & Confidence gap table) calls out:

> "Co-access and contradiction never formalized as graph edges" — Fixed in W1-1.

crt-034 is the operational complement: W1-1 bootstrapped `CoAccess` edges once; crt-034 makes that bootstrap self-refreshing. Without crt-034, the PPR graph's co-access signal is frozen at the bootstrap snapshot, directly contradicting the vision's self-improving relevance function.

The vision further states (crt-032 context): `w_coac` was zeroed in crt-032, making PPR via `GRAPH_EDGES` the sole carrier of co-access signal. A frozen `GRAPH_EDGES` therefore means the vision's PPR-based retrieval has no live co-access signal at all. crt-034 closes this regression.

The feature is entirely within the Wave 1 / Wave 1A intelligence foundation and does not introduce any future-milestone capabilities. Architecture is consistent with the "typed knowledge graph formalizes relationships" pillar. PASS.

### Milestone Fit

crt-034 is positioned between Wave 1 (W1-1 typed graph, complete) and Wave 1A (WA-0 ranking fusion, complete; W3-1 GNN training, roadmapped). It is a correctness fix for an existing Wave 1 deliverable — keeping GRAPH_EDGES current rather than frozen. There is no milestone overreach; no Wave 2 (security, deployment), Wave 3 (GNN), or future-wave capabilities are introduced.

The blocking dependency on GH #409 (intelligence-driven retention) is correctly characterized as a deployment-ordering concern, not a scope issue. PASS.

### Architecture Review

The architecture document is well-structured and internally consistent with the scope:

- Component breakdown covers all four touch points: new module, store constants, config extension, background.rs insertion.
- Integration surface table is precise (function signatures, constant types, config field range).
- Tick ordering diagram correctly places `run_co_access_promotion_tick` between step 2 (orphaned-edge compaction) and step 3 (`TypedGraphState::rebuild()`).
- Write pool path justification is sound: references W1-2 NLI contract (entry #3821) and explains why `AnalyticsWrite::GraphEdge` cannot be used.
- Known limitation (edge directionality, one direction only) is explicitly documented and flagged for follow-up.

One minor inconsistency: the architecture's constants table (§Component Breakdown, table row 3) lists `CO_ACCESS_WEIGHT_UPDATE_DELTA` as a public constant alongside `EDGE_SOURCE_CO_ACCESS` and `CO_ACCESS_GRAPH_MIN_COUNT`. But immediately below the table it states this constant is "module-private in `co_access_promotion_tick.rs`". The integration surface table also lists it as `const f32 = 0.1 (module-private)`. The table header is misleading; the prose and integration surface table are correct. This is a documentation clarity issue, not a semantic divergence. No VARIANCE.

**Open Question 1 (from spec)**: SQL shape for INSERT + conditional UPDATE (SR-02 / SR-01) — two-query loop vs. subquery-embedded MAX vs. CTE. The architecture resolves SR-01 (ADR-001: embedded subquery) but the per-pair loop vs. CTE question is left to the implementor. This is appropriate scope for an open architecture question and does not represent a gap.

**Open Question 2**: GH #409 merge status — confirmed as not yet merged based on git status showing it in the open issues list.

### Specification Review

The specification is complete, detailed, and traceable:

- 15 functional requirements (FR-01 through FR-15) cover all SCOPE.md goals.
- 7 non-functional requirements (NFR-01 through NFR-07) cover latency, contention, idempotency, observability, module size, compatibility.
- 15 acceptance criteria (AC-01 through AC-15) — 13 from SCOPE.md plus AC-14 (double-tick idempotency from entry #3822) and AC-15 (sub-threshold pair not GC'd). These two additions trace back to Unimatrix knowledge patterns, not user-requested scope; they represent defensive correctness tests and are appropriate additions.
- Domain model correctly captures both source and sink table schemas, constants, config fields, and ubiquitous language.
- "NOT in Scope" section mirrors SCOPE.md §Non-Goals exactly.
- Known limitations match SCOPE.md §Known Limitation.

FR-08 ("When co_access is empty or no pairs meet the threshold, the tick shall complete as a no-op with no error **and no warning**") is in tension with the Architecture §SR-05 mechanism which emits a `warn!` when `qualifying_count == 0 AND current_tick < N`. This is the same scope addition flagged in WARN-01. The spec and architecture are slightly out of sync on this point: FR-08 says "no warning" unconditionally; the architecture adds a conditional warning. This inconsistency should be resolved at delivery by clarifying FR-08 to read "no warning (except the SR-05 early-tick detection if authorized)."

### Risk Strategy Review

The risk-test strategy is thorough and well-prioritized:

- 13 risks identified (R-01 through R-13), spanning write failures, SQL correctness, config errors, ordering violations, and implementation hazards.
- All 6 scope risks (SR-01 through SR-06) are traced with explicit resolution references (ADR-001 through ADR-006 in the architecture).
- Critical priority assigned correctly to R-01 (silent absorption of write failures), which maps to the infallible tick contract — the highest-risk design choice.
- Edge cases (E-01 through E-06) cover boundary conditions that are commonly missed: single qualifying pair, all-identical-count ties, exact-cap match, minimum cap, delta boundary, self-loop pair.
- Security section (S-01 through S-03) is appropriately scoped — no external input, config-bounded operator risk, parameterized SQL.
- Failure modes (FM-01 through FM-04) cover all failure paths including server restart mid-promotion.

One coverage note: R-06 tests the `warn!` condition `qualifying_count == 0 AND current_tick < N` but the specification (FR-08) does not yet authorize this mechanism. This is the same inconsistency noted above. The risk strategy is ahead of the spec on this point.

The mandatory test list (13 items) is correct and traceable to spec ACs. The risk strategy does not introduce any new scope.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for vision alignment patterns — found entries #2298 (config key semantic divergence pattern), #3337 (architecture/spec header divergence pattern), #3742 (optional future branch scope addition WARN pattern). Pattern #3742 is directly applicable: "architecture describes a deferred-branch capability not in scope — WARN if architecture and risk diverge from scope deferral." Applied to WARN-01 (SR-05 mechanism present in architecture but not in SCOPE.md). Pattern #3337 noted but not applicable (no informal header divergence found).
- Stored: nothing novel to store — WARN-01 is an instance of the existing pattern #3742 (scope addition originating from risk assessment recommendation). The specific FR-08 / SR-05 tension between "no warning" spec text and conditional-warn architecture is feature-specific and does not yet generalize across 2+ features to warrant a new pattern entry.
