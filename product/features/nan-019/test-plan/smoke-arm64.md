# Test Plan — `smoke-arm64` (release.yml job, `ubuntu-22.04-arm`)

> **Near-identical to `smoke-amd64`.** Same `run_smoke_gate` bytes, same T1 truth table, same
> T2 parity machinery — they are NOT re-implemented here. This file owns only the
> **arm64-specific deltas**: the per-arch suffix, the runner, and the one risk that is
> genuinely arch-sensitive (**R-05 arm64 cold-boot**). Listed as a separate component so
> neither arch is silently dropped (NFR-06 HARD RULE).

## Shared with smoke-amd64 (parameterized by arch)

The T1 gate-logic truth table (R-01/R-02/R-03) and the tag-parity machinery (R-09) are a
single test surface run once; the arm64 row is the suffix variant. Do **not** duplicate the
truth table — parameterize `ARCH` so the same `run_smoke_gate`/`resolve_image_tag` bytes are
asserted for both arches.

## T2 — arm64 tag-parity (R-09) — PRE-MERGE HARD GATE

Ground truth (release.yml 383–384): `type=semver,pattern=v{{version}}-arm64` +
`type=raw,value=latest-arm64`.

| Test fn | Input | Resolved tag MUST equal |
|---------|-------|-------------------------|
| `test_tag_parity_push_arm64` | `GITHUB_REF_NAME=v1.2.3`, push, arm64 | `:v1.2.3-arm64` (un-stripped) |
| `test_tag_parity_dispatch_arm64` | branch ref, dispatch, arm64 | `:latest-arm64` |
| `test_tag_suffix_no_swap_arm64` | arm64 job | `-arm64`, never `-amd64` |

**Same byte-identity assertion as amd64; a swapped suffix (`smoke-arm64` resolving `-amd64`)
is RED at merge** — this is the per-arch-swap half of R-09 scenario 2.

## Config assertions (pre-merge static)
- `test_smoke_arm64_runs_on_ubuntu_2204_arm`: `runs-on: ubuntu-22.04-arm`.
- `test_smoke_arm64_sets_IMAGE`: `IMAGE=ghcr.io/<owner>/unimatrix:<resolved>-arm64`.
- `test_smoke_arm64_no_production_build`: no production `docker build` in the job.
- `test_smoke_arm64_needs_own_build`: `needs: [build-container-arm64]` only — push ordered
  before smoke (R-10), no cross-arch coupling.

## R-05 — arm64 cold-boot exceeds boot deadline — POST-DISPATCH / POST-TAG ONLY

> arm64 is a **never-before-run path** for this smoke (R-13). The smoke's boot-wait deadlines
> are 90s (`docker-http-posture-smoke.sh` lines 82, 108). Whether a cold arm64 first-boot
> ONNX/embedding-model load (#767) clears 90s with margin **cannot be proven by local Linux
> validation** — it is "configured + verified locally; arm64 cold-boot margin confirmed
> post-dispatch/post-tag."

- **T5 (pre-tag, `workflow_dispatch` dry-run):** trigger the workflow manually; record the
  cold first-boot-to-`HTTP transport active` wall time on `ubuntu-22.04-arm` against
  `:latest-arm64`. This is the **first true cross-platform signal on R-05 without cutting a
  release** — the primary reason `workflow_dispatch` is in scope (ADR-004).
- **T6 (post-tag, AC-07):** watch the first real `smoke-arm64` to completion; record actual
  cold-boot wall time vs the 90s deadline and the margin.
- **If 90s is insufficient (NFR-07 / OQ-B):** widen the deadline to clear a healthy cold arm64
  boot **with margin while still bounding a true hang** (a deadline, not removal); re-confirm a
  deliberately-hung boot still fails within bound. Widening is in-scope rework; **never**
  `|| retry` (OQ-6) — a flaky deployability gate is itself the signal.

**Coverage requirement (R-05):** `smoke-arm64` passes on a healthy cold arm64 boot with
recorded margin; the deadline still fails a true hang. Phrased post-dispatch/post-tag, never
asserted pre-execution (#4796).

## R-13 — arm64 first-run as discovery (post-dispatch/post-tag)
Treat the first arm64 execution (dispatch dry-run, then tag) as discovery: any arm64-specific
smoke assumption (path, ENV, model-load timing) that fails is **in-scope rework for this
feature**, not a third-party `xfail`. The amd64 baseline is re-confirmed 3/3 locally in T3
(`docker-http-posture-smoke.md`).

## Edge case
- arm64 cold boot landing at exactly the 90s boundary (±a few seconds) → flaky pass/fail; the
  deadline must carry margin (R-05). This is the one place a too-tight bound manufactures a
  false deployability defect — bound it generously but finitely.
