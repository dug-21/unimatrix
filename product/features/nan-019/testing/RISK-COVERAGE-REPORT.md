# Risk Coverage Report: nan-019

> Stage 3c execution. CI/release-workflow + shell feature. The gate's full hosted behavior is
> provable only post-tag; what is executable PRE-MERGE is the two HARD-gate shell suites, the
> full docker smoke (AC-05) on amd64, the `pytest -m smoke` regression baseline, and static
> re-checks of the edited/new scripts + `release.yml`. Per #4796, CI-dependent ACs are phrased
> "configured + verified locally; GH execution confirmed post-tag" — never asserted as executed.
>
> Execution host: linux aarch64 (runner is arm64 hardware, so the local docker smoke exercises
> the arm64 image path opportunistically; the *named* `smoke-arm64` hosted-runner job still
> requires a post-tag/post-dispatch run — R-05/AC-07 remain PENDING-post-tag by design).

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Gate logic itself is wrong & untested (exit-code `case` + run-marker grep) | `release-gate-logic-test.sh` T1 truth table {0,1,3,early-0,unexpected}×{marker} (6 rows) sourcing shipped `release-gate-lib.sh` | PASS (13/13) | Full |
| R-02 | RC swallowed before read (#4873 `setsid`/pipe class) | `test_gate_rc_survives_capture` (exit 1→RC=1, exit 3→RC=3 **by execution**); `test_gate_captures_stderr` (2>&1); static `no continue-on-error` on smoke jobs/steps | PASS | Full |
| R-03 | Early-exit-0, marker absent passes green | `test_gate_early_exit0_marker_absent`; `test_gate_marker_anchored_substring`; `test_gate_marker_whole_line_anywhere_is_green`; `test_gate_marker_byte_identical` | PASS | Full |
| R-04 | AC-05 grew-signal flaky/un-discriminating (un-retryable, OQ-6) | Docker smoke ×5 monotone (356→372 every run, hash 412 unchanged); negative-control discrimination (mis-route → FAIL; leak → FAIL); static WAL-inclusive `du -s` signal | PASS | Full |
| R-05 | arm64 cold-boot exceeds 90s boot deadline | dispatch dry-run / first-tag wall-time vs 90s margin | **PENDING-post-tag** | Deferred (post-dispatch/post-tag — by design, NOT a gap) |
| R-06 | ADR-004 independence regression (cross-branch `needs` edge) | static `needs:`-graph parse: zero smoke↔binary/npm edge; `create-release needs: package-npm` only; single manifest block point | PASS | Full |
| R-07 | Pushed-bytes contract degrades to a rebuild | static: smoke jobs source `resolve_image`→`IMAGE=`, no `docker build`/`buildx build` in smoke jobs, `docker/login-action@v3` precedes; smoke reuses `IMAGE` (skips build branch, lines 62–68); "using prebuilt image" log line | PASS (config); log PENDING-post-tag | Full (config) |
| R-08 | Manifest not actually gated | static: `create-container-manifest.needs: [smoke-amd64, smoke-arm64]`; no `continue-on-error`; dispatch `if:` present and keeps push-path `needs` | PASS (config); red-smoke→skip behavior PENDING-post-tag | Full (config) |
| R-09 | Tag-resolution mismatch (**OCCURRED** in first draft) | `release-tag-parity-test.sh` T2: push `v1.2.3-{amd64,arm64}` + `v0.8.2-amd64`, dispatch `latest-{amd64,arm64}`, `no_v_strip`, `suffix_no_swap`, + 3 discrimination self-checks (strip/swap/extra-v go RED) | PASS (13/13) | Full (pre-merge) |
| R-10 | GHCR push not yet pullable | static ordering: `smoke-amd64 needs: build-container-x64`; `smoke-arm64 needs: build-container-arm64` | PASS (ordering); race PENDING-post-tag | Full (config) |
| R-11 | Trigger surface over/under-reach | static `on:` parse: `push.tags:['v*']` + `workflow_dispatch`, EXCLUDES `pull_request` | PASS | Full |
| R-12 | AC-05 hardening regresses smoke / breaks marker | Docker smoke ×5: marker `[783-smoke] ALL GATES PASSED …` is the LAST line every run; AC-05 uses `vol()` `:ro` busybox, no executable `docker exec`; bounded one assertion pair before marker (script line 170 = marker) | PASS | Full |
| R-13 | Inherited latent smoke bug; arm64 never-run path | amd64 baseline re-confirmed (smoke ×5 green); arm64 hosted-runner first-run watched as discovery post-dispatch/post-tag | PASS (amd64); arm64 PENDING-post-tag | Full (amd64 baseline) |
| R-14 | Briefly-public un-smoked intermediates | Accepted by design (NFR-09) — documented exposure-window, no mitigation | N/A (accepted) | n/a |

## Test Results

### Shell / Static Tests (pre-merge HARD gates + static re-checks)

**T1+T2+R-02/R-03 — `release-gate-logic-test.sh`** (sources the shipped `release-gate-lib.sh` — same bytes the workflow runs):
- Total: 13 — Passed: 13 — Failed: 0 (exit 0)
- Truth table proves only `(RC=0, marker present)` is green; `exit 1`/`exit 3`/`exit 2`/`exit 139` all RED with cause-specific `::error::`; RC survives capture by execution (1→1, 3→3); stderr captured via `2>&1`; marker anchoring rejects substring spoofs.

**T2 — `release-tag-parity-test.sh`** (byte-identity of resolved tag vs metadata-action push pattern):
- Total: 13 — Passed: 13 — Failed: 0 (exit 0)
- push keeps un-stripped `v` (`v1.2.3-<arch>` == `pattern=v{{version}}-<arch>`); dispatch `latest-<arch>` == `value=latest-<arch>`; per-arch suffix no-swap; **discrimination self-checks confirm a re-introduced `${...#v}` strip, a suffix swap, or an extra `v` all go RED** — the OCCURRED defect cannot recur silently.

**`bash -n` syntax** on all edited/new scripts: 5/5 OK (`docker-http-posture-smoke.sh`, `release-gate-lib.sh`, `release-gate-logic-test.sh`, `release-tag-parity-test.sh`, `fixtures/stub-smoke.sh`).

**`release.yml`** static: YAML parse OK; needs-graph + trigger-surface assertions all PASS (R-06/R-07/R-08/R-10/R-11/AC-08 — see Coverage Summary).

**Shell/static subtotal: 26 named assertions passed (13 + 13), 0 failed; plus 5 `bash -n` + ~12 release.yml static assertions all green.**

### Docker HTTP-posture Smoke (AC-05 — full end-to-end, `IMAGE=` set)

Production image `unimatrix:nan019-smoke` built locally (ENV `UNIMATRIX_HTTP_ENABLED=true` baked), `IMAGE=` set so the smoke reuses pushed-style bytes (skips its build branch). Run **5×**:

| Run | rc | marker last | per-slug grew | hash unchanged |
|-----|----|-----|----|----|
| 1 | 0 | YES | 356 → 372 | 412 |
| 2 | 0 | YES | 356 → 372 | 412 |
| 3 | 0 | YES | 356 → 372 | 412 |
| 4 | 0 | YES | 356 → 372 | 412 |
| 5 | 0 | YES | 356 → 372 | 412 |

- Total: 5 — Passed: 5 — Failed: 0.
- **grew-signal monotone and stable** (WAL-inclusive `du -s` over the per-slug store dir): +16 blocks every run; hash store byte-stable at 412 — no flake (R-04, un-retryable OQ-6 satisfied).
- **Negative-control discrimination** (mirror of shipped assertion lines 164–167): a #783-style mis-route (slug unchanged, hash grew) FAILS the grew check; a leak (both grew) FAILS the hash-unchanged check. The assertion is discriminating, not theater.
- Terminal `[783-smoke] ALL GATES PASSED …` prints LAST every run; assertion pair sits before it (R-12).

### Integration Tests (infra-001 — regression baseline only)

Per OVERVIEW.md Integration Harness Plan: feature touches **no MCP-visible behavior**, so no Python suite is in scope and no new Python test is added. The one mandatory `pytest -m smoke` run is a **regression baseline** proving the AC-05 edit to `docker-http-posture-smoke.sh` perturbed nothing MCP-visible.

- `pytest suites/ -m smoke --timeout=60`: **Total 24 — Passed 24 — Failed 0** (382 deselected; 207.99s).
- No failures → no triage, no `xfail`, no GH Issue. The AC-05 shell edit is MCP-invisible as designed.

## Gaps

**None for the pre-merge-provable core.** Every Critical/High risk that the OVERVIEW marks "PROVABLE PRE-MERGE" is green:
- R-01 / R-02 / R-03 (gate-logic truth table + RC-survives-capture + marker anchoring) — PASS.
- R-09 (tag-parity byte-identity, the OCCURRED defect) — PASS, with discrimination self-checks.
- R-04 (grew-signal monotonicity + discrimination) — PASS (5× monotone, negative-control discriminates).
- R-06 / R-08 / R-11 (needs-graph topology, dispatch gate, trigger surface) — PASS.
- R-07 / R-10 (config: `IMAGE=` set, no production build, push→smoke ordering) — PASS.
- R-12 / R-13(amd64) (marker-last, sidecar, amd64 baseline) — PASS.

## PENDING — post-tag / post-dispatch (configured + verified locally; GH execution confirmed post-tag — NOT gaps)

These cannot be proven by local Linux validation (#4796) and are correctly deferred by the test plan. Per the OVERVIEW's instruction to Gate 3c, their PENDING status is **NOT a coverage gap**:

- **AC-07** — both `smoke-amd64` + `smoke-arm64` actually run green on the hosted runners and the manifest publishes on the first real `v*` release. (process, post-tag)
- **R-05** — arm64 cold first-boot wall-time vs the 90s deadline margin. First true signal on the `workflow_dispatch` dry-run against `:latest-arm64`; confirmed on the first tag. Grew-monotonicity on arm64 image confirmed locally during impl + Stage 3c (smoke ×5 on aarch64 host); the *named hosted `smoke-arm64` job* margin is post-dispatch.
- **R-08 behavior** — a red smoke leaving the manifest **skipped** (not run) on a `v*` push; a `workflow_dispatch` run showing the manifest **green-skipped**, run reducing to the two `smoke-*` statuses. (config PASS; behavior post-tag/dispatch)
- **R-07 log** — "using prebuilt image: ghcr.io/...:v<version>-<arch>" in the real hosted smoke log. (config PASS; log post-tag)
- **R-10 race** — first-try pull success after `--push` propagation; any race is in-scope structural rework, never `|| retry`.
- **R-13 arm64** — arm64 is a never-before-run path for the *hosted* `smoke-arm64` job; first run watched as discovery, surprises are in-scope rework.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS (config) / PENDING (execution) | `on:` has `push.tags:['v*']` + `workflow_dispatch`, excludes `pull_request`; `smoke-amd64` (`ubuntu-22.04`) + `smoke-arm64` (`ubuntu-22.04-arm`) both invoke `docker-http-posture-smoke.sh`. Hosted both-arch execution confirmed post-tag (AC-07). |
| AC-02 | PASS (config) / PENDING (skip behavior) | `create-container-manifest.needs: [smoke-amd64, smoke-arm64]`; no `continue-on-error`/retry. Gate-logic test proves a `fail()` `exit 1` → job RED. A red smoke leaving the manifest skipped is post-tag. |
| AC-03 | **PASS** | `release-gate-logic-test.sh` truth table: `(0,marker)`→green; `exit 1`→red "first-run path is broken"; `exit 3`→red "mis-provisioned … HARD failure"; early-`exit 0`→red "never printed ALL GATES PASSED"; unexpected→red. Only `(0, marker present)` green. RC verified by execution (R-02). |
| AC-04 | **PASS** | Static `needs:`-graph: no `smoke-*` in any `build-linux-*`/`package-npm`/`create-release` `needs`; no binary/npm in any `smoke-*` `needs`; `create-release needs: package-npm` only; single manifest block point. |
| AC-05 | **PASS** | Assertion pair in `docker-http-posture-smoke.sh` uses `vol()` `:ro` busybox (no executable `docker exec`), WAL-robust `du -s` signal, asserts per-slug grew + hash unchanged via `fail()`, placed before the terminal marker. Smoke ×5: signal monotone (356→372), marker still last; negative-control mis-route FAILS the assertion (discriminating). |
| AC-06 | **PASS (pre-merge parity)** / PENDING (log) | `release-tag-parity-test.sh` (13/13): resolved tag byte-identical to push pattern — push un-stripped `:v<version>-<arch>` (== `pattern=v{{version}}-<arch>`), dispatch `:latest-<arch>` (== `value=latest-<arch>`); `${...#v}` strip / suffix swap / extra `v` all RED. Smoke jobs `docker login`, set `IMAGE=` via `resolve_image`, no production build. "using prebuilt image" log post-tag. |
| AC-07 | **PENDING-post-tag** | First real `v*` run watched to completion on both arches; both smokes green + manifest publishes. Verifiable ONLY post-tag — cannot be proven by local Linux validation (#4796). NOT a gap. |
| AC-08 | **PASS (config)** / PENDING (dispatch run) | `create-container-manifest` carries `if: github.event_name != 'workflow_dispatch'` and keeps `needs: [smoke-amd64, smoke-arm64]` for the push path. Manifest green-skip on a dispatch run confirmed post-dispatch. |

## GH Issues Filed

None. No integration-test failure and no pre-existing/unrelated breakage was surfaced; the `pytest -m smoke` baseline was fully green, so no `xfail` and no GH Issue were warranted.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #5192 (CI/release shell-gate shipped-in-release.yml AND unit-tested pattern, this feature's spine), ADR-002 #5187 (test pushed GHCR bytes), ADR-003 #5183 (verify-by-name contract), #4873 (setsid swallows RC — false-green class, R-02), #329 (WAL autocheckpoint — main DB size not monotone, R-04). Applied directly to execution.
- Stored: nothing novel to store — the verify-by-name shell-gate pattern (#5192/#5180), the RC-swallow false-green trap (#4873), and the WAL-non-monotone grew-signal gotcha (#329) already capture the cross-feature lessons; nan-019's results are feature-specific and live in this report. No new fixture or harness technique was discovered (the gate-logic/tag-parity test surface follows the existing infra-001 `scripts/` convention cumulatively).
