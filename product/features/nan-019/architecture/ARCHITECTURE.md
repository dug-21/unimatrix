# nan-019 Architecture — Standing Release Gate for the Shipped Container Image

> NFR-maintenance feature. Advances **N5** (the shipped artifact is always
> deployable as released) and guards **N3** (writes never mis-routed across
> projects). It wires the existing `docker-http-posture-smoke.sh` (#786) into
> `.github/workflows/release.yml` as a standing, verify-by-name, skip-is-failure
> gate that blocks the multi-arch manifest. **This is not Rust application code.**
> The architecture is a YAML job-topology change plus the smoke's exit-code /
> run-marker contract — there are no crate boundaries to draw.

## System Overview

The Unimatrix release pipeline (`release.yml`) has two structurally independent
branches that both trigger on a `v*` tag push (ADR-004 / entry #4572):

- **Binary/npm branch:** `build-linux-x64` + `build-linux-arm64` → `package-npm`
  → `create-release`.
- **Container branch:** `build-container-x64` + `build-container-arm64` →
  `create-container-manifest`.

Today nothing in either branch exercises the **shipped container image's
first-run path** the way an operator would (build → boot HTTP-on → register a
slug → write over the per-slug HTTPS route → confirm it landed in the per-slug
store). #783 and #774 both shipped green because no test ran the real artifact.
The fix-for-the-class (`docker-http-posture-smoke.sh`) exists but runs only when
a human remembers.

nan-019 inserts two smoke jobs **into the container branch only**, between the
per-arch pushes and the manifest. Each smoke pulls and exercises the exact
pushed per-arch image; the manifest — the tag operators actually pull — is
released only if BOTH smokes pass. The binary/npm branch is untouched and gains
no coupling (ADR-004 preserved).

## Component Breakdown

This feature touches three "components": two are CI jobs (new), one is the
existing smoke script (one bounded edit).

| Component | Kind | Responsibility | Change |
|-----------|------|----------------|--------|
| `smoke-amd64` | release.yml job (`ubuntu-22.04`) | Pull `:<tag>-amd64`, run the smoke as a verify-by-name gate | **New** |
| `smoke-arm64` | release.yml job (`ubuntu-22.04-arm`) | Pull `:<tag>-arm64`, run the smoke as a verify-by-name gate | **New** |
| `create-container-manifest` | release.yml job | Assemble the multi-arch manifest operators pull | **`needs:` rewired** to gate on both smokes |
| `docker-http-posture-smoke.sh` | shell script (`infra-001`) | build/boot/register/write/assert the shipped image | **AC-05 grew-assertion added** (ADR-005) |
| workflow triggers | release.yml `on:` | Fire on release + human dry-run | **`workflow_dispatch` added** (ADR-004) |

## Component Interactions (Job Topology)

```
   ── Binary/npm branch (UNCHANGED, no edge to/from container branch) ──
   build-linux-x64  ─┐
   build-linux-arm64 ┴─▶ package-npm ─▶ create-release

   ──────────────── Container branch (nan-019 changes) ────────────────
   build-container-x64  ──▶ smoke-amd64 ─┐
   (push :<tag>-amd64)      (ubuntu-22.04)│
                                          ├─▶ create-container-manifest
   build-container-arm64 ─▶ smoke-arm64 ─┘     (needs: [smoke-amd64,
   (push :<tag>-arm64)      (ubuntu-22.04-arm)              smoke-arm64])
```

**Edge rationale (ADR-001):**
- `smoke-amd64 needs: [build-container-x64]`, `smoke-arm64 needs:
  [build-container-arm64]` — each smoke runs after its own-arch push so the
  pushed bytes exist to pull (ADR-002); one push, one smoke, no cross-arch
  coupling.
- `create-container-manifest needs: [smoke-amd64, smoke-arm64]` (changed from
  `needs: [build-container-x64, build-container-arm64]`) — single gate point;
  BOTH arches must pass; no released artifact is un-smoked. It also carries
  `if: github.event_name != 'workflow_dispatch'` — on a dispatch dry-run the
  build jobs push only `:latest-<arch>` (semver yields nothing off-tag), so the
  manifest's `:<branch>` assembly from `:<branch>-<arch>` would 404 and red the
  job; gating it makes the smoke-* job statuses the only meaningful dispatch
  signal (ADR-001/ADR-004).
- **No** edge crosses into `build-linux-*` / `package-npm` / `create-release`
  (ADR-004 / #4572): a smoke or ARM64-runner failure blocks only the manifest.

**Within a smoke job** (ADR-002, ADR-003, ADR-004):
1. `docker/login-action@v3` → GHCR (read scope via `GITHUB_TOKEN`; no new secret).
2. Resolve the per-arch tag for this trigger (push → `:v<version>-<arch>`,
   i.e. `${GITHUB_REF_NAME}` UN-stripped; dispatch → `:latest-<arch>`).
3. `set +e`; capture `OUT` and `RC` from
   `IMAGE=<resolved> bash .../docker-http-posture-smoke.sh 2>&1`; `set -e`.
4. Branch on `RC`: `0` → continue; `1`/`3`/other → `exit 1` with a code-specific
   `::error::` diagnostic.
5. Assert the anchored positive run-marker line `[783-smoke] ALL GATES PASSED`
   was captured; else `exit 1`. Green iff `RC==0` AND marker present. No retry.

## Technology Decisions (ADRs)

| ADR | Decision | Closes / Honors |
|-----|----------|-----------------|
| ADR-001 | Two per-arch smoke jobs gate `create-container-manifest`; binary/npm branch uncoupled | OQ-1, SR-08, AC-02/04, ADR-004 |
| ADR-002 | Smoke tests the **pushed** GHCR per-arch bytes via `IMAGE=`; no rebuild | OQ-2, SR-02, AC-06 |
| ADR-003 | Verify-by-name: exit-code discrimination (`0`/`1`/`3`) **plus** positive run-marker; no retry | SR-01, SR-04, AC-03, OQ-6 |
| ADR-004 | Add `workflow_dispatch`; resolve per-arch tag per trigger surface | OQ-5, SR-06, AC-01 |
| ADR-005 | AC-05 grew-assertion (per-slug grew, hash store did not) via busybox sidecar | OQ-4, SR-05, AC-05 |

## Integration Points

- **Host workflow:** `.github/workflows/release.yml` — the only edited workflow.
  Edits: add `workflow_dispatch` to `on:`; add `smoke-amd64` + `smoke-arm64`;
  change `create-container-manifest.needs`.
- **Smoke script:** `product/test/infra-001/scripts/docker-http-posture-smoke.sh`
  — one bounded edit (ADR-005 grew-assertion). Cumulative test infra; no new
  scripts.
- **GHCR:** pulled images `ghcr.io/<owner>/unimatrix:<tag>-<arch>` produced by
  the existing `build-container-*` jobs via `docker/metadata-action`.
- **Secrets:** none new. `GITHUB_TOKEN` (already in `permissions: packages:
  write`) provides GHCR read for the pull; the smoke reads token/cert from the
  running container's volume via busybox.

## Integration Surface

These names/contracts are fixed. Downstream agents (specification, pseudocode,
implementation) MUST use them verbatim — do not invent alternatives.

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| Smoke entrypoint | `bash product/test/infra-001/scripts/docker-http-posture-smoke.sh` | existing script |
| Reuse-pushed-image input | env `IMAGE=ghcr.io/<owner>/unimatrix:<tag>-<arch>` (set ⇒ script skips its build branch) | smoke lines 53–60 |
| Smoke exit contract | `0` = ran+passed · `1` = ran+failed (`fail()`) · `3` = self-skipped (Docker absent) | smoke lines 31, 47–51, 142 |
| Positive run-marker (load-bearing) | terminal stdout line **`[783-smoke] ALL GATES PASSED`** (only after all gates incl. AC-05) | smoke line 142 |
| HTTP-on proof (inside smoke) | daemon log `HTTP transport active`; HTTP-off hint `set [http] enabled` | smoke lines 84–90 |
| Per-slug route (inside smoke) | `POST https://localhost:18443/v1/<slug>/observe` expects `204` | smoke lines 124–135 |
| Per-slug store path | `/data/.unimatrix/<slug>/unimatrix.db` (`SLUG=arch-research`) | smoke lines 25, 138 |
| Hash store dir | `HASH_DIR` (discovered; holds `token`, `tls/cert.pem`, `unimatrix.db`) | smoke lines 96–99 |
| Read-only volume inspector | `vol() { docker run --rm -v "$VOL:/data:ro" busybox "$@"; }` (use for AC-05; never `docker exec`) | smoke line 44 |
| Per-arch pushed tags | `:v<version>-amd64` / `:v<version>-arm64` and `:latest-<arch>`, from `type=semver,pattern=v{{version}}-<arch>` + `type=raw,value=latest-<arch>`. The pattern's literal `v` is retained: `{{version}}` resolves to `1.2.3` for tag `v1.2.3`, so the **pushed** tag is `v1.2.3-amd64` (KEEPS the `v`) | release.yml 347–349, 382–384 |
| Image owner | `ghcr.io/${{ github.repository_owner }}/unimatrix` | release.yml 346, 381, 415 |
| GHCR login | `docker/login-action@v3`, `registry: ghcr.io`, `username: ${{ github.actor }}`, `password: ${{ secrets.GITHUB_TOKEN }}` | release.yml 335–340 |
| Tag resolution (push) | `VERSION="${GITHUB_REF_NAME}"` (UN-stripped, keeps the `v`) ⇒ image `:${VERSION}-<arch>` = `:v<version>-<arch>` — byte-identical to what `build-container-*` pushes (`pattern=v{{version}}-<arch>`) and what `create-container-manifest` consumes (`version=${GITHUB_REF_NAME}`, release.yml 410/421–423) | ADR-002/004 |
| Tag resolution (dispatch) | `TAG="latest"` ⇒ image `:latest-<arch>` (from `type=raw,value=latest-<arch>`, pushed verbatim) | ADR-004 |
| Manifest gate (changed) | `create-container-manifest.needs: [smoke-amd64, smoke-arm64]` (was `[build-container-x64, build-container-arm64]`) PLUS `if: github.event_name != 'workflow_dispatch'` (skip the manifest on dispatch dry-runs; see ADR-001/ADR-004) | release.yml 397–398 |
| Runners | `smoke-amd64` → `ubuntu-22.04`; `smoke-arm64` → `ubuntu-22.04-arm` | release.yml 328, 363 |

### Run-marker capture pattern (pinned — ADR-003)

The capture MUST preserve the exit code; this exact shape is the contract:

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

## Error Boundaries

| Origin | Symptom | Propagation |
|--------|---------|-------------|
| Docker absent on runner | smoke `exit 3` | job `exit 1` (hard fail) → manifest blocked (ADR-003) |
| Shipped image first-run broken | smoke `exit 1` (`fail()`) | job `exit 1` → manifest blocked (AC-02) |
| Future early-`exit 0` bug | `RC==0`, marker absent | run-marker assertion `exit 1` → manifest blocked (AC-03) |
| Wrong/absent pushed tag | `docker pull`/`run` fails inside smoke → `fail()`/non-zero | job `exit 1` → manifest blocked |
| ARM64 runner flake | smoke job fails | manifest blocked; **binary/npm release UNAFFECTED** (ADR-004) |
| Mis-routed write (#783 class) | AC-05 grew-assertion `fail()` → `exit 1` | job `exit 1` → manifest blocked (AC-05, guards N3) |

## Validation Strategy (the gate cannot be proven by local Linux validation)

Per the #4796 lesson and SCOPE Constraints, this gate's **first real execution
is post-merge on a `v*` tag push** — it cannot be proven green by local Linux
protocol gates. The spec must phrase coverage as **"configured + verified
locally; GH execution confirmed post-tag,"** never asserted-before-execution:

1. **Local (pre-merge):** lint the workflow YAML; statically confirm the
   `needs:` graph (smoke→manifest, no binary/npm edge); confirm the smoke runs
   3/3 locally with `IMAGE=` set (it already does); unit-test the exit-code
   `case` + run-marker grep logic against synthetic `0`/`1`/`3`/early-exit-0
   outputs. This proves the *gate logic*, not the *hosted-runner execution*.
1a. **Tag-parity static assertion (pre-merge, NEW — converts the #1 defect from
   a post-tag discovery into a pre-merge gate):** a small shell/unit assertion in
   the existing local gate-logic test surface (NOT a new framework) asserts the
   smoke's resolved per-arch tag string is **byte-identical** to what
   `build-container-*` pushes — derived from the same expression or asserting the
   resolution formula equals the metadata-action pattern:
   - push surface: `${GITHUB_REF_NAME}` (un-stripped) ⇒ `:v<version>-<arch>`,
     which must equal `pattern=v{{version}}-<arch>`'s output `v<version>-<arch>`;
   - dispatch surface: `latest` ⇒ `:latest-<arch>`, equal to
     `type=raw,value=latest-<arch>`.
   Example assertion (`GITHUB_REF_NAME=v1.2.3`, `ARCH=amd64`): resolved tag
   `v1.2.3-amd64` MUST equal the build-pushed `v1.2.3-amd64`. This makes the
   feature honor its own verify-by-name thesis: a future edit that re-strips the
   `v` (or otherwise drifts the smoke off the push) fails locally, not on the
   first release tag. Bounded: no new test harness, just the two-surface
   formula-equality check in the gate-logic test.
2. **`workflow_dispatch` dry-run (pre-tag):** trigger the workflow manually on a
   branch to exercise the real hosted `ubuntu-22.04` / `ubuntu-22.04-arm` runners
   against `:latest-<arch>` — the first true cross-platform proof, available
   without cutting a release (this is a primary reason `workflow_dispatch` is in
   scope, ADR-004).
3. **First real tag (post-merge, AC-07):** the delivery leader watches the first
   `v*` release run to completion on BOTH arches and confirms both smoke jobs run
   green and the manifest publishes. Any platform/runner surprise (ARM64 flake,
   cold ONNX model-load timeout SR-03, tag-resolution mismatch) is **in-scope
   rework**, not a reason to add `|| retry` (OQ-6).

## Open Questions

- **OQ-1 — resolved by ADR-001** (gate the manifest; binary/npm uncoupled). No
  residual.
- **AC-05 grew signal monotonicity (for spec/impl, ADR-005):** whether the
  shipped SQLite DB's main-file size is reliably monotone on a single committed
  write, or whether the assertion must measure `du -s` over the store dir
  (WAL-inclusive). The architecture allows either; the implementing agent must
  pick the signal that is reliably non-decreasing for the shipped DB config and
  must NOT introduce a flaky signal (a flaky gate cannot be retried away, OQ-6).
  Flag for the tester/impl agent — not a blocker for spec.
- **Dispatch on a non-default branch:** `workflow_dispatch` against a branch
  whose build jobs push `:latest-<arch>` will smoke those `latest` bytes. The
  manifest job is gated off on dispatch (`if: github.event_name !=
  'workflow_dispatch'`, ADR-001/ADR-004) precisely because the `:<branch>`
  manifest sources do not exist on dispatch — so on a dry-run the smoke-* job
  statuses are the meaningful signal. If a maintainer expects dispatch to
  validate a *specific* historical version, a future enhancement could add a
  `version` dispatch input. Out of scope now (OQ-5 only requires the dry-run);
  noted so it is not read later as a gap.
