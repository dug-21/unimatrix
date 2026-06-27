# C-LN — Standing Isolation Lane

> File: `.github/workflows/release.yml`
> New job `multi-tenant-isolation-amd64`. ADR-003 (#5351) harness containment.
> Runs on `push: tags:['v*']` + `workflow_dispatch`. NOT yet in the manifest
> `needs:` (that is C-FLIP). Mirrors the proven `smoke-amd64` harness plus one
> self-contained sqlite3 step (ADR-003 C/D).

## Purpose

Run `multi-tenant-isolation-smoke.sh` against the pushed per-arch GHCR bytes on every release tag
and dispatch, via the tri-state runner, producing an independent job status. Started as a
non-blocking lane so the AC-11 cold-model GREEN can be demonstrated before the blocking flip
(precedent: `nan-021-https-uds-parity`, `needs:[build-container-x64]`, not in manifest needs).

## Job Definition (pseudocode / YAML shape)

```
multi-tenant-isolation-amd64:
    needs: [build-container-x64]            # gate on the amd64 image build; inherits workflow on: (tags + dispatch)
    runs-on: ubuntu-22.04
    # NO `if:` guard — runs on BOTH push:tags and workflow_dispatch (AC-06, AC-11 dispatch path).
    # NOT in create-container-manifest.needs: yet — C-FLIP adds it.
    steps:
      - uses: actions/checkout@v4

      - name: Provision pinned Node (write/JSON-shaping + read path)
        uses: actions/setup-node@v4
        with:
          node-version: '24'

      - name: Provision sqlite3 (content-read engine; self-contained, coordinate #849)
        run: |
          set -euo pipefail
          sudo apt-get update
          sudo apt-get install -y sqlite3       # absence at runtime => preflight INFRA; THIS step failing => fail-closed (blocks)

      - name: Log in to GHCR
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Multi-tenant cross-tenant isolation gate (tri-state, amd64)
        run: |
          set -euo pipefail
          source product/test/infra-001/scripts/release-gate-lib.sh
          IMAGE="$(resolve_image "${GITHUB_REPOSITORY_OWNER}" "${GITHUB_EVENT_NAME}" "${GITHUB_REF_NAME}" amd64)"
          echo "isolation target IMAGE=${IMAGE}"
          export IMAGE                          # exported so the smoke's nan-019 acquisition path sees it
          run_smoke_gate_tristate "${IMAGE}" bash product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh
```

### Hard constraints (verify in review)

- `resolve_image` is the **sole** tag resolver; push→`:v<ver>-amd64` (UN-stripped), dispatch→`:latest-amd64`.
- **NEVER** `${GITHUB_REF_NAME#v}` anywhere in the lane (C-4, R-09 swallow class).
- No `docker build` step — the lane smokes the **pushed** bytes, never a local rebuild (AC-07).
- amd64-only this round (D-3 / C-12) — no arm64 isolation lane.
- Invocation is via `run_smoke_gate_tristate` (C-TS), never `run_smoke_gate`.

## State Machine — job outcome (pre-FLIP, standalone visibility)

```
build-container-x64 ──ok──▶ multi-tenant-isolation-amd64
   harness steps (checkout / node / sqlite3 / GHCR login)
        │ any step fails ─────────────▶ JOB FAILS (red status; NOT yet release-blocking until C-FLIP)
        ▼ all ok
   run_smoke_gate_tristate(...)  ── return 0 ──▶ JOB SUCCEEDS (GREEN, or INFRA-visible with ::warning::+marker)
                                 ── return 1 ──▶ JOB FAILS (RED / early-exit-0 / SKIP / unexpected)
```

## Initialization Sequence

1. `build-container-x64` pushes `:latest-amd64` (dispatch) / `:v<ver>-amd64` (tag) to GHCR.
2. Lane checks out the repo (for the sourced lib + the smoke script).
3. Provisions node 24 and sqlite3 (self-contained — no hard dep on #849).
4. Logs into GHCR so the smoke's `docker pull "$IMAGE"` can fetch the pushed bytes.
5. Sources `release-gate-lib.sh`, resolves + exports `IMAGE`, runs the tri-state gate once
   (no retry, no continue-on-error — single invocation, per the lib's contract).

## Data Flow

- **Inputs:** GHCR-pushed image (via `resolve_image`), `GITHUB_REPOSITORY_OWNER`,
  `GITHUB_EVENT_NAME`, `GITHUB_REF_NAME`, the GHCR token.
- **Transformation:** `IMAGE` resolution → exported → consumed by `run_smoke_gate_tristate` →
  consumed by the smoke's `setup_container` pull.
- **Outputs:** an independent job status + the full smoke log echoed by C-TS; on INFRA the
  `::warning::` + canonical marker.

## Error Handling / Blast-Radius (ARCH §5 — fail-closed contract)

| Failure source | Layer | Blocks manifest (post-FLIP)? |
|----------------|-------|------------------------------|
| Runner outage / job infra | harness | yes — fail-closed (mirrors the 4 existing blocking lanes) |
| `actions/checkout` fail | harness | yes |
| node / **sqlite3** provisioning **step** fail | harness | yes — a real provisioning break must be fixed, not silently passed |
| GHCR `docker/login` expiry/fail | harness | yes |
| Image pull 404 / tag missing | **script → exit 2** | no — visible INFRA (deliberate divergence from exit-4-blocks) |
| sqlite3/busybox absent at runtime | script → exit 2 | no — preflight INFRA (the provisioning step above is the fail-closed guard) |
| Warmup not ready at deadline | script (C-WB) → exit 2 | no — visible INFRA |
| Genuine cross-tenant leak | script → exit 1 | **yes** (the DoD) |

**Containment rule:** only the script's exit-2 maps to non-blocking (via C-TS). Every harness-step
failure fails the job and (post-FLIP) blocks — fail-closed. C-TS never makes a harness failure
non-blocking and never makes script-INFRA blocking.

## SR-05 / never-green-on-tag posture (ADR-004)

- AC-11 (`workflow_dispatch` on the rebased feature branch) exercises **the entire harness +
  warmup + verdict on the dispatch path** (`:latest-amd64`) before the flip — so a harness-step
  break is found pre-flip.
- It does **not** prove `:v<ver>-amd64` tag-push resolution (dispatch and tag resolve different
  tags, ADR-004). Budget one post-merge tag round; the lane is diagnostic-capture-first (C-TS
  echoes the full log on every path), so the first real tag yields a diagnosis, not a guess. A
  tag-path INFRA (e.g. pull 404) degrades to non-blocking-visible — the safe failure mode.

## Key Test Scenarios (hints — full plan in test-plan/)

1. **YAML review (AC-06/AC-07/AC-10):** job present, triggers inherit tags+dispatch, no `if:`
   guard, `resolve_image` amd64, `IMAGE` exported, `run_smoke_gate_tristate` invoked, node+sqlite3
   steps present, no `docker build`, **no** `${GITHUB_REF_NAME#v}`.
2. **AC-11 cold-model dispatch (R-13):** dispatch run shows the real first-boot HuggingFace
   download lines (not a warm cache / not `:783-smoke`); GREEN verdict; branch SHA == `main` HEAD
   at run time (recorded as evidence, R-11).
3. **Fail-closed cells (R-08):** validate ARCH §5 cell-by-cell — harness step failures fail the
   job; script-exit-2 returns success-with-warning.
4. **Tag-call-shape (R-09):** assert `resolve_image` is the sole resolver and the forbidden
   `${GITHUB_REF_NAME#v}` pattern is absent.
