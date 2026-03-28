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
| Vision Alignment | PASS | Directly activates a named vision placeholder (w_phase_explicit) in the Wave 1A intelligence pipeline |
| Milestone Fit | PASS | Correctly positioned in Wave 1A; non-parametric predecessor to W3-1 as documented in ASS-032 |
| Scope Gaps | WARN | One scope clarification diverges between SCOPE.md and ARCHITECTURE.md on AC-16 |
| Scope Additions | PASS | No items appear in source docs that are absent from SCOPE.md |
| Architecture Consistency | PASS | Architecture resolves all SCOPE-RISK-ASSESSMENT items; component model matches SCOPE.md exactly |
| Risk Completeness | PASS | 14 risks registered; all 7 scope risks traced; Critical/High risks have specific test scenarios |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | AC-16 scope (extract.rs vs. replay.rs) | SCOPE.md states "modify `extract.rs` to select `query_log.phase` and populate `current_phase`." ARCHITECTURE.md and SPECIFICATION.md both clarify that `extract.rs` and `output.rs` already select and propagate `phase` — the actual fix is a one-line change in `replay.rs` only. The scope description is technically incorrect; the source docs correct it with evidence. The fix is narrower than scoped. See VARIANCE-1 below. |
| Simplification | `validate()` range check for `query_log_lookback_days` | SCOPE.md is silent on `validate()` changes. ARCHITECTURE.md adds `[1, 3650]` range check to `validate()`. RISK-TEST-STRATEGY R-08 identifies absence of this validation as a Medium risk. This is a small positive addition over SCOPE.md scope that closes a named risk. Acceptable and beneficial. |

---

## Variances Requiring Approval

### VARIANCE-1 (WARN): AC-16 scope description is technically incorrect in SCOPE.md

**What**: SCOPE.md (AC-16, §Acceptance Criteria) states: "eval/scenarios/extract.rs selects query_log.phase and populates current_phase in emitted scenario output. Bounded change — nothing else in extract.rs is modified." ARCHITECTURE.md §7 states: "extract.rs and output.rs already select and propagate phase. The gap is entirely in replay.rs." SPECIFICATION.md FR-11 similarly states the change is in extract.rs. However, ARCHITECTURE.md's Open Questions resolution at the end clarifies the AC-16 scope: "extract.rs and output.rs already select and populate phase. The gap is in replay.rs — current_phase is not forwarded to ServiceSearchParams."

The SPECIFICATION FR-11 text still says "extract.rs must be modified" — which contradicts the ARCHITECTURE's finding. This creates a delivery ambiguity: a developer reading SPECIFICATION FR-11 will modify extract.rs, while a developer reading ARCHITECTURE §Open Questions will modify replay.rs. Both cannot be correct.

**Why it matters**: Delivery will implement one of two different files. If the wrong file is modified (extract.rs instead of replay.rs), AC-16 will appear to pass code review but AC-12 remains vacuous — the scenario output will not carry non-null current_phase values at replay time. This is exactly the R-02 risk (Critical, High likelihood) in RISK-TEST-STRATEGY.

**Recommendation**: Before delivery begins, the specification FR-11 text must be corrected to name relay.rs as the target file, not extract.rs. The delivery team cannot proceed with an ambiguous AC-16 scope without risking a vacuous AC-12 gate. This is a clarification, not a scope change — the ARCHITECTURE has already resolved the question correctly.

---

## Detailed Findings

### Vision Alignment

**Finding: PASS**

col-031 directly serves the Wave 1A vision goal: "The intelligence pipeline knows where each session is in its workflow" and "`context_search` re-ranking is session-conditioned — identical queries return different rankings based on session context."

Evidence from PRODUCT-VISION.md:
- WA-0 shipped a six-term fused formula with `w_phase_explicit = 0.0` and "0.05 headroom for WA-2."
- The vision explicitly states: "Intelligence pipeline is additive boosts, not a learned function — Roadmapped — Wave 1A + W3-1."
- ASS-032 RESEARCH-SYNTHESIS.md (Loop 2, §147) directly mandates a "phase-conditioned frequency table" as the non-parametric RA-DIT feedback loop, identifies `w_phase_explicit` as "entirely absent from scoring today," and labels it "Add immediately."

col-031 activates a named, reserved placeholder established by crt-026 (ADR-003, Unimatrix #3163), confirmed by ASS-032 as the correct non-parametric predecessor to W3-1. The feature introduces no capability outside the Wave 1A mandate.

The vision's "In-memory hot path" non-negotiable (§"What's Preserved Throughout") is fully respected: `PhaseFreqTableHandle` follows the `Arc<RwLock<_>>` rebuilt-by-tick pattern, never read from the database at query time.

### Milestone Fit

**Finding: PASS**

col-031 is correctly positioned in Wave 1A. The dependency chain is satisfied:
- WA-0 (ranking signal fusion, crt-024) — COMPLETE. The fused formula with `w_phase_explicit = 0.0` is the foundation col-031 activates.
- WA-1 (phase signal, crt-025) — COMPLETE. `current_phase` session state and `query_log.phase` (col-028, schema v17) are the prerequisites col-031 consumes.
- col-028 (schema v17, `query_log.phase`) — COMPLETE (gate-3c PASS 2026-03-26), confirmed in SCOPE.md §Constraints.

col-031 does not attempt W3-1 (GNN) work. It explicitly defers Thompson Sampling, GNN, BM25, and PPR to named future features (#398, #409, ASS-029). The `phase_affinity_score` API is published as the integration contract for #398 without implementing PPR internals — this is milestone discipline correctly exercised.

The vision states W3-1 requires "ASS-029 design spike + Wave 1A signal infrastructure + usage data." col-031 builds the signal infrastructure (the frequency table) without overreaching into W3-1 territory.

### Architecture Review

**Finding: PASS**

The architecture resolves all seven scope risks from SCOPE-RISK-ASSESSMENT.md:

- SR-01 (hidden run_single_tick bypass): Addressed by making `PhaseFreqTableHandle` a required non-optional constructor parameter. Missing wiring is a compile error (ADR-005). Architecture names all affected sites explicitly.
- SR-02 (FusionWeights sum to 1.02): Addressed by updating the FusionWeights sum-check comment. `validate()` unchanged per ADR-004 additive exemption. Architecture confirms the exemption.
- SR-03 (AC-12/AC-16 non-separability): Addressed — the two must ship in the same delivery wave. Gate 3b must reject AC-12 PASS without AC-16 evidence.
- SR-04 (phase rename staleness): Accepted. Cold-start fallback is the only recovery. Documented in CON-09.
- SR-05 (lookback_days session-frequency-dependent): ADR-002 adds `[1, 3650]` range validation and TOML override. #409 owns cycle-aligned GC as the long-term fix.
- SR-06 (two cold-start values from one method): ADR-003 and the `phase_affinity_score` doc comment requirement (AC-17) name both callers explicitly.
- SR-07 (lock acquisition order): Architecture requires a code comment at the tick's lock sequence site naming the required order.

The component model is internally consistent. The SQL is verified against `migration.rs` and `knowledge_reuse.rs` patterns, the `json_each` cast form is correct, and the normalization formula is the correct 1-indexed rank form.

One note: ARCHITECTURE.md §6 (InferenceConfig) adds `[1, 3650]` range validation to `validate()` — a positive addition over SCOPE.md which is silent on this. RISK-TEST-STRATEGY R-08 identifies this as a Medium risk, and the architecture closes it. This is an acceptable, beneficial scope addition.

### Specification Review

**Finding: PASS with one WARN (VARIANCE-1)**

The specification is thorough: 11 functional requirements, 8 non-functional requirements, 17 acceptance criteria, 5 user workflows, 10 constraints, 4 open questions. All acceptance criteria from SCOPE.md are present and covered.

The AC-12/AC-16 non-separability constraint is elevated to a hard gate in NFR-05: "AC-12 must not be declared PASS in any wave unless AC-16 is present in the same or preceding wave AND the eval scenario output is verified to contain non-null current_phase values." This is stronger enforcement than SCOPE.md and is appropriate.

**WARN (VARIANCE-1)**: SPECIFICATION FR-11 names `extract.rs` as the file to modify for AC-16. ARCHITECTURE §Open Questions resolution names `replay.rs`. These contradict each other. This ambiguity must be resolved before delivery begins. See VARIANCE-1 above.

The specification's "NOT in Scope" section faithfully reproduces all SCOPE.md non-goals. No scope creep detected in the specification text.

### Risk Strategy Review

**Finding: PASS**

The RISK-TEST-STRATEGY registers 14 risks across Critical/High/Medium/Low priority tiers. All 7 SCOPE-RISK-ASSESSMENT risks are traced in the "Scope Risk Traceability" table with explicit architecture risk mappings and resolutions.

Two Critical risks (R-01, R-02) are correctly elevated:
- R-01 (silent wiring bypass) references pattern #3213 and lesson #3216, indicating prior-art grounding in Unimatrix knowledge.
- R-02 (vacuous AC-12 gate) references ADR-004 and SR-03, with a process gate requirement on Gate 3b.

R-14 (test helper sites not updated for new constructor parameter) is a High/High risk not present in SCOPE-RISK-ASSESSMENT.md. It is a legitimate implementation risk — the RISK-TEST-STRATEGY has correctly expanded the register based on deeper analysis.

Security assessment is proportionate for an internal-only component. The analysis correctly identifies no untrusted external strings reaching SQL parameters and limits blast radius to search ranking manipulation.

The strategy references specific Unimatrix knowledge entries (#1560, #3213, #2961, #3678, #3681, #3688) as evidence for risk ratings. This is traceable and correct practice.

One minor gap: R-08 (`query_log_lookback_days` range not validated) exists in RISK-TEST-STRATEGY, and ARCHITECTURE.md adds the `[1, 3650]` range check to `validate()`. However, SPECIFICATION.md FR-10 does not mention `validate()` changes — it only specifies the new field and default. If delivery implements FR-10 without also implementing the range check from the architecture, R-08 is not mitigated. This is a specification gap, but it is flagged in R-08 and the test scenarios will catch it. Classified as WARN, not FAIL.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns — found entry #2298 (config key semantic divergence between vision example and implementation, dsn-001). Pattern is not directly applicable to col-031 (no semantic divergence found). No other vision alignment patterns found in the pattern category.
- Stored: nothing novel to store — the SPECIFICATION FR-11 vs ARCHITECTURE replay.rs discrepancy is feature-specific and does not generalize beyond col-031. The AC-12/AC-16 non-separability pattern is already captured in Unimatrix (#3683, #3688) by the prior agents. No duplicate storage warranted.
