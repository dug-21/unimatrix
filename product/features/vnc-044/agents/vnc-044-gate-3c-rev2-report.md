# Agent Report: vnc-044-gate-3c-rev2

> Role: uni-validator — Gate 3c re-validation (iteration 1)
> Feature: vnc-044
> Result: PASS (supersedes prior REWORKABLE FAIL)

## Task
Re-validate the single REWORKABLE FAIL from Gate 3c rev1 — the flaky AC-07/R-08 byte-compare
`test_graph_legacy_summary_alias_equivalent` (GH#405 background-scoring `confidence` drift between
two sequential reads) — plus confirm the other 10 hardened two-read comparisons, the corrected
RISK-COVERAGE-REPORT, no production drift, and re-run the smoke + `-k graph` gates.

## Outcome
PASS. Details in `product/features/vnc-044/reports/gate-3c-report.md` (Re-Validation Outcome section).

- Rework commit `406e4d04` is test-only (test_tools.py, test_lifecycle.py, RISK-COVERAGE-REPORT,
  reports) — zero `crates/**` changes; release binary current (built after last crates commit).
- AC-07 test now asserts exact 8-field key set on every node of both responses AND compares
  structurally with only mutable VALUES neutralized (keys retained) → coverage not weakened.
- 10 other two-read comparisons converted to structural equality the same way; envelope-metadata
  asserts retained per test; `default != full` byte-inequality kept (robust by construction).
- RISK-COVERAGE-REPORT overclaim removed; explicit flake-found-and-fixed record present.
- Validator-run gates: smoke 30/30 rc=0; `-k graph` 64/0 rc=0; stress 9/9 under full-core CPU load
  (the exact prior forced-failure condition) with 0 flakes.

## Knowledge Stewardship
- Queried: reviewed prior gate report `gate-3c-report.md` + RISK-COVERAGE-REPORT + rework diff for context.
- Stored: nothing novel to store -- the resolved defect (byte-comparing a payload carrying a
  background-mutable field) is a feature-specific test-robustness bug tracked via the gate, not a
  recurring cross-feature validation pattern; storing it would poison recall per the
  "bugs are GH issues, not lessons" rule. The general lesson (avoid byte-comparing two live reads
  when the payload carries background-mutable fields) is a test-authoring convention owned by the
  tester agent, already captured in its rework and the RISK-COVERAGE-REPORT; no separate validator
  entry warranted.
