# Agent Report: vnc-031-agent-1-architect

## Task
Architecture for vnc-031 — make `mergeSettings` prune stale Unimatrix-owned hooks
across all matcher groups for managed events (root-cause fix for #728 / nan-016
dogfood workaround). Resolve OQ-1/2/3/4; address SR-01/04/05.

## Artifacts Produced
- `product/features/vnc-031/architecture/ARCHITECTURE.md`
- `product/features/vnc-031/architecture/ADR-001-keep-target-by-entry-identity.md` (Unimatrix #4939)
- `product/features/vnc-031/architecture/ADR-002-cross-group-prune-generalization.md` (Unimatrix #4940)
- `product/features/vnc-031/architecture/ADR-003-retire-dogfood-prune-parity.md` (Unimatrix #4941)

## Key Decisions
- **ADR-001 (SR-01):** Keep-target is the Step 3 repointed-entry OBJECT REFERENCE
  (`keptEntryByEvent[event] = newHookEntry`), kept by `Object.is`, never a command
  string. Zeroing a managed event is impossible by construction; AC-02 is a
  fail-loud test guard.
- **ADR-002 (OQ-2/3/4):** New Step 3c between Step 3 and Step 3b. Prune all uni
  entries outside the managed group unconditionally (OQ-2), registered events only
  (OQ-3, exact partition with Step 3b opt-out), prune the broad hook (OQ-4,
  human-approved). No signature change (OQ-1).
- **ADR-003 (SR-04/05):** Case-by-case parity table proving Step 3c subsumes the
  script's `pruneStaleUniHooks` (incl. `.bak`/old-dir tokens, rollback dirname
  match, the #4931 spaced-path keep). Binding gate: prove on REAL legacy input
  before AC-09 deletion. One behavior for both arms — no init.js edit.

## Resolved Open Questions
OQ-1 no signature change; OQ-2 prune-all-outside-managed; OQ-3 registered events
only; OQ-4 prune (human-approved).

## Open Questions for Human
- OQ-5: harness/runbook assertion inversion scoped to vnc-031 (AC-09) vs nan-016
  follow-up — confirm.
- Branch-base: delivery must verify crt-052 #706 + vnc-027 #4811 are on the
  delivery base branch before pruning telemetry (SR-04 assumption).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_get -- applied vnc-027
  ADR-004 (#4811 matcher-narrowing/SubagentStop opt-in), nan-016 ADR-003 (#4926
  route-through-mergeSettings), dogfood prune patterns (#4931 quote-aware
  tokenizer, #4932 negative control, #4936 stale-"*" limitation, #4938
  enumerate-primitive lesson), install-surface test sensitivity (#4826).
- Stored: #4939 ADR-001, #4940 ADR-002 (Prerequisite->4939), #4941 ADR-003
  (Prerequisite->4940), all via context_store category=decision. Intra-feature
  Prerequisite spine asserted (each ADR must be read before the next to avoid the
  string-compare/zeroing or parity-regression wrong decisions). No edge to
  nan-016 ADR-003 #4926 — supersession-of-intent handled in prose, not graph.
