# Gate 3b Report: vnc-031

> Gate: 3b (Code Review)
> Date: 2026-06-11
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | merge-settings Step 3c, switchover retire, dogfood-effect repoint all match Stage 3a pseudocode |
| 2. Architecture compliance (ADR-001/002/003) | PASS | Keep-target is object identity (`hook !== kept`); Step 3c after Step 3, before Step 3b; script retired with proven parity |
| 3. Interface implementation | PASS | `mergeSettings` signature unchanged; `isUnimatrixHook`/`UNIMATRIX_PATTERNS` byte-unchanged vs main |
| 4. Test case alignment | PASS | Cross-group tests map to AC-01..AC-09 and R-01..R-15; GATE B/C honored |
| 5. Code quality | PASS | No stubs/TODO; both source files < 500 lines; JS syntax clean |
| 6. Security | PASS | Prune bounded by unchanged `isUnimatrixHook`; script removed command-tokenizer (surface shrinks); env-only param passing |
| 7. Knowledge stewardship | N/A | Delivery-agent reports not in scope of this spawn; gate validates committed code |

Verification: `node --test merge-settings.test.js` → 73 pass / 0 fail / 0 skip; `node --test dogfood-effect.test.js` → 8 pass / 0 fail / 0 skip.

## Detailed Findings

### 1. Pseudocode fidelity
**Status**: PASS
**Evidence**:
- `merge-settings.js:329-374` implements `keptEntryByEvent` capture (one line per event, after merge resolves, line 334) and the Step 3c cross-group prune loop exactly as `pseudocode/merge-settings-step3c.md` specifies — `group.hooks.filter(hook => !(isUnimatrixHook(hook) && hook !== kept))`, emptied-group drop with event-key retention (no `delete content.hooks[event]`).
- `scripts/dogfood-switchover.sh:152-183` — `run_promote`/`run_rollback` collapse to a plain `mergeSettings(..., { dryRun })` heredoc owning its write, matching `dogfood-switchover-retire.md`. `fs` import removed; `path` retained only in promote (used for `path.join`).
- `dogfood-effect.test.js:434-471` `unprunedPromoteContent` reconstructs the no-Step-3c post-state directly from the seed (Step-3-only repoint, no cross-group walk), exactly per `dogfood-effect-harness.md`. Does NOT route through the now-pruning `mergeSettings`.

### 2. Architecture compliance (ADR-001/002/003)
**Status**: PASS
**Evidence**:
- **ADR-001 (CRITICAL, keep-by-identity).** Inspected `merge-settings.js:357-359`: keep test is `hook !== kept` (object reference inequality against the `newHookEntry` Step 3 placed, captured at line 334). NO `.command ===`, NO `.includes`, NO tokenize. The only `.includes` occurrences in the file (lines 220, 382) are `events.includes(...)` event-set membership — unrelated to the keep test. R-01 discriminating tests (`test_cross_group_stale_twin_differing_only_by_shape_pruned`, `test_cross_group_survivor_is_exact_fresh_command_not_substring`) seed shape-varying near-twins and pass — a string-compare reimplementation would turn them red.
- **ADR-002.** Step 3c (line 345) runs over `events` (managed) after Step 3 and before Step 3b (line 381, over `HOOK_EVENTS \ events`). Step 3b opt-out path unchanged (no diff to `pruneUnimatrixEvent` body). Event key retained (line 371 filter never deletes the key). New action `Removed stale unimatrix hook: <event> (cross-matcher migration)` is substring-disjoint from `(opt-out)`.
- **ADR-002 AC-02 fail-loud.** `test_each_event_has_exactly_one_unimatrix_entry_cross_group` (test line 923) asserts `count !== 0` with a self-identifying message per managed event, plus `count === 1` and survivor location/command.
- **ADR-003.** Script contains none of `PRUNE_FRAGMENT`, `pruneStaleUniHooks`, `commandReferencesTarget`, `shellTokens`, `emitAndWrite` (the single `targetToken` hit is a retirement comment, line 24). Both arms call `mergeSettings(..., {dryRun})`; no `fs.writeFileSync` remains in the script. `--dry-run`, exit codes 2/5/7, completeness checks (R-05) preserved.

### 3. Interface implementation
**Status**: PASS
**Evidence**: `mergeSettings(filePath, commandSource, options)` signature unchanged (line 203); no `targetToken`/prune-hint param (FR-08/R-15). `git diff main...feature/vnc-031` on `merge-settings.js` shows zero changes to `UNIMATRIX_PATTERNS` or `function isUnimatrixHook` (Non-Goal 2). Surface containment (NFR-01) exact: only the four allowed files changed.

### 4. Test case alignment
**Status**: PASS
**Evidence**:
- 28 new cross-group tests in `merge-settings.test.js` covering AC-01..AC-08 and R-01..R-12 (identity discriminators, fail-loud zero, in+cross-group dedup, foreign/near-miss/non-command preservation, emptied-group drop, idempotency incl. stale-on-run-1, both arms, partition combined-seed, action+dry-run prefix, quoted spaced-path keep, coincidentally-identical pruned, malformed-entry-as-foreign).
- **R-01 (identity discriminator)**: covered and genuinely discriminating (shape-varying near-twins asserted to exact fresh command).
- **R-02 (fail-loud)**: explicit `assert(count !== 0, ...)`; adversarial no-managed-entry-on-input case (`test_cross_group_only_stale_on_input_managed_entry_created_then_kept`).
- **R-07 (near-miss foreign)**: `test_cross_group_preserves_near_miss_foreign_hook` + non-command entry preservation.
- **R-04 (parity)**: T-PARITY (`dogfood-effect.test.js:695`) calls `mergeSettings` directly on REAL legacy-shaped input (seed carries genuine `"*"` Rust PreToolUse hook + `.bak` + old-client-dir + foreign), proving P1-P5/P7/P8 for both arms; P6 proven at unit level. Precondition assertions verify the seed is real-legacy, not pre-narrowed.
- **GATE B (negative control non-vacuous)**: T1d feeds `unprunedPromoteContent` (reconstructed via a path EXCLUDING Step 3c) to the SHARED `assertCleanPromoteState` helper and asserts `assert.throws`; fixture-sanity `staleNoPrune.length >= 1` guards non-vacuity.
- **GATE C ordering**: parity-proof commit 7bf45fbe is a git ancestor of fragment-deletion commit a4ac286b (verified via `git merge-base --is-ancestor`).

### 5. Code quality
**Status**: PASS
**Evidence**: No `TODO`/`FIXME`/`unimplemented`/placeholder in changed files (sole "placeholder" hit is a comment asserting a value is NOT a placeholder). `merge-settings.js` 409 lines, `dogfood-switchover.sh` 185 lines — both under the 500-line gate. `node -c` clean on all three JS files; `sh -n` clean on the script.

### 6. Security
**Status**: PASS
**Evidence**: Step 3c is pure in-memory filtering of already-validated `content` by the unchanged `isUnimatrixHook` predicate — no new throw path, no new file read, no `eval`/dynamic require. Prune blast radius bounded entirely by `isUnimatrixHook` precision (R-07 near-miss test confirms uni-looking-but-unclassified survives). Script retire REMOVES the command-string tokenizer (`commandReferencesTarget`/`shellTokens`), shrinking the surface; parameters still reach node via env only (no string interpolation, no path injection). No hardcoded secrets. Malformed/null entries handled by `isUnimatrixHook` guard (treated as foreign).

### 7. Knowledge stewardship
**Status**: N/A — this gate spawn validates the committed implementation against pseudocode/architecture. Delivery-agent stewardship blocks are evaluated by the coordinator against agent reports, not by this code-review gate.

## Rework Required

None.
