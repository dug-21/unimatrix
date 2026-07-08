# Agent Report: 893-agent-1-fix (bugfix-893 implementer)

## Task
Wire the previously-unwired `blocks_c0_proof` documented-exception escape valve into the
nan-021/nan-022 parity release-gate disposition (GH#893, Option A + B1 + C1). Test-only
Python in `product/test/infra-001`; no production-code diff (NFR-1/NFR-2/AC-11).

## Root cause (confirmed)
`assert_rollup` raised on ANY INFRA row and never consulted `blocks_c0_proof` /
`documented_exceptions`; `rollup`'s docstring claimed a flag-aware GREEN no code
implemented; `documented_exceptions` was derived by a fragile `"MEASURABILITY" in detail`
string sniff. The seam test stopped before `assert_rollup`, so "documented gap → red job"
shipped untested.

## Files modified
- `product/test/infra-001/harness/parity_outcome.py`
  - `DimensionResult`: added structural `documented_exception: bool = False`.
  - `_classify` branch 1b (D5 measurable=False): sets `documented_exception=True` — the
    ONLY setter (not spoofable by a comparator).
  - Added `gate_disposition(results) -> (verdict, exit_code, waived)` + `_is_waivable_infra`
    — the single-source job disposition (ADR-009, Unimatrix #5648). Waived iff ≥1 INFRA,
    zero PARITY_FAIL, and every INFRA row is `documented_exception AND NOT blocks_c0_proof`
    (keyed on the flag, never the id "precompact"; orphan id → blocking).
  - `rollup`: docstring corrected to describe actual (blocks-blind, never-rounded-up)
    behavior; **logic unchanged** — artifact stays verdict:ERROR / exit 7.
- `product/test/infra-001/harness/parity_matrix_support.py`
  - `evidence_table`: `documented_exceptions` now sourced from the STRUCTURAL flag
    (`r.documented_exception`), not the string sniff. Added self-describing `waived` +
    `gate_disposition` fields from the single helper (design-review B1). Honest
    `verdict`/`exit_code` unchanged.
  - `assert_rollup`: consumes `gate_disposition`; a documented-exception-only run is WAIVED
    (returns without raising, prints the table + documented details to the job log) while
    the emitted table still carries verdict:ERROR / exit 7 / documented_exceptions /
    waived:true. Reordered so a PARITY_FAIL always raises RED (never masked behind ERROR),
    and undocumented / still-blocking INFRA still raises.
- `product/test/infra-001/harness/parity_dimensions.py`
  - precompact `blocks_c0_proof=True → False` (ADR-009, Unimatrix #5648, human-signed
    2026-07-08, bugfix-893), with the two revert conditions named in-comment. Other four
    dims stay True.
- `product/test/infra-001/suites/test_parity_dimensions.py`
  - C1: `test_blocks_c0_proof_all_six_true` → `test_blocks_c0_proof_precompact_is_signed_documented_exception`,
    asserting the four block True + precompact False against an explicit signed map
    (non-tautological); the in-repo honesty record of the exception.
- `product/test/infra-001/suites/test_https_uds_parity_matrix.py`
  - Extended the seam test `test_matrix_orchestrator_seam_with_fixture_bundle` THROUGH
    `assert_rollup` (the untested step); updated `test_matrix_evidence_table_routes_intra_and_documents_d5`
    to set the structural flag; added five guard/pin tests (see below).

## New / changed tests
- `test_matrix_documented_exception_only_is_waived_but_artifact_stays_error` — waived run:
  assert_rollup does not raise; table verdict ERROR / exit 7 / waived True / disposition PASS /
  documented_exceptions non-empty (honesty pin, B1).
- `test_matrix_undocumented_infra_still_raises` — GUARD.
- `test_matrix_documented_exception_on_blocking_dim_still_raises` — GUARD (flag not id).
- `test_matrix_documented_exception_with_real_parity_fail_raises_red` — GUARD (waiver never
  masks a real divergence; raises RED).
- `test_matrix_documented_exception_flag_set_only_by_branch_1b` — branch-1b-only origin pin.
- Extended: `test_matrix_orchestrator_seam_with_fixture_bundle`, `test_matrix_evidence_table_routes_intra_and_documents_d5`.

## Test results (component-level, off-Docker)
- `suites/test_https_uds_parity_matrix.py` + `test_parity_dimensions.py` + `test_parity_legs.py`:
  69 passed, 0 failed.
- Full off-Docker parity-family regression (`-k parity -m "not integration and not parity"`,
  covering test_parity_outcome / _comparator / _dimensions / _legs / _workload /
  ranking_tolerance / https_uds_parity[_matrix]): **201 passed, 0 failed**.
- Not run (out of scope, Phase 3): the full Docker integration harness (694 daemon-backed
  tests) and the live `-m parity` matrix drive.

## Issues / blockers
None. A1 human sign-off (ADR-009, #5648) was obtained pre-implementation; the waiver cannot
engage without the signed flag flip by construction. Node-20 action-pin chore left out of
scope per Q3.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (bugfix-893 waiver task) — surfaced ADR-009
  #5648 (the human-signed precompact flip) + release-signal lessons (#5267, #5192, #2758).
  `context_get` #5648 (ADR-009 — revert + honesty invariants; keyed on the flag, verdict
  stays ERROR). Applied: single-source disposition keyed on `blocks_c0_proof`, artifact
  honesty preserved, C1 test as the in-repo exception record.
- Stored: nothing novel to store. Per the standing rule, bugs are GH issues not lessons;
  the disposition ADR (ADR-009 #5648) is already stored by the architect. The generalizable
  lesson (a designed data-flag escape valve shipped un-wired; two decision sites over the
  same results must be single-sourced + the artifact self-describing) belongs to the
  bugfix-893 retro, not a mid-fix store.
