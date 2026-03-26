# Alignment Report: nan-009

> Reviewed: 2026-03-26
> Artifacts reviewed:
>   - product/features/nan-009/architecture/ARCHITECTURE.md
>   - product/features/nan-009/specification/SPECIFICATION.md
>   - product/features/nan-009/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/nan-009/SCOPE.md
> Scope risk source: product/features/nan-009/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly serves W1-3 (Evaluation Harness) and ASS-032 Loop 2 measurement instrument — consistent with vision's self-learning intelligence pipeline |
| Milestone Fit | PASS | Wave 1 / Nanoprobes eval harness extension; does not reach into Wave 1A or Wave 3 capabilities |
| Scope Gaps | PASS | All seven SCOPE.md goals addressed across the three source documents |
| Scope Additions | WARN | ARCHITECTURE.md Open Question 1 (SR-06 stderr warning) not present in SCOPE.md; minor and deferred to implementation agent |
| Architecture Consistency | WARN | FR-04 in SPECIFICATION.md disagrees with ARCHITECTURE.md on serde annotation for runner-side ScenarioResult; architecture position is correct but spec needs update |
| Risk Completeness | WARN | RISK-TEST-STRATEGY BLOCKER heading is stale — the conflict it describes is already resolved in both ARCHITECTURE.md and SPECIFICATION.md; needs human acknowledgement to avoid delivery confusion |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Coverage | Goal 1 — `ScenarioContext.phase` | Addressed in ARCHITECTURE Component 1, SPECIFICATION FR-01/FR-02/FR-03, AC-01/AC-02/AC-10 |
| Coverage | Goal 2 — SQL SELECT phase | Addressed in ARCHITECTURE Component 1, SPECIFICATION FR-01, AC-10 |
| Coverage | Goal 3 — `build_scenario_record` mapping | Addressed in ARCHITECTURE Component 1, SPECIFICATION FR-03 |
| Coverage | Goal 4 — `ScenarioResult.phase` passthrough | Addressed in ARCHITECTURE Component 2, SPECIFICATION FR-04/FR-05/FR-06 |
| Coverage | Goal 5 — per-phase aggregate section | SCOPE.md Goals §5 says "section 7"; SCOPE.md RD-01 and Constraint 5 establish section 6. ARCHITECTURE and SPECIFICATION both adopt section 6. See V-1. |
| Coverage | Goal 6 — phase label in section 2 (Notable Ranking Changes) | Addressed in ARCHITECTURE Component 4, SPECIFICATION FR-10, AC-08 |
| Coverage | Goal 7 — eval-harness.md documentation | Addressed in ARCHITECTURE Change 5, SPECIFICATION FR-11, AC-07 |
| Addition | ADR-001/ADR-002/ADR-003 in architecture | Three explicit ADRs resolving SCOPE.md open decisions (SR-01, SR-02/SR-03, SR-04/RD-03). Not named in SCOPE.md but each resolves a documented SCOPE risk. Acceptable. |
| Simplification | `(phase × profile)` cross-product table | SCOPE.md RD-02 explicitly defers this. All three source documents omit it consistently with documented rationale. |

---

## Variances Requiring Approval

### V-1 (WARN): Section numbering inconsistency within SCOPE.md — Goals text vs Resolved Decisions

**What**: SCOPE.md Goals §5 refers to "section 7: for each distinct phase value." SCOPE.md Constraint 5 and RD-01 establish Phase-Stratified Metrics as section 6 (Distribution Analysis shifts to 7). ARCHITECTURE.md and SPECIFICATION.md both implement section 6 consistently.

**Why it matters**: A delivery agent reading only Goals §5 (and not RD-01 or Constraint 5) would place the new section at position 7, conflicting with the golden-output test (ADR-002) and both source documents.

**Recommendation**: Accept section 6 as authoritative (per SCOPE.md RD-01 and Constraint 5). Human should confirm section 6 is the intended placement before delivery begins. No source document changes required — the inconsistency is internal to SCOPE.md and already resolved downstream.

---

### V-2 (WARN): RISK-TEST-STRATEGY BLOCKER is stale relative to SPECIFICATION.md

**What**: RISK-TEST-STRATEGY.md opens with a mandatory BLOCKER stating the `"(none)"` vs `"(unset)"` null-label conflict "must be resolved by a human before implementation begins." However, SPECIFICATION.md Constraint 5 resolves this conflict explicitly: `"The label '(unset)' is canonical; '(none)' must not be used anywhere in the implementation, tests, or documentation."` SPECIFICATION.md Open Questions section states: `"Human-confirmed decision: '(unset)' unambiguously signals field-not-populated."` ARCHITECTURE.md uses `"(unset)"` throughout via ADR-003.

**Why it matters**: A delivery agent reading RISK-TEST-STRATEGY.md first encounters a BLOCKER with mandatory human escalation. If the agent treats this as a genuine open blocker, it creates unnecessary delay. If the agent proceeds without escalation, there is a protocol violation. Either way the stale heading creates friction.

**Recommendation**: Human should confirm the SR-04 resolution (`"(unset)"` is canonical) and direct that the RISK-TEST-STRATEGY.md BLOCKER section be updated to note "RESOLVED — see SPECIFICATION.md Constraint 5" before delivery begins. The resolution itself (`"(unset)"`) is well-reasoned and should be accepted.

---

### V-3 (WARN): `skip_serializing_if` on runner-side `ScenarioResult` — spec vs architecture disagreement

**What**: SPECIFICATION.md FR-04 specifies the runner-side `ScenarioResult.phase` as `#[serde(default, skip_serializing_if = "Option::is_none")]`. ARCHITECTURE.md Component 2 explicitly says "No serde annotation needed on the writer side (always written)" — meaning the runner copy has no `skip_serializing_if`. RISK-TEST-STRATEGY.md R-05 test scenario 1 (`test_scenario_result_phase_null_serialized_as_null`) asserts the runner copy emits explicit `"phase":null` when phase is None, which requires the absence of `skip_serializing_if` on the runner side.

**Why it matters**: A delivery agent following SPECIFICATION.md FR-04 literally would suppress `"phase"` from null-phase result JSON files. R-05 test scenario 1 would then fail. AC-03 requires phase to be present on `ScenarioResult` output — suppressing null emission arguably contradicts this.

**Recommendation**: Accept the architecture position. Runner-side `ScenarioResult.phase` should be `#[serde(default)]` only — no `skip_serializing_if`. Update SPECIFICATION.md FR-04 accordingly before delivery. Human should confirm before the delivery agent proceeds.

---

## Detailed Findings

### Vision Alignment

nan-009 is the measurement instrument for ASS-032 Loop 2 (phase-conditioned retrieval). The product vision (W1-3) states: "Every intelligence change is measured against real query scenarios before reaching agents. Regressions caught before production." The vision sequences the eval harness as a gate condition for W1-4 and W3-1.

nan-009 extends the eval harness to produce phase-stratified metrics required before the `w_phase_explicit` weight path (WA-2 / W3-1) can be evaluated. It does not activate phase-conditioned retrieval — it builds only the measurement instrument. This is consistent with the vision's framing of the eval harness as a gating mechanism, not a delivery vehicle. The self-learning goal ("The function learns. Every session makes it better.") depends on measuring whether phase-conditioned improvements work; nan-009 delivers exactly that measuring capability.

All three source documents are consistent on this framing and correctly defer phase-conditioned retrieval scoring to a later feature.

### Milestone Fit

W1-3 (Eval Harness) is Wave 1 — Intelligence Foundation. nan-009 is a Nanoprobes-phase extension of that harness. It does not implement Wave 1A, Wave 2, or Wave 3 capabilities. The `w_phase_explicit = 0.0` placeholder is explicitly noted as not activated. The correct sequencing is: W1-3 / nan-007 / nan-008 (eval harness foundation) → nan-009 (phase dimension measurement) → future feature (phase-conditioned retrieval activation and evaluation). Milestone discipline is maintained across all three source documents.

### Architecture Review

The architecture is clean and well-bounded. Five components map directly to the five pipeline stages in SCOPE.md. The data flow diagram is accurate. Integration surface is fully specified with types and signatures.

The dual-type constraint (pattern #3550) is correctly acknowledged and mitigated by a mandatory round-trip integration test (ADR-002). Serde null suppression placement (ADR-001) is correctly resolved: `skip_serializing_if` on the producing side only, `#[serde(default)]` on the consuming side. The section renumbering impact table identifies all five affected sites, and pattern #3426 is enforced via ADR-002.

Open Question 1 (SR-06 warning emission) is an appropriate deferral to the implementation agent — SCOPE.md does not mandate a warning, and the decision is low-stakes.

One discrepancy: ARCHITECTURE.md Component 2 states the runner-side `ScenarioResult` carries no `skip_serializing_if`; SPECIFICATION.md FR-04 contradicts this. See V-3.

### Specification Review

FR-01 through FR-11 cover all seven SCOPE.md goals with no omissions. NFR-01 through NFR-05 cover all SCOPE.md constraints. AC-01 through AC-10 address all SCOPE.md acceptance criteria; AC-11 (round-trip integration test) and AC-12 (golden-output section-order test) are additions that directly mitigate SR-02 and SR-03 — appropriate and well-justified. The "NOT in Scope" section matches SCOPE.md non-goals precisely.

The specification's Open Questions section declares "None" and claims all decisions were resolved, including SR-04. This is accurate: SPECIFICATION.md Constraint 5 explicitly resolves SR-04 with `"(unset)"` as canonical. However the resolution of SR-04 has not been propagated back to RISK-TEST-STRATEGY.md, creating the stale BLOCKER condition (V-2).

FR-04 contains the serde annotation inconsistency with the architecture (V-3).

### Risk Strategy Review

The risk register is thorough: 12 risks, all with severity/likelihood/priority ratings, and specific named test scenarios for the three critical risks (R-01, R-02, R-04). Integration risks (IR-01 through IR-04), edge cases (EC-01 through EC-06), security risks (SEC-01 through SEC-03), and failure modes (FM-01 through FM-05) are all documented. The 18-scenario minimum test coverage estimate is consistent with the risk register.

The security analysis is calibrated appropriately. SEC-02 (SQL injection) is correctly confirmed as non-applicable. SEC-01 (Markdown injection via phase string) is correctly assessed as low-blast-radius and documentation-only.

The Scope Risk Traceability table correctly marks SR-04 as "UNRESOLVED — BLOCKER" — but this reflects the state at the time the risk strategy was written, before SPECIFICATION.md resolved it. The document has not been updated to reflect the resolution. See V-2.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns — found entries #2298 (config key semantic divergence), #3426 (formatter section-order regression guard), #3156 (affinity boost architecture decision). None are specific to vision-alignment review methodology. No prior guardian patterns apply.
- Stored: nothing novel to store — the stale-BLOCKER pattern (a risk document not updated after downstream resolution) is feature-specific; the spec/architecture serde annotation disagreement is an authoring artifact. Neither generalizes across features at this time.
