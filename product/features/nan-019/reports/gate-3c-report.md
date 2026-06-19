# Gate 3c Report: nan-019

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-19
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | Every pre-merge-provable risk (R-01/02/03/04/06/09/11/12, R-07/08/10/13-config) maps to a passing test; post-tag items honestly PENDING |
| Test coverage completeness | PASS | All Phase-2 scenarios provable pre-merge are exercised; the 6 deferred items are legitimately post-tag/post-dispatch, not silent gaps |
| Specification compliance | PASS | FR-01..FR-11 implemented; AC-01..AC-08 verified to their pre-merge-provable extent; un-stripped tag contract honored |
| Architecture compliance | PASS | Job topology matches ADR-001..005; pinned run-marker shape + un-stripped resolve_image implemented verbatim; ADR-004 independence preserved |
| Integration test validation | PASS | pytest -m smoke 24/24 baseline; no-Python-suite determination sound; no xfail/deleted tests; RISK-COVERAGE-REPORT carries integration counts |
| Knowledge stewardship | PASS | Tester report has complete `## Knowledge Stewardship` block (Queried + Stored-with-reason) |

I independently re-ran both pre-merge HARD gates: `release-gate-logic-test.sh` (13/13, rc=0) and `release-tag-parity-test.sh` (13/13, rc=0). Both green.

## Detailed Findings

### Risk mitigation proof
**Status**: PASS
**Evidence**:
- **R-01 (gate logic wrong & untested)** — `release-gate-logic-test.sh` sources the SHIPPED `release-gate-lib.sh` (the same bytes `release.yml` smoke jobs `source` at lines 419/440) and drives `run_smoke_gate` against a controllable stub. Full truth table {0,1,3,early-0,2,139}×{marker}; only `(0, marker present)` is green. Re-run independently: 13/13.
- **R-02 (RC swallowed before read, #4873 class)** — `rc_survives 1→1`, `rc_survives 3→3` proven *by execution* through the exact `set +e; out="$(... 2>&1)"; rc=$?` capture shape (no pipe between smoke and `$?`); stderr-capture path (`STUB_STREAM=stderr`) confirmed both for green-marker and exit-1 fail. PASS.
- **R-03 (early-exit-0, marker absent passes green)** — anchored `grep -qx '\[783-smoke\] ALL GATES PASSED.*'`; substring-on-same-line spoof correctly RED; whole-line-anywhere correctly green (documented lib behavior, not a false expectation); byte-identity cross-check binds the asserted marker to the smoke's real `log "ALL GATES PASSED ..."` emission (script line 170). PASS.
- **R-04 (AC-05 grew-signal flaky/un-discriminating, un-retryable)** — WAL-inclusive `du -s` over the store DIR (not main-file size — correctly avoids the ADR #329 non-monotone trap); smoke ×5 monotone (356→372 every run, hash 412 byte-stable); negative-control discrimination (mis-route → grew-check FAIL; leak → hash-unchanged FAIL). The assertion pair sits before the terminal marker (lines 164–168 precede line 170). PASS.
- **R-06 (ADR-004 independence regression)** — independently traced `needs:` graph: binary/npm needs are `[build-linux-x64, build-linux-arm64]` (line 204) and `package-npm` (line 276); the ONLY `needs:[smoke-*]` edge is on `create-container-manifest` (line 446). Zero cross-branch edge; single manifest block point. PASS.
- **R-08 (manifest not gated)** — `create-container-manifest needs: [smoke-amd64, smoke-arm64]`; no `continue-on-error:` key anywhere (the only "continue-on-error" string is a comment asserting its absence, line 400). PASS (config); red-smoke→skip behavior correctly PENDING-post-tag.
- **R-09 (tag-resolution mismatch — the OCCURRED defect)** — `release-tag-parity-test.sh` derives the BUILD side independently by reading `release.yml`'s `type=semver,pattern=v{{version}}-<arch>` / `type=raw,value=latest-<arch>` patterns and modeling metadata-action semantics, then asserts byte-identity to the SHIPPED `resolve_image`. Push keeps the `v` (`v1.2.3-<arch>`), dispatch `latest-<arch>`, no suffix swap; three discrimination self-checks (re-introduced `${...#v}` strip, suffix swap, extra-v) all go RED — the assertion is non-vacuous. Re-run independently: 13/13. `resolve_image` (lib lines 23–32) resolves un-stripped (`tag="${ref_name}"`, NEVER `${ref_name#v}`). PASS pre-merge.
- **R-11 (trigger surface)** — `on:` is `push.tags:['v*']` + `workflow_dispatch:{}`, excludes `pull_request`. PASS.
- **R-12 (AC-05 hardening regresses marker)** — marker still LAST every run of smoke ×5; AC-05 uses `vol()` `:ro` busybox (line 44), never `docker exec`; bounded single assertion pair. PASS.

### Test coverage completeness
**Status**: PASS
**Evidence**: Every Critical/High risk the strategy marks "PROVABLE PRE-MERGE" is green. The deferred items are honestly listed as PENDING-post-tag/post-dispatch and each is legitimately un-provable by local Linux validation (#4796), NOT a silent cap:
- **AC-07** — hosted both-arch green + manifest publish (process, post-tag only).
- **R-05** — arm64 cold-boot wall-time vs the 90s deadline (first signal on the `workflow_dispatch` dry-run / first tag; named hosted `smoke-arm64` job).
- **R-07 log** — "using prebuilt image" line in the real hosted smoke log (config PASS; log post-tag).
- **R-08 behavior** — red-smoke→manifest-skipped + dispatch green-skip (config PASS; behavior post-tag).
- **R-10 race** — first-try pull after `--push` propagation.
- **R-13 arm64** — never-before-run hosted arm64 path, watched as discovery.

Each is phrased "configured + verified locally; GH execution confirmed post-tag," never asserted as executed. This matches the strategy's validation phasing and the OVERVIEW instruction that PENDING-post-tag is not a coverage gap. R-14 is accepted-by-design (NFR-09), correctly N/A.

### Specification compliance
**Status**: PASS
**Evidence**: FR-01 (two per-arch jobs on the named runners), FR-02 (trigger surface), FR-03/FR-11 (pushed-bytes via `IMAGE=`, un-stripped tag, pre-merge parity), FR-04 (smoke behavior inherited, not re-implemented in YAML), FR-05 (AC-05 grew-assertion via sidecar), FR-06 (manifest gating on both smokes), FR-07 (skip-is-failure exit-code keying), FR-08 (positive run-marker), FR-09 (no retry/continue-on-error), FR-10 (dispatch manifest gating via `if:`) all implemented and verified to their pre-merge extent. AC-03/04/05/06 PASS pre-merge; AC-01/02/08 PASS at config level (execution post-tag); AC-07 correctly PENDING-post-tag.

### Architecture compliance
**Status**: PASS
**Evidence**: Job topology matches the ADR-001 diagram exactly (`smoke-amd64 needs:[build-container-x64]` line 404, `smoke-arm64 needs:[build-container-arm64]` line 425, `create-container-manifest needs:[smoke-amd64,smoke-arm64]` + `if: github.event_name != 'workflow_dispatch'` lines 446–447). The pinned ADR-003 run-marker capture shape is implemented verbatim in `run_smoke_gate`. ADR-002 pushed-bytes-not-rebuild honored (smoke reuses `IMAGE=`, no production build in smoke jobs). ADR-004 un-stripped tag resolution + dispatch gating implemented. ADR-005 grew-assertion via busybox sidecar. No architectural drift.

### Integration test validation (mandatory)
**Status**: PASS
**Evidence**:
- **pytest -m smoke baseline** — report records 24/24 passed, 382 deselected (207.99s). The AC-05 edit to `docker-http-posture-smoke.sh` broke nothing MCP-visible.
- **No-Python-suite determination is SOUND, not an evasion** — independently confirmed the committed diff touches ZERO crate/MCP-server code: changed files are `.github/workflows/release.yml` + 5 shell files under `infra-001/scripts/` only. A CI/release-workflow + shell feature has no MCP-visible surface, so the infra-001 Python SUITE legitimately does not apply; the smoke run is correctly a regression baseline.
- **xfail hygiene** — no `xfail` markers added on this branch (`git diff` over `*.py` clean); none warranted because the baseline was fully green (no real failure masked).
- **No integration tests deleted/commented** — no `def test_` removed or commented across `product/test/` on this branch.
- **RISK-COVERAGE-REPORT integration counts** — includes pytest 24/24 plus the shell gate suites (13+13 = 26 named assertions, smoke ×5, ~12 release.yml static assertions, 5 `bash -n`).

### Knowledge stewardship compliance
**Status**: PASS
**Evidence**: `nan-019-agent-4-tester-report.md` contains a `## Knowledge Stewardship` section with a `Queried:` entry (`context_briefing` surfacing #5192, ADR-002 #5187, ADR-003 #5183, #4873, #329) and a `Stored:` entry with an explicit "nothing novel" reason (cross-feature lessons already captured by #5192/#5180/#4873/#329; tests follow the existing infra-001 convention cumulatively). Complete, reason present — PASS (no WARN).

## Rework Required
None.

## Scope Concerns
None.
