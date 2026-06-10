# vnc-031 Architecture: Cross-Matcher-Group Stale Uni-Hook Prune in `mergeSettings`

## System Overview

This is an **install-surface-only** change. The single behavioral unit is
`mergeSettings` in `packages/unimatrix/lib/merge-settings.js` — the function that
reconciles Unimatrix hook entries into a project's `.claude/settings.json`. It is
called by two consumers (`init` local string arm, `initRemote` object arm in
`lib/init.js`) and one tool (`scripts/dogfood-switchover.sh`).

Today `mergeSettings` repoints uni hooks only within `EVENT_MATCHERS[event]`. A
uni-owned hook living in *another* matcher group for a registered event (e.g. a
legacy `"*"` `PreToolUse` Rust hook) is invisible to it and survives un-repointed
(#4936, #4930). This feature adds a cross-group prune for managed events so that
migration is clean from `mergeSettings` alone, and retires the script-level
workaround nan-016 shipped (#728 root cause).

No Rust, server, transport, daemon, or network surface is touched. The data-source
migration that makes pruning the broad `"*"` hook lossless (PostToolUse +
transcript-fed cycle-review distillation) already shipped on `main` (crt-052 #706,
vnc-027 ADR-004 #4811) and is out of scope (SCOPE Dependencies, Non-Goal 5).

## Component Breakdown

| Component | Responsibility | Change |
|---|---|---|
| `mergeSettings` (merge-settings.js) | Reconcile uni hooks into settings.json | **Add Step 3c**: cross-group prune for registered events |
| `pruneUnimatrixEvent` (merge-settings.js) | Opt-out prune for non-registered events (Step 3b) | **Unchanged**; its emptied-group cleanup shape is reused |
| `isUnimatrixHook` / `UNIMATRIX_PATTERNS` | Sole ownership signal | **Unchanged** (Non-Goal 2) — prune is bounded by its precision |
| `init` / `initRemote` (init.js) | Consumers; pass full `HOOK_EVENTS`, no prune hint | **No code change**; both inherit Step 3c identically (SR-05) |
| `dogfood-switchover.sh` | Promote/rollback tool | **Retire** `PRUNE_FRAGMENT` et al.; call `mergeSettings(..., {dryRun})` directly (AC-09) |
| `merge-settings.test.js` | Regression coverage | **Extend** existing fixtures/helpers (no isolated scaffolding) |
| nan-016 effect harness / runbook | Documents surviving `"*"` delta (#4930) | **Invert** assertion to expect clean migration (AC-09, OQ-5) |

## The Prune Algorithm and Its Placement

The current `mergeSettings` flow:

- **Step 1** read/parse file
- **Step 2** ensure `hooks` is an object
- **Step 3** (lines 258–322) per registered event: compose `newHookEntry`, find/
  create the `EVENT_MATCHERS[event]` group, repoint the first uni hook in place
  (or push), dedup extras — **only within that group**
- **Step 3b** (lines 324–333, `pruneUnimatrixEvent`) per *non-registered* event:
  prune all uni entries across all groups (opt-out)
- **Step 4** write (or `[dry-run]` prefix actions)

### New Step 3c — placement: after Step 3, before Step 3b

Step 3c runs **after** Step 3 (so the kept entry exists and its object reference
is captured) and **before** Step 3b (which handles the disjoint non-registered
set). The two prunes never overlap and never leave a gap:

```
registered events (`events`)        → Step 3 composes + Step 3c cross-group prunes
non-registered (`HOOK_EVENTS\events`) → Step 3b opt-out prunes (all uni hooks)
union = HOOK_EVENTS   intersection = ∅   (SR-03 partition)
```

### Keep-target by object identity (ADR-001 — closes SR-01)

Step 3 already produces exactly one `newHookEntry` object per event and places it
in the managed group. We capture that reference:

```
// Step 3, per event, immediately after the entry is placed
//   matcherGroup.hooks[existingIndex] = newHookEntry   (repoint)  OR
//   matcherGroup.hooks.push(newHookEntry)              (append / new group)
keptEntryByEvent[event] = newHookEntry;
```

Step 3c keeps an entry **iff it is that object** (`Object.is`), removing every
other uni-owned entry in any group for that event:

```
// Step 3c — per registered event
for (const event of events) {
  const eventArray = content.hooks[event];
  if (!Array.isArray(eventArray)) continue;        // managed group guarantees presence
  const kept = keptEntryByEvent[event];
  for (const group of eventArray) {
    if (!group || !Array.isArray(group.hooks)) continue;
    const before = group.hooks.length;
    group.hooks = group.hooks.filter(
      (hook) => !(isUnimatrixHook(hook) && hook !== kept)   // identity, NOT command compare
    );
    if (group.hooks.length !== before) {
      actions.push("Removed stale unimatrix hook: " + event + " (cross-matcher migration)");
    }
  }
  // Drop emptied groups; event key stays — managed group holds `kept` (reuse 3b shape).
  content.hooks[event] = eventArray.filter(
    (group) => group && Array.isArray(group.hooks) && group.hooks.length > 0
  );
}
```

The keep test is **reference equality against the object Step 3 wrote** — never a
re-derived or compared command string. This makes SR-01 unrepresentable: "what was
written" and "what is kept" are one object, so whitespace / `LD_LIBRARY_PATH` /
arg-order / quoting divergence cannot prune the keep-target. Zeroing a managed
event is impossible by construction (the kept object is never a prune candidate and
was placed by Step 3); AC-02 encodes this as a fail-loud test, not a silent pass.

### Prune rule and event subset (ADR-002 — OQ-2/OQ-3/OQ-4)

- **OQ-2:** prune **all** uni entries outside the managed group unconditionally
  (identity keep-test); a coincidentally-identical command in a foreign group is a
  different object and is removed — enforces single-owned-entry invariant.
- **OQ-3:** **registered events only.** Step 3b opt-out covers the complement.
- **OQ-4 (human-approved):** prune — completes an already-shipped data-source
  migration.

## Idempotency Preservation (SR-07 — AC-05)

Idempotency holds by construction. On run 2 of an already-clean file: Step 3 finds
the single managed uni entry, repoints it **in place** (same slot), and captures
that object as `keptEntryByEvent[event]`. Step 3c then finds no other uni entries
(run 1 removed them) and the kept object passes the identity test — zero removals,
zero emptied groups. `deepStrictEqual(first, second)` holds, including for an input
that carried a stale `"*"` uni hook on run 1 (run 1 prunes it; run 2 sees a clean
file). Because the keep test is identity, not string reconstruction, the "fresh
entry" identification is consistent across runs (the failure mode SR-07 warns of
cannot occur).

## Action-String Contract

| Condition | Action string | Source |
|---|---|---|
| Cross-group stale removal (Step 3c) | `Removed stale unimatrix hook: <event> (cross-matcher migration)` | NEW |
| Opt-out removal (Step 3b) | `Removed unimatrix hook: <event> (opt-out)` | unchanged |
| Managed repoint (Step 3) | `Updated hook: <event>` / `Added hook: <event>` | unchanged |
| Dedup within managed group (Step 3) | `Removed duplicate unimatrix hook for <event>` | unchanged |

Under `{dryRun:true}` every action is prefixed `[dry-run] ` by the existing Step 4
map (AC-08). One action emitted per group that lost ≥1 uni entry, consistent with
`pruneUnimatrixEvent`'s per-group emission convention.

## Both Consumers Stay Identical (SR-05 — AC-06)

Step 3c lives inside `mergeSettings`, after `normalizeCommandSource`. Both arms —
string (`init` local / `dogfood rollback`) and object (`initRemote` /
`dogfood promote`) — flow through the same Step 3 (producing `keptEntryByEvent`)
and the same Step 3c. There is **no per-consumer branch and no signature change**
(OQ-1: no `targetToken` param). `init.js` needs no edit. AC-06/AC-10 exercise both
arms on real legacy-shaped input; consumer init tests asserting event/group counts
must be re-run and any fixture touch justified, not rubber-stamped (#4826, SR-05).

## Script Retire and Parity (ADR-003 — SR-04, AC-09)

The script's `commandReferencesTarget` whole-shell-token matcher (quote-aware
tokenizer #4931, rollback dirname special-case) existed only because the script
ran *after* `mergeSettings` and had to reconstruct the keep target. Step 3c holds
the object reference, so every script case maps to the source behavior (full table
in ADR-003): stale `"*"`, `.bak` token, old-client-dir token, rollback dirname
match, quoted spaced-path keep, foreign preservation, emptied-group cleanup — all
subsumed, strictly more correct (identity cannot be fooled by command shape).
**Binding gate:** prove parity on REAL legacy-shaped input before deleting the
script prune (#4938); do not retire on a pre-narrowed seed.

## Integration Surface

| Integration Point | Type / Signature | Source |
|---|---|---|
| `mergeSettings(filePath, commandSource, options)` | `(string, object\|string, {dryRun}) -> {actions: string[], content: object}` | merge-settings.js:203 — **signature unchanged** (OQ-1) |
| `newHookEntry` | `{ type: "command", command: string }` — the kept object | merge-settings.js:262 — captured by reference into `keptEntryByEvent` |
| `keptEntryByEvent` | `Record<event, newHookEntry>` (object-reference map) | NEW, function-local to `mergeSettings` |
| `isUnimatrixHook(entry)` | `(object) -> boolean` | merge-settings.js:121 — **unchanged** (Non-Goal 2) |
| `EVENT_MATCHERS[event]` | `Record<event, string>`; `PreToolUse → PRETOOLUSE_CYCLE_MATCHER` | merge-settings.js:58 — **unchanged** (vnc-027 ADR-004) |
| `pruneUnimatrixEvent(content, event, actions)` | `(object, string, string[]) -> void` | merge-settings.js:166 — **unchanged**; cleanup shape reused inline in 3c |
| emptied-group cleanup | `eventArray.filter(g => g && g.hooks?.length > 0)`; event key retained for registered events | merge-settings.js:181–187 (reused) |
| Action string (new) | `"Removed stale unimatrix hook: <event> (cross-matcher migration)"` | NEW contract |
| `dogfood-switchover.sh` promote/rollback | collapse to `mergeSettings(settingsPath, source, {dryRun})` owning its write | scripts/dogfood-switchover.sh:234,258 — retire `PRUNE_FRAGMENT` |
| Consumer call shapes | `mergeSettings(fp, binaryString, {dryRun})` and `mergeSettings(fp, {events, commandForEvent}, {dryRun})` | init.js:434, init.js:337 — **both back-compat** |

## Error Boundaries

No new error paths. Malformed-JSON throw (Step 1) and non-object-`hooks` throw
(Step 2) are unchanged and precede Step 3c. Step 3c is pure in-memory mutation of
already-validated `content`; like Step 3b it cannot throw on well-formed input.
Fail-loud is a *test* invariant (AC-02 zero-uni-hook = hard failure), not a runtime
exception — by construction the invariant cannot be violated.

## Risk → Decision Map

| Risk | Resolution |
|---|---|
| SR-01 keep-rule fragility | ADR-001: keep-target by object identity, never command compare; zero-uni-hook impossible by construction + fail-loud test |
| SR-02 ownership false-positive blast radius | Prune bounded by unchanged `isUnimatrixHook`; near-miss-foreign-survives regression (extends AC-03) |
| SR-03 partition seam | Step 3c = `events`; Step 3b = `HOOK_EVENTS\events`; union all, intersection empty |
| SR-04 script parity | ADR-003 case-by-case table; binding proof-on-real-legacy-input gate before AC-09 delete |
| SR-05 cross-consumer divergence | One Step 3c inside `mergeSettings`; no signature change, no init.js edit; both arms tested |
| SR-06 vnc-027 adjacency | `EVENT_MATCHERS`/`PRETOOLUSE_CYCLE_MATCHER` unchanged; managed cycle entry is the kept object; SubagentStop opt-in via Step 3b unchanged |
| SR-07 idempotency | Identity keep-test consistent across runs; `deepStrictEqual` incl. stale-`"*"`-on-run-1 (AC-05) |

## Resolved Decisions

| ADR | Decision |
|---|---|
| ADR-001 | Keep-target by repointed-entry object identity, not command-string compare |
| ADR-002 | Generalize Step 3b prune to managed events; prune all uni entries outside managed group; registered events only (OQ-2/3/4) |
| ADR-003 | Retire dogfood script prune with explicit parity argument; one behavior for both consumers |

## Open Questions for the Human

- **OQ-5 (scope confirm):** ARCHITECTURE places the nan-016 effect-harness/runbook
  assertion inversion (surviving `"*"` → clean) inside vnc-031 (AC-09). Confirm
  this is not a separate nan-016 follow-up.
- **Branch-base verification (SR-04 assumption):** the parity proof and the
  lossless-prune justification assume crt-052 #706 and vnc-027 #4811 are present on
  the branch vnc-031 delivers from (not merely "on main as of writing"). Delivery
  must verify before pruning telemetry.
