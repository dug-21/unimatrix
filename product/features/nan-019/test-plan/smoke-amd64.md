# Test Plan — `smoke-amd64` (release.yml job, `ubuntu-22.04`)

> Shared with `smoke-arm64` (see that file for the arch-specific deltas). The two jobs run
> the **same** `run_smoke_gate` bytes and differ only in runner (`ubuntu-22.04` vs
> `ubuntu-22.04-arm`) and per-arch tag suffix (`-amd64` vs `-arm64`). This file owns the
> **gate-logic truth table (R-01/R-02/R-03)** and the **tag-parity static assertion
> (R-09)** — the two pre-merge HARD gates that are the heart of the feature's verifiability.

## Component under test

The gate step's pinned capture/`case`/grep (ARCHITECTURE §Run-marker capture pattern),
extracted into `run_smoke_gate` in `scripts/release-gate-lib.sh` and `resolve_image_tag`,
so the SAME bytes are exercised by the test and shipped in the workflow step.

```bash
set +e
OUT="$(IMAGE="$IMAGE" bash product/test/infra-001/scripts/docker-http-posture-smoke.sh 2>&1)"
RC=$?
set -e
echo "$OUT"
case "$RC" in
  0) : ;;
  3) echo "::error::smoke SKIPPED (exit 3): ... mis-provisioned — HARD failure (SR-01)."; exit 1 ;;
  1) echo "::error::smoke FAILED (exit 1): ... first-run path is broken."; exit 1 ;;
  *) echo "::error::smoke exited unexpectedly (exit $RC)."; exit 1 ;;
esac
echo "$OUT" | grep -qx '\[783-smoke\] ALL GATES PASSED.*' \
  || { echo "::error::... exited 0 but never printed ALL GATES PASSED — early-exit-0 (SR-01)."; exit 1; }
```
Green **iff** `RC == 0` AND the anchored marker line was captured. No retry.

---

## T1 — Gate-logic truth table (R-01/R-02/R-03) — PRE-MERGE HARD GATE

Drive `run_smoke_gate` against `fixtures/stub-smoke.sh` (`STUB_RC` chooses exit code,
`STUB_BODY` chooses stdout). **The RC must be verified to survive capture by EXECUTION, not
by reading the YAML** (the #4873 `setsid`/pipe class — R-02). Each row asserts the resulting
job exit AND the specific `::error::` diagnostic.

| Test fn | STUB_RC | STUB_BODY (marker) | Expect job | Expect diagnostic |
|---------|---------|--------------------|-----------|-------------------|
| `test_gate_pass_exit0_marker_present` | 0 | terminal `[783-smoke] ALL GATES PASSED` present | **green (exit 0)** | none |
| `test_gate_fail_exit1_no_marker` | 1 | `[783-smoke] FAIL: ...`, no marker | **red (exit 1)** | "first-run path is broken" |
| `test_gate_skip_exit3_hard_fail` | 3 | `SKIP: Docker not available` | **red (exit 1)** | "mis-provisioned ... HARD failure" |
| `test_gate_early_exit0_marker_absent` | 0 | partial output up to "PASS gate 1", **no** marker | **red (exit 1)** | "exited 0 but never printed ALL GATES PASSED" |
| `test_gate_unexpected_exit2` | 2 | arbitrary | **red (exit 1)** | "exited unexpectedly (exit 2)" |
| `test_gate_unexpected_exit139` | 139 | empty (OOM/segfault) | **red (exit 1)** | "exited unexpectedly (exit 139)" |

**Coverage requirement (R-01):** the full {0,1,3,early-0,unexpected} × {marker present/absent}
table is exercised against the **actual** capture-and-branch logic; **only `(0, marker present)`
is green.** This table MUST exist before merge.

### R-02 — RC survives capture (verified by EXECUTION)
- `test_gate_rc_survives_capture`: run the stub at `STUB_RC=1` and `STUB_RC=3` through the
  exact `set +e; OUT="$(...)"; RC=$?; set -e` shape and **assert `RC == 1` and `RC == 3`**
  respectively (not 0). This is the #4873 guard — proven by running, never by reading.
- Adversarial variants that MUST be REJECTED in code review (assert their absence in the
  shipped YAML step): smoke inside a pipe (`smoke | tee` → `$?` reads `tee`); a job-level
  `set -eo pipefail` with no `set +e` guard; any `if: ${{ success() }}` / `continue-on-error`
  re-greening the step. `test_no_continue_on_error` (T4) backstops this statically.
- `test_gate_captures_stderr`: stub writes its `FAIL`/marker to **stderr**; assert `2>&1`
  capture still reaches the grep (a `fail()` on stderr must not vanish).

### R-03 — marker anchoring (no spoof)
- `test_gate_marker_anchored_substring`: STUB_BODY contains `... ALL GATES PASSED ...` as a
  **substring** of a longer line → `grep -qx '\[783-smoke\] ALL GATES PASSED.*'` must **not**
  match → **red**.
- `test_gate_marker_anchored_echoed_early`: marker echoed as an earlier diagnostic/comment,
  then `exit 0` with no terminal marker → **red**.
- `test_gate_marker_byte_identical`: assert the literal string the gate greps for is
  byte-identical to the smoke's emitted line `[783-smoke] ALL GATES PASSED` (cross-check
  against `docker-http-posture-smoke.sh` line 142).

---

## T2 — Tag-parity static assertion (R-09 — the OCCURRED defect) — PRE-MERGE HARD GATE

Assert `resolve_image_tag` output is **byte-identical** to the metadata-action push pattern.
Ground truth (release.yml 348–349): `type=semver,pattern=v{{version}}-amd64` +
`type=raw,value=latest-amd64`. RED at merge on any divergence — no tag push required.

| Test fn | Input | Resolved tag MUST equal | Guards |
|---------|-------|-------------------------|--------|
| `test_tag_parity_push_amd64` | `GITHUB_REF_NAME=v1.2.3`, push, amd64 | `:v1.2.3-amd64` | un-stripped `v` kept |
| `test_tag_parity_dispatch_amd64` | branch ref, `workflow_dispatch`, amd64 | `:latest-amd64` | matches `value=latest-amd64` |
| `test_tag_no_v_strip` | `GITHUB_REF_NAME=v1.2.3` via `VERSION="${GITHUB_REF_NAME}"` | resolves `v1.2.3-...`, **NOT** `1.2.3-...` | RED if a `${...#v}` strip is reintroduced (the literal first-draft defect) |
| `test_tag_suffix_no_swap` | amd64 job | `-amd64`, never `-arm64` | per-arch suffix correctness |

**Coverage requirement (R-09):** a deliberate mismatch (re-introduce `${GITHUB_REF_NAME#v}`,
add/drop a `v`, swap the suffix) turns the assertion **RED at merge** with no tag push. This
is the primary mitigation for the defect that already materialized — it converts a post-tag
404-on-every-release into a pre-merge gate.

---

## Config assertions (R-07/R-10/R-11 — pre-merge static, behavior post-tag)

- `test_smoke_amd64_runs_on_ubuntu_2204`: `runs-on: ubuntu-22.04`.
- `test_smoke_amd64_sets_IMAGE`: step sets `IMAGE=ghcr.io/<owner>/unimatrix:<resolved>-amd64`
  (R-07; `IMAGE=` set ⇒ smoke skips its build branch, smoke lines 53–60).
- `test_smoke_amd64_no_production_build`: no `docker build`/`buildx build` of the production
  image in the job (R-07).
- `test_smoke_amd64_logs_in_before_pull`: `docker/login-action@v3` precedes the smoke (R-07).
- `test_smoke_amd64_needs_own_build`: `needs: [build-container-x64]` only — push ordered
  strictly before smoke (R-10); no cross-arch coupling.

## Post-tag confirmations (AC-07 — configured + verified locally; confirmed post-tag)
- Smoke log shows `using prebuilt image: ghcr.io/...:v<version>-amd64` (R-07 log).
- Job runs green on the hosted `ubuntu-22.04` in the first real release run.
- First-try pull succeeds after `--push` (R-10 race surfaced here = in-scope structural rework,
  never `|| retry`).

## Edge cases
- Empty/truncated smoke output (runner OOM) → RC non-zero or marker absent → red (covered by
  `test_gate_unexpected_exit139` / `test_gate_early_exit0_marker_absent`).
- Marker with trailing whitespace/text → `.*` anchor tolerates a trailing-text variant on the
  same terminal line but still rejects a mid-output substring (assert both behaviors so the
  anchor's intent is pinned, not accidental).
