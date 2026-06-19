# Gate 3a Report: nan-019

> Gate: 3a (Component Design Review)
> Date: 2026-06-19
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | Both smoke jobs, manifest rewire, AC-05 edit, and `on:`/dispatch trigger match ADR-001..005 and the Integration Surface verbatim. |
| 2. Specification coverage | PASS | FR-01..FR-11 and AC-01..AC-08 each map to a pseudocode/test-plan artifact; no scope additions. |
| 3. Risk coverage | PASS | All 14 risks mapped; the 4 MUST-EXIST pre-merge HARD gates (gate-logic truth table, tag-parity, AC-05 grew-monotonicity, needs-graph) are present and correctly framed. |
| 4. Interface consistency | PASS | Marker grep, capture shape, `needs:` list, dispatch gate, and tag resolution are byte-identical across OVERVIEW + every component file. |
| 5. Knowledge stewardship compliance | PASS (1 WARN) | Both design-phase reports carry `## Knowledge Stewardship` with `Queried:`. Pseudocode (read-only) report lacks an explicit "nothing to store" line — WARN, non-blocking. |
| Tag resolution UN-stripped (blocking defect class) | PASS | Every artifact resolves `VERSION/TAG="${GITHUB_REF_NAME}"` (kept `v`). No `${GITHUB_REF_NAME#v}` strip exists as an instruction anywhere. |

## Detailed Findings

### 1. Architecture alignment
**Status**: PASS
**Evidence**:
- Component boundaries match the architecture decomposition exactly: two new `release.yml` jobs (`smoke-amd64` `ubuntu-22.04` / `smoke-arm64` `ubuntu-22.04-arm`), one manifest `needs:`/`if:` rewire, one bounded AC-05 edit to `docker-http-posture-smoke.sh`, plus the two pre-merge test artifacts (`pseudocode/OVERVIEW.md` Components table).
- Job topology matches ADR-001: `smoke-<arch> needs: [build-container-<own-arch>]`; `create-container-manifest needs: [smoke-amd64, smoke-arm64]`; builds transitive, no cross-arch edge (`pseudocode/release-smoke-jobs.md` job-parameterization table; `create-container-manifest.md` lines 16-18).
- ADR-002 pushed-bytes honored: `IMAGE=` set ⇒ smoke skips its build branch; no `docker build` of the production image in either job (`release-smoke-jobs.md` rationale).
- ADR-003 verify-by-name contract reproduced verbatim — `set +e / RC=$? / set -e`, `case` on 0/1/3/`*`, anchored `grep -qx '\[783-smoke\] ALL GATES PASSED.*'`; exit 3 AND exit 1 both hard-fail; no retry/`continue-on-error`.
- ADR-004 workflow_dispatch + UN-stripped resolution honored (see blocking-defect check). The ADR-004 *file* on disk is correctly un-stripped (the `#v}` form appears only as a prohibition, line 55).
- ADR-005 grew-assertion via the read-only `vol()` busybox sidecar, WAL-inclusive signal, placed before the terminal marker (`docker-http-posture-smoke.md`).

### 2. Specification coverage
**Status**: PASS
**Evidence**: Every FR and AC has a corresponding artifact:
- FR-01/04 → `release-smoke-jobs.md` (both jobs emitted concretely, NFR-06 honored).
- FR-02/R-11 → `on:` block adds `workflow_dispatch`, excludes `pull_request`.
- FR-03/AC-06 → tag resolution + `IMAGE=` + own-arch `needs`.
- FR-05/AC-05 → `docker-http-posture-smoke.md` grew/hash-unchanged pair.
- FR-06/FR-10/AC-02/04/08 → `create-container-manifest.md` (`needs` + dispatch `if:`).
- FR-07/FR-08/FR-09/AC-03 → capture/`case`/grep + no-retry.
- FR-11/AC-06/R-09 → `test-tag-parity.md` static byte-identity assertion.
No scope additions found: AC-05 is a single bounded assertion pair; no new scripts, no smoke logic re-implemented in YAML (NFR-08/C-12 explicitly honored in `docker-http-posture-smoke.md` and `test-plan/OVERVIEW.md`).

### 3. Risk coverage
**Status**: PASS
**Evidence**: `test-plan/OVERVIEW.md` Risk→Test Mapping covers R-01..R-14. The four MUST-EXIST pre-merge HARD gates are present and correctly shaped:
1. **Gate-logic truth table (R-01/R-02)** — `smoke-amd64.md` T1: {0,1,3,early-0,unexpected (2,139)} × {marker present/absent}; only `(0, marker present)` green; each red row asserts its specific `::error::`. RC survival is verified **by execution** of the stub (`test_gate_rc_survives_capture`, "proven by running, never by reading"), directly addressing the #4873 class.
2. **Tag-parity static assertion (R-09)** — `test-tag-parity.md` + `smoke-amd64.md`/`smoke-arm64.md` T2: byte-identity to the metadata-action `pattern=v{{version}}-<arch>`; `test_tag_no_v_strip` goes RED on a re-introduced `${...#v}` strip; build side derived from a different source than the smoke side (not vacuous). Pre-merge, no tag push.
3. **AC-05 grew-signal monotonicity (R-04)** — `docker-http-posture-smoke.md` test plan T3: `test_ac05_signal_monotone_5x` (≥5 runs), positive + negative (`test_ac05_negative_control_misroute` — discriminating, fails on a hash mis-route), WAL-inclusive signal asserted, un-retryable (OQ-6 hard constraint stated).
4. **Needs-graph assertion (R-06)** — `create-container-manifest.md` test plan T4: zero cross-branch edge, single manifest block point, `create-release needs: package-npm` only.
Integration and edge cases (substring/echoed-marker anchoring, OOM/segfault/timeout codes, WAL one-page write, arm64 boundary boot) are enumerated. Risk priorities are reflected: the Critical/High risks carry the HARD pre-merge gates; accepted R-14 is documented, not tested.

### 4. Interface consistency
**Status**: PASS
**Evidence**: Cross-file grep confirms the load-bearing contracts are byte-identical wherever they appear:
- Marker grep `grep -qx '\[783-smoke\] ALL GATES PASSED.*'` identical in OVERVIEW, `release-smoke-jobs.md`, and `smoke-amd64.md`.
- `needs: [smoke-amd64, smoke-arm64]` identical in OVERVIEW, `create-container-manifest.md` (pseudocode + test plan).
- Dispatch gate `if: github.event_name != 'workflow_dispatch'` consistent across manifest pseudocode + test plan.
- The shared `run_smoke_gate`/`resolve_image_tag` design (test-plan OVERVIEW) extracts the SAME bytes shipped in the workflow step, avoiding the "test asserts X / ship emits Y" divergence (lesson #3548). The OVERVIEW data-flow, the per-component files, and the implementation brief Integration Surface agree; no contradictions.

### 5. Knowledge stewardship compliance
**Status**: PASS (1 WARN)
**Evidence**:
- `nan-019-agent-2-spec-report.md` has `## Knowledge Stewardship` with `Queried:` (context_briefing → #5163/#5180/#5130/#4582/#5184) and a declined-with-reason ("Read-only tier; no storage — spec decisions are feature-specific").
- `nan-019-agent-1-pseudocode-report.md` has `## Knowledge Stewardship` with `Queried:` (context_search → ADR-001..005, #5180) and "Deviations from established patterns: none."
**WARN**: The pseudocode (read-only) report states deviations=none and queried evidence but does not include an explicit `Stored:`/`Declined:` "nothing novel to store — {reason}" line in the prescribed form. Read-only agents are required to show `Queried:` (present); the missing explicit declination line is a minor formatting gap, non-blocking.

### Blocking-defect check: UN-stripped push tag resolution
**Status**: PASS
**Evidence**: Every push-path resolution across pseudocode, test plans, brief, ADR-004 file, and architecture resolves the version UN-stripped: `TAG="${GITHUB_REF_NAME}"` / `VERSION="${GITHUB_REF_NAME}"` ⇒ `:v<version>-<arch>` (`release-smoke-jobs.md` line 71; `test-tag-parity.md`; brief Data Structures). A cross-feature grep for `TAG=/VERSION=${GITHUB_REF_NAME#v}` as an *instruction* returns ZERO hits — the only `#v}` occurrences are (a) the ADR-004 prohibition ("Do not strip the `v`", line 55) and (b) spec/pseudocode report correction-pass notes flagging the stored Unimatrix ADR #5184 for `context_correct`. No stripped tag exists anywhere in the design artifacts. The blocking defect class the feature exists to abolish is absent.

### Honesty of pre-merge vs post-tag split
**Status**: PASS
**Evidence**: `test-plan/OVERVIEW.md` draws an explicit line: R-01/R-02/R-03/R-09/R-04/R-06/R-08/R-11 are PRE-MERGE HARD gates (a PENDING there is a gap); AC-07, R-05 arm64 cold-boot, R-08 skip *behavior*, R-07 log line, R-10 race, R-13 arm64 are "configured + verified locally; GH execution confirmed post-tag/post-dispatch" — never asserted before execution (#4796 honored). The split is honest and matches the architecture Validation Strategy.

## Carry-forward notes for downstream stages (non-blocking)

- **Single-file editing surface**: `release-smoke-jobs.md` + `create-container-manifest.md` edit the SAME `release.yml`. OVERVIEW correctly mandates serializing both on ONE Stage-3b agent (swarm shared-worktree hazard). Gate 3b should confirm this was honored.
- **Stored ADR #5184 still records the stripped contract** (per both design reports). The *files* are correct; the Unimatrix entry must be `context_correct`-ed by the owning architect agent so the knowledge base agrees with the shipped design. Flag for the coordinator — not a 3a blocker (file artifacts govern delivery).
- **OQ-B/OQ-C** (arm64 90s boot deadline; `du -s` block-rounding fallback to `db`+`-wal`+`-shm` byte sum) are correctly flagged as in-scope rework for the tester/impl agent, not blockers.
