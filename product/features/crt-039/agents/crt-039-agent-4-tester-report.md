# Agent Report: crt-039-agent-4-tester

## Phase: Stage 3c — Test Execution

## Summary

All critical and high-priority risks covered. Unit tests: ~3872 passed, 0 failed.
Integration smoke gate: 22/22 passed. Lifecycle suite: 41/41 passed, 2 xfailed (pre-existing).
One partial gap: AC-11 eval harness (`run_eval.py` absent).

## Test Results

### Unit Tests
- Total: ~3900
- Passed: ~3872
- Failed: 0
- `cargo test --workspace` exits 0

### TC-01 / TC-02 (Critical path tests)
- `test_phase4b_writes_informs_when_nli_not_ready` (TC-01): PASS
- `test_phase8_no_supports_when_nli_not_ready` (TC-02): PASS

### TC-03 through TC-07 (New tests)
- `test_apply_informs_composite_guard_temporal_guard` (TC-03): PASS (line 2055)
- `test_apply_informs_composite_guard_cross_feature_guard` (TC-04): PASS (line 2089)
- `test_phase4b_cosine_floor_boundary` (TC-05+TC-06 combined): PASS (line 2128)
- `test_phase4b_explicit_supports_set_subtraction` (TC-07): PASS (line 2157)

### TR-01/TR-02/TR-03 Removals
- All three removed as function definitions; appear as comments only (confirmed by fn-name grep)

### Integration (infra-001)
- Smoke: 22/22 PASS
- Lifecycle: 41/41 PASS (2 xfailed pre-existing, 1 xpassed pre-existing)

### Static Checks
- AC-01: `if nli_enabled` guard absent from background.rs call site — PASS
- AC-03: `apply_informs_composite_guard` single-argument — PASS
- AC-07: Ordering invariant comment present — PASS
- AC-17: `informs_candidates_found` at lines 290, 367, 502 — PASS
- AC-18: `format_nli_metadata_informs` absent from production code — PASS
- R-04: Dead enum variants absent from production code — PASS
- R-10: `informs_candidates_found` incremented at line 367 before dedup at line 370 — PASS

## Gaps

1. **AC-11 PARTIAL**: `product/research/ass-039/harness/run_eval.py` does not exist. The
   scenarios.jsonl is present (1585 scenarios) but the eval runner was never implemented.
   R-05 (cosine floor raise candidate pool impact) lacks its quantitative MRR backstop.
   Mitigated by: ADR-003 implementor corpus scan requirement, FR-14 observability log,
   conservative direction of the change. Recommend tracking as follow-up before Group 3.

2. **Clippy workspace (-D warnings)**: Pre-existing failures in `unimatrix-observe` (54 errors)
   and `unimatrix-engine` (2 errors) — both predating crt-039. Neither is modified by this
   feature. The crt-039-affected crates (unimatrix-server, unimatrix-core) pass clippy.

## GH Issues Filed

None. No failures caused by crt-039.

## Output

`product/features/crt-039/testing/RISK-COVERAGE-REPORT.md`

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned 18 entries; key findings: #2758 (gate-3c must grep for every non-negotiable test function by name), #3949 (per-guard negative tests for composite guards), #3946 (CI gate domain string absence — scan production code only). All applied.
- Stored: nothing novel to store -- test execution followed established patterns from Unimatrix entries #2758, #3949, #3946. No new cross-feature patterns emerged from single-feature execution.
