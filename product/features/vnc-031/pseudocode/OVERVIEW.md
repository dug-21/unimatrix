# vnc-031 Pseudocode Overview — Cross-Matcher-Group Stale Uni-Hook Prune

Install-surface only. One behavioral unit (`mergeSettings` Step 3c) plus two
consumers that inherit it: the dogfood switchover script (retire its bespoke
prune) and the nan-016 effect harness (repoint a clean-state attribution).
No Rust, server, transport, daemon, signature, or ownership-classification change.

## Components

| File | Component | Change kind |
|------|-----------|-------------|
| merge-settings-step3c.md | `mergeSettings` Step 3c cross-group prune (`packages/unimatrix/lib/merge-settings.js`) | Additive logic — new Step 3c between Step 3 and Step 3b |
| dogfood-switchover-retire.md | `scripts/dogfood-switchover.sh` promote/rollback | Delete bespoke prune; collapse to `mergeSettings(..., {dryRun})` owning the write (GATE C blocks the deletion) |
| dogfood-effect-harness.md | `packages/unimatrix/test/dogfood-effect.test.js` | Repoint negative-control reconstruction so it excludes Step 3c (GATE B; assertion-attribution repoint, not a rewrite) |

`lib/init.js` is NOT modified — both consumers (`init` string arm, `initRemote`
object arm) inherit Step 3c identically through `mergeSettings`.

## Data Flow

```
commandSource (string | {events, commandForEvent})
        │  normalizeCommandSource
        ▼
   {events, commandForEvent}
        │
   Step 3  ── per registered event: compose newHookEntry, repoint-or-push into
        │     EVENT_MATCHERS[event] group, dedup in-group.
        │     >>> capture keptEntryByEvent[event] = newHookEntry (by reference) <<<
        ▼
   Step 3c ── per registered event: walk ALL matcher groups; remove every
        │     uni-owned entry that is NOT keptEntryByEvent[event] (Object.is);
        │     drop emptied groups; retain event key; push cross-matcher action.
        ▼
   Step 3b ── per NON-registered event (HOOK_EVENTS \ events): pruneUnimatrixEvent
        │     (opt-out, unchanged).
        ▼
   Step 4  ── write (or [dry-run]-prefix actions)
        ▼
   {actions, content}
```

The script consumes `{actions, content}` and (post-retire) relies on
`mergeSettings`' own Step 4 write instead of a bespoke post-process. The harness
consumes the same `mergeSettings` to reconstruct states for assertion.

## Shared Types

```
keptEntryByEvent : Record<event, newHookEntry>     // NEW; function-local to mergeSettings
                                                   // object-reference map, one kept entry per managed event
newHookEntry     : { type: "command", command: string }  // the kept object, captured BY REFERENCE in Step 3
matcher group    : { matcher: string, hooks: HookEntry[] }  // one entry in content.hooks[event]
HookEntry        : { type?: string, command?: string, ... } // opaque; ownership decided only by isUnimatrixHook
```

`keptEntryByEvent[event]` holds the SAME object that now lives in the managed
group — never a re-derived or string-compared value (ADR-001 / SR-01).

## Action-String Contract (load-bearing, substring-disjoint)

| Source | Phrase |
|--------|--------|
| Step 3c (NEW) | `Removed stale unimatrix hook: <event> (cross-matcher migration)` |
| Step 3b (unchanged) | `Removed unimatrix hook: <event> (opt-out)` |
| Step 3 repoint (unchanged) | `Updated hook: <event>` / `Added hook: <event>` / `Added hook: <event> (new matcher group)` |
| Step 3 dedup (unchanged) | `Removed duplicate unimatrix hook for <event>` |

The new phrase must be substring-disjoint from `(opt-out)` and the others so
init/switchover summaries that grep actions never misclassify (R-11). Under
`{dryRun:true}` every action gets the existing Step 4 `[dry-run] ` prefix. One
action per matcher group that lost ≥1 uni entry (matches `pruneUnimatrixEvent`
per-group emission convention).

## Event Partition (load-bearing, SR-03)

```
events                 = source.events after SubagentStop opt-in filtering
HOOK_EVENTS \ events   = the opt-out complement
```

- Step 3c visits exactly `events` (managed) — cross-group prune.
- Step 3b visits exactly `HOOK_EVENTS \ events` (non-managed) — opt-out prune.
- Union = HOOK_EVENTS; intersection = ∅. No event is double-visited or skipped.

Ordering is binding: **Step 3 → Step 3c → Step 3b**. Step 3c must run after Step 3
(the kept object must exist and be captured) and before Step 3b (disjoint set;
reordering could route a managed event through the opt-out path that removes ALL
uni hooks including the fresh one — R-09).

## Sequencing Constraints (what must be built first)

1. **Step 3c (merge-settings-step3c.md)** lands first and must be GREEN on real
   legacy-shaped input (AC-01..AC-08). It is the root-cause fix everything else
   depends on.
2. **Harness repoint (dogfood-effect-harness.md)** lands with or after Step 3c —
   once `mergeSettings` prunes cross-group, the negative control's reconstruction
   path must be changed to one that excludes Step 3c, or the control goes vacuous
   (GATE B / R-13).
3. **Script retire (dogfood-switchover-retire.md)** is GATE-C-blocked: the
   `PRUNE_FRAGMENT` deletion commit may not precede a GREEN parity proof (P1–P8)
   on REAL legacy-shaped input. The pseudocode describes the post-retire shape;
   the deletion ordering is a delivery gate, not a code-shape decision.

## Cross-Cutting Constraints

- Reuse `isUnimatrixHook` / `UNIMATRIX_PATTERNS` unchanged — sole ownership signal.
- Reuse the `pruneUnimatrixEvent` emptied-group/event-key cleanup shape; no
  parallel machinery (NFR-03).
- Signature of `mergeSettings` is unchanged — no `targetToken`, no prune-hint
  (FR-08 / R-15).
- Output serialization unchanged: 2-space JSON + trailing newline (NFR-05).
- A managed event must NEVER end with zero uni hooks (FR-03 / fail-loud test).

## Open Questions / Gaps

None blocking. The architecture supplies the canonical Step 3c shape verbatim;
all integration points trace to `merge-settings.js` line references in
ARCHITECTURE.md §Integration Surface. Delivery-time gates (A/B/C) are recorded in
the per-component files where they bind.
