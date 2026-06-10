## ADR-002: Generalize the Step 3b Prune to Managed Events (Prune All Uni Entries Outside the Managed Group)

### Context

`mergeSettings` reconciles uni hooks only inside `EVENT_MATCHERS[event]`. Step
3b (`pruneUnimatrixEvent`) already prunes uni-owned entries across *all* matcher
groups — but only for events **not** in the registered set (the opt-out path).
A legacy `"*"` `PreToolUse` uni hook on a *registered* event is therefore a
"foreign-matcher group" from Step 3's view and survives un-repointed (#4936,
#4930). This is the root cause #728 / nan-016's script workaround address.

OQ-2 asks the prune rule: "prune every uni entry **outside** the managed group"
vs "prune uni entries whose command ≠ the fresh command (keeping a coincidental
match in a foreign group)." OQ-3 asks the event subset: all events vs registered
only. OQ-4 (human-approved) confirms any `isUnimatrixHook === true` entry under a
non-managed matcher is reconcilable Unimatrix-owned state, not a user pin.

### Decision

Add **Step 3c**: for each *registered* event, after Step 3 composes the managed
group, walk all matcher groups for `content.hooks[event]` and remove every
`isUnimatrixHook(entry) === true` entry that is **not** the kept object
(`keptEntryByEvent[event]`, ADR-001). Drop matcher groups emptied by this
removal; the event key is retained because the managed group always holds the
kept entry. Reuse the exact emptied-group/empty-event cleanup shape from
`pruneUnimatrixEvent`. Emit one action per removal:
`"Removed stale unimatrix hook: <event> (cross-matcher migration)"`.

- **OQ-2 → prune all uni entries outside the managed group, unconditionally.**
  Identity (ADR-001), not command compare, is the keep test. A uni-owned entry
  in a foreign group whose command *coincidentally* equals the fresh command is
  still removed — it is a different object than the kept one. This enforces the
  existing single-owned-entry invariant
  (`test_each_event_has_exactly_one_unimatrix_entry`) and avoids a surprising
  "two identical commands under two matchers" end-state.

- **OQ-3 → managed (registered) events only.** Step 3c covers `events`; the
  existing Step 3b opt-out prune covers `HOOK_EVENTS \ events` (removing *all*
  uni hooks there, including the managed-matcher one). The partition is exact:
  union = all HOOK_EVENTS, intersection = empty. SubagentStop default-off lands
  in Step 3b unchanged (AC-07); a registered, mid-rename event lands in Step 3c.
  No event is visited by both, none by neither (SR-03).

- **OQ-4 → prune (human-approved).** The broad `"*"` hook's value already
  migrated to PostToolUse + transcript-fed cycle-review distillation (crt-052
  #706, vnc-027 ADR-004 #4811, both on `main`); Step 3c only completes the
  install-surface cleanup of an already-decided, already-backed migration.

Prune scope is strictly `isUnimatrixHook` — identical to Step 3b and the
script. Foreign hooks are never touched; `isUnimatrixHook` / `UNIMATRIX_PATTERNS`
are unchanged (Non-Goal 2). The prune's correctness is fully bounded by
`isUnimatrixHook` precision (SR-02); a near-miss foreign hook in a non-managed
group must survive (regression test extends AC-03).

### Consequences

Easier: a legacy `"*"` → narrowed-matcher migration is clean from `mergeSettings`
alone, for every consumer (ADR-003). The opt-out path is untouched — Step 3c is
additive, sitting between Step 3 and Step 3b. The new prune is the same shape as
the proven Step 3b, so the emptied-group cleanup is shared, not reinvented.

Harder: blast radius widens from one group to all groups for registered events
(SR-02) — but bounded by the unchanged ownership signal. The one genuine
behavioral change (a uni-pattern hook under a non-managed matcher: survive →
pruned) is human-signed-off (OQ-4). Consumer init tests that assert event/group
counts may see a managed event's group set shrink and must be re-run and
justified, not rubber-stamped (#4826, SR-05).

Cross-references: ADR-001 (keep-target identity); ADR-003 (consumer parity +
script retire). Sits beside vnc-027 ADR-004 (#4811) matcher-narrowing without
perturbing it (SR-06). Reuses `pruneUnimatrixEvent` cleanup (lines 181–187).
