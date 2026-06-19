# nan-019 — Implementation Brief

> **NFR-maintenance feature.** Advances capability **N5** (#5163 — "the shipped artifact is always deployable as released") and guards **N3** (#5161 — writes never mis-routed across projects). Wires the existing `docker-http-posture-smoke.sh` (#786) into `.github/workflows/release.yml` as a standing, **verify-by-name / skip-is-failure** gate that blocks the multi-arch manifest on failure. **This is a CI/release-workflow + shell-smoke feature — not a Rust crate change.** No new runtime behavior is added to the shipped artifact.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-019/SCOPE.md |
| Scope Risk Assessment | product/features/nan-019/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nan-019/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-019/specification/SPECIFICATION.md |
| Risk / Test Strategy | product/features/nan-019/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-019/ALIGNMENT-REPORT.md |
| ACCEPTANCE-MAP | product/features/nan-019/ACCEPTANCE-MAP.md |

## Component Map

This feature touches three "components": two new `release.yml` jobs and one bounded edit to the existing smoke script. There are no Rust crate boundaries. Pseudocode and test-plan files are produced in Session 2 Stage 3a; paths below are expected slots filled during delivery.

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `smoke-amd64` (release.yml job, `ubuntu-22.04`) | pseudocode/smoke-amd64.md | test-plan/smoke-amd64.md |
| `smoke-arm64` (release.yml job, `ubuntu-22.04-arm`) | pseudocode/smoke-arm64.md | test-plan/smoke-arm64.md |
| `create-container-manifest` (release.yml job, `needs:` rewire) | pseudocode/create-container-manifest.md | test-plan/create-container-manifest.md |
| `docker-http-posture-smoke.sh` (AC-05 grew-assertion) | pseudocode/docker-http-posture-smoke.md | test-plan/docker-http-posture-smoke.md |

> Note: `smoke-amd64` and `smoke-arm64` are near-identical (same gate logic, differ only in runner + arch tag suffix). A single shared pseudocode/test-plan covering both jobs is acceptable; the table lists them separately so neither arch is silently dropped (NFR-06 HARD RULE).

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Every release of the GHCR multi-arch container image is exercised end-to-end against the **actual pushed per-arch bytes** on **both** architectures (amd64 + arm64) before the multi-arch manifest operators pull is released. The gate runs the existing smoke (reuse-pushed-bytes → boot-HTTP-on → register slug → restart → per-slug HTTPS write → assert-landed) and is wired so a smoke failure, a self-skip (`exit 3`), or a silent early-`exit 0` all block the manifest loudly. This closes the shipped-but-broken-on-first-run class (#774, #783) on both arches and kills the false-green class (#4796/#4970) by verifying the smoke provably ran to its terminal `ALL GATES PASSED` line.

## Resolved Decisions

| Decision | Resolution | Source | ADR |
|----------|-----------|--------|-----|
| OQ-1: where the `needs` edge lands to gate the release without violating ADR-004 | Gate the **manifest** — `create-container-manifest needs: [smoke-amd64, smoke-arm64]`; each smoke `needs:` only its own-arch build; no edge into binary/npm branch | ARCHITECTURE §Component Interactions | ADR-001 (#5186) — architecture/ADR-001-smoke-job-topology-and-manifest-gate.md |
| OQ-2: test pushed bytes vs. rebuild | Test the **pushed** GHCR per-arch bytes via `IMAGE=ghcr.io/<owner>/unimatrix:<resolved-tag>-<arch>` after `docker login`; **self-build rejected**; no duplicate production build in smoke job | SCOPE OQ-2 (DECIDED) | ADR-002 (#5187) — architecture/ADR-002-test-pushed-ghcr-bytes.md |
| SR-01 / AC-03: verify-by-name contract | Capture exit code with `set +e; RC=$?; set -e`; branch on `0`/`1`/`3`/other (only `0` continues); **plus** assert anchored run-marker `[783-smoke] ALL GATES PASSED`; no retry, no `continue-on-error` | ARCHITECTURE §Run-marker capture pattern | ADR-003 (#5183) — architecture/ADR-003-verify-by-name-contract.md |
| OQ-5: trigger surface + dispatch tag resolution + dispatch manifest gating | Add `workflow_dispatch` to `on:` (alongside `push.tags: ['v*']`); **NOT** `pull_request`. Tag resolution (UN-stripped — keeps the `v`): push → `VERSION="${GITHUB_REF_NAME}"` ⇒ `:${VERSION}-<arch>` = `:v<version>-<arch>`; dispatch → `:latest-<arch>`. Manifest gated OFF on dispatch via `if: github.event_name != 'workflow_dispatch'` (FR-10) | ARCHITECTURE §Integration Surface | ADR-004 (#5188) — architecture/ADR-004-workflow-dispatch-and-tag-resolution.md |
| OQ-4 / AC-05: grew-assertion mechanics | Extend smoke to assert per-slug store **grew** and hash store **did not** via the read-only busybox `vol()` sidecar; WAL-robust size signal; marker stays last | SCOPE OQ-4 (DECIDED) | ADR-005 (#5185) — architecture/ADR-005-ac05-grew-assertion-integration-surface.md |
| FR-11 / AC-06: pre-merge tag-parity gate | A bounded static parity assertion in the local gate-logic test surface proves the smoke's resolved per-arch tag is **byte-identical** to the metadata-action push pattern (push `v<version>-<arch>` vs `pattern=v{{version}}-<arch>`; dispatch `latest-<arch>` vs `value=latest-<arch>`) — RED at merge on any value mismatch (e.g. a stray `${...#v}` strip); no tag push or post-tag run required | ARCHITECTURE §Validation Strategy (1a) | ADR-002 (#5187) / ADR-004 (#5188) |
| OQ-6: retry on flake | **No silent retry** — no `|| retry`, retry loop, or `continue-on-error`; a flaky deployability gate is itself a signal | SCOPE OQ-6 (DECIDED) | ADR-003 (#5183) |

## Files to Create / Modify

| Path | Change |
|------|--------|
| `.github/workflows/release.yml` | Add `workflow_dispatch: {}` to `on:`; add `smoke-amd64` (ubuntu-22.04) and `smoke-arm64` (ubuntu-22.04-arm) jobs to the container branch; change `create-container-manifest.needs` from `[build-container-x64, build-container-arm64]` to `[smoke-amd64, smoke-arm64]` AND add `if: github.event_name != 'workflow_dispatch'` to that job (FR-10, dispatch green-skip). Only edited workflow. |
| `product/test/infra-001/scripts/docker-http-posture-smoke.sh` | One bounded edit (AC-05 / ADR-005): assert per-slug DB grew and hash store did not, via the existing `vol()` busybox sidecar, placed **before** the terminal `ALL GATES PASSED` marker. No new scripts; no logic duplicated in YAML. |
| (test) `product/test/infra-001/...` (location TBD by tester) | Stub-smoke unit test for the gate's exit-code `case` + run-marker grep — the truth table {0,1,3,early-0,unexpected} × {marker present/absent}. MUST exist before merge (R-01). Extend existing `infra-001` infra cumulatively. |
| (test) `product/test/infra-001/...` (location TBD by tester) | **Pre-merge tag-parity assertion (FR-11):** a bounded static check that the smoke's resolved per-arch tag string is byte-identical to the metadata-action push pattern — push `v<version>-<arch>` vs `pattern=v{{version}}-<arch>`; dispatch `latest-<arch>` vs `value=latest-<arch>`. RED at merge on any mismatch (R-09). No new framework; the two-surface formula-equality check in the existing gate-logic test surface. |

## Data Structures / Contracts

This is a workflow + shell feature; "data structures" are the fixed contract strings the gate keys on. Use **verbatim** (ARCHITECTURE §Integration Surface):

| Contract | Value |
|----------|-------|
| Smoke entrypoint | `bash product/test/infra-001/scripts/docker-http-posture-smoke.sh` |
| Reuse-pushed-image input | env `IMAGE=ghcr.io/<owner>/unimatrix:<resolved-tag>-<arch>` (set ⇒ smoke skips its build branch) |
| Smoke exit contract | `0` = ran+passed · `1` = ran+failed (`fail()`) · `3` = self-skipped (Docker absent) |
| Positive run-marker (load-bearing) | terminal line **`[783-smoke] ALL GATES PASSED`** (printed only after all gates incl. AC-05) |
| HTTP-on proof (inside smoke) | daemon log `HTTP transport active`; HTTP-off hint `set [http] enabled` |
| Per-slug route (inside smoke) | `POST https://localhost:18443/v1/<slug>/observe` expects `204` |
| Per-slug store path | `/data/.unimatrix/<slug>/unimatrix.db` (`SLUG=arch-research`) |
| Hash store dir | `HASH_DIR` (discovered; holds `token`, `tls/cert.pem`, `unimatrix.db`) |
| Read-only volume inspector | `vol() { docker run --rm -v "$VOL:/data:ro" busybox "$@"; }` (AC-05 must use this; never `docker exec`) |
| Per-arch pushed tags (UN-stripped — keeps the `v`) | `:v<version>-amd64` / `:v<version>-arm64`, `:latest-<arch>` (from `type=semver,pattern=v{{version}}-<arch>` + `type=raw,value=latest-<arch>`). The pattern's literal `v` is retained: tag `v1.2.3` ⇒ pushed `:v1.2.3-amd64` (NOT `:1.2.3-amd64`) |
| Image owner | `ghcr.io/${{ github.repository_owner }}/unimatrix` |
| GHCR login | `docker/login-action@v3`, `registry: ghcr.io`, `username: ${{ github.actor }}`, `password: ${{ secrets.GITHUB_TOKEN }}` |
| Tag resolution (push) — UN-stripped | `VERSION="${GITHUB_REF_NAME}"` (KEEP the `v`) ⇒ `:${VERSION}-<arch>` = `:v<version>-<arch>` — byte-identical to what `build-container-*` pushes (`pattern=v{{version}}-<arch>`) and what `create-container-manifest` consumes (`version=${GITHUB_REF_NAME}`). NEVER `${GITHUB_REF_NAME#v}` (a stripped `:1.2.3-<arch>` was never pushed → 404 on every release) |
| Tag resolution (dispatch) | `TAG="latest"` ⇒ `:latest-<arch>` (from `type=raw,value=latest-<arch>`, pushed verbatim) |
| Manifest gate (changed) | `create-container-manifest.needs: [smoke-amd64, smoke-arm64]` PLUS `if: github.event_name != 'workflow_dispatch'` (dispatch dry-runs green-skip the manifest; only the `smoke-*` statuses carry signal — FR-10) |
| Manifest pushed tags | `:v<version>` and `:latest` (multi-arch index; `version=${GITHUB_REF_NAME}` un-stripped) |
| Runners | `smoke-amd64` → `ubuntu-22.04`; `smoke-arm64` → `ubuntu-22.04-arm` |

## Key Interfaces (the pinned run-marker capture pattern — ADR-003)

The gate step in each smoke job MUST use this exact shape — the exit code must survive `set -e`/pipefail and no YAML `if:`/`continue-on-error` may re-green a non-zero RC:

```bash
set +e
OUT="$(IMAGE="$IMAGE" bash product/test/infra-001/scripts/docker-http-posture-smoke.sh 2>&1)"
RC=$?
set -e
echo "$OUT"
case "$RC" in
  0) : ;;
  3) echo "::error::smoke SKIPPED (exit 3): Docker-capable lane mis-provisioned — HARD failure (SR-01)."; exit 1 ;;
  1) echo "::error::smoke FAILED (exit 1): shipped image first-run path is broken."; exit 1 ;;
  *) echo "::error::smoke exited unexpectedly (exit $RC)."; exit 1 ;;
esac
echo "$OUT" | grep -qx '\[783-smoke\] ALL GATES PASSED.*' \
  || { echo "::error::smoke exited 0 but never printed ALL GATES PASSED — early-exit-0 (SR-01)."; exit 1; }
```

Green iff `RC == 0` AND the anchored marker line was captured. No retry.

### AC-05 grew-assertion surface (ADR-005)

Around the per-slug `204` write, sample a **WAL-robust, non-decreasing** size signal via `vol()` for both stores (`SLUG_BEFORE`/`SLUG_AFTER` for `/data/.unimatrix/<slug>/unimatrix.db`; `HASH_BEFORE`/`HASH_AFTER` for `$HASH_DIR/unimatrix.db`). "Before" = after register+restart, before the POST; "after" = after `204`. Assert with the existing `fail()`:
- per-slug **grew**: `SLUG_AFTER > SLUG_BEFORE` else `fail`.
- hash store **unchanged**: `HASH_AFTER == HASH_BEFORE` else `fail` (the #783 mis-route symptom).

**Signal choice (R-04 / ADR #329):** the main `.db` file size is NOT monotone on a single small committed write under WAL (autocheckpoint ~1000 pages). Use a WAL-inclusive signal (`du -s` over the per-slug store dir, or sum of `unimatrix.db` + `-wal` + `-shm`) validated monotone over ≥5 runs. A flaky signal cannot be retried away (OQ-6).

### Pre-merge tag-parity assertion (FR-11 / R-09 — primary coverage for the OCCURRED defect)

The first design draft resolved the push tag **stripped** (`${GITHUB_REF_NAME#v}` ⇒ `:1.2.3-<arch>`) while `build-container-*` pushes **un-stripped** (`pattern=v{{version}}-<arch>` ⇒ `:v1.2.3-<arch>`) — a guaranteed `docker pull` 404 on every release. R-09 is raised to **High** because it materialized, not hypothetical. The primary mitigation is now a **bounded, pre-merge, static** parity check in the existing local gate-logic test surface (NOT a new framework):

- **push surface:** the smoke's resolved tag `:v<version>-<arch>` (from `VERSION="${GITHUB_REF_NAME}"`, un-stripped) MUST equal the metadata-action `pattern=v{{version}}-<arch>` output. Example: `GITHUB_REF_NAME=v1.2.3`, `ARCH=amd64` ⇒ resolved `v1.2.3-amd64` == build-pushed `v1.2.3-amd64`.
- **dispatch surface:** the smoke's resolved tag `:latest-<arch>` MUST equal `type=raw,value=latest-<arch>`.
- **per-arch suffix:** `smoke-amd64`→`-amd64`, `smoke-arm64`→`-arm64` — no swapped suffix (same byte-identity assertion).

Any divergence (a stray `${...#v}` strip, a missing/extra `v`, a swapped suffix) turns the local assertion **RED at merge** — no tag push or post-tag run required. This converts the post-tag surprise into a pre-merge gate (NFR-10, C-13).

## Constraints

- **C-01 Verify-by-name, not by green suite (load-bearing).** Exit-code keyed (`0`/`1`/`3`) AND run-marker asserted. Getting this wrong re-creates the false-green class the feature exists to kill. (SR-01)
- **C-02 Arch coverage named — no silent caps (HARD RULE).** Both arches smoked; any deferral is a NAMED, TRACKED fast-follow with N5 staying PARTIAL — never buried under a green check. (SR-07)
- **C-03 Pushed bytes only — self-build forbidden.** `IMAGE=` GHCR per-arch tag; never a rebuild on the smoke runner. The resolved tag keeps the `v` on a `v*` push (`:v<version>-<arch>`, un-stripped) and is `:latest-<arch>` on dispatch — byte-identical to the build's pushed tag. (OQ-2)
- **C-13 Tag parity is pre-merge-provable.** A bounded static parity assertion (FR-11) proves the smoke's resolved tag equals the metadata-action push tag for both surfaces, RED at merge time on mismatch — no post-tag round-trip to discover a tag-string defect. (SR-01, R-09)
- **C-14 Dispatch manifest gating.** `create-container-manifest` carries `if: github.event_name != 'workflow_dispatch'`; on a dispatch dry-run only the `smoke-*` statuses signal (the manifest green-skips rather than going falsely red). (FR-10, NFR-11)
- **C-04 No silent retry.** No `|| retry`, retry loop, or `continue-on-error` on the smoke. (OQ-6)
- **C-05 Gate the manifest, not the per-arch pushes.** Block lands on `create-container-manifest`. (OQ-1)
- **C-06 ADR-004 container independence (#4572).** No `needs` edge between the container branch and the binary/npm branch; `create-release` stays independent.
- **C-07 No duplicate image build / runner cost.** Reuse pushed bytes; do not rebuild the ONNX-bearing image.
- **C-08 Docker availability is the gate's whole point.** `exit 3` is a hard failure; never tolerate the skip; do not assume hosted runners always ship Docker.
- **C-09 Distroless / busybox sidecar.** AC-05 uses read-only `vol()`; never `docker exec` into the shell-less runtime image.
- **C-10 No new secrets.** GHCR read via existing `GITHUB_TOKEN` / `packages: write` only.
- **C-11 Release runs on tag push → first execution post-merge.** Gate cannot be proven by local Linux validation; budget a post-tag CI round-trip (#4796).
- **C-12 Test infrastructure is cumulative.** Wire + extend the existing `infra-001` smoke (AC-05 only); no parallel script; no smoke logic re-implemented in YAML.

## Dependencies

- **`product/test/infra-001/scripts/docker-http-posture-smoke.sh`** (#786) — the smoke wired by this feature; only sanctioned change is AC-05. Its exit contract (0/1/3) and run-marker are load-bearing.
- **`.github/workflows/release.yml` container branch** (`build-container-x64`, `build-container-arm64`, `create-container-manifest`, ~lines 327–428) — host for the new smoke jobs and the gating `needs` edge.
- **GHCR + `secrets.GITHUB_TOKEN`** — `docker login` to pull pushed per-arch images; `packages: write` already granted. No new secret.
- **Hosted runners `ubuntu-22.04` + `ubuntu-22.04-arm`** — ship Docker preinstalled; gate must not assume this is eternal (the `exit 3` guard catches a mis-provisioned lane).
- **ADR-004 (#4572)** — container-lane independence; constrains where the `needs` edge may land.
- **Pattern #5180** — the verify-by-name / skip-is-failure / run-marker pattern this gate implements.
- **Lesson #4873** — `setsid`/pipe/`pipefail` can swallow RC and silently return 0; RC propagation must be verified by execution, not by reading (R-02).
- **ADR #329** — WAL autocheckpoint means the main DB file size is not monotone on a single small write; informs the AC-05 grew-signal choice (R-04).
- **Capability N5 (#5163)** — the capability this feature flips from PARTIAL toward maintained; **N3 (#5161)** — guarded.

## NOT in Scope

- No new deploy behavior or container changes (Dockerfile, runtime posture, served routes, first-boot). If the gate reveals a real artifact defect, fixing it is a separate feature/bugfix.
- Not the functional per-slug analytics work (crt-056 / #787 = C5) — orthogonal, no dependency either direction.
- Not a new smoke or a smoke rewrite — only the bounded AC-05 grew-assertion. No new scenarios, no smoke logic duplicated in YAML.
- Not a CI-lane (`ci.yml` / `pull_request`) gate — `pull_request` is explicitly excluded (OQ-5).
- No change to the binary/npm release jobs — additive on the container branch only.
- No new secrets; no silent retry / no `continue-on-error`.

## Validation / Delivery Routing

This is a CI/release-workflow + shell feature whose gate logic runs only post-merge on a `v*` tag. Per #4796 and the `local-gates-linux-only-ci-is-crossplatform` memory, coverage is phrased **"configured + verified locally; GH execution confirmed post-tag,"** never asserted before execution.

1. **Local (pre-merge, MUST exist):** lint the workflow YAML; statically confirm the `needs:` graph (smoke→manifest, **zero** cross-branch edge into binary/npm, single manifest block point); **unit-test the exit-code `case` + run-marker grep against a stub smoke** driving the truth table {0,1,3,early-0,unexpected} × {marker present/absent} — only `(0, marker present)` is green (R-01). Empirically verify RC survives capture (`exit 1`/`exit 3` read as 1/3, not 0 — R-02, the #4873 class). Run the full smoke 3/3 locally with `IMAGE=` set; run it ≥5× post-AC-05 to confirm the grew-signal is monotone and that the marker still prints **last** (R-04, R-12).
1a. **Tag-parity static assertion (pre-merge, MUST exist — FR-11/R-09):** assert the smoke's resolved per-arch tag string is **byte-identical** to the metadata-action push pattern — push `:v<version>-<arch>` (un-stripped) vs `pattern=v{{version}}-<arch>`; dispatch `:latest-<arch>` vs `value=latest-<arch>`. RED at merge on any value mismatch (e.g. a re-introduced `${...#v}` strip). This converts the OCCURRED tag-strip defect from a post-tag discovery into a pre-merge gate; no tag push required.
2. **`workflow_dispatch` dry-run (pre-tag):** trigger the workflow manually to exercise the real hosted `ubuntu-22.04` / `ubuntu-22.04-arm` runners against `:latest-<arch>` — the first true cross-platform proof without cutting a release (primary reason dispatch is in scope). On dispatch the manifest job is **green-skipped** (`if: github.event_name != 'workflow_dispatch'`) so the run's pass/fail reduces to the two `smoke-*` job statuses; a skipped manifest is not a false-red (FR-10).
3. **First real tag (post-merge, AC-07):** the delivery leader watches the first `v*` release run to completion on **both** arches; confirm both smoke jobs run green and the manifest publishes. Any platform/runner surprise (ARM64 flake, cold ONNX model-load timeout, tag-resolution mismatch) is **in-scope rework**, never a reason to add `|| retry`.

## Alignment Status

All six alignment checks **PASS** (ALIGNMENT-REPORT.md, reviewed 2026-06-19). No VARIANCE or FAIL findings; **no items require human approval.** Vision: quality-guard for the `personal-cloud` goal (#4946) — flips N5 from PARTIAL toward maintained, guards N3, adds no new product behavior (correct posture for an NFR-maintenance feature). Every SCOPE AC-01..AC-07 maps to FR/NFR/AC and to an architecture component/edge; no scope gaps, no scope additions. The two refinements (tag resolution per trigger; WAL-robust grew-signal) are mechanics inside DECIDED OQs, not new scope.
