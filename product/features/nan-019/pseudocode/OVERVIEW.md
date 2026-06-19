# nan-019 Pseudocode — Overview

> CI/release-workflow + shell feature. **Not Rust.** "Components" are YAML jobs in
> `.github/workflows/release.yml` and one bounded edit to a bash smoke script, plus two
> pre-merge test artifacts. Pseudocode is written at job-topology / step-shape altitude.
> Every contract string below is VERBATIM from the brief Integration Surface — do not invent.

## Components

| Component | File | Edits |
|-----------|------|-------|
| Smoke jobs (amd64 + arm64) | `release-smoke-jobs.md` | New: two near-identical jobs in `release.yml` (parameterized by arch) |
| Manifest rewire | `create-container-manifest.md` | `needs:` + `if:` change in the same `release.yml` |
| Workflow triggers | covered in both `release.yml` files above | Add `workflow_dispatch` to `on:` |
| AC-05 grew-assertion | `docker-http-posture-smoke.md` | One bounded edit to the smoke script |
| Gate-logic stub-smoke test | `test-gate-logic-stub-smoke.md` | New pre-merge test artifact |
| Tag-parity static test | `test-tag-parity.md` | New pre-merge test artifact |

## SINGLE-FILE EDITING SURFACE — `release.yml` (Stage 3b must serialize)

> **HARD CONSTRAINT for Stage 3b.** Three logical edits — `smoke-amd64`, `smoke-arm64`,
> and the `create-container-manifest` `needs:`/`if:` rewire — plus the `on:` trigger change
> **ALL live in one file: `.github/workflows/release.yml`**. They are split into two
> pseudocode files only for readability. **Do NOT assign these to parallel agents editing
> the same file** (swarm shared-worktree git hazard — a parallel writer clobbers the other's
> diff). One agent owns `release.yml` end-to-end and applies all `release.yml` edits in a
> single coherent pass. The smoke-script edit (`docker-http-posture-smoke.sh`) and the two
> test artifacts are different files and may proceed independently.

## Component Interactions (Job Topology — ADR-001)

```
   ── Binary/npm branch (UNCHANGED — zero edge to/from container branch, ADR-004) ──
   build-linux-x64 ─┐
   build-linux-arm64┴─▶ package-npm ─▶ create-release

   ──────────────── Container branch (nan-019 changes) ────────────────
   build-container-x64  ─▶ smoke-amd64 ─┐
   (push :<tag>-amd64)     (ubuntu-22.04)│
                                         ├─▶ create-container-manifest
   build-container-arm64 ─▶ smoke-arm64 ─┘     needs: [smoke-amd64, smoke-arm64]
   (push :<tag>-arm64)     (ubuntu-22.04-arm)   if: github.event_name != 'workflow_dispatch'
```

- `smoke-amd64 needs: [build-container-x64]`; `smoke-arm64 needs: [build-container-arm64]`
  — each smoke runs after its own-arch push so the pushed bytes exist to pull. No cross-arch
  edge. (ADR-001/ADR-002)
- `create-container-manifest needs: [smoke-amd64, smoke-arm64]` (was
  `[build-container-x64, build-container-arm64]`) — single gate point; builds stay transitive
  through the smokes, so re-listing them is redundant and omitted (FR-06).
- **No** edge into `build-linux-*` / `package-npm` / `create-release` (ADR-004 / R-06).

## Data Flow (what crosses boundaries)

1. `build-container-<arch>` → GHCR: pushes `:v<version>-<arch>` (push) / `:latest-<arch>`
   (dispatch). These bytes are the artifact under test.
2. Smoke job resolves the per-arch tag string from `GITHUB_REF_NAME` + trigger + arch,
   `docker login`s, exports `IMAGE=...:<resolved>`, and invokes the smoke.
3. Smoke (`docker-http-posture-smoke.sh`) crosses back two signals to the job:
   - **exit code** ∈ {0,1,3,other} (must survive `set -e`/pipefail capture — R-02)
   - **stdout/stderr** containing (on full success) the terminal marker line.
4. Job branches on exit code + asserts the marker → green ⟺ `RC==0` AND marker present.
5. Both smokes green (on a `v*` push) → manifest job assembles + pushes the multi-arch index.

## Shared Contract Strings (VERBATIM — used across component files)

| Name | Value |
|------|-------|
| Smoke entrypoint | `bash product/test/infra-001/scripts/docker-http-posture-smoke.sh` |
| Reuse-image input | `IMAGE=ghcr.io/<owner>/unimatrix:<resolved-tag>-<arch>` (set ⇒ smoke skips build) |
| Image owner | `ghcr.io/${{ github.repository_owner }}/unimatrix` |
| Exit contract | `0`=ran+passed · `1`=ran+failed (`fail()`) · `3`=self-skipped (Docker absent) |
| Run-marker (load-bearing, terminal line) | `[783-smoke] ALL GATES PASSED` (smoke line 142; has trailing prose after it) |
| Marker grep (anchored) | `grep -qx '\[783-smoke\] ALL GATES PASSED.*'` |
| Tag resolution (push) — UN-stripped | `VERSION="${GITHUB_REF_NAME}"` ⇒ `:${VERSION}-<arch>` = `:v<version>-<arch>`. NEVER `${GITHUB_REF_NAME#v}` |
| Tag resolution (dispatch) | `TAG="latest"` ⇒ `:latest-<arch>` |
| Build push pattern (parity target) | `type=semver,pattern=v{{version}}-<arch>` + `type=raw,value=latest-<arch>` |
| Manifest `needs` (changed) | `[smoke-amd64, smoke-arm64]` |
| Manifest dispatch gate | `if: github.event_name != 'workflow_dispatch'` |
| GHCR login | `docker/login-action@v3`, `registry: ghcr.io`, `username: ${{ github.actor }}`, `password: ${{ secrets.GITHUB_TOKEN }}` |
| Runners | `smoke-amd64` → `ubuntu-22.04`; `smoke-arm64` → `ubuntu-22.04-arm` |
| Read-only volume inspector | `vol() { docker run --rm -v "$VOL:/data:ro" busybox "$@"; }` (AC-05 MUST use; never `docker exec`) |
| Per-slug store | `/data/.unimatrix/<slug>/unimatrix.db` (`SLUG=arch-research`) |
| Hash store dir | `$HASH_DIR` (holds `token`, `tls/cert.pem`, `unimatrix.db`) |

## Sequencing Constraints (what must be built first)

1. **`docker-http-posture-smoke.md` (AC-05 edit) is the foundation** — it must keep the
   terminal marker LAST (R-12) and stay 0/1/3-correct, because the smoke jobs and the
   gate-logic test both key on its exit contract + marker. Apply + locally re-run (3/3,
   then ≥5× for grew-signal monotonicity) before relying on the jobs.
2. **`release-smoke-jobs.md` + `create-container-manifest.md`** edit the same `release.yml`
   — serialize them on one agent (see single-file note above).
3. **The two test artifacts** depend only on the contract strings here (stub the smoke; do
   not invoke the real one) and may be built in parallel with the script edit.

## Validation Phasing (the gate cannot be proven by local Linux validation — #4796)

- **Provable pre-merge (MUST exist):** gate-logic stub-smoke truth table (R-01/02/03);
  RC-survives-capture empirical check (R-02); tag-parity static assertion (R-09); grew-signal
  monotone over ≥5 local runs (R-04); `needs:`-graph + no-cross-branch-edge static check (R-06/08).
- **`workflow_dispatch` dry-run (pre-tag):** first real `ubuntu-22.04` / `ubuntu-22.04-arm`
  exercise against `:latest-<arch>`; manifest green-skips.
- **First `v*` tag (post-merge, AC-07):** delivery leader watches both arches green + manifest
  publishes. Runner surprises = in-scope rework, never `|| retry`.
