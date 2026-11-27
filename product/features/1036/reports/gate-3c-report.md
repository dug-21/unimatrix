# Gate 3c Report: 1036

> Gate: 3c (Risk Validation)
> Date: 2026-11-27
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Behavioral-outcome proof (AC-1..10, S-1..6 named tests, real entry points) | PASS | Spawned init/status tests, real `claude mcp get` (ac1c) and real `opencode debug config` (ac2b x3) ran, not skipped |
| Risk mitigation proof (R-01..R-25) | PASS (WARN) | All mapped; R-01 spawned remote.json leg (D-036/#1053) and R-25 Codex real-binary (D-012/#1053) accepted gaps |
| Coverage completeness | WARN | `wiring-e2e` linux-local leg (3 tests) not proven in a clean-environment run; see (a) |
| Specification / architecture compliance | PASS | No crates changed; D-034 footprint cap 500 KB; atomic-write.js removed; consumers on hardened writer |
| Integration test validation | PASS | Smoke 37/0 per report (not re-run, 275 s); no xfail added; no integration tests deleted |
| Branch hygiene | FAIL | Platform binary package from node_modules is committed on the branch (see below) |
| Knowledge stewardship | PASS | Tester report has `Queried:` and `Stored: nothing novel -- reason` |

## Detailed Findings

### Spot-run results (re-executed by validator)
- `--set package`: 973 tests, 969 pass, 2 fail (resolve-binary), 2 skip. Matches report. Named tests present and passing: `test_ac1c_*`, `test_ac2b_*` (3), `test_ac1b_*` (3), `test_s3_*` (5), AC-5 (5), AC-8 (4), `test_ac10_spawned_*`.
- `--set wiring-e2e` in the working tree: 45 tests, 42 pass, 3 cancelled ("refusing to overwrite an existing .../node_modules/@dug-21/unimatrix-linux-x64").
- With `node_modules/@dug-21` moved aside (restored afterwards; tree clean): wiring-e2e 45/45, resolve-binary 9/9.

### FAIL: platform binary package committed to the branch
Commit a26ad553 ("impl: wave 2") tracks `packages/unimatrix/node_modules/@dug-21/unimatrix-linux-x64/{package.json,bin/unimatrix}` (43 MB binary). main does not track them (`git ls-tree main` empty); `.gitignore` has no `node_modules` rule. The report treats the failures as an environmental artefact of a "locally installed" package. On this branch it is part of the checkout, so every clone or CI run of the branch carries the fixture that makes `resolve-binary` fail (2) and, locally, cancels the wiring-e2e linux-local tests. This is a branch defect, not only a host condition. Fix: `git rm -r --cached` those two files (a rewrite of history is a human/PR-strategy decision given the 43 MB blob already sits in a26ad553), and add a `.gitignore` entry.

### (a) wiring-e2e cancellations vs D-037/D-038
D-037 covers only `resolve-binary` (2). D-038 (added during this gate) says the 3 wiring-e2e cancellations are not waived and must pass in a clean environment. Evidence:
- They pass (45/45) when the package is absent, in the working tree with the release binary present.
- A clean `git worktree` of HEAD (which includes the committed package) ran only 42 tests, 0 cancelled, 0 refusals; the 3 linux-local tests did not execute at all there (no built binary), both with and without the package. So that run does not satisfy D-038.
- D-037 condition ("fail on main with same node_modules") was not demonstrated by a main run in the report; the report only shows test files unchanged. The failure mechanism is clearly independent of feature code (resolve-binary: `Missing expected exception` when the package exists), and I confirmed the 2 fail on the branch checkout, but the main-with-same-node_modules run is not evidenced. Because the package is tracked only on the branch, "fails on main" is not a straightforward claim.
- Disposition (waive vs fix) is the human's. Merits: the cancellations are not a defect in feature code; they are not proven in CI either, and D-038's clean-environment requirement is unmet until the nan-024-e2e workflow (or a clean checkout with a built binary and no package) runs green.

### (b) Latency
I re-ran `benchmark-wiring.js` on branch and on a main worktree (a59964e7), n=50:
| case | budget | branch p95 | main p95 |
|---|---|---|---|
| cloud FNF PostToolUse | 74 | 150.2 | 127.2 |
| cloud sync UserPromptSubmit | 74 | 132.1 | 120.2 |
| NOT-A-PROJECT hook | 50 | 71.8 | 71.0 |
| mcp-stdio NOT-A-PROJECT | 50 | 73.3 | 71.7 |
| node -e 0 | - | 52.8 | 52.5 |
Combined with the report's two branch runs (121/131, 118/122) the branch range for cloud rows overlaps main's 115-127; single-run variance is about 30 ms p95. "Within noise of main" is a sound reading. Branch-vs-main is adequate evidence of no measurable regression, supported by code evidence: the only hook-path source change is `credstore.js` (write path lazy-requires safe-write; read path untouched) and `hook-client/index.js` is unchanged. It is not evidence of meeting the budget: the host baseline alone (52 ms p95) breaches 50 ms, so the absolute budget is unverified on this host. D-039 makes it report-only; fine per the decision, but the budget is unproven until measured on CI hardware.

### (c) Accepted gaps
- D-036 remote.json spawned test: #1053 is OPEN. Condition (mode repair before ping) is evidenced by the write-before-ping order test (commit 1c431cba) per the report; credstore unit test covers enforcement. Acceptable per D-036.
- Codex real-binary: skipped test names #1053 and D-012. Acceptable per D-012.

### Removed tests vs main (merge-base a59964e7..HEAD)
Only file deleted in code: `lib/atomic-write.js` (+ its tests). Removed/renamed test names:
- `test_write_ignores_xdg_config_home`, `test_write_fs_error_is_skipped_not_thrown`, `test_within_project_guard_rejects_traversal`: obsolete behaviour (skip-on-error replaced by throw per R-12; XDG/guard now in config-dirs/safe-write). Replacements: `test_r12_rename_failure_*`, `containWithin*`, config-dirs tests.
- `test_remote_install_under_440kb`: renamed/cap raised to 500,000 B per D-034.
- `test_wire_gitignore_warning_only_and_gitignore_untouched`: superseded by D-011 git-exclude (`git-exclude.test.js`, AC-8 tests).
`check-test-count.js` PASS (1421 tests, no decrease). No `.only`, no new skips beyond the four documented. No weakening found. Note: the diff of main tip (not merge-base) also shows ass-111 deletions; those are main moving ahead (1 commit), not branch changes.

### Other observations (non-blocking)
- `lib/init.js` is 632 lines (638 on main; pre-existing, reduced); `hook-client/index.js` 509 (unchanged). Not introduced here.
- Untracked-ignored core dumps (`packages/unimatrix/core.*`, several hundred MB each) exist in the working directory; ignored by .gitignore but should be deleted.
- Unimatrix entries consulted and relevant to gate practice: #2758 (grep named tests; done), #4202.

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| node_modules platform package tracked on branch | uni-rust-dev / SM (history policy: human) | Untrack `packages/unimatrix/node_modules/**`, add .gitignore rule, re-run package + wiring-e2e |
| D-038 clean-environment proof absent | uni-tester | Run wiring-e2e linux-local (45 tests) on a clean checkout with the package untracked and a built binary, or confirm via nan-024-e2e CI; record in RISK-COVERAGE-REPORT |
| D-037 "fails on main" not shown | uni-tester | Run resolve-binary on main with the same installed package (or cite that main's checkout lacks it) and record |

Unimatrix entries consulted: #5768, #5177, #2758, #4147, #4473, #4202 (briefing index); no context_get needed.

## Knowledge Stewardship
- Stored: nothing novel to store -- a tracked node_modules artefact is a one-off to this branch; reassess if it recurs.
