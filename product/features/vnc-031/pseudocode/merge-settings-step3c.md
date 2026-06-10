# Component: `mergeSettings` Step 3c — Cross-Matcher-Group Stale Uni-Hook Prune

File: `packages/unimatrix/lib/merge-settings.js`
Function: `mergeSettings(filePath, commandSource, options)` — signature UNCHANGED.

## Purpose

For each **managed (registered-this-run) event**, after Step 3 composes the
managed `EVENT_MATCHERS[event]` group, remove every Unimatrix-owned hook entry
that is NOT the freshly-written keep-target — across ALL matcher groups of
`content.hooks[event]`, not only the managed group. Drop matcher groups emptied
by the removal; retain the event key (the managed group always holds the
keep-target). This makes a legacy `"*"` → narrowed-matcher migration clean from
`mergeSettings` alone, for every consumer.

The keep-target is identified by **object identity** (`Object.is` / strict `!==`)
against the entry Step 3 placed — NEVER a command-string compare (ADR-001 /
SR-01). This is the load-bearing rule of the whole feature.

## Integration Points (verified against merge-settings.js)

| Name | Source | Use in Step 3c |
|------|--------|----------------|
| `events` | local, line 213–216 (post SubagentStop filter) | the managed set Step 3c iterates |
| `content.hooks[event]` | array of matcher groups | walked group-by-group |
| `newHookEntry` | line 262 | captured by reference into `keptEntryByEvent` |
| `isUnimatrixHook(entry)` | line 121 | sole ownership predicate (unchanged) |
| `actions` | line 206 | cross-matcher action pushed here |
| emptied-group cleanup idiom | lines 182–184 (`pruneUnimatrixEvent`) | reused shape (NO event-key delete in 3c) |

## New Function-Local State

```
// Declared alongside `actions`, near line 206:
keptEntryByEvent = {}    // Record<event, newHookEntry>; object-reference map
```

## Modified Step 3 — capture the kept reference

Step 3 (lines 258–322) is unchanged in behavior; add ONE capture line at every
point where the kept entry's final object is known. There are three placement
outcomes in the existing loop; the captured reference is the SAME `newHookEntry`
object in all three:

```
FOR each event in events:
    compose newHookEntry = { type: "command", command: source.commandForEvent(event) }
    ensure content.hooks[event] is an array

    found managed group (matcherGroup.matcher === EVENT_MATCHERS[event])?
        YES:
            dedup extra uni entries in-group (unchanged)
            IF existingIndex >= 0:
                matcherGroup.hooks[existingIndex] = newHookEntry   // repoint
                action "Updated hook: <event>"
            ELSE:
                matcherGroup.hooks.push(newHookEntry)               // append
                action "Added hook: <event>"
        NO (no managed group found):
            content.hooks[event].push({ matcher, hooks: [newHookEntry] })  // new group
            action "Added hook: <event> (new matcher group)"

    # --- NEW: single capture line, valid for ALL three branches ---
    keptEntryByEvent[event] = newHookEntry
```

Rationale: `newHookEntry` is created once per event before the branch and is the
object placed into the managed group in every branch (assigned, pushed, or
wrapped in a new group). Capturing it once after the merge resolves is correct
and branch-independent. Place the capture at the end of the per-event body, after
`merged` resolution, so it runs exactly once per event regardless of path.

> R-02 edge: when the input has NO managed-group uni entry (the only uni hook is
> a stale cross-group one), Step 3 takes the append/new-group branch and creates
> the managed entry. The capture MUST follow that create — capturing the
> reference at the end of the per-event body guarantees this ordering. Capturing
> before the repoint/push would risk a stale/wrong reference.

## New Step 3c — cross-group prune (placement: after Step 3 loop, before Step 3b)

```
// Step 3c: cross-group stale-uni prune for managed events.
FOR each event in events:
    eventArray = content.hooks[event]
    IF eventArray is not an Array: continue        // managed group guarantees presence; defensive
    kept = keptEntryByEvent[event]

    FOR each group in eventArray:
        IF group is falsy OR group.hooks is not an Array: continue   // mirror pruneUnimatrixEvent guard
        before = group.hooks.length
        group.hooks = group.hooks.filter(
            hook => NOT ( isUnimatrixHook(hook) AND hook !== kept )   // identity keep; NEVER command compare
        )
        IF group.hooks.length !== before:
            actions.push("Removed stale unimatrix hook: " + event + " (cross-matcher migration)")

    // Drop matcher groups emptied solely by uni removal; RETAIN the event key
    // (the managed group holds `kept`). Reuse pruneUnimatrixEvent's filter idiom
    // — but DO NOT delete content.hooks[event] (3b deletes; 3c never empties the event).
    content.hooks[event] = eventArray.filter(
        group => group AND Array.isArray(group.hooks) AND group.hooks.length > 0
    )
```

Key differences from `pruneUnimatrixEvent` (Step 3b), do not conflate them:

| Aspect | Step 3b (`pruneUnimatrixEvent`) | Step 3c (NEW) |
|--------|----------------------------------|----------------|
| Event set | `HOOK_EVENTS \ events` (non-managed) | `events` (managed) |
| Keep rule | none — removes ALL uni hooks | keep `kept` by identity; remove the rest |
| Filter predicate | `!isUnimatrixHook(hook)` | `!(isUnimatrixHook(hook) && hook !== kept)` |
| Action | `Removed unimatrix hook: <event> (opt-out)` | `Removed stale unimatrix hook: <event> (cross-matcher migration)` |
| Event-key delete | yes, if event empties | NO — managed group always retains `kept` |

## The Keep Test — why identity, not string (ADR-001 / SR-01 / R-01)

`hook !== kept` is reference inequality against the object Step 3 wrote.
Because `kept` IS the object now living in the managed group:
- It can never be a prune candidate → the managed event can never drop to zero
  uni hooks (FR-03, by construction).
- A foreign-group uni entry whose command COINCIDENTALLY equals the fresh command
  is a different object → pruned anyway (FR-04, single-owned-entry invariant).
- Whitespace / `LD_LIBRARY_PATH` prefix / arg-order / quoting divergence cannot
  prune the survivor — there is no string to diverge (SR-01 unrepresentable).

ANTI-PATTERN (must not appear in implementation or future refactor): any form of
`hook.command === kept.command`, `hook.command === fresh`, `.includes(token)`, or
tokenizing the command. Identity is the only correct keep test. The dogfood
script's `commandReferencesTarget` existed only because the script ran as a
separate process and lacked the object reference (#4931); `mergeSettings` is the
writer and holds it.

## Data Flow

- Input: validated `content` (Steps 1–2), `events`, `source.commandForEvent`,
  `keptEntryByEvent` populated by Step 3.
- Output: mutated `content.hooks[event]` per managed event — exactly one uni
  entry (the keep-target) under `EVENT_MATCHERS[event]`; all other uni entries in
  any group removed; emptied groups dropped; foreign hooks byte-unchanged;
  `actions` extended with one cross-matcher line per group that lost a uni entry.

## Error Handling

- No new error paths. Malformed-JSON throw (Step 1) and non-object-`hooks` throw
  (Step 2) precede Step 3c and are unchanged. Step 3c is pure in-memory mutation
  of already-validated `content`; like Step 3b it cannot throw on well-formed
  input.
- Defensive guards (`Array.isArray`) mirror `pruneUnimatrixEvent` so a malformed
  group (`null` entry, missing `hooks`, non-array `hooks`) is skipped, never
  thrown on. A `null`/non-string-command entry is `isUnimatrixHook === false`
  (line 122 guard) → treated as foreign → left untouched.
- Fail-loud is a TEST invariant (AC-02), not a runtime exception: by construction
  a managed event cannot reach zero uni hooks; the test asserts it to guard
  future regressions.

## Key Test Scenarios (hints for the tester; not the test plan)

- **AC-01** legacy `"*"` PreToolUse Rust uni hook → migrates clean: survivor under
  `PRETOOLUSE_CYCLE_MATCHER`, zero uni under `"*"`.
- **AC-02 / R-02** every managed event seeded with an extra stale uni hook under a
  non-managed matcher → exactly one uni hook per event, in `EVENT_MATCHERS[event]`,
  with the exact fresh command; explicit `assert(count !== 0, ...)`. Plus an
  adversarial seed with NO managed-group uni entry pre-merge (only the stale
  cross-group one) → managed entry created, NOT pruned.
- **R-01 (discriminating)** stale uni hook whose command differs from fresh ONLY
  by collapsible whitespace / arg spacing, in a non-managed group → near-twin
  pruned, survivor is the placed object (asserted by matcher + exact fresh
  command). A string-compare reimplementation would keep the wrong object → red.
- **AC-03 / R-07** foreign `"*"` hook + near-miss uni-looking-but-unclassified
  command + non-`command` (`type:"url"`) entry under non-managed matchers → all
  preserved byte-for-byte; foreign-retaining group not dropped.
- **AC-04 / R-08** `"*"` group whose only entry is a stale uni hook → group
  dropped, `content.hooks.PreToolUse` retained with the cycle group.
- **R-03** managed group seeded with TWO uni entries (dup) + stale uni in a
  non-managed group → exactly one survivor in managed group (Step 3 dedup + Step
  3c compose). Three groups each with a uni hook → both stale groups lose theirs,
  one action per group.
- **AC-05 / R-06** idempotency: seed legacy `"*"`, run twice, `deepStrictEqual`;
  run 2+ emit no cross-matcher action; 3-run no-growth.
- **AC-06 / R-05** string arm (`init`/rollback) and object arm
  (`initRemote`/promote) both produce the clean migration on the same seed.
- **AC-07 / R-09** combined seed: SubagentStop stale uni (→ Step 3b `(opt-out)`)
  AND stale `"*"` PreToolUse uni (→ Step 3c cross-matcher) in one file → both
  removed via their own action strings, no double emission; existing SubagentStop
  opt-out tests pass unmodified.
- **AC-08 / R-11** action emitted per cross-group removal; under `{dryRun:true}`
  carries `[dry-run]` prefix and no file is written; phrase substring-disjoint
  from `(opt-out)` etc.
- **R-10** surviving managed PreToolUse entry is under `PRETOOLUSE_CYCLE_MATCHER`
  (cycle frame is the keep-target, never pruned); SubagentStop opt-in path with a
  stale cross-group SubagentStop seed → survivor is the fresh `"*"` managed entry.
