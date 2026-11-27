# Gate 3c Report (rework iteration 1): 1036

> Gate: 3c (Risk Validation), re-run
> Date: 2026-11-27
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| node_modules untracked in main..HEAD | PASS | `git ls-files` has 0 node_modules entries; `git rev-list --objects main..HEAD` has 0; 14 commits on branch (history rewritten, unpushed) |
| D-038 clean-env proof | PASS | Own detached worktree, `npm install --omit=optional`, `node_modules/@dug-21` empty |
| Test changes not weakening | PASS | Last commit swaps `BIN` to `stagedCli()` only |
| D-037 / D-039 | PASS | resolve-binary passes in clean env; latency remains report-only |
| Knowledge stewardship | PASS | Unchanged from iteration 0 |

## Detailed Findings

### (1) node_modules tracking
`git ls-files | grep -c node_modules` = 0; `git rev-list --objects main..HEAD | grep -c node_modules` = 0; no `core.*` tracked under packages/unimatrix. The 43 MB blob is gone from branch history. Working tree status shows only pre-existing unrelated untracked/modified files.

### (2) D-038 clean-environment re-run (validator-executed)
Detached worktree of HEAD (3f897af4), `npm install --omit=optional` (rc 0), no platform package present, tmp HOME, `target/release` symlinked, file output with timeouts:
- `--set package`: 973 tests, 971 pass, 0 fail, 0 cancelled, 2 skip. No `not ok` lines. The 29 spawned-init failures recorded in agent-23 are resolved; `resolve-binary` passes.
- `--set wiring-e2e` with `E2E_REQUIRE_LOCAL=1`: 45 tests, 45 pass, 0 cancelled, 0 skipped.
Worktree removed and pruned.

### (3) Assertion weakening
`git show HEAD` on test files: wire-fixture.js adds `stagedCli()` (copies bin/lib/skills/protocols/package.json to a tmp root, adds fixture platform packages under `node_modules`, cleans up on exit); init-integration.test.js and wire.test.js change only the `BIN` constant in four describe blocks. No assertion, skip, or `.only` changes. The fixture binary is a stub, but these tests already supply a fake `UNIMATRIX_BINARY`; required-binary copy behaviour is now exercised with a fixture instead of being bypassed.

### (4) D-037 / D-039
resolve-binary passes in the clean environment, so the D-037 failures are environmental (#1055) and unaffected. D-039: latency stays report-only; the numbers were recorded in iteration 0 (host baseline 52 ms p95 exceeds the 50 ms budget, so the absolute budget remains unverified; no regression versus main).

## Residual notes (non-blocking)
- Untracked-ignored `packages/unimatrix/core.*` dumps in the main checkout should be deleted.
- Accepted gaps unchanged: D-036 remote.json spawned leg (#1053), Codex real-binary (D-012/#1053).

Unimatrix entries consulted: #2463, #4473, #2656, #4202, #2758, #2477 (briefing index; no context_get needed).

## Knowledge Stewardship
- Stored: nothing novel to store -- one-off tracked node_modules on an unpushed branch; clean-env spawned-init test dependence on an installed optional package is feature-specific.
