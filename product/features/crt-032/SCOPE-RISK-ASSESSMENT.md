# Scope Risk Assessment: crt-032

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | The `default_w_coac()` fn and compiled-defaults struct are two separate definition sites — missing one leaves an inconsistency between serde deserialization default and runtime default | Med | Med | Architect must enumerate both sites explicitly; spec must have separate AC for each |
| SR-02 | Inline doc comments in `InferenceConfig` (lines 367, 381) reference the old 0.95 sum figure; stale comments will mislead future delivery agents | Low | High | Spec must include AC for comment updates alongside code changes |
| SR-03 | `make_weight_config()` test helper is shared across many weight-validation tests; updating `w_coac: 0.10 → 0.0` inside it ripples to any test that computes an exact sum — sum assertions expecting `≤ 0.95` pass naturally at 0.85, but any `== 0.95` assertion would break silently | Med | Med | Architect must identify and enumerate all assertions that depend on the exact helper sum value |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | SCOPE.md explicitly defers Phase 3 (code removal of `compute_search_boost` call), but the comment on `FusionWeights.w_coac` in search.rs line 118 says "default 0.10" — leaving it unchanged contradicts the zeroed default and creates drift between code and docs | Low | High | G-04 (comment update) must be in scope; spec must verify the comment reflects "default 0.0" post-delivery |
| SR-05 | The `w_coac=0.10` fixtures in search.rs are intentional test inputs (per human decision); if delivery confuses them with default-assertion tests, they may be incorrectly changed, breaking regression coverage for non-zero w_coac scoring paths | Med | Med | Spec must clearly separate "default-assertion" tests from "test-input fixture" tests; architect should document the distinction |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `ppr_blend_weight` is the sole remaining carrier of co-access signal after w_coac→0.0; if PPR is disabled (ppr_blend_weight=0.0) by an operator, co-access signal is completely absent from scoring — this is a valid operational state but must be documented | Low | Low | ADR must explicitly document the PPR-as-co-access-carrier dependency and the implication when PPR is disabled |
| SR-07 | `CO_ACCESS_STALENESS_SECONDS` governs 3 independent call sites (search.rs prefetch, status.rs stats, maintenance tick cleanup); the constant must NOT change but is referenced near w_coac in the same files — risk of accidental modification during delivery | Low | Low | Spec should include AC verifying `CO_ACCESS_STALENESS_SECONDS` is unchanged post-delivery |

## Assumptions

| Assumption | SCOPE.md Reference | Risk if Wrong |
|-----------|-------------------|---------------|
| Phase 1 eval shows zero measurable difference between ppr-plus-direct and ppr-only | Background Research / Phase 1 Measurement Results | If measurement was instrument error, zeroing w_coac regresses retrieval quality — mitigated by fact that delivery does not re-run eval |
| Operators who set w_coac > 0.0 via config file are unaffected | Constraints / Backward compatibility | Field remains valid; no breaking change |
| `compute_briefing_boost` is already dead (never called) | Non-Goals | If there is an undiscovered call site, zeroing has no additional effect — safe assumption |

## Design Recommendations

- **SR-01**: Architect must explicitly list both definition sites (`default_w_coac()` fn and compiled-defaults struct literal) as separate line-level changes in the architecture doc.
- **SR-02 + SR-04**: Spec must include ACs for updating the three inline doc comments that reference stale sum figures (lines 367, 381 in config.rs; line 118 in search.rs).
- **SR-03**: Architect should enumerate every test assertion that depends on `make_weight_config()` returning `w_coac: 0.10` and classify each as: (a) exact-sum assertion needing update, (b) upper-bound assertion passing naturally, or (c) unrelated fixture.
- **SR-05**: Specification must define which tests must NOT change (search.rs FusionWeights fixtures) and which must change (config.rs default-assertion tests).
- **SR-06**: ADR-001 should include a section on the operator implication of PPR-disabled + w_coac=0.0.
