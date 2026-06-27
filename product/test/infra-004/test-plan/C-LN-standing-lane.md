# Test Plan — C-LN: Standing Isolation Lane

> File under test: `.github/workflows/release.yml` (NEW job).
> Test file: **NEW** `release-gate-isolation-lane-static-test.sh` (sibling of
> `release-gate-bundle-static-test.sh`; YAML grep / parse assertions).
> Risks: R-08 (fail-closed), R-09 (pull-404 / tag swallow), R-10 (never-green-on-tag).
> ACs: AC-06, AC-07, AC-10. (AC-11 cold-model run is CI-only — see OVERVIEW §5.)

## What the lane is
A new `release.yml` job mirroring the proven `smoke-amd64` lane (checkout, setup-node, GHCR
login) **plus one self-contained `apt-get install -y sqlite3` step**, that resolves the image via
`resolve_image(...amd64)` with `IMAGE` exported and invokes the smoke **once** via
`run_smoke_gate_tristate` (not `run_smoke_gate`). Runs on `push: tags:['v*']` and
`workflow_dispatch`. amd64-only (D-3).

## Static / YAML assertion expectations

### AC-06 — job present, correct triggers, independent status
- `test_lane_job_exists`: a new job (the isolation lane id) exists in `release.yml` with
  `needs: [build-container-x64]` and its own job id/status.
- `test_lane_inherits_triggers`: the lane runs on both tag-push (`tags:['v*']`) and
  `workflow_dispatch` (no `if:` excluding dispatch — AC-11 needs the dispatch path).

### AC-07 — pushed GHCR bytes via `resolve_image`; no rebuild; no tag swallow (R-09)
- `test_lane_calls_resolve_image`: the lane step `source`s `release-gate-lib.sh` and calls
  `resolve_image "${GITHUB_REPOSITORY_OWNER}" "${GITHUB_EVENT_NAME}" "${GITHUB_REF_NAME}" amd64`,
  exports `IMAGE`, and has **no `docker build`** step (smokes pushed bytes).
- `test_lane_no_ref_strip` (R-09 / C-4): assert **NO `${GITHUB_REF_NAME#v}`** (nor any `#v`
  strip) anywhere in the lane — the nan-019 tag-resolution swallow class. Edge case from the
  Risk Strategy: `${GITHUB_REF_NAME#v}` anywhere → tag-resolution swallow.
- `test_lane_invokes_tristate`: the lane invokes `run_smoke_gate_tristate "${IMAGE}" bash …multi-tenant-isolation-smoke.sh`
  (the tri-state runner, so exit-2 → non-blocking-visible), **not** `run_smoke_gate`.

### AC-10 — node AND sqlite3 provisioned; absence → INFRA, not empty-pass (R-08)
- `test_lane_provisions_node_and_sqlite3`: the lane has `actions/setup-node@v4` AND a
  self-contained `apt-get install -y sqlite3` step (coordinate #849; no hard dep).
- `test_sqlite3_absent_is_infra`: the **runtime** guard is already proven by the existing
  isolation-logic `test_c1_sqlite3_absent_is_infra` (sqlite3 absent → exit 2 INFRA, not pass) —
  re-confirm it still holds post-change. The **provisioning-step** failure fails closed (blocks),
  the loud guard against the quieter runtime-missing→vacuous mode.

### R-08 — fail-closed blast-radius (ARCHITECTURE §5 table, cell-by-cell)
- `test_lane_harness_mirrors_siblings`: the lane's harness surface (checkout, setup-node, GHCR
  login) mirrors `smoke-amd64` plus exactly one extra sqlite3 step — no novel harness mechanism
  that would widen blast radius.
- Map each §5 row to an assertion: harness-step failures (checkout / GHCR login / sqlite3 setup)
  fail the job (block); only script-exit-2 (warmup / pull / dep) maps to non-blocking-visible
  (the latter proven by C-TS, not YAML). No harness path maps to non-blocking.

### R-09 / R-15 — pull-404 → visible-INFRA marker stability
- The pull-404 → exit-2 → `::warning::` + canonical marker behavior is proven in **C-TS**
  (`test_tristate_infra_exit2_nonblocking_visible`). Here assert only the **call-shape** (correct
  event/ref → `resolve_image`) so a wrong tag is the only way to mis-resolve, and the divergence
  from `run_smoke_gate`'s exit-4-blocks is documented in the lane comment.

## CI-only (OVERVIEW §5) — NOT in this static test
AC-11 cold-model dispatch GREEN, real first-boot HF download evidence (R-13), branch-point ==
`main` HEAD (R-11), GHCR `:latest-amd64` push-from-branch (R-12), and the post-merge tag round
(R-10) are operational evidence, not static assertions.

## Coverage requirement
Every lane-shape AC (job, triggers, `resolve_image` call-shape, no `#v` strip, tristate
invocation, node+sqlite3 provisioning) covered by a static YAML assertion; the §5 fail-closed
table mapped cell-by-cell to a YAML or C-TS assertion; tag-resolution + cold-model proof
explicitly deferred to §5.
