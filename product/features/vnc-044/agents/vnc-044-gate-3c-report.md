# Agent Report: vnc-044-gate-3c

**Role**: Gate 3c validator (Final Risk-Based Validation)
**Result**: REWORKABLE FAIL (narrow — test-robustness + report accuracy; feature code correct)
**Glass-box report**: `product/features/vnc-044/reports/gate-3c-report.md`

## Outcome
- Checks: 4 PASS / 1 FAIL (5 total).
- Mandatory smoke gate re-run by validator: 30 passed.
- vnc-044 graph integration re-run: 63 passed / 1 failed.
- FAIL: `test_graph_legacy_summary_alias_equivalent` (AC-07/R-08) flakes ~50% (100% under CPU load) — byte-compares two sequential reads whose summary payload carries `confidence`, which background scoring (GH#405 dynamic) mutates between calls. Test-only defect; alias resolution is correct (payloads byte-identical modulo the mutated field; unit resolver test is deterministic and passes).
- Rework routed to uni-tester: make AC-07 assertion robust to `confidence` drift; correct RISK-COVERAGE-REPORT "no flakes" claim.

## Knowledge Stewardship
- Stored: nothing novel to store -- the flaky-test root cause (byte-comparing a payload carrying a background-mutable `confidence`, GH#405 scoring dynamic) is a feature-specific test defect, not a recurring cross-feature validation pattern; filing it would poison recall per "bugs are GH issues, not lessons". Routed as gate rework instead.
