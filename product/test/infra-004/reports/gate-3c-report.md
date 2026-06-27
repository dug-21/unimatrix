# Gate 3c Report: infra-004

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-27
> Result: PASS

## Scope note

Test/CI-only feature (shell + YAML), no `crates/` change. The mandatory integration
gate is `pytest -m smoke` (no cargo test exists for this feature — not a gap, by
design). Validated by re-executing the shipped bytes, not by reading the report.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof | PASS | All 15 risks mapped; every non-negotiable test name grep-verified present in actual suites; both Critical (R-01, R-05) proven by executing shipped bytes |
| 2. Test coverage completeness | PASS | 86/86 shell + 24/24 smoke re-run green; edge cases (early-exit-0, substring marker, exit-3, unexpected exit) covered; needs-graph cross-component coverage present |
| 3. Specification compliance | PASS | 12 ACs PASS pre-merge; AC-04/AC-11 PENDING-operational (legit carve-out); AC-14 deferred human gate; all FRs/NFRs verified where measurable |
| 4. Architecture compliance | PASS | C-WB/C-TS/C-LN/C-FLIP implemented per ARCHITECTURE; capture-shape §7 honored; §5 blast-radius covered; needs-flip in place; zero crates/ drift |
| 5. Knowledge stewardship | PASS | RISK-COVERAGE-REPORT has `## Knowledge Stewardship` with `Queried:` + `Stored:` (reason given) |

## Integration Test Validation (mandatory)

- `pytest -m smoke`: **re-run in foreground → 24 passed, 601 deselected, rc=0 in 208s.** Matches reported 24/24.
- Shell logic/static suites: **re-run all four in foreground → 39 + 19 + 15 + 13 = 86/86, 0 failed, each prints its summary line (R-14 completeness witness).** Matches reported 86/86.
- xfail markers: feature added **none**. The xfail markers present under `suites/*.py`
  are all pre-existing infra-001 markers (GH#405, GH#111, GH#406, ONNX-env limits) —
  the feature diff touches no `suites/*.py` file, so none were introduced. Report's
  "none filed, no xfails" claim is accurate for this feature.
- No integration tests deleted/commented: confirmed — `git diff --name-only main...HEAD`
  contains zero `suites/*.py` paths; 24 smoke tests collected and passed.
- RISK-COVERAGE-REPORT includes integration test counts: yes (24 smoke + 86 shell tabulated).

## Detailed Findings

### Check 1 — Risk mitigation proof
**Status**: PASS
**Evidence**: Per the Gate-3c lesson (#2758), every non-negotiable test function name
listed in RISK-COVERAGE-REPORT was grep-verified against the actual suite source —
all present (no report-only ghost names):
- C-TS / R-05: `test_tristate_rc_survives_capture`, `test_tristate_no_pipe_static_return_not_exit`,
  `test_tristate_captures_stderr[_fail]`, `test_tristate_only_exit2_nonblocking`,
  `test_tristate_marker_anchored_substring`, `test_tristate_marker_whole_line_anywhere_is_green`,
  `test_tristate_marker_byte_identical`, `test_tristate_infra_exit2_nonblocking_visible`,
  `test_tristate_infra_exit2_canonical_marker_pinned`, `test_run_smoke_gate_sibling_unchanged_exit4`
  → all in `release-gate-tristate-logic-test.sh`.
- C-WB / R-01: `test_warmup_present_requires_durable_read_roundtrip`, `test_warmup_result_is_consumed`,
  `test_warmup_uses_write_then_barrier_not_store_size`, `test_warmup_present_proceeds_to_matrix`,
  `test_warmup_timeout_is_infra_not_pass`, `test_warmup_marker_non_substring_asserted`,
  `test_warmup_row_inert_to_negatives`, `test_assert_routes_live_precedes_barrier`,
  `test_warmup_bound_default_documented`, `test_post_barrier_{green,red,infra}_still_drives`
  → all in `release-gate-isolation-logic-test.sh`.
- C-LN/C-FLIP: lane static suite functions all present in `release-gate-isolation-lane-static-test.sh`.

**Critical R-01 (ceremonial-warmup)** — refuted as ceremonial in the *shipped* bytes:
`multi-tenant-isolation-smoke.sh:430 warmup_barrier()` reuses `write_then_barrier` (a
real durable own-store write→read-as-barrier round-trip through `SMOKE_*_CMD`, NOT a
liveness-only `store_size` poll), is invoked at line 486 between `assert_routes_live`
and `run_isolation_matrix`, and timeout → `infra_fail` (exit 2, never RED/GREEN).
Load-bearing + consumed-to-gate proven pre-merge; the cold-path zero-flap leg is the
AC-11 carve-out.

**Critical R-05 (swallowed-exit-code)** — `release-gate-lib.sh:83 run_smoke_gate_tristate`
uses the exact capture shape `set +e; out="$(IMAGE="${image}" "$@" 2>&1)"; rc=$?; set -e`
(no pipe between smoke and `$?`), `return`s on every path (never `exit`), and maps
exit 2 → return 0 (visible) while 1/3/other → return 1. Full truth table proven by the
real sourced lib (19/19 executed green).

### Check 2 — Test coverage completeness
**Status**: PASS
**Evidence**: All 15 Phase-2 risks have a row in the coverage matrix. Pre-merge risks
(R-01..R-09 logic legs, R-02, R-06, R-07, R-08, R-14) are PASS via executed suites.
Edge cases from the strategy (early-exit-0 not credited, substring marker rejected by
`-qxE` full-line anchor, exit-3 SKIP hard-fails, unexpected exit blocks) are each a
truth-table cell. Cross-component coverage: `test_lane_in_manifest_needs` (needs-graph)
+ smoke no-regression (24/24). No Phase-2 risk lacks coverage.

### Check 3 — Specification compliance
**Status**: PASS
**Evidence**: AC-01/02/03/05/06/07/08/09/10/12/13/15 PASS pre-merge (verified by code
review + executed suites). AC-15 confirmed: `git diff --name-only main...HEAD` has zero
`crates/` paths; production diff = `multi-tenant-isolation-smoke.sh`, `release-gate-lib.sh`,
`.github/workflows/release.yml` only. AC-04 and AC-11 are PENDING-operational — a genuine
CI-only carve-out (require a real `workflow_dispatch` cold build + GHCR pull + first-boot
HF download; not unit-testable pre-merge). AC-14 / the R-15 chronic-INFRA VARIANCE is the
human gate explicitly deferred post-delivery — not failed here, per spawn instruction.

### Check 4 — Architecture compliance
**Status**: PASS
**Evidence**: All four components match ARCHITECTURE §2. C-FLIP confirmed at
`release.yml:666` — `create-container-manifest.needs:` includes `multi-tenant-isolation-amd64`.
The lane (`release.yml:632-663`) calls `resolve_image` + `run_smoke_gate_tristate`, no
docker build, `IMAGE` exported. The forbidden `${GITHUB_REF_NAME#v}` (R-09/C-4) is absent
from the lane — the one workflow occurrence (`release.yml:248`) is inside the pre-existing
`package-npm:` job, out of scope; `test_lane_no_ref_strip` correctly scopes the assertion
to the lane. §7 capture-shape invariants and §5 blast-radius mapping are honored.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md carries a `## Knowledge Stewardship` block with a
`Queried:` entry (`context_briefing` → #5192/#5350/#5349/#5354/#5335/#840) and a `Stored:`
entry ("nothing novel — patterns already captured as #5192/#5345/#5267/#4974/#5354"),
i.e. a reason is supplied after "nothing novel". No WARN.

## Carve-Out Confirmation (NOT masking a pre-merge gap)

| Item | Verdict | Why genuinely CI-only |
|------|---------|-----------------------|
| AC-04 / AC-11 cold-model dispatch GREEN | LEGITIMATE | Requires a real `workflow_dispatch` cold build + GHCR pull + first-boot HF download on a runner (C-10/SR-05). Pre-merge load-bearing construction of the barrier IS proven; only the empirical cold-path leg is deferred. |
| R-10 `:v<ver>` tag-push resolution | LEGITIMATE | First executes on a real tag post-merge; one tag round budgeted; tag-path INFRA degrades non-blocking (safe). |
| R-11 / R-12 | LEGITIMATE | Branch-point==main + GHCR-write-from-branch are run-time operational facts. |
| AC-14 / R-15 VARIANCE (OQ-3) | DEFERRED (human) | Human-accepted residual; explicitly out of Gate 3c scope. Not failed. |

## Rework Required

None.
