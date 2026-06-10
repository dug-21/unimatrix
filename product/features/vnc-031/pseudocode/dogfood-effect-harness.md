# Component: `dogfood-effect.test.js` — Clean-State Attribution Repoint

File: `packages/unimatrix/test/dogfood-effect.test.js`
Change: repoint the clean-state ATTRIBUTION from "script-prune produces it" to
"`mergeSettings` alone produces it"; PRESERVE the prune negative control (T1d)
by reconstructing the unpruned post-state via a path that EXCLUDES Step 3c.

> ⛔ GATE B (binding scope guard). This is an assertion-attribution repoint, NOT a
> harness rewrite. The clean-state assertions are ALREADY present (nan-016 added
> the script-level prune, so the harness already asserts the clean post-state).
> Do NOT expand into a nan-016 test rewrite (OQ-C / R-13). Two surgical changes
> only: (1) the negative-control reconstruction must stop relying on
> "mergeSettings without the script prune" because mergeSettings now prunes
> itself; (2) drop any dependence on the script's `pruneCount` output.

## The Core Problem (why this repoint is mandatory, R-13)

Today the negative control works like this:

```
noPrunePromoteContent(settingsPath, clientDir):
    result = installed.merge.mergeSettings(settingsPath, {events, commandForEvent}, {dryRun:true})
    return result.content        // "mergeSettings ALONE = no prune" — the stale "*" SURVIVES
```

T1d then asserts `assertCleanPromoteState(noPruneContent, ...)` THROWS, proving
the clean-state helper is non-vacuous.

**After vnc-031 Step 3c lands, this breaks silently:** `mergeSettings` now prunes
cross-group internally, so `noPrunePromoteContent` returns a CLEAN state, the
`assert.throws` no longer throws, and the negative control becomes vacuous (the
exact #4932 failure mode). The comment "mergeSettings ALONE / no-prune
post-state" is now false.

## The Fix — reconstruct the unpruned state WITHOUT Step 3c

The negative control needs a post-state that still carries the stale `"*"` uni
hook — i.e. the state as it was BEFORE Step 3c existed. Since Step 3c is now baked
into `mergeSettings`, reconstruct that state from the SEED directly, not by
calling the pruning `mergeSettings`.

Rename + repoint `noPrunePromoteContent` to make the no-Step-3c path explicit:

```
unprunedPromoteContent(settingsPath, clientDir):
    // Reconstruct the post-state as mergeSettings WOULD have produced it WITHOUT
    // Step 3c: the fresh managed-group entry is added under PRETOOLUSE_CYCLE_MATCHER,
    // but the legacy stale "*" PreToolUse uni hook is NOT pruned (it lived outside
    // the managed group — Step 3's in-group-only repoint never touched it).
    //
    // Build this by reading the SEED settings and applying ONLY the managed-group
    // repoint, leaving every other matcher group intact. This is the pre-Step-3c
    // behavior, reconstructed without invoking the (now-pruning) mergeSettings.
    seed   = JSON.parse(read(settingsPath))                       // the makeScratchRoot seed
    indexJs = path.join(clientDir, "lib/hook-client/index.js")

    // Apply the managed-group merge only (mirror Step 3 in-group repoint, NOT Step 3c):
    FOR each event in installed.merge.HOOK_EVENTS (after SubagentStop opt-in filtering):
        matcher    = installed.merge.EVENT_MATCHERS[event]
        freshEntry = { type:"command", command: installed.merge.buildHookClientCommand(indexJs, event) }
        ensure seed.hooks[event] is an array
        group = find/create the group with matcher === matcher
        repoint-or-push freshEntry into group.hooks (dedup uni in-group)
        // DELIBERATELY DO NOT walk other groups — the stale "*" PreToolUse uni hook survives.
    return seed
```

Implementation guidance for the developer (keep it minimal, GATE B):
- The simplest faithful reconstruction reuses the seed and performs ONLY the
  managed-matcher repoint per event, leaving foreign and stale-uni groups
  untouched — exactly the pre-Step-3c `mergeSettings` behavior. This keeps the
  negative control asserting against a genuinely unpruned post-state.
- It MUST still produce: fresh entry under `PRETOOLUSE_CYCLE_MATCHER` AND the
  stale `"*"` PreToolUse Rust uni hook still present. The existing fixture-sanity
  assertion (`staleNoPrune.length >= 1`) stays and guards this.
- Do NOT reconstruct by calling `installed.merge.mergeSettings(...)` — that now
  includes Step 3c and would re-introduce the vacuity bug.

Update T1d to call `unprunedPromoteContent` and keep the `assert.throws` against
the SHARED `assertCleanPromoteState` helper. The shared-helper guarantee
(positive T1 + negative T1d use ONE helper) is preserved (#4932).

## Positive Path (T1) — attribution repoint, assertions unchanged

T1 already asserts the CLEAN post-state via `assertCleanPromoteState` after the
REAL `promote(...)` script run. With the script retired (sibling component), the
clean state is now produced by `mergeSettings` Step 3c instead of the script
prune. T1's ASSERTIONS do not change — only their attribution does:
- The `promote()` driver still runs the (now-simplified) script.
- `assertCleanPromoteState(s, installed.dir, 8)` still passes — the clean state is
  now mergeSettings-produced.
- The explicit "stale `"*"` Rust uni hook is GONE" assertions stay verbatim.

No change to the re-fire core, the AC-03 isolation test (T3), the live-settings
guard (T4), or the tmpdir/hash safety machinery — all out of scope (GATE B).

## `pruneCount` Removal

The retired script no longer emits `prunes` / `pruneCount` (sibling component).
The harness MUST NOT read or assert on `pruneCount` / `prunes` from script output.
Search the test for any `pruneCount` / `prunes` reference in the script-output
path and remove that dependence (the clean state is asserted from PARSED SETTINGS,
not from script stdout — T1 already reads `settingsPath`, so this is a no-op for
the assertions but must be confirmed). If the script-output JSON is parsed
anywhere expecting those keys, repoint to `actions` only.

## RUNBOOK / test-plan doc note

Where the nan-016 RUNBOOK / test-plan documents "one stale `"*"` group survives"
(#4930) or attributes the clean post-state to the script's prune step, update the
prose to attribute it to `mergeSettings` Step 3c (cross-matcher migration). This
is a doc-string/comment attribution update co-located with the assertion repoint
(AC-09 / FR-16), not new test logic.

## Error Handling

- `unprunedPromoteContent` does pure in-memory reconstruction over the seed; it
  reads the seed file (already written by `makeScratchRoot`) — no new throw paths.
  Reuse the existing `JSON.parse(fs.readFileSync(...))` shape the harness already
  uses.
- The fixture-sanity assertion (`staleNoPrune.length >= 1`) is the guard that the
  reconstruction genuinely left the stale hook in place; if it ever fails, the
  reconstruction regressed to a pruning path — surface loudly (existing message).

## Key Test Scenarios (hints)

- **AC-09b / R-13 (negative control non-vacuous)** `unprunedPromoteContent` still
  carries the stale `"*"` uni hook; feeding it to `assertCleanPromoteState` still
  `assert.throws`. The control is REPOINTED (now excludes Step 3c), not deleted.
- **Fail-on-break (R-13.3)** temporarily neuter the source Step 3c → the POSITIVE
  T1 assertion fails while the negative control still passes; restore after. This
  proves the clean-state assertion is driven by the source prune.
- **T1 positive** real `promote` produces the clean state from `mergeSettings`
  alone; all existing T1 assertions green with the simplified script.
- **T2 round-trip** promote→rollback clean Rust form, idempotent, foreign
  preserved — unchanged (now driven by mergeSettings Step 3c on both arms).
- **No pruneCount dependence** the harness asserts only over parsed settings and
  `result.actions`; no `prunes` / `pruneCount` reads remain.
