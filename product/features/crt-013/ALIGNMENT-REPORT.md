# Alignment Report: crt-013

> Reviewed: 2026-03-08
> Artifacts reviewed:
>   - product/features/crt-013/architecture/ARCHITECTURE.md
>   - product/features/crt-013/specification/SPECIFICATION.md
>   - product/features/crt-013/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly serves "Trust + Lifecycle + Integrity + Learning" by removing dead code, validating penalties, and improving retrieval quality |
| Milestone Fit | PASS | Explicitly listed as Wave 3 of Intelligence Sharpening milestone |
| Scope Gaps | PASS | All 4 components and 11 ACs from SCOPE.md addressed in source documents |
| Scope Additions | PASS | No material scope additions — ADRs and edge cases are design-level details supporting scope items |
| Architecture Consistency | WARN | Minor inconsistency between Architecture and Specification on `StatusAggregates` struct definition |
| Risk Completeness | PASS | 12 risks with 27 scenarios, all traced to scope risks, comprehensive edge case coverage |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | MicroLoRA evaluation deferred | Scope explicitly defers empirical MicroLoRA vs scalar boost evaluation to col-015. Architecture and Spec both honor this. Acceptable — matches Wave 4 dependency. |
| Inconsistency | StatusAggregates struct shape | Architecture defines `StatusAggregates` *without* `active_entries` and provides separate `load_active_entries_with_tags()` method (ARCHITECTURE.md lines 125-136). Specification's domain model defines `StatusAggregates` *with* `active_entries: Vec<EntryRecord>` (SPECIFICATION.md lines 106-112). Implementation must pick one — architecture's separation is cleaner. See detailed finding below. |

## Variances Requiring Approval

None. All checks are PASS or WARN. No VARIANCE or FAIL items requiring human approval.

The single WARN (StatusAggregates inconsistency) is a minor document-level discrepancy that the implementation phase will naturally resolve. The architecture's design (separate methods) is the better approach — it separates scalar aggregation (SQL-only, no deserialization) from entry loading (requires deserialization + tag joins), which is the whole point of the optimization.

## Detailed Findings

### Vision Alignment

**Core value proposition check:** The product vision states the defensible position is "Trust + Lifecycle + Integrity + Learning + Invisible Delivery." crt-013 directly serves this:

- **Trust**: Validating status penalties ensures deprecated knowledge is ranked correctly — users get trustworthy results, not stale ones (Component 2)
- **Integrity**: Removing dead code (`co_access_affinity()`, episodic stub) eliminates confusing signal overlap and reduces attack surface (Component 1)
- **Learning**: Co-access signal consolidation clarifies the learning pipeline architecture — two well-defined mechanisms instead of four overlapping ones (Component 1, ADR)
- **Invisible Delivery**: Configurable briefing k improves recall for hook-driven context injection without changing the delivery interface (Component 3)

**Self-learning pipeline alignment:** Vision describes "observation hooks → SQLite persistence → rule-based extraction → neural extraction → quality gates → auto-stored entries." crt-013 does not modify this pipeline — it calibrates the *retrieval* side (how stored knowledge is ranked and returned). This is complementary, not conflicting.

**Zero cloud dependency:** No external services introduced. All changes are local engine improvements. PASS.

### Milestone Fit

Product vision explicitly lists crt-013 as Wave 3 of "Intelligence Sharpening" milestone:

> **Wave 3 — Retrieval tuning (depends on crt-011):**
> - [ ] **crt-013: Retrieval Calibration** — Verify and fix episodic augmentation vs co-access double-counting (#50)...

The scope, architecture, and specification all honor the Wave 3 dependency on crt-011 (Wave 1). Architecture explicitly states: "Depends on crt-011 merged and CI green" (ARCHITECTURE.md line 239). Specification constraint C-01 reinforces this with test isolation strategy (deterministic confidence injection).

No capabilities from future milestones (Graph Enablement, Platform Hardening) are pulled forward. Architecture ADR-002 explicitly notes the two-mechanism co-access design is "transitional design pending Graph Enablement" — proper milestone discipline.

### Architecture Review

**Component 1 (Co-Access Consolidation):** Architecture provides grep-confirmed evidence that `W_COAC` and `co_access_affinity()` are dead code (ARCHITECTURE.md lines 41-43). The four-to-two mechanism reduction is well-justified. ADR-001 (Option A: delete W_COAC) and ADR-002 (two-mechanism architecture) are sound. Affected files enumerated exhaustively.

**Component 2 (Status Penalty Validation):** Six test cases defined (T-SP-01 through T-SP-06) covering flexible mode, strict mode, co-access exclusion, and edge cases. ADR-003's decision to assert ranking outcomes (not score values) is forward-looking — tests will survive Graph Enablement's replacement of hardcoded constants. Test isolation from crt-011 via deterministic confidence injection is well-designed.

**Component 3 (Briefing k):** Minimal design (field + env var, clamp [1, 20], default 3). No config framework introduced. Follows SR-05 mitigation guidance. Clean.

**Component 4 (Status Scan):** SQL queries are well-specified. Single compound query for scalar aggregates reduces round-trips. Architecture correctly identifies that active entries still need full loading for lambda/coherence computation.

**StatusAggregates inconsistency (WARN):** Architecture separates scalar aggregates from active entry loading into two methods (`compute_status_aggregates()` returning scalars + trust distribution, `load_active_entries_with_tags()` returning entries). Specification combines them into one struct with `active_entries` included. The architecture's separation is the superior design — it avoids coupling a lightweight SQL aggregation with a heavier deserialization + tag-join operation. Recommendation: implementation follows architecture's two-method design.

### Specification Review

**Functional requirements (FR-01 through FR-08):** Map 1:1 to scope components. FR-04 (ADR) adds the transitional status note per NFR-06 — appropriate elaboration, not scope addition.

**Non-functional requirements (NFR-01 through NFR-06):** All trace to scope constraints or risk assessment mitigations. NFR-03 (behavior-based assertions) and NFR-06 (transitional architecture) are human framing notes from the design process — valuable guardrails for implementation.

**Constraints (C-01 through C-07):** All trace to scope constraints or risk assessment recommendations. No new constraints introduced beyond what scope and risk assessment defined.

**Not in Scope section:** Comprehensive and consistent with SCOPE.md non-goals. Explicitly re-states "no empirical evaluation of MicroLoRA vs scalar boost overlap — deferred to col-015." Proper discipline.

### Risk Strategy Review

**Coverage:** 12 risks, 27 test scenarios. All 7 scope risks (SR-01 through SR-07) have corresponding architecture risks with explicit traceability (RISK-TEST-STRATEGY.md lines 226-232).

**High-priority risks (R-03, R-04, R-05):** All three relate to Component 2 (status penalty validation) — the highest-value component. Mitigation strategies are concrete: injected embeddings (R-04), deterministic confidence (R-05), edge case analysis with documented crossover point (R-03).

**Edge cases (EC-01 through EC-09):** Good coverage of boundary conditions. EC-03 (entry both superseded and deprecated) is a real edge case that could surface in production — good catch.

**Security risks (SEC-01 through SEC-03):** Appropriately scoped. Dead code removal reduces attack surface. Env var parsing is bounded. SQL queries use no external input.

**Integration risks (IR-01 through IR-03):** IR-03 (search pipeline ordering) is important — tests must assert on final results through `SearchService`, not intermediate state. Architecture confirms this approach.
