# C5 — Gate wiring (shell/YAML)

**Extends:** `scripts/release-gate-lib.sh` (`run_smoke_gate`, exit-code truth table, anchored run-marker
regex) REUSED AS-IS; nan-019's `docker pull || inspect || exit-4` acquisition verbatim; the release
workflow (`workflow_dispatch`/tag) — NOT `pull_request` (D-3). **Net-new code:** NONE in the gate runner;
a NEW release-workflow job + reuse of the existing stub-drive logic-test for the new C2 gate. Any new
gate-runner logic is a fork smell (R-10).

## Purpose

Make a green run PROVABLY mean the fixture ran end-to-end over the live bridge: skip-when-Docker-absent is
a HARD failure (exit-code discriminator), and a positive anchored terminal run-marker is asserted. No
early-exit-0 / environment skip masquerades as parity-proven. Satisfies FR-8 / AC-05 / SR-03 / R-08/R-12.

## Reused gate contract (verbatim — `release-gate-lib.sh`)

```
run_smoke_gate IMAGE SMOKE_CMD...:
    invoke the smoke ONCE (no pipe — preserve rc), capture rc + output
    discriminate via the truth table:
        0  -> passed       (proceed to marker check)
        3  -> skipped / Docker-absent  -> HARD FAIL (skip-is-failure, AC-05)   # never "passed"
        4  -> image unacquirable       -> FAIL (distinct from absent — R-08)
        1  -> shipped-image-path broken -> FAIL
        *  -> unexpected               -> FAIL
    assert anchored whole-line run-marker (no substring forge):
        grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'
    return 0 IFF rc==0 AND marker captured
```
The orchestration: the release-gate job invokes the **pytest orchestrator** (ADR-001), which itself
shells out to the smoke's C1+C2 (`cloud_cycle_gates`) leg. C5 wraps the smoke invocation in
`run_smoke_gate` so the exit-code + verify-by-name contract still guards it. The smoke's terminal marker
(`[783-smoke] ALL GATES PASSED ...`) is emitted only after C2's cycle+barrier+review+parity-emit succeed.

## Release-gate lane wiring (YAML — `workflow_dispatch`/tag)

```
job: nan-021-https-bridge-parity-gate      # mirrors the nan-019 release-gate job
  trigger: workflow_dispatch OR tag push    # NOT pull_request (D-3 — GH ci.yml is JS-client-only)
  runs-on: ubuntu-22.04 (+ -arm)            # CI runners ship Docker
  steps:
    - resolve_image  (release-gate-lib.sh — pinned shipped image)
    - run the pytest orchestrator under run_smoke_gate semantics
        (pytest drives UDS + shells the smoke C1/C2; smoke emits MetricVector(HTTPS) + marker)
    - the exit-code discriminator + anchored-marker assertion gate the verdict
```

## Image acquisition (reuse nan-019 verbatim — R-08/#5208)

```
docker pull "$IMAGE" || docker image inspect "$IMAGE" >/dev/null 2>&1 || { exit 4; }
# pull FIRST (cross-runner cache miss safe), THEN inspect; inspect-only is the #5208 false-fail trap.
# Docker-absent is a DISTINCT exit 3 (handled by run_smoke_gate as a hard fail), unacquirable is exit 4.
```

## Pre-merge stub-drive (R-12 — first-green tax mitigation, mirrors nan-019)

The gate-spine bytes (exit-code discrimination, run-marker grep, orchestration control flow, and C2's new
`cloud_cycle_gates` control flow) MUST be unit-tested pre-merge via the existing `SMOKE_*_CMD` stub seams
(and an optional `SMOKE_CYCLE_CMD` for C2) BEFORE the live tag run — extends the existing
`release-gate-logic-test.sh` / `release-gate-bundle-logic-test.sh`. This validates the gate spine without
Docker so first-green is budgeted as N tag rounds advancing one live link at a time (cert → bridge →
cycle → review → parity), not a one-shot.

## Error handling

- exit 3 (Docker absent) → gate FAILS hard; NEVER reports passed (false-green guard, AC-05/R-08).
- exit 4 (image unacquirable) → distinct FAIL; distinguishable from absent (R-08).
- rc==0 WITHOUT the anchored marker → gate FAILS (substring/early-exit forge guard, R-08).
- any smoke child stderr (except `emit_bundle`) tail-dumped from `$SANDBOX` on failure (R-13).

## Key test scenarios (hints for tester)

- Distinct exit codes for absent (3) / unacquirable (4) / passed (0); skip-is-failure enforced (R-08).
- Anchored whole-line marker via `grep -qxE`; a run that exits 0 without the marker FAILS (R-08).
- Acquisition path is nan-019's `pull || inspect || exit-4` verbatim — NOT re-authored (R-08/R-10).
- Gate-spine + C2 control flow stub-tested pre-merge via `SMOKE_*_CMD` seams (R-12).
- Lane is `workflow_dispatch`/tag, NOT `pull_request` (D-3); release-gate job mirrors nan-019.
