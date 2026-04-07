# Alignment Report: crt-049

> Reviewed: 2026-04-07 (re-review after corrections 1-4)
> Artifacts reviewed:
>   - product/features/crt-049/architecture/ARCHITECTURE.md
>   - product/features/crt-049/specification/SPECIFICATION.md
>   - product/features/crt-049/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope: product/features/crt-049/SCOPE.md
> Prior report: ALIGNMENT-REPORT.md (2026-04-07, pre-correction)

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature strengthens intelligence pipeline signal fidelity — direct Wave 1A enabler |
| Milestone Fit | PASS | Cortical phase; all deferrals honored; no future-milestone anticipation |
| Scope Gaps | PASS | All 17 SCOPE.md acceptance criteria (AC-01 through AC-17) addressed |
| Scope Additions | WARN | ADR-004 (500-ID cardinality cap) not in SCOPE.md; resolves SR-03 recommendation; benign |
| Architecture Consistency | VARIANCE | Component 2 guard condition (`search_exposure_count == 0 && explicit_read_count == 0 && injection_count == 0`) contradicts the corrected guard in Component 4 and SPECIFICATION.md FR-08 / AC-17 |
| Risk Completeness | VARIANCE | Coverage summary gate list omits AC-16 [GATE] and AC-17 [GATE]; R-05 scenario 2 and R-06 scenario 3 describe the old guard condition, not the corrected one |

**Overall: PASS with one WARN and two VARIANCEs.** Variances are localized — both involve the guard condition correction (correction 3) not being applied consistently to ARCHITECTURE.md Component 2 and the RISK-TEST-STRATEGY.md test scenarios. Corrections 1, 2, and 4 are consistently applied. Human review required for the two VARIANCEs before delivery proceeds.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | ADR-004: 500-ID cardinality cap on `explicit_read_by_category` batch lookup | In ARCHITECTURE.md (ADR-004, Unimatrix #4217) and RISK-TEST-STRATEGY.md (R-04), not in SCOPE.md. Directly responsive to SR-03 from SCOPE-RISK-ASSESSMENT.md. Benign — no user-visible behavior change, resolves a scoped risk. |
| Addition | `EXPLICIT_READ_META_CAP = 500` constant and `tracing::warn` at cap | Implied by ADR-004; test-specified in R-04 scenario 4. Not requirement-specified in SPECIFICATION.md constraints section — minor completeness gap. |
| Simplification | Architecture uses a second `batch_entry_meta_lookup` call rather than a single combined lookup | SCOPE.md step 4 implies a single join; architecture correctly separates the two sets. Rationale is sound: query_log+injection IDs and explicit read IDs have different sources and processing paths. |

No scope gaps detected. All 17 SCOPE.md acceptance criteria (AC-01 through AC-17) are addressed in the specification and architecture.

---

## Variances Requiring Approval

### VARIANCE 1: ARCHITECTURE.md Component 2 guard condition not updated (Correction 3)

**What**: ARCHITECTURE.md Component 2 (`knowledge_reuse.rs`) line 48 still reads:
> `search_exposure_count == 0 && explicit_read_count == 0 && injection_count == 0`

ARCHITECTURE.md Component 4 (`retrospective.rs`) line 69 correctly reads:
> `reuse.total_served == 0 && reuse.search_exposure_count == 0`

SPECIFICATION.md FR-08 and AC-17 [GATE] specify the correct guard as `total_served == 0 && search_exposure_count == 0`. SCOPE.md AC-17 is the authoritative source of this correction.

**Why it matters**: Component 2 is the computation layer spec for `knowledge_reuse.rs`. The old three-condition guard in Component 2 does not match what AC-17 requires. If a delivery engineer implements Component 2 using the architecture document as the primary reference, they will implement the wrong guard — the exact bug that AC-17 was introduced to prevent. An injection-only cycle (`injection_count > 0`, `search_exposure_count == 0`, `explicit_read_count == 0`) hits the old guard's early-return path and returns an empty report, suppressing served knowledge entirely.

**Recommendation**: Update ARCHITECTURE.md Component 2, line 48, to read:
> `Update early-return guard: zero check is now `total_served == 0 && search_exposure_count == 0``

This aligns Component 2 with Component 4, FR-08, and AC-17.

---

### VARIANCE 2: RISK-TEST-STRATEGY.md gate list and two test scenarios not updated (Corrections 2 and 3)

**What — gate list**: RISK-TEST-STRATEGY.md coverage summary (final line of Coverage Summary table) lists gate items as:
> `AC-02, AC-06, AC-13, AC-14, AC-15`

SCOPE.md and SPECIFICATION.md both designate AC-16 [GATE] (string-form ID handling) and AC-17 [GATE] (injection-only cycle render guard) as non-negotiable gate items. Neither appears in the gate list.

**What — R-05 scenario 2**: RISK-TEST-STRATEGY.md R-05, test scenario 2 reads:
> "Inspect the guard expression in code review: must read `search_exposure_count == 0 && explicit_read_count == 0 && injection_count == 0` (all three conditions)."

This describes the old (incorrect) guard. After correction 3, the guard is `total_served == 0 && search_exposure_count == 0`. R-05 scenario 2 would validate the wrong implementation if followed.

**What — R-06 scenario 3**: RISK-TEST-STRATEGY.md R-06, test scenario 3 reads:
> "Verify the zero-delivery guard uses `search_exposure_count == 0 && explicit_read_count == 0` — a struct with only `explicit_read_count > 0` produces non-empty output."

This again describes the old guard condition, not the corrected one. The corrected guard is `total_served == 0 && search_exposure_count == 0`. A struct with `explicit_read_count > 0` and `search_exposure_count == 0` still produces `total_served > 0`, so the corrected guard would produce non-empty output — but the scenario is testing the wrong predicate.

**Why it matters**: The risk strategy is the delivery engineer's test construction guide. R-05 scenario 2 directs a code review check for a guard condition that must NOT be present. If a reviewer finds `total_served == 0 && search_exposure_count == 0` (the correct guard), they will fail the R-05 scenario 2 check. The gate list omission of AC-16 and AC-17 means these [GATE] items will not be treated as delivery blockers unless the delivery engineer reads SCOPE.md independently.

**Recommendation**: Three targeted updates to RISK-TEST-STRATEGY.md:

1. R-05 scenario 2: Replace with:
   > "Inspect the guard expression in code review: must read `total_served == 0 && search_exposure_count == 0`. The old three-condition form (`search_exposure_count == 0 && explicit_read_count == 0 && injection_count == 0`) must NOT be present."

2. R-06 scenario 3: Replace with:
   > "Verify the zero-delivery guard uses `total_served == 0 && search_exposure_count == 0` — a struct with only injections present (total_served > 0, search_exposure_count == 0) produces non-empty output — AC-17 [GATE]."

3. Coverage summary gate item list: Add AC-16 and AC-17:
   > `Gate items (delivery merge blocked if failing): AC-02, AC-06, AC-13, AC-14, AC-15, AC-16, AC-17.`

---

## Detailed Findings

### Correction 1: ObservationRecord.input Two-Branch Parse — Consistently Applied

SCOPE.md Proposed Approach Step 2 describes both `Value::String` (hook listener) and `Value::Object` (direct MCP) branches. All three source documents handle this correctly:

- ARCHITECTURE.md: Component 2 describes the filter; the data flow diagram (lines 221-224) explicitly shows both branches with the `serde_json::from_str(s).ok()` path for Value::String.
- SPECIFICATION.md: Filter predicate conditions 3-5 name both forms; the Central Distinction section (line 143) explicitly states both arrival forms and requires handling.
- RISK-TEST-STRATEGY.md: R-02 confirms the hook-sourced prefix problem; S-01 covers untrusted input handling. The two-branch parse is implicit in the test scenarios through AC-03 and AC-12.

**Verdict: Consistently applied across all three artifacts.**

### Correction 2: String-Form ID Handling + AC-16 [GATE] — Partially Consistent

SCOPE.md AC-16 [GATE] (string-form ID fallback via `as_str().and_then(|s| s.parse().ok())`) is present in both ARCHITECTURE.md and SPECIFICATION.md:

- ARCHITECTURE.md: Data flow diagram line "extracts id as u64 or parseable string; returns HashSet<u64>" captures the behavior.
- SPECIFICATION.md: AC-16 [GATE] is present (lines 107-109) with full failure mode documented; filter predicate condition 5 specifies the exact fallback sequence.
- RISK-TEST-STRATEGY.md: No dedicated R-xx for string-form ID handling. AC-16 is a [GATE] item per SCOPE.md and SPECIFICATION.md, but the coverage summary gate list (line 282) does not include it. This is the gate list omission captured in VARIANCE 2 above.

**Verdict: Behavior is consistently specified in ARCHITECTURE.md and SPECIFICATION.md. The gate designation is missing from RISK-TEST-STRATEGY.md (captured in VARIANCE 2).**

### Correction 3: Render Guard `total_served == 0 && search_exposure_count == 0` + AC-17 [GATE] — Inconsistently Applied

This is the primary finding of this re-review. The correction is applied in SPECIFICATION.md and partially in ARCHITECTURE.md, but not in ARCHITECTURE.md Component 2 or the RISK-TEST-STRATEGY.md test scenarios:

- SCOPE.md AC-17 [GATE]: Authoritative source. Guard is `total_served == 0 && search_exposure_count == 0`. Correct.
- ARCHITECTURE.md Component 4 (retrospective.rs, line 69): Correct guard. Aligned with SCOPE.md.
- ARCHITECTURE.md Component 2 (knowledge_reuse.rs, line 48): OLD guard. Contradicts SCOPE.md and Component 4. (VARIANCE 1)
- SPECIFICATION.md FR-08: Correct guard. Aligned with SCOPE.md.
- SPECIFICATION.md AC-17 [GATE]: Present and correct. Aligned with SCOPE.md.
- RISK-TEST-STRATEGY.md R-05 scenario 2: OLD guard described. (VARIANCE 2)
- RISK-TEST-STRATEGY.md R-06 scenario 3: OLD guard described. (VARIANCE 2)
- RISK-TEST-STRATEGY.md coverage summary: Missing AC-16 and AC-17 from gate list. (VARIANCE 2)

**Verdict: Inconsistently applied. Two VARIANCE items raised.**

### Correction 4: AC-13 Group 10 Characterization — Consistently Applied

SCOPE.md AC-13 now correctly characterizes `explicit_read_by_category` as a "cycle-level reporting field, not training input." All three source documents are aligned:

- ARCHITECTURE.md: The data flow diagram and component descriptions describe the field without claiming it is a training input. Neutral — no conflict.
- SPECIFICATION.md: AC-13 [GATE] (lines 95-97) explicitly states "It is NOT the training input for Group 10 — Group 10 requires phase-stratified `(phase, category)` aggregates from `observations` directly (out of scope per C-08)." Domain model table row for `explicit_read_by_category` (line 128) repeats "NOT the primary Group 10 training input."
- RISK-TEST-STRATEGY.md: R-09 (line 143) states Group 10 "declares a hard dependency on `explicit_read_by_category`" and describes it as a "primary input." The word "primary" could be read as implying training primacy, but R-09 is describing Group 10's dependency on the field's existence and contract, not asserting it is a training input in the ML sense. The characterization is not contradictory to SCOPE.md AC-13 as corrected — it does not claim the field is a training input.

**Verdict: Consistently applied. R-09 phrasing is acceptable in context.**

### Vision Alignment

crt-049 directly serves the Wave 1A intelligence pipeline. The product vision identifies "No session-conditioned relevance — every query treated identically" as a high-severity gap. ASS-040 Group 10 (phase-conditioned category affinity, a Wave 1A WA-2 enabler) depends on `explicit_read_by_category`. The feature's `total_served` redefinition (consumption-side signal only) is consistent with the vision's signal fidelity emphasis.

No domain coupling introduced. Category strings flow from `entries.category`, which is config-driven per W0-3. The feature does not anticipate any future-wave capability.

**Verdict: PASS.**

### Milestone Fit

Cortical phase, Wave 1A. The observations table source (crt-043 `phase` column) is Wave 1A infrastructure. Downstream target (Group 10 / WA-2 / W3-1 training) is current roadmap. C-08 explicitly defers phase-stratified breakdowns to Group 10. All deferrals from SCOPE.md Non-Goals are honored in all three source documents.

**Verdict: PASS.**

### Architecture Review

The architecture is precise and resolves all seven SCOPE-RISK-ASSESSMENT items through ADRs. The single internal inconsistency (Component 2 vs. Component 4 guard condition) is captured as VARIANCE 1. All other component interactions match the specification domain model exactly. The integration surface table is complete and consistent with the specification.

**Verdict: PASS** subject to VARIANCE 1 resolution.

### Specification Review

The specification is comprehensive. All 17 AC items from SCOPE.md (including the corrected AC-13, new AC-16, and new AC-17) are present with verification methods and [GATE] designations. FR-08 states the corrected guard condition explicitly. The domain model table for `explicit_read_by_category` correctly characterizes it as a cycle-level reporting field. Advisory message wording (FR-10) addresses SR-05. No field name, type, or alias discrepancy between specification and architecture integration surface table.

**Verdict: PASS.**

### Risk Strategy Review

The risk strategy covers 13 risks, 4 integration risks, and 6 edge cases with appropriate gate-level assignments for high-severity items. The two VARIANCEs are localized to the guard condition update not being applied to R-05/R-06 scenarios and the gate list not being updated to include AC-16 and AC-17. All other risk content is internally consistent and appropriately calibrated.

**Verdict: PASS** subject to VARIANCE 2 resolution.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for topic `vision`, category `pattern` — found #2298 (config key semantic divergence), #3742 (optional future branch / scope addition WARN pattern), #3158 (deferred scope resolution / AC reference ambiguity), #4082 (per-source migration test independence). Pattern #3742 is relevant: it flags the case where a scope addition (ADR-004 cardinality cap) diverges from scope deferral intent. Applied as the basis for the WARN classification on ADR-004.
- Stored: nothing novel to store. The inconsistency pattern here (correction applied to some artifacts but not all) is feature-specific; crt-049 is a multi-artifact correction round and the guard condition divergence is a point failure, not a recurring misalignment type that generalizes across features.
