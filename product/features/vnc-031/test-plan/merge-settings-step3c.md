# Test Plan — `mergeSettings` Step 3c (cross-group prune)

Component: `packages/unimatrix/lib/merge-settings.js#mergeSettings` (new Step 3c).
Tests: extend `packages/unimatrix/test/merge-settings.test.js` (cumulative — reuse `tempSettingsPath`, `writeSettings`, `writeOptIn`, `BINARY`, `BIN_DIR`, `expectedLocalCommand`, `DEFAULT_EVENTS`, `buildHookClientCommand`, `isUnimatrixHook`, `PRETOOLUSE_CYCLE_MATCHER`, `HOOK_EVENTS`, `EVENT_MATCHERS`).

New shared helper to add (derive everything from one source, #4263):

```
// seed a managed event with the fresh-shaped managed group AND an extra stale
// uni hook under a DIFFERENT (non-managed) matcher group. staleCommand defaults
// to a Rust "*"-PreToolUse legacy form; foreign optionally adds a foreign hook.
seedWithCrossGroupStale(fp, { event = "PreToolUse", staleMatcher = "*",
  staleCommand, foreign })   // writes settings.json via writeSettings
```

All assertions on the *survivor* use **exact equality** to the arm's fresh command (`assert.strictEqual(cmd, expectedLocalCommand(event))` / `=== buildHookClientCommand(client, event)`), never `includes` — exact-command is the structural proxy for object identity (R-01 §scenario 3).

---

## AC-01 — legacy `"*"` PreToolUse migrates clean (R-01)

`test_legacy_star_pretooluse_migrates_clean`

- **Arrange:** `writeSettings(fp, { hooks: { PreToolUse: [{ matcher: "*", hooks: [{ type:"command", command: "/old/path/unimatrix hook PreToolUse" }] }] } })`.
- **Act:** `mergeSettings(fp, BINARY, {})`.
- **Assert:**
  - a `PRETOOLUSE_CYCLE_MATCHER` group exists; its single uni hook `command === expectedLocalCommand("PreToolUse")`.
  - **no** uni hook exists under any `"*"` matcher for `PreToolUse` (`PreToolUse.find(g=>g.matcher==="*" && g.hooks.some(isUnimatrixHook))` is undefined).
  - exactly one uni hook total for `PreToolUse`.

## R-01 (Critical) — identity must not degrade to string compare

The discriminating tests. **A `command ===` reimplementation must turn at least one red.**

`test_cross_group_stale_twin_differing_only_by_shape_pruned`
- **Arrange:** seed `SessionStart` with the fresh managed group absent on input, and a stale uni hook under a non-managed matcher whose command is byte-equal to the *fresh* command **except** collapsible whitespace / arg spacing — e.g. stale `unimatrix  hook   SessionStart` vs fresh `LD_LIBRARY_PATH=<dir> <bin> hook SessionStart`. (Use a stale form that `isUnimatrixHook` still classifies true but differs in shape from `expectedLocalCommand`.)
- **Act:** `mergeSettings(fp, BINARY, {})`.
- **Assert:** exactly one uni hook for `SessionStart`; it lives in `EVENT_MATCHERS.SessionStart` group; `command === expectedLocalCommand("SessionStart")` (exact); the near-twin's matcher group no longer holds a uni hook. (A `command===fresh` keep-rule would keep the wrong object or both.)

`test_cross_group_pretooluse_star_shares_prefix_with_cycle_survivor`
- **Arrange:** stale `"*"` Rust PreToolUse uni hook whose command shares a long common prefix with the fresh cycle command.
- **Assert:** the `"*"` uni hook is gone; the survivor is under `PRETOOLUSE_CYCLE_MATCHER` with the exact fresh command.

`test_cross_group_survivor_is_exact_fresh_command_not_substring`
- Structural proxy for object identity: for the managed event, exactly one uni entry AND `command === source-fresh-command` **exactly** (not `includes`). Pair shape-variance so a substring/`includes` keep-rule would also fail.

## AC-02 — exactly one uni hook per managed event, never zero (R-02, fail-loud)

`test_each_event_has_exactly_one_unimatrix_entry` (extend existing)
- **Arrange:** for every event in `DEFAULT_EVENTS`, seed an extra stale uni hook under a non-managed matcher group (in addition to the normal merge). Then merge.
- **Assert per event:**
  - `count === 1`;
  - the survivor's group matcher `=== EVENT_MATCHERS[event]` and `command === expectedLocalCommand(event)`;
  - explicit `assert(count !== 0, "managed event " + event + " dropped to zero uni hooks")` with that self-identifying message (FR-03).

`test_cross_group_only_stale_on_input_managed_entry_created_then_kept` (R-02 scenario 2 — Step 3↔3c integration)
- **Arrange:** the **only** uni hook present pre-merge is the stale one under a non-managed matcher (no uni hook in the managed group on input).
- **Assert:** Step 3 creates the managed entry, Step 3c does NOT prune it; `count === 1` in the managed group. (Guards a capture-before-create refactor bug.)

## R-03 — wrong-scope prune in/out managed group

`test_cross_group_in_group_dup_plus_cross_group_stale` (extends `test_dedup_removes_extra_unimatrix_hooks`)
- **Arrange:** managed group seeded with TWO uni entries (in-group dup) PLUS a stale uni hook in a non-managed group.
- **Assert:** exactly one uni hook total, in the managed group, `command === expectedLocalCommand(event)`; a `Removed duplicate` action present (Step 3) AND a cross-matcher action present (Step 3c).

`test_cross_group_multiple_stale_groups_all_pruned`
- **Arrange:** one event with three matcher groups each holding a uni hook (managed + two stale foreign-matcher groups).
- **Assert:** both stale groups lose their uni hook, only the managed survivor remains, one cross-matcher action per stale group.

## AC-03 — foreign + near-miss + non-command preserved (R-07)

`test_cross_group_preserves_foreign_star_hook`
- **Arrange:** stale uni `"*"` PreToolUse hook AND foreign `"*"` PreToolUse hook (`my-tool pre-check`).
- **Assert:** foreign hook survives byte-for-byte in its group; stale uni hook gone; foreign-retaining group NOT dropped.

`test_cross_group_preserves_near_miss_foreign_hook` (security-relevant, SR-02)
- **Arrange:** a uni-**looking** but unclassified command under a non-managed matcher — `my-unimatrix-wrapper run` (contains `unimatrix`, fails `UNIMATRIX_PATTERNS` anchors so `isUnimatrixHook===false`).
- **Assert:** it survives byte-for-byte. (The discriminating test a too-loose ownership check fails.)

`test_cross_group_preserves_non_command_entry` (extends `test_hook_entry_without_type_command_is_preserved`)
- **Arrange:** a `{ type:"url", url:"..." }` entry in a non-managed group of a managed event.
- **Assert:** preserved unchanged.

## AC-04 — emptied group dropped, event key retained (R-08)

`test_cross_group_drops_emptied_group_keeps_event`
- **Arrange:** a `"*"` group whose only entry is a stale uni hook.
- **Assert:** the `"*"` group is absent post-merge; `content.hooks.PreToolUse` still exists and contains the `PRETOOLUSE_CYCLE_MATCHER` group.

`test_cross_group_foreign_retaining_group_not_dropped` (R-08 / R-07.4)
- **Arrange:** a `"*"` group with a stale uni hook + a foreign hook.
- **Assert:** the group remains with only the foreign hook.

## AC-05 — idempotency incl. stale-`"*"`-on-first-run (R-06)

`test_cross_group_migration_idempotent`
- **Arrange:** seed legacy `"*"` case; run twice.
- **Assert:** `assert.deepStrictEqual(first.content, second.content)`.

`test_cross_group_three_run_stability` (extends `test_three_consecutive_merges_no_growth`)
- **Arrange:** multi-stale seed (R-04 P-shape: `"*"` + `.bak` + old-dir under one event).
- **Assert:** runs 2 and 3 are no-ops (no cross-matcher action emitted on run 2+; `deepStrictEqual` run2 vs run3).
- **Also:** existing `test_merge_idempotent_round_trip`, `test_subagentstop_optout_idempotent` pass unmodified.

## AC-06 — both call arms identical (R-05, R-15)

`test_cross_group_migration_string_arm`
- `mergeSettings(fp, BINARY, {})` on the legacy `"*"` seed → AC-01/AC-02 post-state; survivor `command === expectedLocalCommand("PreToolUse")`.

`test_cross_group_migration_object_arm`
- `mergeSettings(fp, { events: HOOK_EVENTS, commandForEvent: (e)=>buildHookClientCommand(clientPath, e) }, {})` on the **same** legacy `"*"` seed → AC-01/AC-02 post-state; survivor `command === buildHookClientCommand(clientPath, "PreToolUse")`.
- Both assert single-survivor-per-event; passing both with no per-consumer branch proves SR-05. No new parameter (R-15) — both call the current signature.

## AC-07 — opt-out path unchanged + partition seam (R-09)

- Existing `test_subagentstop_pruned_on_opt_out`, `test_subagentstop_optout_preserves_foreign_hook`, `test_subagentstop_optout_idempotent`, `test_subagentstop_optin_then_optout_round_trip` pass **unmodified**.

`test_partition_combined_subagentstop_optout_and_pretooluse_cross_group`
- **Arrange:** one file with a stale SubagentStop uni hook (non-registered → Step 3b opt-out) AND a stale `"*"` PreToolUse uni hook (registered → Step 3c). No opt-in.
- **Assert:** both removed; the SubagentStop removal emits the `(opt-out)` phrase, the PreToolUse removal emits the cross-matcher phrase; **no double emission**; neither path emits the other's phrase. (Probes 3→3c→3b ordering, SR-03 partition.)

## R-10 — vnc-027 adjacency preserved

- Existing `test_pretooluse_matcher_exactly_cycle_tools`, `test_all_other_matchers_unchanged`, SubagentStop opt-in matrix tests pass **unmodified**.

`test_cross_group_pretooluse_survivor_under_cycle_matcher` (FR-11)
- After migration from a stale `"*"` seed, the surviving PreToolUse uni entry is under `PRETOOLUSE_CYCLE_MATCHER` (cycle frame = keep-target, never pruned).

`test_cross_group_subagentstop_optin_composes` (R-10)
- **Arrange:** `writeOptIn(fp, true)` + a stale cross-group SubagentStop uni hook under a non-`"*"` matcher.
- **Assert:** the SubagentStop survivor is the fresh `"*"` managed entry; the stale cross-group entry is pruned (opt-in registration + cross-group prune compose).

## AC-08 — action string contract (R-11)

`test_cross_group_emits_action_and_dry_run_prefix`
- **Arrange:** seed legacy `"*"`.
- **Assert non-dry-run:** an action matching `Removed stale unimatrix hook: PreToolUse (cross-matcher migration)`.
- **Assert dry-run:** same action present with `[dry-run] ` prefix; **no file written** (`!fs.existsSync(fp)`).
- **Assert disjoint:** the cross-matcher phrase is substring-disjoint from `(opt-out)`, `Updated hook`, `Added hook`, `Removed duplicate`.
- One action per group that lost ≥1 uni entry.

## P6 quoted-spaced-path keep-target (R-01 / GATE C P6, unit-level)

`test_cross_group_quoted_spaced_path_target_kept`
- **Arrange:** managed group's keep-target command is the quoted spaced-path form (e.g. object arm `node "/a b/lib/hook-client/index.js" PreToolUse`) plus a stale uni hook under `"*"`.
- **Assert:** the quoted keep-target is kept (object identity — quoting irrelevant, no tokenizer), stale pruned. (The #4931 failure mode is gone by construction; this is the unit proof referenced by GATE C P6.)

## Edge cases (Risk Strategy §Edge Cases)

- `test_cross_group_coincidentally_identical_command_pruned` — a uni hook in a foreign group whose command byte-equals the fresh command is **still pruned** (different object, FR-04); assert no "two identical commands under two matchers" end-state.
- `test_cross_group_malformed_entry_treated_as_foreign` — a `null` entry / non-string `command` inside a non-managed group's `hooks`: no throw; `isUnimatrixHook` returns false; the malformed entry is left untouched.
- `test_cross_group_group_missing_hooks_key_skipped` — a group with `hooks: []` or no `hooks` key: `Array.isArray` guard skips it, no throw.

## Coverage requirement (component)

- R-01 mitigated only when a string/`includes`-compare reimplementation makes ≥1 test red (shape-varying near-twin + exact-fresh-command assert).
- R-02 zero-uni-hook is a hard, self-identifying failure, exercised on a seed with no pre-existing managed-group uni entry.
- R-07 near-miss + non-command foreign survive; no foreign hook mutated/removed under any seed.
