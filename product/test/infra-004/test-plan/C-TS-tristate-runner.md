# Test Plan — C-TS: `run_smoke_gate_tristate`

> File under test: `product/test/infra-001/scripts/release-gate-lib.sh` (NEW additive function;
> `run_smoke_gate` byte-unchanged).
> Test file: **NEW** `release-gate-tristate-logic-test.sh` (sibling of `release-gate-logic-test.sh`;
> sources the REAL lib, drives the new function against `fixtures/stub-smoke.sh`).
> Critical risk: **R-05** (swallowed-exit-code false-green). Also R-06, R-07, R-08, R-14.
> ACs: AC-08, AC-09, AC-13.

## Function contract (ADR-002)
`run_smoke_gate_tristate IMAGE CMD…` — capture spine `set +e; out="$(IMAGE="$image" "$@" 2>&1); rc=$?; set -e; echo "$out"`
(NO pipe between smoke and `$?`; **`return` never `exit`**). Exit-code map:

| stub rc | marker | expected return | expected output |
|---------|--------|-----------------|-----------------|
| 0 | present (runtime line) | **0** (GREEN) | no `::error::` |
| 0 | absent | **1** | `::error::` early-exit-0 |
| 1 | — | **1** (RED, blocks) | `::error::` RED |
| 2 | — | **0** (non-blocking) | `::warning::` + canonical INFRA marker |
| 3 | — | **1** (hard fail) | `::error::` mis-provisioned |
| other (e.g. 139) | — | **1** | `::error::` unexpected |

GREEN credited via `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'` on the **runtime**
`log()`-prefixed line.

## Unit / stub-seam test expectations (full truth table — R-05 CRITICAL)

Drive the REAL sourced `run_smoke_gate_tristate` against `fixtures/stub-smoke.sh` (honors
`STUB_RC`/`STUB_BODY`/`STUB_STREAM`), via a `run_case` driver identical in shape to
`release-gate-logic-test.sh`:

- `test_tristate_green_exit0_marker_present` — rc 0 + `[infra003-smoke] ALL GATES PASSED …` →
  **return 0**, no `::error::` (AC-08 GREEN cell).
- `test_tristate_early_exit0_marker_absent` — rc 0, no marker → **return 1**, `::error::`
  early-exit-0 (AC-09 / R-06).
- `test_tristate_red_exit1_blocks` — rc 1 → **return 1**, `::error::` RED (AC-08 / DoD).
- `test_tristate_infra_exit2_nonblocking_visible` — rc 2 → **return 0** AND output contains
  `::warning::` AND the **canonical literal** `[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN`
  (AC-13 / R-09). Pin the exact string (WARN: marker-pinning #3337).
- `test_tristate_skip_exit3_hard_fail` — rc 3 → **return 1**, `::error::` mis-provisioned (AC-08).
- `test_tristate_unexpected_exit139` — rc 139 → **return 1**, `::error::` unexpected (R-05/R-08).

### R-06 — anchored runtime-marker (no spoof)
- `test_tristate_marker_anchored_substring` — rc 0 with the marker as a **mid-line substring**
  of a longer line → **return 1** (early-exit-0); the `-qxE` full-line anchor must not be spoofed.
- `test_tristate_marker_whole_line_anywhere_is_green` — rc 0 with the marker on its own whole
  line + later prose → **return 0** (documented lib behavior).
- `test_tristate_marker_byte_identity` — reconstruct the **runtime** `[infra003-smoke] …` line
  (the smoke's `log()` prefix at runtime, NOT the source literal, #5345 finding c) and confirm
  the shipped grep matches it.

### R-05 — capture-shape invariants proven by EXECUTION
- `test_tristate_rc_survives_capture` — run the exact `set +e; out="$(STUB_RC=1 … 2>&1)"; rc=$?`
  shape: exit 1 reads `rc==1`, exit 3 reads `rc==3` (a pipe/pipefail/setsid would swallow → would
  read 0). Assert by running, never by reading YAML (#4873).
- `test_tristate_no_pipe_static` — grep the function: **no pipe** between the smoke invocation
  and `$?`; assert the function `return`s and never `exit`s (keeps it unit-testable when sourced).
- `test_tristate_captures_stderr` — a marker / `fail()` on stderr still reaches the grep via
  `2>&1`; a stderr-only exit-1 stays RED (no false green).

### R-08 — fail-closed mapping (only exit-2 → non-blocking)
- `test_tristate_only_exit2_nonblocking` — across the whole table assert the **single** cell
  returning 0-without-marker-error is exit 2; 1 / 3 / 0-no-marker / other all return 1 (no
  rounding of a non-GREEN to pass).

### R-07 — sibling no-regression (shared lib)
- `test_run_smoke_gate_byte_unchanged` (static, `git diff`): `run_smoke_gate` is byte-identical;
  the change is purely the NEW `run_smoke_gate_tristate` function.
- **Re-run `release-gate-logic-test.sh` unchanged** post-edit → identical results (the four
  existing blocking lanes' runner is unaffected).
- `test_no_existing_lane_emits_exit2` (static): confirm the exit-2 branch is net-new surface.

### R-14 — verification harness must not go false-green
- After sourcing the real lib (which sets `set -euo pipefail`), the harness does
  `set +e; set -uo pipefail` before driving RED cells.
- `test_summary_line_prints` — inject an intentionally-RED row **first**; assert the final
  `release-gate-tristate-logic-test: N passed, M failed` summary line prints (all rows ran) —
  the completeness witness, not just per-row oks.

## Coverage requirement
Every exit-code cell proven by executing the REAL sourced lib against a stub; capture-shape
(no-pipe, return-not-exit, rc-survives) asserted by execution; runtime-marker credit +
substring-rejection both present; `run_smoke_gate` proven unchanged by diff + re-run; the
harness proven to run all rows including intentionally-RED ones.
