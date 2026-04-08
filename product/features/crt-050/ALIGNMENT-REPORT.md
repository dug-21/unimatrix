# Alignment Report: crt-050

> Reviewed: 2026-04-07
> Artifacts reviewed:
>   - product/features/crt-050/architecture/ARCHITECTURE.md
>   - product/features/crt-050/specification/SPECIFICATION.md
>   - product/features/crt-050/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-050/SCOPE.md
> Scope risk source: product/features/crt-050/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature advances the Wave 1A intelligence pipeline and prepares the W3-1 GNN cold-start path |
| Milestone Fit | PASS | Correctly targets Wave 1A signal quality improvement; defers GNN to ASS-029/W3-1 |
| Scope Gaps | PASS | All SCOPE.md goals and acceptance criteria are addressed in source docs |
| Scope Additions | WARN | FR-17/AC-14 minimum coverage threshold gate is a meaningful scope addition not explicit in SCOPE.md AC-11 |
| Architecture Consistency | WARN | One open question (OQ-1 per-phase weight aggregation) left to implementer without an ADR; field naming disagreement between ARCHITECTURE.md and SPECIFICATION.md |
| Risk Completeness | WARN | R-01 (spec C-02/AC-SV-01 incorrect) is confirmed critical; spec must be corrected before implementation begins |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | None identified | All 5 SCOPE.md goals are traceable to architecture components and specification functional requirements |
| Addition | `min_phase_session_pairs` coverage threshold as a `use_fallback` gate (FR-17, AC-14) | SCOPE.md AC-11 calls for an observations-coverage *diagnostic warning*. The specification promotes this to a hard `use_fallback = true` gate when the pair count falls below the threshold. SCOPE.md does not authorize the fallback gate, only the warning. |
| Addition | `min_phase_session_pairs` new `InferenceConfig` field | SCOPE.md does not mention adding a new config field. The architecture and spec both add `min_phase_session_pairs: u32` (default 5) to `InferenceConfig`. |
| Simplification | SR-06 / infer_gate_result() module boundary | SCOPE.md proposes calling `infer_gate_result()` directly. Architecture ADR-003 resolves this as an inline `outcome_weight()` function with a canonical reference comment. Rationale is documented: importing from `tools.rs` violates crate layering. This simplification is acceptable. |
| Simplification | SR-01 / double-encoding storage contract | SCOPE.md identifies this as a critical open question requiring architect verification. Architecture ADR-005 resolves it as confirmed no double-encoding — pure-SQL approach valid for all write paths. The RISK-TEST-STRATEGY confirms the architect's finding is correct and the spec writer's C-02/AC-SV-01 is wrong. |

---

## Variances Requiring Approval

### VARIANCE 1: Coverage threshold gate promotes advisory diagnostic to hard fallback

**What**: SCOPE.md AC-11 specifies an observations-coverage diagnostic that "when distinct `(phase, session)` count within the lookback window falls below a configurable minimum threshold, emit a tick-time warning." The specification (FR-17, AC-14) and architecture both implement this as a `use_fallback = true` gate — not just a warning. When the pair count falls below `min_phase_session_pairs` (default 5), the rebuild actively degrades to cold-start semantics and disables phase scoring.

**Why it matters**: SCOPE.md authorized a warning. A `use_fallback = true` gate has a behavioral consequence: it silences phase affinity scoring in production when observation coverage is sparse. In development and early deployment environments, the default threshold of 5 could trigger spuriously and suppress a potentially useful (if low-confidence) signal. The RISK-TEST-STRATEGY (R-04) itself identifies that the architecture default (5) and the spec suggestion (10) are in disagreement — and the implementer needs a single authoritative value.

**Recommendation**: Human approval needed to confirm that (a) the hard `use_fallback` gate rather than warning-only is intended, and (b) a default threshold of 5 distinct `(phase, session)` pairs is the approved value. If the gate behavior is approved, update SCOPE.md AC-11 to reflect the hard gate. If only the warning was intended, demote FR-17 to emit a warning without setting `use_fallback = true`.

---

### VARIANCE 2: Field naming inconsistency between ARCHITECTURE.md and SPECIFICATION.md

**What**: The new `InferenceConfig` field for the minimum coverage threshold is named `min_phase_session_pairs` in ARCHITECTURE.md (Integration Surface section and domain models) but is named `min_phase_session_coverage` in the SPECIFICATION.md domain models section. These are the same field. The implementer will face a compile-time choice between the two names with no authoritative source.

Specifically:
- ARCHITECTURE.md Integration Surface: `min_phase_session_pairs: u32` (default 5, range [1, 1000])
- SPECIFICATION.md Domain Models (InferenceConfig block): `min_phase_session_coverage: u32`

**Why it matters**: The implementer will use one name and the other will be wrong, causing confusion at code review or generating a defect-fix PR. Given the serde alias pattern is already being used for `phase_freq_lookback_days`, the correct name must be decided before implementation.

**Recommendation**: Resolve before handing off to implementation. Prefer the ARCHITECTURE.md name (`min_phase_session_pairs`) as it appears in the Integration Surface table, which is the implementation contract. Update SPECIFICATION.md domain models to match.

---

### VARIANCE 3: Spec C-02/AC-SV-01 contains an incorrect assertion that must be corrected before implementation

**What**: The SPECIFICATION.md at C-02, FR-07, and AC-SV-01 contains a constraint describing a double-encoding problem on the hook-listener write path and requiring the architect to resolve it before implementation. The RISK-TEST-STRATEGY (R-01, SR-01 resolution section) and ARCHITECTURE.md (ADR-005) both confirm this is incorrect: the hook-listener write path produces a plain JSON object string, not a double-encoded string. `json_extract(o.input, '$.id')` works for all rows.

This means the spec contains an incorrect blocking gate (AC-SV-01: "This criterion is satisfied by ADR-005 existing in Unimatrix (#4227). No blocking implementation gate.") alongside incorrect rationale in C-02 and FR-07. The contradiction is between the spec's own language ("incorrectly blocks implementation" in AC-SV-01) and the body of C-02 and FR-07, which still describe the problem as if it requires resolution.

**Why it matters**: An implementation agent reading C-02 verbatim would encounter conflicting signals — C-02 says the hook path requires resolution; AC-SV-01 says the ADR resolves it. This creates a decision point the spec should not leave open. The RISK-TEST-STRATEGY explicitly identifies this as a Critical risk (R-01, highest priority in the risk register).

**Recommendation**: The specification must be corrected before implementation begins. C-02 should be rewritten to state the confirmed storage contract (hook path is not double-encoded, pure-SQL approach valid). FR-07 should be simplified to remove the conditional "Option A / Option B" framing. The RISK-TEST-STRATEGY's write-path contract test requirement (R-01 test scenario 1) is valuable and should be retained as a test requirement regardless.

---

### VARIANCE 4: OQ-1 per-phase weight aggregation strategy is unresolved and untracked

**What**: ARCHITECTURE.md explicitly leaves OQ-1 (the per-phase weight aggregation strategy for the outcome map) as an open question for the implementer: "The implementer should choose mean-weight (AC-04 references Query B as the weighting source; mean-weight is the most principled interpretation of 'weighted frequency'). This is flagged for the implementer to document in the implementation pseudocode."

No ADR entry is cited for this decision in the Technology Decisions table. Eight other decisions have ADR entries (#4223–#4230). OQ-1 is not in that table.

**Why it matters**: The mean-weight vs. best-weight choice has a direct impact on the correctness and direction of the learned signal (as analyzed in RISK-TEST-STRATEGY R-03). RISK-TEST-STRATEGY R-03 confirms this matters: "If the implementation accidentally applies per-cycle weights... rows in the same bucket could have different multipliers, scrambling the ordering." Mean-weight was chosen by the architect, but it is not recorded as a decision. It should be.

**Recommendation**: Before implementation, record an ADR for OQ-1 per-phase weight aggregation (mean-weight strategy). The implementer should not be left to re-derive this from the pseudocode comment. The test scenario in R-03 is well-specified and should be included in the implementation test plan.

---

## Detailed Findings

### Vision Alignment

crt-050 advances the Wave 1A adaptive intelligence pipeline in a targeted way: it replaces a noisy signal (search exposures from `query_log`) with a cleaner signal (explicit reads from `observations`) in `PhaseFreqTable::rebuild()`. This directly serves the vision's core intelligence pipeline goal: "given what the agent knows, what they have been doing, and where they are in their workflow, surface the right knowledge — before they ask for it."

The feature also prepares the W3-1 GNN cold-start path via `phase_category_weights()`, which replaces hand-tuned WA-2 constants. This is consistent with the vision statement that "the intelligence pipeline cannot learn from usage it cannot observe" and that "W3-1 replaces those formulas with a learned function that keeps improving."

The feature correctly defers the W3-1 GNN itself to ASS-029, and correctly defers changes to `w_phase_explicit` / `w_phase_histogram` default values to W3-1. This is milestone discipline: building only what is needed now, with a clean interface for future waves.

Outcome-based weighting from `cycle_events` is a meaningful signal quality improvement that is not explicitly mentioned in the vision but is consistent with the vision's principle of learning from actual usage patterns rather than proxies.

No domain coupling violations were identified. The feature uses existing schema without adding domain-specific columns. Config field rename (`query_log_lookback_days` → `phase_freq_lookback_days`) moves the field name away from a dev-workflow-specific reference, which is net-positive for domain agnosticism.

**Verdict**: PASS.

---

### Milestone Fit

crt-050 sits correctly in Wave 1A — the "Adaptive Intelligence Pipeline" wave. The predecessor features (WA-2 col-031 for `PhaseFreqTable`, crt-043 for `observations.phase`, crt-049 for `observations.input`) are confirmed merged. The dependency graph in the vision places Wave 1A after W1-4 (NLI, complete) and W1-5 (in progress). crt-050 does not require W1-5 to be complete — it reads from `observations` which is already generalized to the `hook` column form.

The MRR eval harness gate (AC-12, ≥ 0.2788) correctly references the W1-3 evaluation harness baseline. This is the right gate for a signal quality change.

No Wave 2 or Wave 3 capabilities are being implemented prematurely. The `phase_category_weights()` accessor is the minimum interface needed for W3-1 cold-start; it does not implement any GNN logic.

**Verdict**: PASS.

---

### Architecture Review

The architecture is coherent and well-bounded. The two-query + Rust post-process approach (ADR-001) is clearly motivated and the trade-off between SQL complexity and testability is well-explained. The data flow diagram correctly captures the path from `observations` through outcome weighting to the final `PhaseFreqTable`.

The SR-06 resolution (inline `outcome_weight()` instead of calling `infer_gate_result()`) is well-reasoned: layering violation avoided, simpler interface, drift risk accepted and mitigated by a doc-comment cross-reference and exhaustive vocabulary test. The architecture makes the right call here.

The ADR table is mostly complete (8 decisions, 8 Unimatrix IDs). The exception is the per-phase weight aggregation strategy (OQ-1), which was decided in the ARCHITECTURE.md prose ("the implementer should choose mean-weight") but not recorded as a named ADR. This is the subject of VARIANCE 4.

The `PhaseOutcomeRow` visibility note (OQ-2: `pub(crate)` or internal to `unimatrix-store`) is correctly left as implementation-time discretion. The W3-1 visibility deferral for `phase_category_weights()` is correctly tracked as a C-10 open item.

The data flow note acknowledging that `observations` does not directly carry `feature_cycle` — requiring the per-phase mean aggregation strategy — is important and is correctly flagged in the data flow section.

**Verdict**: WARN (field naming inconsistency, unrecorded ADR for OQ-1).

---

### Specification Review

The specification is thorough. All SCOPE.md acceptance criteria are addressed, with additional precision added. The ubiquitous language section is clear and correct.

The specification adds FR-10 (NULL `feature_cycle` degradation) and FR-17 (`min_phase_session_pairs` gate), which go beyond SCOPE.md. FR-10 is a straightforward defensive requirement for historical data compatibility — it requires no new behavior from the scope's perspective and is acceptable. FR-17's promotion of the coverage diagnostic from a warning to a `use_fallback = true` gate requires human approval (see VARIANCE 1).

The spec's AC-SV-01 is self-contradictory and incorrect (see VARIANCE 3). The RISK-TEST-STRATEGY correctly identifies this as Critical and provides the test that would validate the correct behavior. The spec must be corrected before delivery begins.

The AC-13 sub-items in the specification are well-structured and traceable. However, sub-item (g) in AC-13 as written only covers "filter-based context_lookup excluded" — the test for `phase_category_weights()` (empty/non-empty) is labeled (h) in the specification text at line 221 but labeled (g) in the Functional Requirements at FR-12. This is a minor numbering inconsistency; verify during implementation.

The domain models section has the field naming discrepancy noted in VARIANCE 2 (`min_phase_session_coverage` vs. `min_phase_session_pairs`).

**Verdict**: WARN (incorrect C-02/AC-SV-01 content, field naming inconsistency, FR-17 scope addition).

---

### Risk Strategy Review

The RISK-TEST-STRATEGY is well-executed. It correctly identifies R-01 as the highest-priority risk, provides concrete test scenarios for each risk, and maps all scope risks (SR-01 through SR-07) to architecture risks with explicit resolution status.

Notably, the risk agent independently confirmed the architect's ADR-005 finding by reading the actual source code (`listener.rs` lines 2686–2697), rather than accepting the spec writer's claim. This is the correct resolution behavior. The risk agent's verdict — "Architect (ADR-005) is correct. The spec writer's C-02 is wrong." — is well-supported by the evidence presented.

R-07 (`phase_category_weights()` formula uses entry count not weighted-freq sum) is an important flag. The ADR-008 "normalized bucket size" formula counts distinct entries per category (breadth), not total weighted reads (intensity). The risk agent correctly identifies this as a semantic mismatch for W3-1 cold-start purposes. The test scenario is appropriate, but the risk register should be read by the human before sign-off: if W3-1 needs intensity-based weights, ADR-008 must be revisited.

R-04 explicitly notes the default value disagreement (architecture says 5, spec suggests 10). This needs resolution before implementation.

The security section is proportionate: no new attack surfaces are introduced, and the existing write path analysis is sound.

**Verdict**: WARN (R-01 must be resolved via spec correction before implementation; R-04 default value disagreement must be resolved; R-07 formula semantics should be confirmed with W3-1 team).

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `vision alignment patterns` — found entries #2298 (config key semantic divergence pattern), #3742 (optional future branch in architecture must match scope intent — WARN if architecture and risk diverge from scope deferral). Entry #3742 directly informed the VARIANCE 4 finding (untracked OQ-1 decision).
- Stored: nothing novel to store — the variances found (spec contradiction, field naming mismatch, scope addition via fallback gate, unrecorded ADR) are all feature-specific to crt-050 and do not yet constitute a cross-feature pattern. If the same "spec writer incorrectly asserts a storage contract" pattern appears in a future feature, it warrants a pattern entry.
