# Agent Report: crt-048-agent-2-testplan

**Phase:** Stage 3a — Test Plan Design
**Feature:** crt-048 — Drop Freshness from Lambda
**Date:** 2026-04-06

---

## Output Files

| File | Status |
|------|--------|
| `product/features/crt-048/test-plan/OVERVIEW.md` | Created |
| `product/features/crt-048/test-plan/coherence.md` | Created |
| `product/features/crt-048/test-plan/status.md` | Created |
| `product/features/crt-048/test-plan/response-status.md` | Created |
| `product/features/crt-048/test-plan/response-mod.md` | Created |

---

## Risk Coverage Mapping

| Risk | Priority | Primary Test(s) | Component Plan |
|------|----------|----------------|----------------|
| R-01 | Critical | `lambda_specific_three_dimensions` (distinct inputs detect transposition), `lambda_single_dimension_deviation` (per-slot isolation) | coherence.md |
| R-02 | Critical | Build gate; `make_coherence_status_report()` explicit grep (0.8200 sentinel) | response-mod.md |
| R-03 | Critical | Grep: exactly 1 `DEFAULT_STALENESS_THRESHOLD_SECS` definition; build gate | coherence.md |
| R-04 | High | `lambda_weight_sum_invariant` uses `(sum - 1.0_f64).abs() < f64::EPSILON` — test body inspection in Stage 3c | coherence.md |
| R-05 | Medium | `test_status_json_no_freshness_keys` (unit); `test_status_json_no_freshness_fields` (integration in `test_tools.py`) | response-status.md |
| R-06 | Critical | Grep: exactly 2 `compute_lambda(` with 4 args in `status.rs`; `coherence_by_source_uses_three_dim_lambda` unit test | status.md |
| R-07 | High | `lambda_renormalization_without_embedding` non-trivial case (0.8/0.6 with re-derived expected); all other re-normalization tests use `0.46/0.77` and `0.31/0.77` | coherence.md |
| R-08 | Medium | Build gate + JSON key-absence test | response-status.md |
| R-09 | Low | Build gate | coherence.md |
| R-10 | High | Post-delivery `context_get` on #179 and new ADR entry | (manual, Stage 3c) |

---

## Integration Suite Plan

**Suites to run in Stage 3c:**
1. `python -m pytest suites/ -v -m smoke --timeout=60` (mandatory gate)
2. `python -m pytest suites/test_tools.py -v --timeout=60` (covers `context_status` tool)
3. `python -m pytest suites/test_confidence.py -v --timeout=60` (covers Lambda/coherence)

**New integration test required:**
- `test_status_json_no_freshness_fields` in `suites/test_tools.py`
  (R-05, AC-06: JSON key absence at wire protocol level; uses `server` fixture)

**Audit required (not new tests):**
- Any existing `test_confidence.py` test asserting specific Lambda floats from
  `context_status` responses must have expected values re-checked against 3-dimension
  weights (R-07 propagation into integration suite).

---

## Key Design Decisions in Test Plans

1. **R-01 detection uses distinct-value triangulation.** The test `lambda_specific_three_dimensions`
   uses inputs (0.8, Some(0.5), 0.3) yielding 0.576. Every two-argument swap produces a
   result at least 0.015 away from 0.576, making transposition reliably detectable.

2. **R-07 requires two re-normalization sub-cases.** The trivial all-1.0 case (AC-08)
   passes for any weights summing to 1.0; the non-trivial case (0.8/0.6) distinguishes
   old weights (0.35+0.30 base) from new weights (0.46+0.31 base) through the re-normalization
   formula.

3. **R-02's `make_coherence_status_report()` is covered by build gate and 0.82-sentinel
   grep.** Default-value fixture sites would be found by naive search-and-replace; this
   site has non-default values (0.8200/15) and requires explicit identification.

4. **`lambda_weight_sum_invariant` epsilon guard is non-negotiable.** NFR-04 mandates
   `< f64::EPSILON`, not `< 0.001`. Test plan specifies the exact form. Stage 3c must
   inspect the test body to confirm compliance (not just that it passes).

---

## Open Questions

None. All decisions are resolved per IMPLEMENTATION-BRIEF.md, ADR-001 (#4199),
ADR-002 (#4193), and ALIGNMENT-REPORT.md.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned ADR-001 (#4199) and ADR-002 (#4193)
  for crt-048 directly; also surfaced pattern #724 (behavior-based testing with distinct values
  for transposition detection) and pattern #2984 (warn against copying expected score values
  from spec inputs different from test inputs). Both informed R-01 and R-07 test design.
- Queried: `context_search` for "crt-048 architectural decisions" — confirmed #4199 and #4193
  as the two active ADRs; no other crt-048 decisions exist.
- Queried: `context_search` for "coherence lambda testing edge cases" — entry #2428
  (weight normalization with empty-table guard) and pattern #179 (original 4-dim ADR)
  surfaced; #179 is the superseded ADR, not applicable as a guide.
- Stored: nothing novel to store — the distinct-value transposition detection pattern
  is well-established (#724). The re-normalization test design (trivial + non-trivial
  sub-cases) is feature-specific reasoning, not a cross-feature pattern. R-02's
  `make_coherence_status_report()` non-default-value trap is documented in the risk
  strategy and IMPLEMENTATION-BRIEF already; no new Unimatrix entry adds value.
