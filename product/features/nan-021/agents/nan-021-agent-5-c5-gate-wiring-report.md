# nan-021 C5 — Gate Wiring — Agent Report

Agent: nan-021-agent-5-c5-gate-wiring | Stage 3b, Wave 3 | Pure test-infra, zero production-code diff.

## 1. Files modified / created
- CREATED `product/test/infra-001/scripts/cloud-cycle-https-leg.sh` (60 lines) — the UNIMATRIX_HTTPS_SMOKE wrapper that wraps the smoke in run_smoke_gate.
- CREATED `product/test/infra-001/scripts/release-gate-cloud-cycle-logic-test.sh` (430 lines) — the C5 gate-spine stub-drive test (R-12).
- MODIFIED `.github/workflows/release.yml` — added job `nan-021-https-uds-parity` (release-gate Docker lane).
- MODIFIED `product/test/infra-001/scripts/release-gate-bundle-static-test.sh` — fixed `test_no_new_smoke_script` miscount.

Diff scope (AC-06): only `.github/workflows/release.yml` + `product/test/infra-001/**`. No `crates/**`, `lib/**`, `packages/**`. Confirmed via `git status --porcelain`.

## 2. Stub-drive + static-test results (foreground)
- `release-gate-cloud-cycle-logic-test.sh`: 22 passed, 0 failed (EXIT=0).
- `release-gate-bundle-static-test.sh`: 12 passed, 0 failed (was 11/1 fail on clean HEAD) (EXIT=0).
- No regression: release-gate-logic-test.sh, release-gate-bundle-logic-test.sh, release-tag-parity-test.sh all PASS.
- Off-Docker orchestrator suite (`-m "not integration"`): 10 passed (the seam Stage 3c plugs the live HTTPS leg into).
- `bash -n` + `shellcheck -S warning` clean on both new scripts. release.yml valid YAML. Both new files <500 lines.

## 3. How the lane wires orchestrator -> C2 HTTPS leg + enforces the false-green discriminator
The pytest orchestrator (ADR-001) drives the UDS leg in-process; its `run_https_leg` (C3, unmodified) shells out to `$UNIMATRIX_HTTPS_SMOKE` with the C2 contract env (`MANIFEST_PATH/RUN_TOKEN/HTTPS_VECTOR_OUT/SANDBOX`) already exported. The lane sets that env to `scripts/cloud-cycle-https-leg.sh`, which `source`s the SHIPPED `release-gate-lib.sh` and calls `run_smoke_gate "$IMAGE" bash docker-http-posture-smoke.sh`. The smoke inherits the C2 env so Gate 8 (`cloud_cycle_gates`) runs and writes `MetricVector(HTTPS)` to the out-file; `run_smoke_gate` applies the VERBATIM exit-code truth table (0=pass · 3=Docker-absent HARD fail · 4=unacquirable · 1=broken · *=unexpected) + the anchored `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'` marker. The smoke's nan-019 `docker pull || inspect || exit 4` acquisition is reused as-is. The wrapper re-authors NO gate-runner logic and `:${VAR:?}`-guards the C2 env (mis-wire = loud fail). The lane is in `release.yml` (tags `v*` + `workflow_dispatch`), NOT `ci.yml` `pull_request` (D-3). Per the ADR-005 first-green budget (#5266/#5267) it is wired as an INDEPENDENT gate, intentionally NOT in `create-container-manifest`'s `needs:` until it greens once (a never-green lane must not block every release) — documented inline for promotion after first-green.

## 4. test_no_new_smoke_script reconciliation
Found: the assertion counted `*smoke*.sh` (minus `stub-smoke.sh`) and asserted `-eq 1`. On clean HEAD there are TWO permitted smoke scripts — `docker-http-posture-smoke.sh` (#783) and `docker-embed-readiness-smoke.sh` (#767, shipped before nan-020) — so it FAILED (found 2). The R-15 intent is "no FORKED parallel smoke is silently added," not "exactly one smoke exists"; the `-eq 1` was wrong-scoped. nan-021 adds NO new `*smoke*.sh` (C2 lives in `cloud-cycle-lib.sh`, a sourced library; C5 adds `cloud-cycle-https-leg.sh` — a wrapper, not a `*smoke*.sh`), so it does not trip the counter.
Fixed (in C5's named scope per pseudocode): re-scoped to a closed allow-list `KNOWN_SMOKE_SCRIPTS=(docker-http-posture-smoke.sh docker-embed-readiness-smoke.sh)`; the on-disk smoke set must EQUAL the allow-list — a fork addition (unknown smoke) goes RED, a known smoke vanishing goes RED. Static test now 12/0 green. No GH issue needed (fix was in C5's safe scope).

## 5. Issues / blockers
None. The live cross-leg run is owned by Stage 3c / the release-gate tag lane (ADR-005 first-green budget: expect N tag rounds advancing cert -> bridge -> cycle -> review -> parity). The gate spine is proven off-Docker pre-merge so this is not the first place the logic runs.

Note (live-only seam gap, not a blocker): C2's `cloud_cycle_gates` stub seam (`SMOKE_CYCLE_CMD`) covers the bridge drive but NOT `_fire_observe_hooks` (pinned curl) or `cycle_durability_barrier` (busybox-vol du) — both live-only. The C5 logic test overrides those two helpers IN THE HARNESS (never in the shipped C2 lib) to exercise the read-back -> 8a -> 8c -> emit spine off-Docker. Flagged for C2's awareness; no modification made to C2 files.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search -- surfaced ADR-005 (#5290), nan-019 stub-drive (#5258), gate-spine extraction (#5192), verify-by-name contract (#5183/#5208). All applied verbatim.
- Stored: entry #5299 "Wrap a pytest-orchestrated HTTPS leg in run_smoke_gate via a UNIMATRIX_HTTPS_SMOKE wrapper (no C3 edit, no re-authored gate runner)" via context_store (pattern), edged Supports->#5290, Supports->#5258.
