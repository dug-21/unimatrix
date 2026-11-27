# Gate 3a Report (rework iteration 1): 1036

> Gate: 3a (Design Review), re-run
> Date: 2026-11-27
> Result: PASS

Unimatrix entries consulted: #5963, #5959, #2463, #4791 (briefing); prior report's #5768, #2577, #3644, #1204, #5763, #5937, #5960, #5962, #5955, #5943, #5961.

## Summary

| Prior item | Status | Evidence |
|---|---|---|
| FR-16/D-022 merge-settings byte-compare (FAIL) | PASS | merge-settings.md `computeMerged`: `changed = raw===null \|\| !deepEqual(before, content)`; `text = changed ? serialize : null`. Non-canonical correct file -> `unchanged`. Same rule in `computeRemoveOwned`. Fixpoint holds. |
| Test-plan line for non-canonical file | PASS | test-plan/merge-settings.md: (a) no trailing newline, (b) CRLF, (c) compact -> W1 `unchanged`, bytes+mtime equal, zero renames; re-mutate returns null. |
| Git-exclude naming in ARCH | PASS | ARCH §3.3 X1 and §6 now `claude-code` / `git-exclude`; the §6a-row supersession table keeps the old form only as the superseded value. Consistent with SPEC FR-40/46. |
| `before` on `updateFileSync` | PASS | safe-write.md return type and ARCH §6 both include `before`. |
| `ConfigDirs.value` | PASS | ARCH §6 has `value`, excluded from `status --json`; status.md uses single name `_dirs`, with a JSON-exclusion test note. |
| wire-plan `legacyOc` ordering, `Step.run` signature | PASS | `legacyOc` assigned (line 41) before use (43-44); `run(dryRun)` consistent with call `s.run(dryRun)`. |
| Regressions | none found | Other modules and risk/AC mapping untouched; artifacts committed (clean tree for the feature dir). |

## Notes
- Minor (non-blocking): `computeMerged` compares via JSON.stringify (key-order sensitive); a reorder-only difference counts as changed. Acceptable, conservative, and documented.
- Stewardship: unchanged from the prior report (PASS).

Checks: 5 passed / 5 total (0 warnings that block).
