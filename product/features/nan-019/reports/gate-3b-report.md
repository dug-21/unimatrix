# Gate 3b Report: nan-019

> Gate: 3b (Code Review)
> Date: 2026-06-19
> Result: PASS (cleared at REWORK-1 — see re-validation section at bottom)
> Original result: REWORKABLE FAIL (single failing check: #7 knowledge stewardship — missing Wave-1 gate-spine agent report)
> Scope note: CI/release-workflow + shell feature — no Rust/cargo. "Compiles" = YAML parse + `bash -n` + pre-merge tests pass.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | release.yml smoke jobs, manifest rewire, `release-gate-lib.sh`, and AC-05 edit reproduce the Stage-3a pseudocode verbatim (capture shape, `case`, anchored grep, tag resolution, GATE 4 placement). |
| 2. Architecture compliance | PASS | ADR-001 (manifest-gated, builds transitive) · ADR-002 (pushed bytes via `IMAGE=`) · ADR-003 (verify-by-name capture, no retry) · ADR-004 (`workflow_dispatch` + UN-stripped tag + dispatch manifest gate) · ADR-005 (AC-05) all honored. |
| 3. Interface implementation | PASS | `resolve_image`/`run_smoke_gate` signatures, marker string, `needs:` list, dispatch `if:` byte-identical to OVERVIEW contract strings. |
| 4. Test case alignment | PASS | Both pre-merge tests map to test plans (T1 truth table R-01/02/03, T2 tag-parity R-09); tests are REAL — they source the shipped lib and read release.yml (non-vacuous). 13/13 + 13/13 pass. |
| 5. Code quality | PASS | YAML parses; `bash -n` clean on all 5 scripts; no anti-stub violations; no source file over 500 lines (max 180). |
| 6. Security | PASS | No new secrets (GHCR read via `GITHUB_TOKEN`); read-only `:ro` busybox sidecar; anchored marker grep removes spoof class; no injection/traversal surface added. |
| **7. Knowledge stewardship compliance** | **FAIL** | The Wave-1 agent that authored the load-bearing gate spine (`release-gate-lib.sh` + the `release.yml` smoke jobs / manifest rewire) has **no agent report** in `agents/` — therefore no `## Knowledge Stewardship` block. |
| Tag resolution UN-stripped (blocking defect class) | PASS | `release-gate-lib.sh` resolves `tag="${ref_name}"` (keeps `v`). No `${...#v}` strip on any smoke path. (release.yml:245 `#v` strip is the pre-existing npm-version step, not the smoke.) |
| RC-swallow re-confirmation | PASS | Verified by execution: exit 1 reads 1, exit 3 reads 3; YAML step propagates `run_smoke_gate`'s `return 1` to fail the job. |

## Detailed Findings

### 1. Pseudocode fidelity — PASS
- `release-gate-lib.sh` `run_smoke_gate` is the ADR-003 shape verbatim: `set +e; out="$(IMAGE=... "$@" 2>&1)"; rc=$?; set -e`, `case` on `0`/`3`/`1`/`*`, then `grep -qx '\[783-smoke\] ALL GATES PASSED.*'`. Uses `return 1` (not `exit 1`) so it is sourceable — matches the OVERVIEW single-source-of-truth design.
- AC-05 edit (`docker-http-posture-smoke.sh` lines 46–52, 122–170) matches `docker-http-posture-smoke.md`: `store_size()` = `vol du -s | awk` (WAL-inclusive), BEFORE sample after register+restart before the POST, AFTER sample after the 204, GATE 4 `-gt`/`-eq` via `fail()`, placed BEFORE the terminal marker (line 170 stays last, R-12).

### 2. Architecture compliance — PASS
- **ADR-004 UN-STRIPPED tag (CRITICAL):** `resolve_image` push path `tag="${ref_name}"` ⇒ `:v<version>-<arch>`. No `${ref_name#v}` anywhere on the smoke path. Confirmed by `release-tag-parity-test.sh`: `smoke='v1.2.3-amd64' == build='v1.2.3-amd64'`, and the `test_tag_no_v_strip` guard goes RED on a re-introduced strip.
- **ADR-001/FR-06:** `create-container-manifest needs: [smoke-amd64, smoke-arm64]`; builds transitive via each smoke's own-arch `needs`. Zero needs-edge into `build-linux-*`/`package-npm`/`create-release` (needs-graph traced: `package-npm needs [build-linux-x64, build-linux-arm64]`, `create-release needs package-npm`, smokes need only their container builds). ADR-004 independence intact (R-06/AC-04).
- **ADR-004 dispatch gate:** `if: github.event_name != 'workflow_dispatch'` on the manifest. `on:` = {push.tags ['v*'], workflow_dispatch}; no `pull_request`.

### 3 / 4. Interface + test alignment — PASS
- Marker capture: exit 3 AND exit 1 both hard-fail (`return 1`); anchored `grep -qx`; no `continue-on-error`/retry. The YAML `run:` block runs under `set -euo pipefail` with `run_smoke_gate` as the final command, so its non-zero return fails the step/job — the #4873 RC-swallow class is closed (proven by execution in T1, not by reading).
- Tests are real, not vacuous: T1 `source`s `release-gate-lib.sh` and drives the actual `run_smoke_gate` against `fixtures/stub-smoke.sh`; T2 sources shipped `resolve_image` AND reads release.yml's `metadata-action` patterns (two independent sources) with explicit discrimination self-checks (strip/swap/extra-v go RED).
- `fixtures/stub-smoke.sh` is a legitimate TEST fixture (env-driven exit-code/output stand-in), not a production stub — confirmed.

### 5 / 6. Quality + security — PASS
- `bash -n` clean on lib, smoke, both tests, stub. YAML parses (PyYAML). No TODO/FIXME/placeholder code (the one "placeholder" hit is a comment word). Largest file 180 lines.
- Read-only `:ro` volume mount for all AC-05 inspection; no `docker exec` into distroless. No new secret. Anchored marker grep removes the success-marker spoofing class (R-03).

### 7. Knowledge stewardship compliance — FAIL (the only blocking finding)
**Evidence:** `agents/` contains reports for agent-1 (pseudocode), agent-2 (spec), agent-4 (AC-05 smoke), agent-5 (pre-merge tests), and the 3a gate report. Agent-5's report explicitly states it consumed "**Wave 1's committed output (`release-gate-lib.sh`, `release.yml`, smoke script)**". Wave-1 commit `6e033c5d` ("release.yml smoke gate + sourceable gate lib + AC-05 grew-assertion") bundles the gate-spine work, but **no agent report exists for the agent that authored `release-gate-lib.sh` and the `release.yml` smoke jobs / manifest rewire** — the single most load-bearing component of the feature. Agent-4 covers only the AC-05 smoke edit.

Per the Gate 3b check set, each implementation agent report must carry a `## Knowledge Stewardship` block with `Queried:` and `Stored:`/"nothing novel" entries. The Wave-1 gate-spine implementer's report (and therefore its stewardship block) is **missing** = REWORKABLE FAIL.

**Mitigating context:** the code/logic this agent produced is fully correct and independently verified (checks 1–6 all PASS). This is a process/reporting gap, not a code defect. Agent-4 (#5193) and agent-5 (#5194) did store novel patterns, so the cross-feature knowledge is partly captured — but the gate-spine implementer's own `Queried:` evidence and any decline/store is unaccounted.

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Missing Wave-1 gate-spine implementer report (no `## Knowledge Stewardship` block) | Wave-1 impl agent (release.yml + `release-gate-lib.sh` owner) / Delivery Leader | Produce the missing agent report at `agents/nan-019-agent-3-*.md` with a `## Knowledge Stewardship` block: `Queried:` (the /uni-query-patterns evidence used before writing the gate lib — e.g. pattern #5180 verify-by-name, #4873 RC-swallow, the gate-spine-as-lib pattern #5192 referenced by agent-5) and `Stored:`/"nothing novel to store -- {reason}". No code change required — checks 1–6 all PASS. |

## Notes
- All five static/dynamic verifications requested in the spawn prompt pass: `release-gate-logic-test.sh` (13/13, rc 0), `release-tag-parity-test.sh` (13/13, rc 0), `bash -n` on all edited scripts, YAML parse of release.yml.
- Post-tag ACs (AC-07 arm64 cold-boot margin, hosted-runner execution) are correctly NOT marked executed — provable only post-tag (#4796), as designed.

---

## REWORK-1 Re-Validation (2026-06-19) — Result: PASS

The original REWORKABLE FAIL had ONE cause: Check #7, the missing Wave-1 gate-spine implementer's agent report / `## Knowledge Stewardship` block. Checks 1–6 all PASS'd in the original run. This re-validation confirms only the previously-failing item plus a no-regression spot-check.

### Check #7 — Knowledge stewardship compliance — now PASS
**Evidence:** `agents/nan-019-agent-3-release-workflow-report.md` now exists (created by commit `1d950835`, a `docs:` commit — report-only, no code). It carries a `## Knowledge Stewardship` block:
- **Queried:** `context_search` (decision/nan-019) + `context_get(5183)` → surfaced ADR-001 #5186, ADR-002 #5187, ADR-003 #5183, ADR-004 #5188, pattern #5180. Findings applied (ADR-003 capture-and-branch shape, UN-stripped tag, rc=3 skip-is-failure, ADR-004 independence) — non-vacuous, traceable evidence.
- **Stored:** entry **#5192** (pattern: sourceable gate-spine, single-source-of-truth, `return`-not-`exit`) via context_store with Supports edges to #5183 and #5180.

The block has both a real `Queried:` line and a `Stored:` line — not a bare "nothing novel". No WARN. The report content is also technically coherent with the implementation already verified in checks 1–6 (resolve_image / run_smoke_gate contract, UN-stripped tag, ADR-004 independence all described consistently).

### No-regression spot-check — PASS
- Commit touching the report (`1d950835`) is `docs:` only. `git diff 6e033c5d..HEAD` on the load-bearing implementation files (`release.yml`, `release-gate-lib.sh`, `docker-http-posture-smoke.sh`) shows **no changes**. (The two pre-merge test files show as additions in that range only because they were authored by agent-5 in a *later* Wave-1 commit, after the gate-spine commit `6e033c5d` — not a regression.)
- `release-gate-logic-test.sh` → **13 passed, 0 failed (rc 0)**.
- `release-tag-parity-test.sh` → **13 passed, 0 failed (rc 0)**, including `test_tag_no_v_strip` (the OCCURRED-defect guard) and the strip/swap/extra-v discrimination self-checks all RED-on-violation.

### Cleared gate result
| Check | Status |
|-------|--------|
| 1. Pseudocode fidelity | PASS (unchanged) |
| 2. Architecture compliance | PASS (unchanged) |
| 3. Interface implementation | PASS (unchanged) |
| 4. Test case alignment | PASS (unchanged) |
| 5. Code quality | PASS (unchanged) |
| 6. Security | PASS (unchanged) |
| **7. Knowledge stewardship compliance** | **PASS (cleared this iteration)** |

All 7 checks PASS, 0 WARN. **Gate 3b result: PASS.**
