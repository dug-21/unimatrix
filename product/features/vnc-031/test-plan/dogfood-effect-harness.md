# Test Plan — dogfood-effect harness attribution repoint (GATE B)

Component: `packages/unimatrix/test/dogfood-effect.test.js` (nan-016 Component 3 effect harness).
Scope (NFR-01, R-13, OQ-C): an **attribution repoint + negative-control preservation**, NOT a harness rewrite and NOT a nan-016 RUNBOOK flip. nan-016 already shipped the clean-switch rework, so the harness already asserts the CLEAN post-state via `assertCleanPromoteState` and already carries the prune negative control `T1d`. vnc-031's job: keep those assertions GREEN once the *source* (`mergeSettings` Step 3c) — not the retired script fragment — produces the clean state, and keep the negative control non-vacuous by reconstructing the no-prune state via a path that does **not** include Step 3c.

The RUNBOOK (`product/features/nan-016/RUNBOOK.md`) carries **no** surviving-`"*"` assertion (confirmed by grep) — no RUNBOOK edit is in scope. Do not expand into a nan-016 test rewrite.

---

## What changes in the harness (GATE B)

1. **`buildSeedSettings` extended to real legacy P-shapes** (GATE C requirement) — add to `PreToolUse`: the existing `"*"` Rust uni hook (P1), a `.../index.js.bak` uni hook (P3), an old-client-dir uni hook `.../dogfood-client-OLD/lib/hook-client/index.js PreToolUse` (P4). Keep the existing foreign `Bash` hook (P7). Add a managed event whose sole stale entry sits alone in a `"*"` group (P8). All derived from one seed object (#4263). The seed remains a real live-shape mirror, never a pre-narrowed cycle-matcher seed.

2. **`noPrunePromoteContent` repointed to a genuinely-no-prune path.** Today it calls the installed `mergeSettings(..., {dryRun:true})` and relies on that NOT pruning cross-group (the nan-016 reality). Once Step 3c ships, `mergeSettings` itself prunes cross-group, so this reconstruction would no longer carry a stale hook and the negative control would go vacuous (R-13). It must be repointed to reconstruct the **no-Step-3c** post-state — e.g. mirror Step 3's repoint without the cross-group filter (compute the managed-group merge but re-inject the seed's stale `"*"` uni hook), so the fed content still carries the stale hook. The negative control's contract is unchanged: feed an unpruned post-state to the SHARED `assertCleanPromoteState` helper and assert it `throws`.

## T1 — promote → CLEAN post-state (positive, AC-09b)

Existing test; assertions stay, now satisfied by `mergeSettings` Step 3c alone (script no longer prunes):
- `assertCleanPromoteState(s, installed.dir, 8)`: zero stale uni hooks off the entrypoint (P1/P3/P4 pruned); every uni hook === `buildHookClientCommand` form; PreToolUse matcher === `PRETOOLUSE_CYCLE_MATCHER`; foreign preserved (P7); event count 8.
- explicit: no uni hook survives under `"*"` PreToolUse; emptied `"*"` group dropped, event key retained (P8).
- re-fire the installed entrypoint → exit 0, empty stdout (fail-open, unchanged).

## T1d — prune NEGATIVE CONTROL (mandatory, R-13 / GATE B)

Existing test; **preserve, repoint the reconstruction**:
- `noPrunePromoteContent(settingsPath, installed.dir)` returns the no-Step-3c post-state (still carries the stale `"*"` uni hook — sanity-asserted `staleNoPrune.length >= 1`).
- `assert.throws(() => assertCleanPromoteState(noPruneContent, installed.dir, 8))` — the SHARED helper MUST fail on the unpruned post-state. This proves the positive T1 is non-vacuous: a regression to a no-op Step 3c reproduces THIS state and turns T1 red.

`test_prune_negative_control_fails_on_unpruned` is the existing T1d; the assertion contract is unchanged, only the reconstruction source moves off `mergeSettings`-with-3c.

### Optional fail-on-break drill (R-13 scenario 3)
A delivery-time manual check (not a committed test): temporarily neuter Step 3c → T1 fails while T1d still passes → restore. Documents the positive assertion's detection power. Recorded in the coverage report as performed/observed, not left in the tree.

## T2 — promote→rollback → CLEAN Rust legacy form (rollback arm, GATE C P2/P3/P5/P7/P8)

Existing test; assertions stay, now satisfied by `mergeSettings` Step 3c alone:
- every uni hook === exact `LD_LIBRARY_PATH=<binDir> <rustBinary> hook <event>` form (P5 kept by identity).
- node-client uni hook count == 0 (P2 pruned); no `.bak` survivor (P3).
- foreign preserved (P7); no duplicates.
- second rollback byte-identical (idempotent prune + repoint).

## T3, T1b, T1c, T4 — unchanged

Isolation/code-freeze (T3), re-fire negative controls (T1b broken path, T1c malformed stdin), and the live-settings guard (T4) are **not** touched by vnc-031. They must continue to pass unmodified — re-run as regression.

## Live-surface invariants — unchanged

The `after()` hook's zero-live-write / untouched-real-dogfood / clean-worktree sha256 assertions are unchanged and must still hold (the harness still never writes live settings; it routes every script call through `tmpdirGuard`).

## Skip handling (R-05)

The suite self-skips loudly (`suiteSkipReason`) when npm/tar cannot stage the temp install. **A SKIP is not a PASS.** If the harness skips in the Stage 3c environment, GATE C's real-input parity proof is NOT satisfied — record it as an open gap in the RISK-COVERAGE-REPORT and run the harness in an environment where the install can be staged before the fragment-deletion commit lands.

## Coverage requirement (GATE B)

- Clean-state assertion non-vacuous — `assertCleanPromoteState` must `throw` on an unpruned post-state (T1d).
- Negative control **repointed, not removed** when the script prune is retired.
- No nan-016 RUNBOOK rewrite; no new harness scaffolding beyond extending `buildSeedSettings` and repointing `noPrunePromoteContent`.
