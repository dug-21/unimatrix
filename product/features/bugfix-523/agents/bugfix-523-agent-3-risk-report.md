# Agent Report: bugfix-523-agent-3-risk

**Role**: Risk Strategist (Architecture-Risk mode)
**Feature**: bugfix-523 — NLI Tick Gate + Log Downgrade + NaN Guards + Session Sanitization

## Deliverable

`product/features/bugfix-523/RISK-TEST-STRATEGY.md` — written.

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 4 |
| High | 4 |
| Med | 3 |
| Low | 1 |

**Total**: 12 risks identified.

## Top 3 Test Scenarios the Tester Must Not Skip

1. **R-03 — All 19 NaN fields individually tested (AC-06 through AC-24)**: This is a checklist
   verification, not a sampling exercise. The tester must count 19 named test functions present
   and passing. A count of 18 is a gate failure. Fusion weight fields (w_sim through w_prov, loop-
   based) and phase weight fields (w_phase_histogram, w_phase_explicit) must use the exact field
   name string in `assert_validate_fails_with_field` — spot-check at least three loop-group fields
   by intentionally supplying the wrong string to confirm the assertion is not vacuous.

2. **R-01/R-02 — Path A and Path C run unconditionally with `nli_enabled=false` (AC-02)**: AC-01
   (Path B skipped) without AC-02 (Path A+C unaffected) does not prove ADR-001 compliance. The
   tester must assert that Informs edges and cosine Supports edges are written even when the gate
   fires. This is the only runtime proof that the gate is placed after both paths, not before them.

3. **R-04 — Dispatch-arm invalid session_id rejection plus valid-path regression (AC-28 + AC-29)**:
   Both tests must be present. AC-28 alone proves rejection; AC-29 proves the guard does not break
   the success path. The tester must also inspect the source to confirm no use of `event.session_id`
   appears between the capability check and the sanitize guard — test pass alone does not prove
   insertion order.

## Key Observation

R-11 (log-level gap acknowledgment) is process-level, not functional. The gate report must
explicitly cite ADR-001(c) (Unimatrix entry #4143) when marking AC-04 and AC-05 as behavioral-
only. Failure to document this is the single most likely cause of a Gate 3b WARN based on
historical pattern #3935.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"lesson-learned failures gate rejection"` — found #3935
  (tracing-test deferral, crt-036), #4076 (zero-test-module failure, nan-009/crt-042). Both
  directly inform R-06 and R-11.
- Queried: `/uni-knowledge-search` for `"risk pattern test coverage omission NaN guard"` — found
  #4133 (NaN guard pattern), #3949 (per-guard negative tests). Confirms R-03 pattern.
- Queried: `/uni-knowledge-search` for `"sanitize session_id UDS dispatch guard injection"` —
  found #3902, #4141, #3921. Informs R-04 and security section.
- Stored: nothing novel to store — all patterns already captured in Unimatrix (#3935, #4133,
  #3921). No new cross-feature risk pattern visible from this batch alone.
