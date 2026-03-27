# Alignment Report: col-031

> Reviewed: 2026-03-27
> Artifacts reviewed:
>   - product/features/col-031/architecture/ARCHITECTURE.md
>   - product/features/col-031/specification/SPECIFICATION.md
>   - product/features/col-031/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/col-031/SCOPE.md
> Scope risk source: product/features/col-031/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly activates Wave 1A intelligence pipeline per roadmap intent |
| Milestone Fit | PASS | Correctly targets Wave 1A; W3-1 and GNN are explicitly deferred |
| Scope Gaps | PASS | All SCOPE.md goals and constraints are addressed in source docs |
| Scope Additions | WARN | Architecture adds lock ordering detail (ADR-004) and timing observability (SR-07) not in SCOPE.md — both are architectural elaborations, not functional additions |
| Architecture Consistency | PASS | Exact TypedGraphState pattern followed; crate boundaries respected; lock ordering documented |
| Risk Completeness | PASS | 14 risks catalogued; all SCOPE-RISK-ASSESSMENT.md risks traced; Critical and High risks have multi-scenario coverage |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | PPR wire-up (Goals 5, AC-07, AC-08) | SCOPE.md states wire-up if #398 not shipped; source docs replicate this conditional correctly. AC-07 and AC-08 are published API contract ACs, not deferred. Rationale: the `phase_affinity_score` public method is the integration surface; the PPR call site is a documented integration point only. Acceptable. |
| Simplification | Open Question 3 (join shape) | SCOPE.md flags "confirm this join is within unimatrix-store query layer." Architecture confirms the join in `unimatrix-store/src/query_log.rs` and documents crate boundary reasoning (ADR-002). Resolved. |
| Simplification | Open Question 4 (tick placement) | SCOPE.md raises ordering question. Architecture documents the convention explicitly: structural state (TypedGraphState) before analytical state (PhaseFreqTable). Resolved. |
| Addition | Three-handle lock ordering protocol (ADR-004) | Architecture formalizes acquisition order for `EffectivenessStateHandle` → `TypedGraphStateHandle` → `PhaseFreqTableHandle`. SCOPE.md documents the sole-writer contract and raises the concern (SR-06) but does not specify a formal acquisition order. Architecture's formalization is a necessary elaboration, not a scope addition — it directly addresses SR-06. |
| Addition | `tracing::debug!` timing instrumentation for rebuild | ARCHITECTURE.md Open Question 2 and RISK-TEST-STRATEGY.md R-14 require a timing log for the SQL rebuild. SCOPE.md §Background Research mentions `< 5ms` as an estimate but does not require instrumentation. This is an observability addition consistent with NFR-02 in the spec. |

---

## Variances Requiring Approval

No VARIANCE or FAIL classifications were found. Both items in the Addition row above are minor scope elaborations (not additions of new user-visible capability) that resolve risks explicitly called out in SCOPE-RISK-ASSESSMENT.md. They do not require explicit approval but are noted for human awareness.

---

## Detailed Findings

### Vision Alignment

**Finding: PASS**

The product vision declares (Wave 1A introduction): "The intelligence pipeline cannot learn from usage it cannot observe, cannot predict what agents need without knowing where they are in the cycle, and cannot close the feedback loop." col-031 directly operationalizes this by activating `w_phase_explicit` — the phase-conditioned term in the fused scoring formula that has been a placeholder (hardcoded `0.0`) since crt-026.

The vision states (WA-1 completion notes): "current_phase is the explicit phase dimension of the session context vector." col-031 makes that dimension a live scoring signal, not just metadata storage, by building `PhaseFreqTable` from actual `query_log` history. This is the non-parametric predecessor to W3-1 (GNN) that the vision roadmap designates as the right sequencing before `CC@k ≥ 0.7`.

The vision's core intelligence pipeline goal ("self-improving relevance function: given what the agent knows, what they have been doing, and where they are in their workflow, surface the right knowledge") is directly served. The feature does not introduce any domain-coupling regression: phase vocabulary remains runtime strings, no SDLC-specific logic is hardcoded.

The vision explicitly states (Critical Gaps — Intelligence & Confidence): "No session-conditioned relevance — every query treated identically — High — Roadmapped (Wave 1A + W3-1)." col-031 resolves this gap for the phase-explicit dimension.

### Milestone Fit

**Finding: PASS**

col-031 belongs in Wave 1A. The roadmap sequences: WA-0 (fusion, COMPLETE) → WA-1 (phase signal, COMPLETE) → WA-2 (session context enrichment, COMPLETE) → WA-4 (proactive delivery, COMPLETE). col-031 activates the `w_phase_explicit` placeholder that WA-1 reserved. This is the correct sequencing: data collection (WA-1 + col-028) must precede the aggregation that produces the frequency table.

The feature correctly defers:
- W3-1 GNN: explicitly out of scope. The frequency table is the "non-parametric predecessor" per roadmap language.
- Thompson Sampling: explicitly deferred per roadmap ("after PPR baseline ICD measured").
- Gap detection: deferred to a separate feature (#409 / Loop 3).
- PPR implementation: deferred to #398.

No Wave 2 or Wave 3 capabilities are implemented. No schema migration occurs (consistent with col-028 being the prerequisite data supplier). The feature does not pull forward any future milestone work.

### Architecture Review

**Finding: PASS**

**Pattern adherence**: The `PhaseFreqTable` / `PhaseFreqTableHandle` implementation exactly follows the `TypedGraphState` pattern identified in SCOPE.md §Background Research. The architecture document explicitly maps every element: `new()`, `new_handle()`, `rebuild()`, tick swap, poison recovery, cold-start behavior. This is the strongest signal of disciplined pattern reuse.

**Crate boundary**: The SQL aggregation is placed in `unimatrix-store/src/query_log.rs` (ADR-002). The service module contains only the in-memory struct and scoring logic. This follows the established codebase separation between storage access and service computation. `json_each` is confirmed available from existing usage in `mcp/knowledge_reuse.rs`.

**Lock ordering**: The three-handle acquisition sequence is formally documented and enforced structurally: each lock is acquired, used, and released before the next is acquired. Background tick swap blocks are separate non-nested scopes. The documented sequence (`EffectivenessStateHandle` → `TypedGraphStateHandle` → `PhaseFreqTableHandle`) is consistent with the existing tick ordering (structural state before analytical state).

**Cold-start safety**: The architecture correctly characterizes cold-start behavior. One nuance is accurately noted: when `current_phase = Some(...)` and `use_fallback = true`, `phase_affinity_score` returns `1.0` for all candidates, producing a uniform `+0.05` offset (not bit-for-bit score identity). Architecture Open Question (SR-07 / timing instrumentation) remains open but is flagged for delivery.

**Eval harness fix**: The architecture correctly treats `eval/scenarios/extract.rs` (AC-16) as a non-separable deliverable from the scoring activation. Section 7 of the component breakdown documents this with the note "SR-03 constraint documented in SPECIFICATION." This is correct — shipping scoring without the eval fix would make AC-12 a vacuous gate.

**Minor observation (non-blocking)**: Architecture lists `PhaseFreqRow` returning `Vec<(String, String, u64, i64)>` in FR-06 of SPECIFICATION.md while ARCHITECTURE.md Integration Point table shows `freq: u64`. The SPECIFICATION.md FR-06 uses `i64` (likely reflecting sqlx i64 deserialization from SQLite INTEGER). This is a type annotation inconsistency that the implementer must resolve, but it does not represent a design disagreement. Flagged for awareness.

### Specification Review

**Finding: PASS**

**Scope coverage**: All 7 SCOPE.md Goals are addressed:
- Goal 1 (`PhaseFreqTable` struct): FR-01, FR-02, AC-01
- Goal 2 (`PhaseFreqTableHandle` pattern): FR-03, AC-03
- Goal 3 (ServiceLayer wiring): FR-08, AC-05
- Goal 4 (scoring activation): FR-10, FR-11, FR-12, AC-06, AC-09, AC-11
- Goal 5 (PPR affinity API): FR-07, AC-07
- Goal 6 (`query_log_retention_cycles` config): FR-11, AC-10
- Goal 7 (cold-start degradation): NFR-03, AC-11

**Non-Goals enforced**: All 8 SCOPE.md Non-Goals are preserved. No schema migration (NFR-07), no Thompson Sampling, no gap detection, no PPR implementation (only API surface), no W3-1 GNN, no BM25, no `query_log` GC, no UI or diagnostic endpoint. The specification adds no capability not requested in SCOPE.md.

**Cold-start invariant nuance**: NFR-03 correctly documents the distinction between two scenarios: (a) `current_phase = None` → `phase_explicit_norm = 0.0` → bit-for-bit identical to pre-col-031; (b) `current_phase = Some(...)` + cold-start → uniform `+0.05` offset → ranking-preserving but not score-identical. The specification accurately states this and AC-11 is scoped to the `None` path only. This is correct and self-consistent.

**AC-12 / AC-16 linkage**: NFR-05 explicitly states "AC-12 must not be declared passing if `current_phase` is absent from eval scenarios." AC-12 verification text repeats the gate dependency. This is the correct treatment of SR-03.

**`w_phase_explicit = 0.05` calibration**: SCOPE-RISK-ASSESSMENT.md SR-02 flags that 0.05 is judgment-calibrated, not empirically derived from ASS-032. The architecture document (ADR-005, Open Question 4) accepts this risk explicitly: "ASS-032 research provides directional calibration but no numerically derived value for `0.05` specifically. The risk is accepted and documented: configurable via InferenceConfig; cold-start degrades to near-zero net effect; AC-12 is the safety gate." This acceptance is correctly documented. The risk is real but mitigated by configurability and the eval regression gate.

**One structural note (non-blocking)**: SPECIFICATION.md FR-06 specifies the return type of `Store::query_phase_freq_table` as `Vec<(String, String, u64, i64)>` (note `i64` for freq_count). SCOPE.md shows `COUNT(*)` which in SQLite via sqlx would return `i64`. The RISK-TEST-STRATEGY.md R-13 scenario 1 flags exactly this: "`freq: u64`" may need to be `i64` for sqlx compatibility. This is an implementation-time resolution item, not a specification design flaw.

### Risk Strategy Review

**Finding: PASS**

**Coverage breadth**: 14 risks are identified across the complete risk surface: data layer (R-01 json_each, R-06 retention boundary), concurrency (R-03 lock ordering), correctness (R-02 cold-start semantics, R-07 linear scan, R-11 single-entry bucket), configuration (R-08 default change, R-09 comment staleness), eval process (R-04 vacuous gate, R-05 partial ship), observability (R-10 phase mismatch, R-12 stale state persistence), API surface (R-13), and performance (R-14).

**Critical risk coverage (R-01, R-02, R-04, R-05)**: Each critical risk has 2-3 test scenarios. Notably:
- R-01: Integration test against real SQLite `TestDb` (not mock) — correctly identified as the only meaningful test surface for `json_each` behavior.
- R-02: Two AC-11 sub-cases distinguished (None path vs. Some + cold-start) — the specification NFR-03 aligns with this distinction.
- R-04 / R-05: Gate procedure constraint added (eval pre-check for `current_phase != null`) — a procedural control in addition to code-level testing.

**SCOPE-RISK-ASSESSMENT.md traceability**: All 7 SCOPE-RISK-ASSESSMENT.md risks (SR-01 through SR-07) are explicitly traced in the RISK-TEST-STRATEGY.md §Scope Risk Traceability table. Each maps to a RISK-TEST-STRATEGY.md risk ID and a resolution. This is complete.

**Risks with code-review-only coverage (R-09, R-13)**: Both are appropriately classified. R-09 is a comment update (no logic); R-13 is a compile-time contract. Designating these as code-review-only is correct.

**Knowledge stewardship in RISK-TEST-STRATEGY.md**: The risk document reports querying Unimatrix entries #3678 (json_each), #3682 (ADR-004 lock ordering), #3579/#3580 (gate failures), and #3555 (eval harness phase gap) to ground its severity ratings. This is good epistemic practice.

**One gap (non-blocking, WARN)**: R-02 scenario 3 asks for a "Code-comment verification" that the `phase_freq_table.rs` module documents the cold-start/ranking-preservation distinction. This is a documentation-level control. No automated test enforces it. Given that the distinction is subtle (uniform `+0.05` vs. true bit-for-bit identity), this code comment is important for future maintainers. The specification (NFR-03) and architecture both discuss it, but no AC explicitly requires the comment to be present in the module. This is noted as a WARN for human awareness — the implementer should be asked to include this comment.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for topic `vision` with queries for "vision alignment scope addition milestone discipline" and "scope gap addition variance alignment review" — no prior vision alignment patterns found in Unimatrix. Results returned were feature-specific patterns (config semantic divergence, signal fusion, affinity boost architecture) not generalizable to vision alignment review.
- Stored: nothing novel to store — col-031 variances are feature-specific (type annotation inconsistency, observability addition, calibration acceptance). The only potentially generalizable pattern from this review is "architects consistently add lock ordering formalization when SCOPE.md raises the concern but does not specify the protocol" — however this is already expected behavior (design agent's job) and does not constitute a recurring misalignment pattern requiring storage.
