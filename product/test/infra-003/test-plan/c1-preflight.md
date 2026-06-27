# Test Plan — C1: Read-dependency preflight

> Pseudocode: `pseudocode/c1-preflight.md` · Component: read-dependency preflight
> (docker / sqlite3 / `vol`). Risks: **R-06** (read-dep absent → silent
> empty-pass), R-10 (tri-state), R-13 (cumulative coupling).

C1 is the gate's front door: it asserts the read dependencies before any write so
that a missing dependency becomes a distinct INFRA/SKIP exit, **never** an
all-cells-empty vacuous GREEN. The test of C1 proves each absence is caught and
classified to the correct exit code.

## What C1 must do (behavior under test)

- `docker`/`docker info` absent → **SKIP exit 3** (matches posture-smoke contract).
- `sqlite3` absent on the host (`command -v sqlite3`) → **hard INFRA** (distinct
  from exit 1), never warn+continue (#4473), never empty-pass.
- `vol` busybox sidecar cannot mount the volume read-only → INFRA.
- No marker write occurs before all three pass.

## Verification tier 1 — off-Docker gate-logic test (stub-driven, primary)

Following the nan-019/nan-020 precedent (#5192, #5258), C1's dependency checks
must be exercisable without Docker by sourcing the gate and overriding the probe
commands. Assertions:

- `test_c1_docker_absent_skips_exit3` — with a stub that reports docker missing,
  the gate exits **3**, prints a SKIP reason, and writes **no** marker.
- `test_c1_sqlite3_absent_is_infra` — `command -v sqlite3` forced to fail → gate
  exits the **distinct INFRA code** (not 1, not 3, not 0); message names sqlite3
  and "provision like node" (mirrors `cloud-bundle-lib.sh:83`).
- `test_c1_vol_mount_fail_is_infra` — `vol` stub returns non-zero → INFRA exit.
- `test_c1_all_present_proceeds` — all three present → C1 returns success and the
  flow continues to C2 (no early exit).
- `test_c1_no_warn_continue` — assert there is no code path where a missing
  dependency logs a warning and continues (grep the script: every dependency
  check ends in `fail`/exit, never a bare `log` + fallthrough). R-06/#4473.

## Verification tier 2 — live run (Docker present)

- On a provisioned runner, C1 passes silently and the gate proceeds. Confirm the
  `sqlite3` presence assertion fires **before** the first `vol cat` (ordering:
  preflight precedes any read), so a mid-run sqlite3 absence cannot masquerade as
  a 0-row read.

## Exit-code contract assertions (shared with C7/R-10)

| Condition | Required exit | Distinct from |
|-----------|---------------|---------------|
| Docker absent | 3 (SKIP) | RED(1), INFRA, GREEN(0) |
| sqlite3 absent | INFRA (distinct) | RED(1), SKIP(3), GREEN(0) |
| vol mount fails | INFRA (distinct) | RED(1), SKIP(3), GREEN(0) |

## Coverage requirement

Every read dependency is presence-asserted before first use; each absence maps to
a distinct, non-GREEN-coercible exit (R-06, R-10). No dependency check can fall
through to a 0-row "pass."
