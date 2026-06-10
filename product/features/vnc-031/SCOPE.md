# vnc-031: Prune Stale Cross-Matcher-Group Unimatrix Hooks During mergeSettings

## Problem Statement

`packages/unimatrix/lib/merge-settings.js` (`mergeSettings`) reconciles Unimatrix hook
entries only inside the matcher group it manages per event — `EVENT_MATCHERS[event]`.
For `PreToolUse` that is the narrowed `PRETOOLUSE_CYCLE_MATCHER`
(`"context_cycle|mcp__unimatrix__context_cycle"`, vnc-027 ADR-004 §1), never `"*"`.

It does **not** prune Unimatrix-owned hooks (per `isUnimatrixHook`) that live in *other*
matcher groups for the same event. So a settings file carrying a legacy `"*"` `PreToolUse`
Unimatrix hook (Rust-binary form) keeps that stale hook after merge: the result has the
fresh hook under the narrowed matcher **and** the stale Rust hook under `"*"`.
`isUnimatrixHook` recognizes both, but only the managed-group entry is repointed; the other
is a "foreign-matcher group" from `mergeSettings`' view and survives un-repointed (pattern
#4930).

**Who is affected:** every `mergeSettings` consumer — `init` (local binary), `init --remote`
(node-client), and `scripts/dogfood-switchover.sh` — for any project whose `.claude/settings.json`
has a legacy or hand-written `"*"`-shaped Unimatrix `PreToolUse` hook. This repo's own live
settings had exactly this shape.

**Why now:** discovered during nan-016 (#681) delivery — a dogfood switchover left 8 of 9 uni
hooks repointed and one stale Rust `PreToolUse` hook still firing, producing an unclean soak.
nan-016 shipped a **script-level prune workaround** in `scripts/dogfood-switchover.sh`
(the `PRUNE_FRAGMENT` / `pruneStaleUniHooks`), but C-8 froze `lib/merge-settings.js` for that
feature, so the root-cause fix was deferred here. Until fixed, every consumer must reimplement
the same prune, and ordinary `init` on an affected project migrates uncleanly.

**The stale `"*"` hook is not pure waste — pruning it completes a data-source migration.**
Under the old Rust client, the broad `"*"` `PreToolUse` hook fired on every tool call and recorded
a per-event observation, and `context_cycle_review` analyzed that per-tool-call telemetry. So in the
old model the broad hook carried real value. vnc-027 ADR-004 (#4811) already decided to retire it,
on two grounds: (a) the standalone `PreToolUse` observation **duplicates the `PostToolUse` signal**
(every tool call fires both), so narrowing the matcher to `PRETOOLUSE_CYCLE_MATCHER` loses "no signal
`PostToolUse` already carries"; and (b) the richer, replacement observation source — the full
transcript-fed distillation that `context_cycle_review` now consumes (crt-052, **shipped** as #706) —
supersedes per-event hook records for the analysis they previously enabled. The cycle-event
`PreToolUse` frames (`context_cycle`) are *retained* under the narrowed matcher; only the broad
per-tool-call firing is dropped. Therefore pruning the stale `"*"` hook is not "zero loss" in the
abstract: it **finishes a data-source migration** (per-event hook observations → `PostToolUse` +
transcript-fed distillation) that vnc-027's matcher-narrowing (ADR-004) set in motion. vnc-031
completes the install-surface cleanup; it does **not** originate the information-loss decision and does
not touch the observation/analysis path that consumes the replacement source (see Dependencies,
Non-Goal 5).

## Goals

1. Make `mergeSettings` prune stale Unimatrix-owned hook entries across **all** matcher
   groups (not just the managed `EVENT_MATCHERS[event]` group) for each **managed event**,
   so a legacy `"*"` → narrowed-matcher migration is clean for all callers.
2. Preserve every existing semantic: ownership scoping via `isUnimatrixHook`, in-place repoint
   of the managed-group entry, duplicate removal, foreign-hook preservation, dropping emptied
   matcher groups and event keys, and full idempotency.
3. Add regression coverage in `packages/unimatrix/test/merge-settings.test.js` for the
   legacy-`"*"` → narrowed-`PreToolUse` migration and the cross-group prune invariants.
4. Retire the bespoke prune in `scripts/dogfood-switchover.sh` once the source behavior
   subsumes it, so promote/rollback rely on shipped `mergeSettings` alone (the nan-016 ADR-003
   intent: one battle-tested code path).

## Non-Goals

1. **No change to the matcher-narrowing decision itself.** `EVENT_MATCHERS.PreToolUse` stays
   `PRETOOLUSE_CYCLE_MATCHER`; the SubagentStop opt-in matrix (vnc-027 ADR-004 §2) is untouched.
2. **No change to `isUnimatrixHook` or `UNIMATRIX_PATTERNS`.** Ownership identification is the
   existing contract; this feature only acts on what it already classifies.
3. **No new pruning of foreign (non-Unimatrix) hooks**, ever — the prune is strictly scoped by
   `isUnimatrixHook`, identical to the existing `pruneUnimatrixEvent` scope.
4. **No change to the opt-out prune for non-registered events** (`pruneUnimatrixEvent` over
   `HOOK_EVENTS \ events`); that path already prunes across all matcher groups for events we are
   NOT registering. This feature closes the gap for events we **are** registering.
5. **No client-side / Rust-binary / transport changes.** Scope is the npm package install surface
   only.
6. **No new CLI flags or `mergeSettings` signature change** unless design proves an additive,
   back-compatible target-hint is strictly required (see Open Questions OQ-1). Default behavior
   change must hold for the existing call shapes used by `init`/`initRemote`.
7. **No re-litigation of which events register** — `init`/`initRemote` pass the full `HOOK_EVENTS`
   set; SubagentStop default-off filtering is unchanged.

## Background Research

### `mergeSettings` structure (lib/merge-settings.js)
- Step 3 loops registered `events`, computes `matcher = EVENT_MATCHERS[event]` and the fresh
  `newHookEntry` (`{type:"command", command: source.commandForEvent(event)}`). It finds/creates
  the group whose `matcher === EVENT_MATCHERS[event]`, repoints the first uni hook in place, and
  dedups extras — **only within that one group** (lines 258–322). Stale uni hooks in any *other*
  group for that event are never visited. This is the bug.
- Step 3b (`pruneUnimatrixEvent`, lines 324–333) already prunes uni-owned entries across **all**
  matcher groups — but only for events **not** in the registered set. It is the exact prune shape
  we want, just applied to the wrong event subset. It drops emptied groups, deletes the event key
  if empty, scopes by `isUnimatrixHook`, and pushes an `"...(opt-out)"` action per removal.
- `isUnimatrixHook` is the single ownership signal used everywhere (repoint, dedup, prune). The
  codebase already treats any `isUnimatrixHook === true` entry as Unimatrix-owned and freely
  rewrites/removes it. So "prune stale uni hooks in other groups" is consistent with established
  semantics — it is not introducing a new ownership concept.
- Idempotency is an existing tested invariant (`test_merge_idempotent_round_trip`,
  `test_subagentstop_optout_idempotent`). The fix must preserve `deepStrictEqual(first, second)`.

### The dogfood workaround being root-caused (scripts/dogfood-switchover.sh)
- `PRUNE_FRAGMENT` → `pruneStaleUniHooks(content, targetToken, isUnimatrixHook, allowDirname)`:
  after calling `mergeSettings(..., {dryRun:true})`, it walks **all** events/groups and removes
  uni-owned entries whose command does **not** reference the switch target token, drops emptied
  groups + event keys, preserves foreign hooks, then owns the single `writeFile`.
- Critical nuance: it matches the target by **whole shell token equality** (quote-aware
  `shellTokens`), not naive `includes` — so a stale hook at `.../index.js.bak` or an old client
  dir is a different token and is correctly pruned, while the genuine just-written command is kept.
  Rollback additionally accepts a dirname-level match for the `LD_LIBRARY_PATH=<dir>` prefix.
- This is the reference implementation for the source fix. The source-level version differs in
  one important way: `mergeSettings` already *knows* the fresh command it wrote
  (`source.commandForEvent(event)` / `newHookEntry.command`), so it does not need a token-matching
  heuristic against an externally-supplied `targetToken` — it can compare against the entry it
  just composed for that event (see Proposed Approach).

### Consumers (lib/init.js)
- `init` (local): `mergeSettings(settingsPath, binaryPath /* string */, {dryRun})` — the legacy
  string arm; `normalizeCommandSource` emits `LD_LIBRARY_PATH=<binDir> <binary> hook <event>` over
  `HOOK_EVENTS`.
- `initRemote`: `mergeSettings(settingsPath, {events: HOOK_EVENTS, commandForEvent}, {dryRun})` —
  object arm; emits `node <clientPath> <event>`.
- Neither passes any prune hint; both expect the migration to be clean from `mergeSettings` alone.
  No init-side change is needed beyond what `mergeSettings` does internally — but consumer init
  tests that iterate `HOOK_EVENTS` or assert event counts are sensitive to install-surface changes
  (pattern #4826), so they must be re-run and may need a fixture touch if a managed event's group
  set changes.

### Constraints from prior decisions
- vnc-027 (#680) owns `lib/merge-settings.js`. ADR-004 (#4811) is the matcher-narrowing /
  opt-in decision this fix sits beside; the fix must not perturb it.
- nan-016 ADR-003 (#4926) deliberately routes switchover through shipped `mergeSettings` so
  promote/rollback share one ownership-aware path; retiring the bespoke prune (Goal 4) realizes
  that intent fully.
- merge-settings.js lives in `lib/`, NOT `lib/hook-client/`, so it is **outside** the C-04 size
  gate (ROOT=lib/hook-client) (pattern #4826). The settings file is the install artifact, not the
  frozen wire contract, so adding/removing entries there is permitted.

## Proposed Approach

Generalize the Step 3b prune so it also runs for **managed events**, removing every Unimatrix-owned
hook that is **not** the entry just written under the managed matcher group.

Concretely, after Step 3 composes the managed groups, for each registered `event`:
1. Walk all matcher groups for `content.hooks[event]`.
2. Within each group, filter out any entry where `isUnimatrixHook(entry) === true` **and** the
   entry is not the freshly-written managed entry for this event. The "is the fresh entry" test
   compares against the command `mergeSettings` itself just wrote (`newHookEntry.command` for that
   event) — `mergeSettings` knows its own target, so no external `targetToken` heuristic is needed
   (unlike the script, which had to reconstruct the target). The managed-group repoint in Step 3
   guarantees that the kept entry equals the fresh command; every other uni-owned entry (stale `"*"`
   Rust hook, pre-rename forms in foreign groups) is stale-by-definition and pruned.
3. Drop emptied matcher groups; the event key remains because the managed group always holds the
   fresh entry. Reuse the existing emptied-group/empty-event cleanup shape from `pruneUnimatrixEvent`.

**Rationale for "compare against the fresh command, not a token":** `mergeSettings` is the writer,
so the authoritative "keep" target is the exact entry it composed. This is simpler and more correct
than the script's token-matching (which existed only because the script ran *after* `mergeSettings`
and had to infer the target). It also means a uni-owned entry in a foreign group that *happens to
already equal* the fresh command is harmless to keep — but in practice it would be a different group
than the managed one, so dedup intent says prune it; design should pick: prune all uni-owned entries
outside the managed group unconditionally (simplest, matches the "single owned entry per event"
invariant already asserted by `test_each_event_has_exactly_one_unimatrix_entry`), which is the
recommended rule. (See OQ-2.)

**Action strings:** emit a distinct action per cross-group removal (e.g.
`"Removed stale unimatrix hook: <event> (cross-matcher migration)"`) so init/switchover summaries
and the dry-run output surface the migration, consistent with existing `actions` conventions.

**Retiring the script workaround (Goal 4):** once `mergeSettings` prunes cross-group, the
`PRUNE_FRAGMENT` / `pruneStaleUniHooks` / `commandReferencesTarget` machinery and the
`{dryRun:true}` + bespoke-write pattern in `dogfood-switchover.sh` collapse to a plain
`mergeSettings(..., {dryRun})` call that owns its own write — matching `initRemote`'s call shape
exactly (nan-016 ADR-003). The script's own `--dry-run`, exit codes, and completeness checks stay.
The dogfood effect harness assertion that "one stale `"*"` group survives" (the documented
operator-facing delta, #4930) must be updated to assert a **clean** migration instead.

## Acceptance Criteria

- AC-01: After `mergeSettings` runs on a settings file containing a legacy `"*"` `PreToolUse`
  Unimatrix hook (Rust-binary form), the result contains the fresh Unimatrix hook under
  `PRETOOLUSE_CYCLE_MATCHER` and **no** Unimatrix-owned hook under `"*"` for `PreToolUse`.
- AC-02: For every registered event, exactly one Unimatrix-owned hook entry remains across all
  matcher groups, and it lives in the `EVENT_MATCHERS[event]` group with the freshly-written command
  (extends `test_each_event_has_exactly_one_unimatrix_entry` to the cross-group case).
- AC-03: Foreign (non-`isUnimatrixHook`) hooks in any matcher group, including a foreign `"*"`
  `PreToolUse` hook, are preserved byte-for-byte; matcher groups that retain a foreign hook are not
  dropped.
- AC-04: A matcher group emptied solely by removing stale Unimatrix hooks is dropped; the event key
  is retained because the managed group holds the fresh entry.
- AC-05: `mergeSettings` remains idempotent — running it twice on any seeded input yields
  `deepStrictEqual` content (including inputs with a stale `"*"` uni hook on the first run).
- AC-06: The legacy local (string) arm and the node-client (object) arm both produce a clean
  cross-group migration (parity with the two call shapes `init`/`initRemote` use).
- AC-07: The existing Step 3b opt-out prune for non-registered events (e.g. SubagentStop default-off)
  is unchanged — its tests still pass and its action strings are preserved.
- AC-08: An `actions` entry is emitted for each cross-group stale-hook removal, surfaced under
  dry-run with the `[dry-run]` prefix.
- AC-09: `scripts/dogfood-switchover.sh` no longer contains a bespoke prune; promote and rollback
  call `mergeSettings(..., {dryRun})` and rely on its write, and the dogfood effect/harness
  assertions are updated to expect a clean (no stale `"*"`) migration.
- AC-10: The full `packages/unimatrix` test suite passes, including the existing
  `merge-settings.test.js` cases and any `init`/`init --remote` consumer tests that assert event
  shape (pattern #4826).

## Constraints

- **Surface:** `packages/unimatrix/lib/merge-settings.js` (logic),
  `packages/unimatrix/test/merge-settings.test.js` (regression), `scripts/dogfood-switchover.sh`
  (retire workaround), and the dogfood effect-harness/runbook assertion that documents the surviving
  stale group. No other files.
- **Ownership identification is fixed:** must reuse `isUnimatrixHook` / `UNIMATRIX_PATTERNS`
  unchanged.
- **Back-compat call shapes:** both `mergeSettings(fp, binaryPathString, opts)` and
  `mergeSettings(fp, {events, commandForEvent}, opts)` must keep working with no signature break for
  current callers.
- **vnc-027 adjacency:** matcher-narrowing and SubagentStop opt-in semantics (ADR-004 #4811) must be
  preserved exactly; this is an additive prune, not a redesign.
- **Size gate:** merge-settings.js is outside the C-04 `lib/hook-client` ROOT, so it does not count
  against that gate — but keep the change minimal.
- **Test discipline:** extend existing fixtures/helpers in `merge-settings.test.js`
  (`writeSettings`, `tempSettingsPath`, `seedWith*` patterns); do not create isolated scaffolding.
- **No daemon / no network / no Rust changes.** Install-surface only.

## Dependencies

- **Replacement observation source (SHIPPED, not pending).** The data source that supersedes the
  pruned broad `"*"` `PreToolUse` per-event observations is the transcript-fed cycle-review
  distillation: `context_cycle_review` reads loaded observations and, at response-assembly time,
  distills transcript candidates (crt-052 ADR-004 #4850), reconstructing from observations when the
  buffer is empty (crt-052 ADR-006). This landed in **crt-052 "Transcript-Fed Cycle Review
  Distillation" (#706)** and the matcher-narrowing it pairs with landed in **vnc-027 ADR-004 (#4811,
  #680)**. Both are merged on `main` as of this writing — so vnc-031 rests on an **already-satisfied**
  dependency, not a pending one. The broad-hook prune does not race a missing replacement.
- **The replacement path is server-side Rust and OUT of vnc-031 scope.** `context_cycle_review` is an
  MCP handler in `crates/unimatrix-server/src/mcp/tools.rs` (with distillation in
  `crates/unimatrix-server/src/mcp/distill_handler.rs` and reconstruction in
  `crates/unimatrix-observe`). vnc-031 **depends on** that work; it does **not** own, modify, or
  re-validate it. Any change to how `context_cycle_review` consumes observations or transcript is a
  separate, server-side concern explicitly excluded here (Non-Goal 5). This makes the dependency
  explicit rather than implied: vnc-031's install-surface prune is correct precisely *because* the
  server-side replacement source already exists and is independent of the install surface.

## Open Questions

- **OQ-1 (signature):** Can the prune work purely from state `mergeSettings` already has (the fresh
  command per event), needing **no** new parameter? Recommended: yes — compare kept entries against
  the just-written `newHookEntry.command`, so no `targetToken` parameter and no caller change. Confirm
  no consumer relies on a stale uni hook surviving (none found; the dogfood harness assertion is the
  only place that *expects* survival, and it is being updated by AC-09).
- **OQ-2 (prune rule):** Should the rule be "prune every uni-owned entry **outside** the managed
  matcher group" (simplest; enforces the single-owned-entry invariant) or "prune uni-owned entries
  whose command ≠ the fresh command" (would keep a coincidentally-identical command in a foreign
  group)? Recommended: prune all uni-owned entries outside the managed group unconditionally — it
  matches the existing `test_each_event_has_exactly_one_unimatrix_entry` invariant and avoids a
  surprising "two identical commands under two matchers" state. Human to confirm.
- **OQ-3 (cross-group vs managed-events-only):** Should cross-group pruning apply to **all** events
  or only events being registered this run? Recommended: registered (managed) events only — the
  non-registered events are already handled by the existing Step 3b opt-out prune, which removes
  **all** uni hooks for those events. Together the two paths cover the full event set. Confirm this
  partition is the intended division of labor.
- **OQ-4 (intentionally-retained user hooks, from #728):** Could a user legitimately want to keep a
  Unimatrix-pattern-matching hook under a non-managed matcher (e.g. a hand-pinned `"*"` `PreToolUse`
  uni hook)? Given `isUnimatrixHook` is the project-wide ownership signal and the codebase already
  freely repoints/dedups any matching entry, the position is **no** — `isUnimatrixHook === true`
  means Unimatrix-owned and subject to reconciliation. The substantive question — "but the broad
  `"*"` `PreToolUse` hook produced per-event telemetry `context_cycle_review` used; does pruning it
  lose information?" — is resolved by the data-source migration, not by claiming the hook had no
  value. It *did* have value under the old client. That value has already been **migrated** to
  `PostToolUse` (which carries the duplicate signal) plus the transcript-fed cycle-review
  distillation (crt-052 #706), by the vnc-027 ADR-004 (#4811) matcher-narrowing decision — both
  shipped (see Dependencies). vnc-031 only completes the install-surface cleanup of a migration
  already decided and already backed by a live replacement source; it does not originate or relitigate
  the information-loss tradeoff. Surfaced for explicit human sign-off because the *ownership-reconciliation*
  judgment (treating any `isUnimatrixHook` entry as reconcilable) remains the one behavioral call in
  #728 — the data-source question is settled.
- **OQ-5 (harness/runbook):** The nan-016 effect harness + RUNBOOK document the surviving stale `"*"`
  group as the operator-facing matcher-narrowing delta (#4930). After this fix the delta disappears.
  Confirm scope to update those assertions/docs lives in this feature (AC-09) vs a nan-016 follow-up.

## Tracking
GH Issue #728 — https://github.com/dug-21/unimatrix/issues/728
