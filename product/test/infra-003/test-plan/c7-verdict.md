# Test Plan — C7: Verdict gate (bidirectional 2×2, positive-gates-negative, tri-state)

> Pseudocode: `pseudocode/c7-verdict.md`. Risks: **R-03** (positive-gates-negative
> inversion), **R-10** (tri-state collapse), **R-02** (vacuous MCP pass), **R-18**
> (substring), R-08, R-14. ACs: **AC-05**, **AC-09**, **AC-10**, **AC-14**.

C7 is the integrity core: it applies, **per surface per direction independently**,
the rule that a cross-store (negative) cell is reported GREEN **only after** that
direction's positive control reached PRESENT, and it discriminates GREEN / RED /
INFRA / SKIP into distinct exit states. The test of C7 is the headline
**teeth test**: it must demonstrably emit RED on a planted leak and INFRA (never
GREEN) on an own-store timeout — proving the gate is not a vacuous pass.

## What C7 must do (behavior under test)

- Per direction: evaluate the positive control first (C5). If PRESENT → evaluate
  the negative cell (C6). If the positive is not PRESENT by deadline → that
  direction is **INFRA**; the cross-cell "other store clean" is **never** reported
  as a pass (no vacuous GREEN).
- **Cross-store marker present → RED**, definitively and independently of the
  positive outcome (catches B mis-resolving into A even when B's own positive
  timed out as INFRA).
- A surface passes only when **both** directions' positives are PRESENT **and**
  both negative cells are clean. Both surfaces GREEN → emit the terminal marker.
- Tri-state exit: GREEN(0) / RED(1) / INFRA(distinct) / SKIP(3); no non-pass ever
  rounds to exit 0 (#5180, R-10).

## Verification tier 1 — off-Docker gate-logic test (stub-driven, the teeth)

Drive the full verdict truth table by injecting synthesized two-store read results
(the C5/C6 stub seam), no Docker:

- `test_c7_planted_leak_is_red` (**fault-injection teeth**, R-02/R-03) — inject
  `B-mcp` PRESENT in A's store (and symmetrically `A-mcp` in B, `B-obs` in A,
  `A-obs` in B, one per case) → assert the gate exits **RED 1** at the
  cross-store cell. A vacuous GREEN here is the single worst failure.
- `test_c7_positive_gates_negative_per_direction` (R-03/AC-05/AC-09) — for each of
  the four directions, force the own positive absent → that direction is INFRA and
  its cross-cell pass path is **skipped** (never reported GREEN); a planted
  cross-marker is still RED regardless.
- `test_c7_own_timeout_infra_not_red` (R-05/AC-10) — own positive times out, other
  store clean → **INFRA**, distinct from RED and from GREEN.
- `test_c7_all_green_emits_marker` — all four positives PRESENT + all four
  cross-cells absent → exit 0 and emit exactly one terminal
  `ALL GATES PASSED` line (verify-by-name, see below).
- `test_c7_tristate_exit_codes` (R-10) — each of {missing dep, pre/stale-barrier
  read, absent route, missing main db} → INFRA exit; {any cross-marker present} →
  RED 1; Docker absent → SKIP 3. Assert all four exit states are distinct and no
  non-GREEN maps to 0.
- `test_c7_surface_and_direction_independence` (R-03 sc.3) — one direction's RED
  does not let another pass on residue; the four mutually non-substring markers
  make cross-attribution impossible.

## Verification tier 2 — live run

- `test_c7_green_on_correct_isolation` — against the real shipped container, all
  four positives present and all cross-cells clean → GREEN with the terminal
  marker. Point-in-time isolation proof.

## Marker-set & overclaim discipline

- `test_c7_four_markers_mutually_non_substring` (R-18) — assert the four literals
  `infra003-{obs,mcp}-{a,b}-<run>` are pairwise non-substring before any write
  (the load-bearing precondition for the `LIKE` reads).
- `test_c7_no_overclaim_no_parity` (R-14/AC-14) — output claims point-in-time
  proof only ("advances, does not close N3"); no UDS write path, no parity-matrix
  shape; ADR-006 `FORBIDDEN_IN_LOCAL` is referenced, not re-run.

## Verify-by-name / exit-code contract (#5180 — shared with the release lane)

The gate is consumed by `run_smoke_gate` (`release-gate-lib.sh`), which treats
exit 3 as a **hard failure** (never green) and requires a terminal run-marker:

- `test_c7_terminal_marker_matches_grep` — the GREEN path emits exactly one line
  matching `\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*` (e.g.
  `[isolation-smoke] ALL GATES PASSED`); the script's `log()` prefix is a
  `*-smoke` tag so the anchored grep in `release-gate-lib.sh:59` matches.
- `test_c7_no_early_exit_0` — no path exits 0 without printing the marker
  (closes the early-exit-0 false-green class, #4796/SR-01).

> Note: the gate's **distinct INFRA exit code** is C7's own tri-state discipline
> (R-10); when wired into `run_smoke_gate` an INFRA exit is a non-zero,
> non-green outcome, consistent with the harness treating skip/non-pass as
> failure.

## Coverage requirement

No cross-cell is reported for a direction whose positive has not passed; a planted
leak is RED, an own-store timeout is INFRA, Docker-absent is SKIP — four distinct,
non-GREEN-coercible exit states (R-03/R-10); the four markers are mutually
non-substring (R-18); output is point-in-time only (R-14).
