# nan-019 Agent Report — release-workflow (smoke gates + sourceable gate spine)

**Agent:** nan-019-agent-3-release-workflow
**Scope:** Wave-1 — wire the verify-by-name / skip-is-failure smoke gates into `.github/workflows/release.yml` between the per-arch container builds and the multi-arch manifest, and extract the gate spine into a sourceable lib that both the YAML and the pre-merge test consume.
**Commit:** 6e033c5d

## Files created / modified
- `.github/workflows/release.yml` (modified)
- `product/test/infra-001/scripts/release-gate-lib.sh` (created)

## What changed (release.yml)
1. Added `workflow_dispatch: {}` to `on:` — enables the dry-run lane that exercises the gate against the `:latest-<arch>` bytes without cutting a tag.
2. Added `smoke-amd64` (`runs-on: ubuntu-22.04`, `needs: [build-container-x64]`) and `smoke-arm64` (`runs-on: ubuntu-22.04-arm`, `needs: [build-container-arm64]`). Each: checkout → GHCR login → `source` the gate lib → `resolve_image` → `run_smoke_gate` invoking the existing `docker-http-posture-smoke.sh`.
3. Re-pointed `create-container-manifest.needs` from the build jobs to `[smoke-amd64, smoke-arm64]` — the manifest now only assembles after BOTH per-arch gates pass (blocking gate, no `continue-on-error`, no retry).
4. Added `if: github.event_name != 'workflow_dispatch'` to `create-container-manifest` — the dispatch dry-run runs the gates but does NOT publish a manifest (no `:v<version>` exists on a branch ref; only the gate is being exercised).

## Decision: gate block as a sourceable lib (single source of truth, R-01)
The load-bearing gate logic lives in `release-gate-lib.sh`, not inline in the YAML. The smoke jobs `source` it; the pre-merge gate-logic stub-smoke test (`test-gate-logic-stub-smoke.md`, infra-001) sources the SAME file. Both consume identical bytes, so the tested logic cannot silently diverge from the shipped logic. This is extraction mechanism (a) from the test plan OQ.

The lib exposes two functions:
- `resolve_image OWNER EVENT_NAME REF_NAME ARCH` — echoes the per-arch GHCR ref.
- `run_smoke_gate IMAGE SMOKE_CMD...` — the ADR-003 capture-and-branch gate spine, applied **verbatim**: `set +e`; capture `out="$(IMAGE=… "$@" 2>&1)"`; `rc=$?`; `set -e`; surface the log; `case "$rc"` discriminating 0 / 3 / 1 / other; then assert the anchored terminal run-marker `[783-smoke] ALL GATES PASSED`. There is NO pipe between the smoke and `$?` (the #4873 class — a pipe would mask the smoke RC). It uses `return 1`, never `exit 1`, so a sourcing test-harness shell survives across truth-table rows; the YAML caller runs `set -euo pipefail` and exits on the returned status.

Exit-code contract honored verbatim: `0` = ran+passed; `1` = ran+failed → image first-run path broken; `3` = self-skipped (Docker absent) → HARD failure on a Docker-capable lane (skip-is-failure, SR-01 / pattern #5180); `*` = unexpected. Each branch emits a cause-specific `::error::`. Smoke invoked exactly once — no retry (OQ-6 / ADR-003 / C-04).

## Decision: UN-stripped tag resolution
`resolve_image` keeps the leading `v` on a tag push: `push` (REF_NAME=`v1.2.3`) → `ghcr.io/<owner>/unimatrix:v1.2.3-<arch>`; `workflow_dispatch` → `…:latest-<arch>`. It NEVER applies `${GITHUB_REF_NAME#v}`. This matches what the build-container jobs actually push (`type=semver,pattern=v{{version}}-<arch>` plus `latest-<arch>`) — stripping the `v` would resolve to a name that was never pushed and the gate would fail to pull. The verify-by-name guarantee is exactly that the gate pulls the bytes the build job pushed, by name.

## ADR-004 independence
The container/smoke/manifest branch carries NO `needs` link to the binary/npm/create-release jobs. The smoke gates depend only on their respective `build-container-<arch>` job; the manifest depends only on the two smoke gates. The container release path stays an independent branch — a binary-job failure does not block the container gate and vice versa.

## Local validation (actionlint unavailable in env)
`actionlint` was not present in the environment, so I validated the workflow structurally and the lib empirically:
- **YAML parse PASS** — Python `yaml.safe_load` parsed `release.yml` cleanly (well-formed, no tab/anchor issues).
- **Static needs-graph assertions PASS** — confirmed `smoke-amd64.needs == [build-container-x64]`, `smoke-arm64.needs == [build-container-arm64]`, `create-container-manifest.needs == [smoke-amd64, smoke-arm64]`, the `if: github.event_name != 'workflow_dispatch'` guard present on the manifest job, and `workflow_dispatch` present under `on:`.
- **`bash -n` PASS** — `release-gate-lib.sh` parses with no syntax errors.
- **Empirical truth-table / RC spine check PASS** — sourced the lib in a harness shell and drove `run_smoke_gate` with stub commands across the RC matrix: rc=0+marker → return 0; rc=0 no-marker → return 1 (early-exit-0); rc=1 → return 1; rc=3 → return 1 (skip-is-failure); rc=other → return 1. Because the spine uses `return` not `exit`, the harness survived all rows in one shell. `resolve_image` checked for both event types: push keeps the `v`, dispatch yields `latest`.

## Issues / blockers
- None for Wave-1. The arm64 smoke on real GHCR bytes is a never-run-locally path — it first executes on the `workflow_dispatch` dry-run / first `v*` tag (Stage 3c / post-tag), consistent with the test-plan split.

## Knowledge Stewardship
- Queried: `context_search` (decision/nan-019) + `context_get(5183)` → surfaced ADR-001 #5186, ADR-002 #5187, ADR-003 #5183, ADR-004 #5188, and pattern #5180 (self-skip → hard-fail). Findings applied: the ADR-003 capture-and-branch gate shape verbatim, the UN-stripped tag resolution, the skip-is-failure (rc=3) hard-fail from #5180, and the ADR-004 independence constraint on the container branch.
- Stored: entry **#5192** "Extract the verify-by-name gate spine into a sourceable lib so YAML and the pre-merge test share bytes" via context_store (pattern), with `Supports` edges to ADR-003 #5183 and pattern #5180 — records the single-source-of-truth extraction and the `return`-not-`exit` requirement for test-harness survival.

## Git
Per spawn instruction: did NOT run any git commands. Delivery Leader owns all git.
