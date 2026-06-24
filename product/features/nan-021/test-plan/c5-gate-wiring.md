# C5 — Gate Wiring (shell/YAML) — Test Plan

> **Component:** verify-by-name run-marker + exit-code discriminator; Docker acquisition; release-gate lane
> via `workflow_dispatch`/tag. **Reuses** `release-gate-lib.sh:run_smoke_gate`, nan-019's acquisition,
> the release workflow — AS-IS, no new gate-runner logic. **ACs:** AC-05 (primary), AC-06 (zero prod diff),
> AC-07 (extends, no fork). **Risks:** R-08 (false-green/false-fail), R-12 (first-green tax), R-13 (stderr).

---

## Why this is the R-12 safety net (stub-drive, pre-merge, off-Docker)

The gate-spine bytes (exit-code discriminator, run-marker grep, acquisition control flow, orchestration)
only ever run LIVE on a release tag. R-12 (release-only gate never exercised pre-tag, High likelihood) is
mitigated EXACTLY as nan-019 did (#5258 stub-drive, #5192 gate spine): drive the gate logic via the
`SMOKE_*_CMD` seams with stubs, asserting the arithmetic BEFORE the first tag round. These tests run in
plain `bash`, no Docker.

---

## Test Expectations

### AC-05 / R-08 — false-green / false-fail discriminator (stub-drive)

- **`test_c5_docker_absent_exits_3_hard_fail`**: stub Docker-absent; assert the gate exits **3** and
  `run_smoke_gate` treats exit 3 as a HARD failure (skip-is-failure). Assert a Docker-absent run does NOT
  report `passed` — the worst false-green is blocked.
- **`test_c5_image_unacquirable_exits_4_distinct`**: stub the acquisition to fail; assert image-unacquirable
  is a DISTINCT exit code (**4**) from Docker-absent (3) — the two failure classes are unambiguous (OQ3).
- **`test_c5_acquisition_pull_then_inspect_verbatim`**: assert the acquisition reuses nan-019's
  `docker pull "$IMAGE" || docker image inspect "$IMAGE" || exit 4` VERBATIM — `inspect` does NOT pull, so
  the cross-runner cache-miss false-FAIL (#5208) is avoided by trying `pull` first. Re-authored acquisition
  is a FORK smell.
- **`test_c5_run_marker_anchored_whole_line`**: assert the anchored whole-line terminal run-marker via
  `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'` (no substring forge). A run that exits 0 WITHOUT the
  marker FAILS the gate (`run_smoke_gate` returns 0 iff rc==0 AND marker captured).
- **`test_c5_exit_truth_table`**: assert the full truth table — `0`=passed, `3`=skipped/Docker-absent (HARD
  fail), `4`=image unacquirable, `1`=shipped-image-path broken, `*`=unexpected.

### AC-05 / R-12 — first-green tax / release-only gate

- **`test_c5_gate_spine_stub_driven_pre_merge`**: assert the gate-spine logic (exit-code discrimination,
  run-marker grep, orchestration control flow) is stub-driven via `SMOKE_*_CMD` seams BEFORE the live tag
  run — mirroring nan-019. This is the pre-merge unit coverage for the gate spine.
- **First-green sequencing (process expectation, recorded in RISK-COVERAGE-REPORT):** budget MULTIPLE tag
  rounds — failures surface in sequence (cert read → bridge spawn → cycle → review → parity), each round
  advancing one link. Do NOT assume one-shot green.

### AC-05 — CI home: release-gate Docker lane, NOT per-PR (file-check)

- **`test_c5_lane_is_workflow_dispatch_tag`**: assert the fixture is wired into the release-gate Docker lane
  via `workflow_dispatch`/tag in the release workflow — NOT `pull_request`, NOT the JS-only `ci.yml` matrix.
  (Standing rule: `ci.yml` is JS-client-only; Rust/container validation lives in the release workflow +
  protocol gates.)
- **`test_c5_lane_invokes_pytest_orchestrator`**: assert the release-gate job invokes the pytest orchestrator
  (which invokes the smoke gate via `run_smoke_gate`) — pytest-as-orchestrator (ADR-001), so both legs run
  in one invocation (R-03).

### AC-06 — zero production-code diff (git-diff scope check)

- **`test_c6_git_diff_scope`** (run as part of C5 gate report): assert `git diff` touches ONLY
  `product/test/infra-001/**`, the release-gate CI lane, and `product/features/nan-021/**` docs — assert NO
  `crates/**` or `lib/**` runtime modification. Image, routes, bridge, TLS provisioning, attribution chain
  exercised as-is.

### AC-07 — extends infra-001, no parallel scaffolding (diff review)

- **`test_c5_no_new_gate_runner_logic`**: assert C5 reuses `run_smoke_gate` / the exit-code truth table /
  the run-marker regex AS-IS — no new gate-runner logic.
- **`test_c5_fork_audit`**: diff review — every net-new helper names the existing asset it extends; no
  net-new server-spawn / cert-pin / credstore / bundle path anywhere in the change set; C4 is the sole
  substantial net-new module. (R-11 `projectHash` read-back audit is asserted in C1.)

### R-13 — capture-first child stderr (gate-level)

- **`test_c5_failure_dumps_captured_stderr`**: assert a release-only red dumps the captured child stderr
  tail (the container / bridge / init stderr written to `$SANDBOX`) — never an undiagnosable red with no
  captured cause (#5266). The gate's failure path tail-dumps the sandbox stderr files.

---

## Edge cases

- Docker-absent runner early-exit-0 → blocked (exit 3 hard fail).
- Cross-runner image cache miss with `inspect`-no-`pull` → avoided (pull-first, #5208).
- Run exits 0 without the anchored marker → gate FAILS (false-green guard).
- Release-only first-green tax → budgeted as N tag rounds (R-12), not a flake.

## Integration boundary

C5 wraps the whole fixture: it acquires the image (C1's precondition), invokes the pytest orchestrator that
drives both legs (C2/C3) and runs the comparator (C4), and asserts the verify-by-name + exit-code contract.
A green C5 run PROVABLY means the fixture ran end-to-end over the live bridge — no skip/early-exit-0 can
masquerade as parity-proven.
