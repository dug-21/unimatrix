# vnc-031 Specification — Cross-Matcher-Group Stale Unimatrix Hook Prune in `mergeSettings`

Source: `product/features/vnc-031/SCOPE.md` (Goals, Non-Goals, AC-01..AC-10, Constraints, Dependencies, Open Questions with approved resolutions) and `product/features/vnc-031/SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-07).

## Objective

Make `packages/unimatrix/lib/merge-settings.js#mergeSettings` prune stale Unimatrix-owned hook entries across **all** matcher groups — not only the managed `EVENT_MATCHERS[event]` group — for each **managed (registered-this-run) event**, so a legacy `"*"` → narrowed-matcher migration is clean for every consumer (`init`, `init --remote`, dogfood switchover). This is the root-cause fix for #728 and the source-level subsumption of the nan-016 `PRUNE_FRAGMENT` script workaround. The change is install-surface only — no Rust, server, transport, signature, or ownership-classification changes.

## Domain Vocabulary

These terms are used precisely throughout this spec and must carry the same meaning downstream.

- **Managed event** — a hook event in the `events` set that `mergeSettings` registers this run, i.e. `source.events` after SubagentStop opt-in filtering. The set actively merged in Step 3. (`HOOK_EVENTS` minus the opt-out subset.)
- **Matcher group** — one `{ matcher, hooks: [...] }` object inside `content.hooks[event]`. An event holds an array of matcher groups keyed by their `matcher` string.
- **Managed matcher group** — for a managed event, the matcher group whose `matcher === EVENT_MATCHERS[event]`. This is the single group `mergeSettings` Step 3 repoints/creates. For `PreToolUse` it is `PRETOOLUSE_CYCLE_MATCHER` (`"context_cycle|mcp__unimatrix__context_cycle"`), never `"*"`.
- **Uni-owned hook** — a hook entry for which `isUnimatrixHook(entry) === true` (matches `UNIMATRIX_PATTERNS`). The project-wide ownership signal; the codebase already freely repoints/dedups/prunes any such entry.
- **Foreign hook** — any hook entry where `isUnimatrixHook(entry) === false` (including a non-command-type entry, e.g. a `type:"url"` hook). Never pruned, ever.
- **Keep-target** — the single uni-owned entry that survives the cross-group prune for a managed event: the freshly-written managed-group entry from Step 3 (`newHookEntry`). Identified by **object identity** (the specific entry Step 3 placed/mutated), not by re-deriving and string-comparing a command (SR-01).
- **Cross-group prune** — the new behavior: for each managed event, remove every uni-owned hook in any matcher group **other than** the keep-target, drop emptied matcher groups, and retain the event key (the managed group always holds the keep-target).
- **Stale `"*"` hook** — the canonical defect instance: a legacy broad `"*"` `PreToolUse` uni hook (Rust-binary form) that survives un-repointed because it lives outside the managed `PRETOOLUSE_CYCLE_MATCHER` group (pattern #4930).

## Functional Requirements

Each FR is individually verifiable. "The result" means `mergeSettings(...).content` (and, when not dry-run, the written file, which must be byte-identical to `content`).

- **FR-01 (cross-group prune for managed events).** For every managed event, after Step 3 composes the managed group, `mergeSettings` removes every uni-owned hook entry that is **not** the keep-target, across all matcher groups of `content.hooks[event]`.
- **FR-02 (keep-target by identity, exactly one).** Exactly one uni-owned hook remains for each managed event: the keep-target, living in the `EVENT_MATCHERS[event]` group with the freshly-written command (`source.commandForEvent(event)`). The keep-target is identified by the entry-identity Step 3 produced (the object it placed or mutated), not by a re-derived string compare, so whitespace / `LD_LIBRARY_PATH` / arg-order variation in the command cannot cause the kept entry to be pruned (SR-01).
- **FR-03 (fail-loud non-zero invariant).** A managed event must never end with zero uni-owned hooks. The keep-target is excluded from pruning by construction; the design must guarantee (and tests must assert) the post-state has exactly one uni hook per managed event — never zero (SR-01).
- **FR-04 (prune rule = outside managed group, unconditional — OQ-2).** The prune removes **every** uni-owned entry outside the keep-target's managed group unconditionally. A uni-owned entry in a foreign matcher group is pruned even if its command happens to equal the fresh command. This enforces the single-owned-entry-per-event invariant and avoids a "two identical commands under two matchers" state.
- **FR-05 (foreign-hook preservation, byte-for-byte).** No foreign hook is ever removed or mutated. A foreign `"*"` `PreToolUse` hook (and any non-uni entry, including non-`command` entries) is preserved byte-for-byte. A matcher group that retains any foreign hook after pruning its uni entries is **not** dropped.
- **FR-06 (emptied-group drop, event-key retention).** A matcher group emptied solely by removing stale uni hooks is dropped from `content.hooks[event]`. The event key is retained because the managed group holds the keep-target. (Reuse the emptied-group / empty-event cleanup shape from `pruneUnimatrixEvent`.)
- **FR-07 (action-string emission).** A distinct `actions` entry is pushed for each cross-group stale-hook removal, with a stable, distinct phrase (e.g. `Removed stale unimatrix hook: <event> (cross-matcher migration)`), separate from the existing `(opt-out)`, `Updated hook`, `Added hook`, and `Removed duplicate` phrases. Under `dryRun:true` these entries carry the `[dry-run]` prefix like all others.
- **FR-08 (signature unchanged — OQ-1).** `mergeSettings(filePath, commandSource, options)` keeps its exact signature and both accepted `commandSource` shapes: legacy string (`init`) and `{events, commandForEvent}` object (`initRemote`). No new parameter, no `targetToken`, no prune-hint. The prune derives the keep-target purely from state `mergeSettings` already holds.
- **FR-09 (both call arms — SR-05).** The cross-group prune produces a clean migration identically for the string arm (`normalizeCommandSource` → `LD_LIBRARY_PATH=<binDir> <binary> hook <event>`) and the object arm (`node <clientPath> <event>`). Both consumers (`init`, `initRemote`) achieve a clean migration from `mergeSettings` alone with no init-side change.
- **FR-10 (opt-out path unchanged — Non-Goal 4).** The existing Step 3b opt-out prune (`pruneUnimatrixEvent` over `HOOK_EVENTS \ events`) is unchanged in behavior, scope, and action strings. The event partition is exact: managed-event cross-group prune covers `events`; opt-out prune covers `HOOK_EVENTS \ events`; union = `HOOK_EVENTS`, intersection = empty (SR-03).
- **FR-11 (vnc-027 adjacency preserved — SR-06).** `EVENT_MATCHERS` is untouched. The managed `PreToolUse` group retains its fresh entry under `PRETOOLUSE_CYCLE_MATCHER` (the retained cycle-event frame is never pruned). The SubagentStop opt-in/opt-out matrix (vnc-027 ADR-004 §2) is byte-unchanged.
- **FR-12 (idempotency).** `mergeSettings` remains idempotent: a second run on any seeded input — including an input with a stale `"*"` uni hook on the first run — yields `deepStrictEqual` content versus the first run. The keep-target identification must produce the same result whether or not stale entries were present on entry (SR-07).
- **FR-13 (ownership classification fixed — Non-Goal 2).** The prune uses `isUnimatrixHook` / `UNIMATRIX_PATTERNS` unchanged as the sole ownership signal. No new pattern, no relaxation. Correctness is fully bounded by `isUnimatrixHook` precision; a uni-looking-but-unclassified entry must survive in a non-managed group (SR-02).
- **FR-14 (retire script workaround — Goal 4 / AC-09).** `scripts/dogfood-switchover.sh` no longer contains a bespoke prune: the `PRUNE_FRAGMENT` (`shellTokens`, `commandReferencesTarget`, `pruneStaleUniHooks`, `emitAndWrite`) is removed. `promote` and `rollback` call `mergeSettings(..., {dryRun})` and rely on its own write (matching `initRemote`'s call shape — nan-016 ADR-003). The script's `--dry-run`, exit codes, completeness checks (R-05), env-only parameter passing, and mode handling are retained. Removal is gated on proven source parity (FR-15).
- **FR-15 (script-parity before retire — SR-04).** Before the bespoke prune is removed, every case the script's whole-shell-token matcher handled must be shown to be subsumed by the source prune on real legacy-shaped input:
  - the stale `"*"` `PreToolUse` uni hook (promote) and stale node-client uni hook (rollback) are pruned;
  - the genuine just-written command is kept (FR-02);
  - `.bak` / old-client-dir / pre-rename token variants are pruned (they are uni-owned and outside the managed group, so FR-04 prunes them unconditionally — the source rule subsumes the token heuristic);
  - the rollback dirname-level `LD_LIBRARY_PATH=<dir>` keep case is preserved by FR-02 (the legacy command's managed-group entry is the keep-target).
  The verification must run on real legacy-shaped input, not a pre-narrowed seed (#4938 discipline).
- **FR-16 (harness/runbook assertion flip — AC-09 / OQ-5).** The nan-016 dogfood effect-harness and RUNBOOK assertions that document the surviving stale `"*"` group (pattern #4930) are updated to expect a clean migration produced by `mergeSettings` alone. Where the harness currently attributes the clean post-state to the script's prune step (or asserts `pruneCount ≥ 1` from the script fragment), those assertions are updated so the clean state is satisfied by `mergeSettings` and the script no longer reports a separate `pruneCount`. The prune NEGATIVE CONTROL intent (the clean-state assertion must fail on an unpruned post-state) is preserved against the source behavior.

## Non-Functional Requirements

- **NFR-01 (surface containment).** Edits are limited to: `packages/unimatrix/lib/merge-settings.js` (logic), `packages/unimatrix/test/merge-settings.test.js` (regression), `scripts/dogfood-switchover.sh` (retire workaround), and the nan-016 dogfood effect-harness / RUNBOOK / test-plan assertion(s) that document the surviving stale group (AC-09). No other files. No `lib/init.js` change beyond what `mergeSettings` does internally.
- **NFR-02 (no signature break / back-compat).** Both `mergeSettings(fp, binaryPathString, opts)` and `mergeSettings(fp, {events, commandForEvent}, opts)` keep working for current callers with no observable change other than the intended clean migration.
- **NFR-03 (minimal change).** `merge-settings.js` is outside the C-04 `lib/hook-client` size gate (ROOT=`lib/hook-client`), so it does not count against that gate; nonetheless keep the change minimal and reuse the existing `pruneUnimatrixEvent` emptied-group/empty-event cleanup shape rather than introducing parallel machinery.
- **NFR-04 (no daemon / no network / no Rust).** Install-surface only; the prune is pure in-memory transformation of `content` plus the existing single write. It does not start/stop/probe a daemon and makes no network calls.
- **NFR-05 (output format stable).** Written output remains 2-space-indented JSON with a trailing newline (existing `JSON.stringify(content, null, 2) + "\n"`); the prune does not alter serialization.
- **NFR-06 (test discipline).** Extend existing fixtures/helpers in `merge-settings.test.js` (`tempSettingsPath`, `writeSettings`, `writeOptIn`, `seedWith*`, `expectedLocalCommand`); do not create isolated scaffolding (test infrastructure is cumulative).

## Acceptance Criteria

Each AC carries an AC-ID traceable from SCOPE.md and a verification method naming the test that asserts it. New tests extend `packages/unimatrix/test/merge-settings.test.js`; harness changes land in the nan-016 dogfood effect test.

- **AC-01 — legacy `"*"` PreToolUse migration is clean.**
  After `mergeSettings` runs on a settings file containing a legacy `"*"` `PreToolUse` uni hook (Rust-binary form), the result contains the fresh uni hook under `PRETOOLUSE_CYCLE_MATCHER` and **no** uni-owned hook under `"*"` for `PreToolUse`.
  Verification: new test `test_legacy_star_pretooluse_migrates_clean` (string arm) — seed `PreToolUse: [{matcher:"*", hooks:[{command:"/old/path/unimatrix hook PreToolUse"}]}]`; assert a `PRETOOLUSE_CYCLE_MATCHER` group with the fresh command exists and no uni hook exists under `"*"`.

- **AC-02 — exactly one uni hook per managed event, in the managed group, never zero.**
  For every managed event, exactly one uni-owned hook remains across all matcher groups, living in the `EVENT_MATCHERS[event]` group with the freshly-written command. The post-state is never zero uni hooks for a managed event (fail-loud invariant, SR-01).
  Verification: extend `test_each_event_has_exactly_one_unimatrix_entry` to a cross-group seed (each event seeded with an extra stale uni hook under a non-managed matcher); assert count === 1 per event AND that the survivor lives in `EVENT_MATCHERS[event]` with the fresh command; add an explicit `assert count !== 0` failure message. Covers FR-01, FR-02, FR-03, FR-04.

- **AC-03 — foreign hooks preserved byte-for-byte; near-miss survives.**
  Foreign (`isUnimatrixHook === false`) hooks in any matcher group — including a foreign `"*"` `PreToolUse` hook and a uni-looking-but-unclassified entry — are preserved byte-for-byte, and groups retaining a foreign hook are not dropped.
  Verification: new test `test_cross_group_preserves_foreign_star_hook` — seed both a stale uni `"*"` PreToolUse hook and a foreign `"*"` PreToolUse hook (e.g. `my-tool pre-check`); assert the foreign hook survives unchanged in its group and the stale uni hook is gone. Plus `test_cross_group_preserves_near_miss_foreign_hook` — a non-classified uni-looking command (e.g. `my-unimatrix-wrapper run`) under a non-managed matcher survives (SR-02). Covers FR-05, FR-13.

- **AC-04 — emptied group dropped, event key retained.**
  A matcher group emptied solely by removing stale uni hooks is dropped; the event key is retained because the managed group holds the keep-target.
  Verification: new test `test_cross_group_drops_emptied_group_keeps_event` — seed a `"*"` group whose only entry is a stale uni hook; assert the `"*"` group is absent post-merge, `content.hooks.PreToolUse` still exists, and contains the `PRETOOLUSE_CYCLE_MATCHER` group. Covers FR-06.

- **AC-05 — idempotency including stale-`"*"`-on-first-run.**
  `mergeSettings` is idempotent: two runs on any seeded input yield `deepStrictEqual` content, including inputs carrying a stale `"*"` uni hook on the first run.
  Verification: new test `test_cross_group_migration_idempotent` — seed the legacy `"*"` case, run twice, `assert.deepStrictEqual(first.content, second.content)`; existing `test_merge_idempotent_round_trip` and `test_subagentstop_optout_idempotent` continue to pass. Covers FR-12, SR-07.

- **AC-06 — both call arms produce a clean cross-group migration.**
  The string (`init`) arm and the object (`initRemote`) arm both produce a clean cross-group migration from the same legacy-shaped input.
  Verification: parameterized new tests `test_cross_group_migration_string_arm` and `test_cross_group_migration_object_arm` — same legacy `"*"` seed, run each `commandSource` shape, assert AC-01/AC-02 post-state for each (object-arm fresh command via `buildHookClientCommand`). Covers FR-09, SR-05.

- **AC-07 — opt-out prune for non-registered events unchanged.**
  The existing Step 3b opt-out prune for non-registered events (e.g. SubagentStop default-off) is unchanged: its tests pass and its `(opt-out)` action strings are preserved.
  Verification: existing `test_subagentstop_pruned_on_opt_out`, `test_subagentstop_optout_preserves_foreign_hook`, `test_subagentstop_optout_idempotent`, `test_subagentstop_optin_then_optout_round_trip` all pass unmodified; the new cross-group action phrase is distinct from `(opt-out)`. Covers FR-10, FR-11 (SubagentStop side), SR-03.

- **AC-08 — action emitted per cross-group removal, dry-run prefixed.**
  An `actions` entry is emitted for each cross-group stale-hook removal and is surfaced under dry-run with the `[dry-run]` prefix.
  Verification: new test `test_cross_group_emits_action_and_dry_run_prefix` — seed the legacy `"*"` case; non-dry-run asserts an action matching the cross-matcher phrase; dry-run asserts the same action present with `[dry-run]` prefix and that no file is written. Covers FR-07.

- **AC-09 — dogfood script bespoke prune retired; harness asserts clean migration from `mergeSettings`.**
  `scripts/dogfood-switchover.sh` no longer contains a bespoke prune; `promote` and `rollback` call `mergeSettings(..., {dryRun})` and rely on its write. The nan-016 dogfood effect/harness assertions are updated so the clean (no stale `"*"`) post-state is produced and asserted from `mergeSettings` alone, with the prune negative-control preserved against the source behavior, and source-parity proven on real legacy input before removal.
  Verification:
  (a) `dogfood-switchover.sh` contains no `PRUNE_FRAGMENT` / `pruneStaleUniHooks` / `commandReferencesTarget` / `emitAndWrite` and both arms call `mergeSettings(..., {dryRun})` owning the write;
  (b) `packages/unimatrix/test/dogfood-effect.test.js` clean-state assertions (zero stale uni hooks off the entrypoint; no uni hook under `"*"`) pass with the simplified script, and the negative-control still fails on an unpruned post-state;
  (c) parity check (FR-15) executed on a real legacy-shaped seed for promote and rollback before the fragment is deleted. Covers FR-14, FR-15, FR-16, SR-04.

- **AC-10 — full package suite green.**
  The full `packages/unimatrix` test suite passes, including all existing `merge-settings.test.js` cases, the new cross-group cases, the dogfood effect/switchover tests, and any `init` / `init --remote` consumer tests that assert event/group shape (pattern #4826); any fixture touch is justified as intended, not rubber-stamped.
  Verification: `node --test` over `packages/unimatrix/test/` passes; consumer init tests that iterate `HOOK_EVENTS` or assert counts are re-run; reviewer confirms each changed fixture is an intended install-surface shape change.

## User / Agent Workflows

- **Local `init`** — `init` calls `mergeSettings(settingsPath, binaryPath /* string */, {dryRun})`. On a project whose live `.claude/settings.json` carries a legacy `"*"` `PreToolUse` uni hook, the merge now yields a clean single-uni-hook-per-event result with no init-side change. (This repo's live settings had exactly this shape.)
- **Remote `init --remote`** — `initRemote` calls `mergeSettings(settingsPath, {events: HOOK_EVENTS, commandForEvent}, {dryRun})`. Same clean migration via the object arm.
- **Dogfood promote/rollback** — the operator runs `scripts/dogfood-switchover.sh promote|rollback [--dry-run]`; the script calls `mergeSettings(..., {dryRun})` and relies on its write. Promote/rollback share one battle-tested ownership-aware path (nan-016 ADR-003 intent), with no bespoke prune to drift.

## Constraints

- **Surface (C):** only the four file areas in NFR-01. No other files; no `lib/init.js` logic change.
- **Ownership identification fixed:** reuse `isUnimatrixHook` / `UNIMATRIX_PATTERNS` unchanged (Non-Goal 2).
- **No new foreign-hook pruning, ever:** prune strictly scoped by `isUnimatrixHook`, identical to `pruneUnimatrixEvent` scope (Non-Goal 3).
- **Back-compat call shapes:** no signature break for `init`/`initRemote` (Non-Goal 6, OQ-1).
- **vnc-027 adjacency:** matcher-narrowing and SubagentStop opt-in semantics (ADR-004 #4811) preserved exactly; additive prune, not a redesign (Non-Goal 1).
- **Event partition is load-bearing:** managed-event cross-group prune covers `events`; opt-out prune covers `HOOK_EVENTS \ events`; union = all, intersection = empty (SR-03).
- **Script retire gated on parity:** do not delete the script prune until source parity is proven on real legacy-shaped input (SR-04, #4938).
- **Behavioral change sign-off:** `isUnimatrixHook === true` entries are treated as reconcilable Unimatrix-owned state — a user cannot pin a uni-pattern hook under a non-managed matcher (OQ-4, human-approved = prune). This is the one genuine survive→pruned behavioral change.
- **No daemon / no network / no Rust changes** (Non-Goal 5).

## Dependencies

- **Replacement observation source (SHIPPED).** The data source superseding the pruned broad `"*"` `PreToolUse` per-event observations — `PostToolUse` (duplicate signal) plus the transcript-fed cycle-review distillation (crt-052 ADR-004/006, #706) paired with vnc-027 ADR-004 matcher-narrowing (#4811) — is merged on `main`. Architect must verify both are present on the delivery base branch (not merely "on main as of writing") before pruning, so the broad-hook prune does not race a missing replacement (SCOPE Assumption 1).
- **Out of scope (depended-on, not modified):** `context_cycle_review` and its observation/transcript consumption are server-side Rust (`crates/unimatrix-server`, `crates/unimatrix-observe`); vnc-031 depends on them but does not own, modify, or re-validate them (Non-Goal 5).
- **Existing primitives reused:** `isUnimatrixHook`, `UNIMATRIX_PATTERNS`, `EVENT_MATCHERS`, `PRETOOLUSE_CYCLE_MATCHER`, `normalizeCommandSource`, `buildHookClientCommand`, and the `pruneUnimatrixEvent` cleanup shape — all in `lib/merge-settings.js`.
- **Consumers exercised:** `lib/init.js#init` (string arm) and `#initRemote` (object arm); `scripts/dogfood-switchover.sh`; `packages/unimatrix/test/dogfood-effect.test.js` and the nan-016 RUNBOOK / test-plan docs.
- **Test harness:** `node --test` (built-in `node:test`), `assert`; existing fixtures in `merge-settings.test.js`.

## NOT in Scope

- Changing `EVENT_MATCHERS`, the matcher-narrowing decision, or the SubagentStop opt-in matrix (vnc-027 ADR-004).
- Changing `isUnimatrixHook` or `UNIMATRIX_PATTERNS`.
- Pruning foreign (non-uni) hooks under any circumstance.
- Changing the opt-out prune (`pruneUnimatrixEvent`) behavior, scope, or action strings.
- Any `mergeSettings` signature change, new parameter, or `targetToken` hint.
- Re-litigating which events register (`init`/`initRemote` pass full `HOOK_EVENTS`; SubagentStop default-off filtering unchanged).
- Any client-side / Rust-binary / server / transport / daemon / network change.
- Modifying the replacement observation/analysis path (`context_cycle_review`, distillation, reconstruction).
- Any `lib/init.js` logic change beyond inheriting `mergeSettings`' new internal behavior.

## Key Decisions / Interpretations

- **OQ-1 → no signature change.** The keep-target is derived from state `mergeSettings` already holds; no `targetToken`, no caller change. (FR-08)
- **OQ-2 → prune all uni entries outside the managed group unconditionally** — even one whose command coincidentally equals the fresh command — to enforce single-owned-entry-per-event. (FR-04)
- **OQ-3 → cross-group prune applies to managed events only;** non-registered events remain covered by the existing opt-out prune. Partition union = all, intersection = empty. (FR-10, SR-03)
- **OQ-4 → prune (human-approved).** Any `isUnimatrixHook === true` entry is reconcilable; users cannot pin a uni-pattern hook under a non-managed matcher.
- **OQ-5 / AC-09 → harness + RUNBOOK updates are in scope** for vnc-031 (not a nan-016 follow-up).
- **SR-01 keep-target is entry-identity, not string compare** — the writer knows which object it kept; the keep guarantee must be structural so a managed event can never silently drop to zero uni hooks. (FR-02, FR-03)
- **SR-04 parity-before-retire** — the script's whole-shell-token cases map onto the source rule "prune all uni entries outside the managed group" (which subsumes `.bak`/old-dir tokens by being unconditional, and preserves the rollback dirname case via the keep-target). Prove on real legacy input before deleting the fragment. (FR-15)

## Open Questions for Architect / Human

- **OQ-A (keep-target mechanism — architect).** SR-01 requires identity-based keep, but Step 3 currently does not return or tag the entry it placed/mutated. The architect must choose the structural mechanism (e.g. tag/track the `newHookEntry` object per event and exclude it by reference in the prune, or fold the prune into Step 3 so the kept entry is known in-loop) such that command-string variation can never prune the survivor. Recommendation noted; mechanism is an architecture decision.
- **OQ-B (delivery-base dependency check — architect/human).** SCOPE Assumption 1 requires confirming crt-052 #706 and vnc-027 #4811 are present on the actual delivery base branch, not merely on `main`. If either is absent on the base, the prune removes telemetry without a live replacement — confirm before delivery.
- **OQ-C (nan-016 harness attribution — for the SM/architect).** The nan-016 dogfood-effect tests already assert a clean post-state (count of stale `"*"` uni hooks == 0) because nan-016 added the script-level prune. AC-09's work is therefore to (a) make `mergeSettings` itself produce that clean state, (b) retire the script fragment, and (c) ensure the harness's clean-state assertions and prune negative-control still hold against the source behavior — including how/whether the script still reports `pruneCount`. Confirm the harness changes stay within the assertion-flip scope and don't expand into a nan-016 test rewrite.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned vnc-030/vnc-027 install-surface decisions and pattern #4809 (verify event registration before keying hook behavior); no entry directly covering the cross-group prune. Read-only tier; no storage.
