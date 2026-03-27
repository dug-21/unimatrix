# Alignment Report: nan-010

> Reviewed: 2026-03-26
> Artifacts reviewed:
>   - product/features/nan-010/architecture/ARCHITECTURE.md
>   - product/features/nan-010/specification/SPECIFICATION.md
>   - product/features/nan-010/RISK-TEST-STRATEGY.md
> Scope source: product/features/nan-010/SCOPE.md
> Scope risk assessed: product/features/nan-010/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Eval harness improvement directly serves W1-3 strategic role as gate for intelligence pipeline features |
| Milestone Fit | PASS | Correctly targets W1-3 eval harness; no future-milestone capabilities added |
| Scope Gaps | WARN | One scope item (baseline profile `distribution_change = true` error behaviour) left open in spec as OQ-01 |
| Scope Additions | PASS | No out-of-scope material detected in source documents |
| Architecture Consistency | WARN | Architecture OQ-01 and OQ-02 are unresolved open questions that must be closed before implementation; architecture render output example uses wrong heading level (`### 5.`) in a single-profile context |
| Risk Completeness | WARN | R-07 in RISK-TEST-STRATEGY.md contains a factual mischaracterisation of ARCHITECTURE.md Component 7, creating a phantom conflict; this needs correction to avoid implementation confusion |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | Baseline profile `distribution_change = true` error handling | SCOPE.md Design Decision #7 requires a hard `ConfigInvariant` error with message "baseline profile must not declare `distribution_change = true`". SPECIFICATION.md constraint 9 documents the requirement but explicitly defers the exact behaviour (error vs. ignore) to the architect as OQ-01. The spec leaves this open; it has not been resolved in any document. Delivery cannot implement correct behaviour without a closed decision. |
| Gap | `render_distribution_gate_section` baseline MRR reference row | SCOPE.md Design Decision #5 mandates a "Baseline MRR (reference)" informational row in the Distribution Gate table. The architecture raises this as OQ-01 (unresolved). The spec's FR-12 and AC-08 do not include the baseline reference row in the required table columns. If the architecture OQ-01 is resolved to include the row, FR-12 and AC-08 will need to be updated before implementation. |
| Simplification | `check_distribution_targets` visibility (`pub(super)`) | Architecture uses `pub(super)` for `DistributionGateResult` and `MetricGateRow`. Spec uses no visibility qualifier in the domain model descriptions. Acceptable — architecture is the authority on visibility. |

---

## Variances Requiring Approval

### 1. WARN — Baseline Profile Error Behaviour Left Open (OQ-01 in both ARCHITECTURE.md and SPECIFICATION.md)

**What**: SCOPE.md Design Decision #7 states that a baseline profile declaring `distribution_change = true` must produce a `ConfigInvariant` error. Both ARCHITECTURE.md (OQ-01) and SPECIFICATION.md (OQ-01 + constraint 9) leave this unresolved, explicitly asking the architect to close it before implementation. The RISK-TEST-STRATEGY includes a specific test (`test_distribution_gate_baseline_rejected`) and failure mode entry asserting `ConfigInvariant` with the exact message "baseline profile must not declare `distribution_change = true`" — which assumes the decision is already closed in favour of the hard error, despite neither source document closing it.

**Why it matters**: This is not a minor ambiguity. The test strategy has pre-declared a non-negotiable test name (`test_distribution_gate_baseline_rejected`) in the gate-3b checklist. If implementation follows the scope (hard error) but the spec never formalised it, the spec is incomplete. If implementation follows only the spec's hedged language ("may be treated as a `ConfigInvariant` error"), the test name will still pass but the assertion message content may be wrong. Either way, a delivery agent working from the spec alone cannot implement this correctly.

**Recommendation**: Close OQ-01 explicitly before delivery begins. Update SPECIFICATION.md constraint 9 to remove the hedging language and specify exactly: `parse_profile_toml` must return `EvalError::ConfigInvariant` with the message text from SCOPE.md Design Decision #7 when the baseline profile sets `distribution_change = true`. The architecture's OQ-01 should similarly be closed with the same decision.

---

### 2. WARN — Baseline MRR Reference Row: SCOPE Requirement vs. Specification Silence (OQ-01 in ARCHITECTURE.md)

**What**: SCOPE.md Design Decision #5 states: "Distribution Gate table includes a 'Baseline MRR (reference)' row. Without it, `mrr_floor` values are set blind." This is an explicit design decision in the authoritative scope. ARCHITECTURE.md Component 5 acknowledges this via OQ-01: "Should the architecture mandate that the rendered Distribution Gate table always includes a 'Baseline MRR (reference)' row... Decision needed before implementation starts." SPECIFICATION.md FR-12 and AC-08 list the Distribution Gate table columns without this row; they do not mention it at all.

**Why it matters**: SCOPE.md is the authoritative source for what the user wants. A design decision in SCOPE.md is a requirement, not a suggestion. The architecture correctly identified that this needs resolution but deferred it. The specification was written without resolving it, producing a specification that is silent on a scoped requirement. If implementation follows the spec literally (FR-12, AC-08), the baseline MRR reference row will be absent and the scope requirement will not be met.

**Recommendation**: Close ARCHITECTURE.md OQ-01 before delivery begins. If the decision is to include the baseline MRR row (which SCOPE.md Design Decision #5 clearly implies it should be), update SPECIFICATION.md FR-12 and AC-08 to add the row to the required table structure, and update `render_distribution_gate_section` signature in the architecture to accept baseline `AggregateStats` as a second parameter.

---

### 3. WARN — R-07 in RISK-TEST-STRATEGY Contains Factual Mischaracterisation of ARCHITECTURE.md

**What**: RISK-TEST-STRATEGY.md R-07 states: "the ARCHITECTURE.md Component 7... describes a WARN+fallback path. This inconsistency is a direct implementation risk." However, ARCHITECTURE.md Component 7 actually specifies: "If present but malformed JSON → return `EvalError` with message 'profile-meta.json is malformed — re-run eval to regenerate', abort the report, and exit non-zero." This is abort behaviour, not WARN+fallback. The architecture and SCOPE.md Design Decision #8 are consistent. The risk document invented a conflict that does not exist in the current documents.

**Why it matters**: R-07 is classified as High severity and listed as a High priority risk. Delivery agents or reviewers acting on this risk entry will spend time searching for a conflict that does not exist, and may introduce a WARN+fallback code path under the mistaken belief that the architecture requires it, contradicting both the actual architecture and the scope.

**Recommendation**: Correct R-07 in RISK-TEST-STRATEGY.md to remove the false characterisation. The risk should either be demoted to a straightforward implementation check ("ensure abort is implemented, not fallback") or removed entirely. The phantom conflict text must not mislead the delivery team.

---

### 4. WARN — Architecture Render Example Uses Wrong Section Heading Level (Component 5)

**What**: ARCHITECTURE.md Component 5's rendered output example shows `### 5. Distribution Gate — {profile_name}` for a single-profile Section 5 render. However, SCOPE.md Design Decision #6 and ARCHITECTURE.md Component 6 both specify that `## 5.` (H2) is used for single-candidate runs, and `### 5.N` (H3) is used for multi-candidate runs. The example in Component 5 uses H3 unconditionally, which contradicts the heading level rule stated in Component 6 of the same document.

**Why it matters**: The render example in Component 5 is what delivery agents will reference when implementing `render_distribution_gate_section`. If they implement to the example, single-profile reports will use `### 5.` instead of `## 5.`, breaking CI tooling anchors (R-09 in the risk strategy). RISK-TEST-STRATEGY R-09 tests for this exact scenario but the implementation risk is heightened by the misleading example.

**Recommendation**: Update ARCHITECTURE.md Component 5 to correct the heading level in the example. For single-profile (default): `## 5. Distribution Gate`. For multi-profile: `### 5.1 Distribution Gate — {profile_name}`. Alternatively, add a note to the example clarifying it shows the multi-profile format.

---

## Detailed Findings

### Vision Alignment

nan-010 is an incremental improvement to the W1-3 evaluation harness. The product vision (W1-3) states the eval harness purpose as: "Every intelligence change is measured against real query scenarios before reaching agents. Regressions caught before production." The vision also lists the eval harness as the gate condition for W2-4 and W3-1.

Features such as PPR (phase-conditioned retrieval), contradicts suppression, and NLI re-ranking all intentionally shift result distributions. Without nan-010, the zero-regression check produces false positives for every one of these intelligence pipeline features — making the W1-3 gate actively misleading for the class of features it is gating. nan-010 directly restores the gate's validity for distribution-changing features.

The feature does not introduce domain coupling, does not touch the intelligence pipeline itself, and does not reach into future wave capabilities. It is narrowly scoped to the eval reporting and profile parsing path.

**Verdict: PASS.** The feature is necessary infrastructure for the eval harness to correctly serve its W1-3 role.

---

### Milestone Fit

The product vision places the eval harness at W1-3 (Wave 1 — Intelligence Foundation). nan-010 is a Nanoprobes-phase feature (build/CI tooling) extending the W1-3 harness. It does not build Wave 2 or Wave 3 capabilities, does not add ML inference, and does not touch deployment infrastructure. All new code is in `eval/` within `unimatrix-server` — correctly scoped to the harness.

The feature's gate condition fit is direct: PPR (#398), phase-conditioned retrieval, and contradicts suppression (#395) are all Wave 1A/Wave 1 intelligence pipeline features that the harness must correctly gate.

**Verdict: PASS.** Milestone fit is appropriate.

---

### Architecture Review

The architecture is well-structured and demonstrates strong alignment with the scope. Specific observations:

**Strengths:**
- Seven-component breakdown maps cleanly to the six source files changed.
- Sidecar file approach (ADR-002, Component 3) correctly avoids the dual-type constraint documented at #3574 and #3550. Zero changes to `ScenarioResult` is explicitly stated and tracked.
- Atomic rename for sidecar write (ADR-004, Component 3) correctly mitigates SR-01.
- Module pre-split plan (ADR-001) for both `aggregate.rs` and `render.rs` addresses the 500-line hard constraint (SR-02, SR-03) before any feature code.
- Implementation order constraints (section "Implementation Order Constraints") are explicit and correct.
- Component interactions diagram accurately reflects the data flow.

**Issues:**

1. **OQ-01 unresolved** (see Variance #2): Baseline MRR reference row (SCOPE.md Design Decision #5) is deferred without closure. Renders FR-12 and AC-08 potentially incomplete.

2. **OQ-02 unresolved**: Corrupt `profile-meta.json` behaviour is documented as NFR-02 ("must surface as an error at `eval report` time") but OQ-02 in the spec asks the architect to specify the exact error type, message, and exit behaviour. ARCHITECTURE.md Component 7 actually does specify this ("return `EvalError` with message '...', abort the report, and exit non-zero") — so OQ-02 in the specification appears to be resolved by the architecture but not closed in the spec itself. Spec OQ-02 should be marked resolved.

3. **Component 5 heading level example inconsistency** (see Variance #4).

---

### Specification Review

The specification is thorough, with 16 functional requirements, 6 non-functional requirements, and 14 acceptance criteria. Scope-to-spec traceability is strong — every SCOPE.md goal and non-goal is reflected.

**Strengths:**
- FR-01 through FR-16 map directly to SCOPE.md goals 1–8 and constraints 1–7.
- Non-goals in SCOPE.md are cleanly carried into spec's "NOT in Scope" section.
- Ubiquitous language section is well-defined and consistent.
- NFR-02 (atomic write) and NFR-03 (no ScenarioResult changes) are explicit constraints that directly mitigate the highest integration risks.
- Knowledge stewardship block confirms prior patterns were consulted (#3582, #3574, #3550, #3563, #3583).

**Issues:**

1. **Constraint 9 hedging** (see Variance #1): "may be treated as a `ConfigInvariant` error" is not a specification — it is a deferral. SCOPE.md Design Decision #7 is unambiguous. The spec must close this.

2. **FR-12 / AC-08 missing baseline MRR reference row** (see Variance #2): If the architecture OQ-01 resolution includes the reference row (as SCOPE.md Design Decision #5 requires), FR-12 and AC-08 need updating.

3. **OQ-02 is resolved in ARCHITECTURE.md but not closed in SPECIFICATION.md**: The architecture specifies abort + non-zero exit for corrupt sidecar. The spec should mark OQ-02 as resolved with the architecture's answer rather than leaving it open.

4. **FR-07 schema has `"version": 1` per-entry** but ARCHITECTURE.md Component 3 `ProfileMetaFile` has `version: u32` as a top-level field, not a per-entry field. The spec's JSON example places `"version": 1` inside each profile entry object; the architecture places it in the top-level `ProfileMetaFile` struct. This is a minor schema-level inconsistency that will cause the implementation to choose one shape, and the round-trip test (R-10) to validate against the chosen shape. Both documents need to agree.

---

### Risk Strategy Review

The risk strategy is the most thorough of the three source documents. The risk register is comprehensive, the risk-to-scenario mapping provides specific test scenarios, and the non-negotiable test name list is complete and grounded in specific acceptance criteria.

**Strengths:**
- 15 risks identified and prioritised; coverage spans implementation order, parse logic, atomic writes, rendering, exit codes, dual-type constraint, and schema drift.
- Non-negotiable test name pre-declaration (20 test function names) directly addresses the gate-3b failure mode from nan-009 (#3579).
- Scope risk traceability table explicitly maps all 7 SCOPE-RISK-ASSESSMENT risks to risk register entries and resolution decisions.
- Edge case table covers boundary conditions including `mrr_floor > 1.0`, single-scenario results, and empty results directory.
- Security risk analysis is appropriately proportional: TOML and sidecar surfaces are low-risk and correctly characterised.

**Issues:**

1. **R-07 factual mischaracterisation** (see Variance #3): The stated conflict between ARCHITECTURE.md Component 7 and SCOPE.md Design Decision #8 does not exist in the current documents. This is the highest-impact error in the risk strategy — it is a High priority risk that will mislead delivery.

2. **R-03 assumes OQ-01 is resolved**: R-03 lists baseline profile rejection as a High risk with a specific test asserting `EvalError::ConfigInvariant` and the exact message string. The specification has not closed OQ-01. This creates a situation where the test name is pre-declared for a behaviour the spec has not finalised. The risk entry is correct in intent but premature given the spec's open question.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns — found #2298 (config key semantic divergence: same TOML key, different weights payload than vision example) and #3426 (formatter overhaul underestimates section-order regression risk). #2298 pattern was checked: nan-010's TOML key semantics (`distribution_change`, `distribution_targets`) are consistent with the scope description — no semantic divergence. #3426 section-order pattern noted but not applicable (nan-010 adds a new gate mode, not a section reorder).
- Stored: nothing novel to store — the variance patterns found here (spec leaving arch open questions unresolved, risk strategy mischaracterising a document it reviewed) are feature-specific. The heading-level inconsistency between a component's example and the component's own spec (Variance #4) is potentially recurring, but a single instance is insufficient to establish a pattern. Flag for retrospective if it recurs in subsequent features.
