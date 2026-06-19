# Component: `smoke-amd64` + `smoke-arm64` (release.yml)

> **One shared pseudocode for both arches** (Component Map note / NFR-06). The two jobs are
> byte-identical except for two parameters: the **runner** and the **arch suffix**. Both jobs
> MUST be emitted concretely — neither arch may be silently dropped (NFR-06 HARD RULE). This
> file and `create-container-manifest.md` edit the SAME file `.github/workflows/release.yml`
> — see OVERVIEW single-file editing-surface note (serialize on one Stage-3b agent).

## Purpose

Pull the exact pushed per-arch GHCR bytes and run `docker-http-posture-smoke.sh` as a
**verify-by-name / skip-is-failure** gate. Green ⟺ smoke exited `0` AND the terminal
run-marker was captured. A `fail()` (exit 1), a Docker-absent self-skip (exit 3), an
early-exit-0 (marker absent), or any unexpected code all fail the job loudly and block the
manifest. No retry, no `continue-on-error`. (FR-01/03/04/07/08/09; AC-01/02/03/06)

## Workflow-level edit (`on:` block — done once, shared)

```
on:
  push:
    tags: ['v*']
  workflow_dispatch: {}      # ADD — human pre-tag dry-run. Do NOT add pull_request. (FR-02, R-11)
```
- Keep existing `concurrency`, `permissions` (`packages: write` already present — no new secret).

## Job parameterization (the ONLY differences between the two jobs)

| Param | `smoke-amd64` | `smoke-arm64` |
|-------|---------------|---------------|
| `runs-on` | `ubuntu-22.04` | `ubuntu-22.04-arm` |
| `needs` | `[build-container-x64]` | `[build-container-arm64]` |
| ARCH suffix | `amd64` | `arm64` |

Everything below is identical; `<ARCH>` = `amd64` or `arm64`.

## Job shape (pseudocode)

```
job smoke-<ARCH>:
  needs: [build-container-<own-arch-build>]
  runs-on: <runner-for-ARCH>
  # NO continue-on-error. NO job-level `if:` that could re-green. (C-04, R-02)
  steps:

    STEP 1 — checkout:
      uses actions/checkout@v4          # needed: the smoke script lives in the repo

    STEP 2 — GHCR login:
      uses docker/login-action@v3
      with: registry=ghcr.io
            username=${{ github.actor }}
            password=${{ secrets.GITHUB_TOKEN }}   # existing token; GHCR read; no new secret (NFR-04)

    STEP 3 — resolve per-arch tag + run smoke as verify-by-name gate:
      run: |  (bash; multi-line — see "Gate step body" below)
```

### Gate step body (the load-bearing logic — ADR-003, pinned VERBATIM)

```bash
set -euo pipefail   # job default; the capture below MUST locally `set +e` around the smoke

OWNER_IMAGE="ghcr.io/${GITHUB_REPOSITORY_OWNER}/unimatrix"
ARCH="<ARCH>"                              # amd64 | arm64 — literal per job

# --- Tag resolution per trigger surface (ADR-002/004; R-09 — UN-stripped) ---
if [ "${GITHUB_EVENT_NAME}" = "workflow_dispatch" ]; then
  TAG="latest"                             # branch ref: only :latest-<arch> was pushed
else
  TAG="${GITHUB_REF_NAME}"                 # v* push: KEEP the v. NEVER ${GITHUB_REF_NAME#v}
fi
IMAGE="${OWNER_IMAGE}:${TAG}-${ARCH}"      # => :v<version>-<arch>  OR  :latest-<arch>
echo "smoke target IMAGE=${IMAGE}"

# --- Run-marker capture pattern (ADR-003 — exact shape; RC must survive) ----
set +e
OUT="$(IMAGE="$IMAGE" bash product/test/infra-001/scripts/docker-http-posture-smoke.sh 2>&1)"
RC=$?
set -e
echo "$OUT"                                # surface full smoke log into the job log

case "$RC" in
  0) : ;;
  3) echo "::error::smoke SKIPPED (exit 3): Docker-capable lane mis-provisioned — HARD failure (SR-01)."; exit 1 ;;
  1) echo "::error::smoke FAILED (exit 1): shipped image first-run path is broken."; exit 1 ;;
  *) echo "::error::smoke exited unexpectedly (exit $RC)."; exit 1 ;;
esac

echo "$OUT" | grep -qx '\[783-smoke\] ALL GATES PASSED.*' \
  || { echo "::error::smoke exited 0 but never printed ALL GATES PASSED — early-exit-0 (SR-01)."; exit 1; }
```

## Why each piece is shaped this way (rationale, not optional)

- **`IMAGE=` set** ⇒ the smoke skips its `docker build` branch (smoke lines 53–60) and tests
  the **pushed bytes** — never a rebuild (C-03/C-07; R-07). No `docker build`/`buildx build`
  of the production image appears anywhere in this job.
- **`set +e ... RC=$?; set -e`** with **no pipe** between the smoke and `$?` — the #4873 class
  (R-02): a `| tee` or unguarded `pipefail` would make `$?` read the wrong process and swallow
  a non-zero RC into green. The smoke is a direct command substitution, not piped.
- **`2>&1`** — `fail()` writes to stderr (smoke line 31); capturing both streams ensures the
  `fail()` text AND the marker reach `$OUT`.
- **`case` on RC, only `0` continues** — exit `3` and `1` are BOTH red; each emits a
  cause-specific `::error::` so a post-tag failure is diagnosable without a re-run (R-01).
  `*)` catches `139`/`137`/`124` etc. (R-01 edge cases).
- **Anchored `grep -qx '\[783-smoke\] ALL GATES PASSED.*'`** — whole-line match (R-03): a
  substring, an echoed-earlier diagnostic, or a mid-log occurrence must NOT satisfy it. The
  `.*` tolerates the trailing prose the real smoke prints after "ALL GATES PASSED" while still
  anchoring the line start to `[783-smoke] ALL GATES PASSED`.
- **Green ⟺ `RC==0` AND marker present.** Job-exit-0 alone is insufficient proof (NFR-01).

## Data Flow

- **In:** `GITHUB_EVENT_NAME`, `GITHUB_REF_NAME`, `GITHUB_REPOSITORY_OWNER` (GH context);
  pushed GHCR per-arch bytes.
- **Transform:** trigger+arch → resolved tag string → `IMAGE=` → smoke `(RC, OUT)`.
- **Out:** job conclusion (success/failure) → consumed by `create-container-manifest.needs`.

## Error Handling / Propagation

| Condition | Symptom | Job result |
|-----------|---------|------------|
| Docker absent | smoke `exit 3` | `exit 1` + "mis-provisioned" `::error::` → manifest blocked |
| First-run broken / mis-route / pull 404 | smoke `exit 1` (`fail()`) | `exit 1` + "first-run path broken" → manifest blocked |
| Early-exit-0 (future bug) | `RC==0`, marker absent | `exit 1` + "never printed ALL GATES PASSED" → manifest blocked |
| Segfault / OOM / timeout | `RC` ∈ {139,137,124,...} | `*)` → `exit 1` "unexpected" → manifest blocked |
| Full pass | `RC==0` + marker present | success → manifest may proceed |

No path degrades to green on uncertainty. No `|| retry`. (Failure Modes table, RISK-TEST-STRATEGY.)

## Key Test Scenarios (hints for tester — full plan in test artifacts)

- Config: `on:` has `push.tags:['v*']` + `workflow_dispatch`, excludes `pull_request` (R-11).
- Config: both `smoke-amd64`/`smoke-arm64` exist on the correct runners; correct own-arch
  `needs`; no swapped arch suffix (R-09).
- Config: neither smoke job has `continue-on-error` / re-greening `if:` (R-02/R-08).
- Behavioral (delegated to gate-logic stub-smoke test): the {0,1,3,early-0,unexpected} ×
  {marker present/absent} truth table; only `(0, marker present)` is green (R-01/03).
- Empirical (R-02): the exact `set +e; OUT="$(...)"; RC=$?; set -e` shape reads `exit 1` as 1
  and `exit 3` as 3 — verified by execution, not by reading.
- Post-tag (AC-07): smoke log shows `using prebuilt image: ghcr.io/...:v<version>-<arch>` (R-07).

## Open Questions

- **OQ-B / NFR-07 (flagged for tester/impl):** the smoke's 90s boot deadline may be tight for a
  cold arm64 first-boot ONNX model load (#767). This pseudocode does NOT widen it (out of this
  component's bounded edit); if the dispatch dry-run or first tag shows margin pressure, widening
  the deadline inside the smoke (bounded, still failing a true hang) is in-scope rework (R-05).
