# Agent Report: vnc-031-agent-1-pseudocode

Stage 2 / 3a — per-component pseudocode for the cross-matcher-group stale
uni-hook prune.

## Deliverables

- product/features/vnc-031/pseudocode/OVERVIEW.md
- product/features/vnc-031/pseudocode/merge-settings-step3c.md
- product/features/vnc-031/pseudocode/dogfood-switchover-retire.md
- product/features/vnc-031/pseudocode/dogfood-effect-harness.md

## Components Covered

1. `mergeSettings` Step 3c (merge-settings.js) — new cross-group prune; identity
   keep-target captured in Step 3; reuse of pruneUnimatrixEvent cleanup shape.
2. `dogfood-switchover.sh` retire — post-retire promote/rollback shape; GATE-C
   blocks the deletion until P1–P8 parity proven on real legacy input.
3. `dogfood-effect.test.js` harness — negative-control repoint (GATE B).

## Load-Bearing Findings

- The harness negative control (`noPrunePromoteContent`/T1d) breaks SILENTLY once
  Step 3c lands: it currently uses "mergeSettings alone = no prune" to
  reconstruct an unpruned state, but mergeSettings now prunes internally → the
  `assert.throws` goes vacuous (#4932 mode). Pseudocode repoints reconstruction
  to a managed-group-only merge that EXCLUDES Step 3c. This is the single most
  error-prone part of the feature.
- The Step 3c capture line must sit at the END of the Step 3 per-event body
  (after repoint/push/new-group resolves) so the R-02 no-pre-existing-managed-
  entry edge captures the just-created object, not a stale reference.
- Step 3c must NOT delete the event key (unlike Step 3b) — the managed group
  always holds the keep-target.
- Script retire drops `path`-only-for-`fs` and the `targetToken`/`pruneCount`
  shape; harness must stop reading `pruneCount`.

## Open Questions / Gaps

None blocking. Architecture supplied the canonical Step 3c shape verbatim; all
integration points trace to merge-settings.js line references. Delivery-time
gates A (base-branch dep), B (harness scope), C (parity-before-delete) are
recorded in the component files where they bind.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) -- found #4936
  (mergeSettings manages only EVENT_MATCHERS group; stale "*" survives — the root
  cause), #4826 (matcher-narrowing needs opt-out prune + consumer init test
  ripples), #1195 (prefix-match settings merge), and the three vnc-031 ADRs
  (#4939/#4940/#4941). All directly informed the pseudocode.
- Deviations from established patterns: none. Step 3c deliberately reuses the
  pruneUnimatrixEvent emptied-group cleanup idiom (NFR-03) rather than
  introducing parallel machinery; keep-by-identity supersedes the script's
  command-token keep-rule (#4931) by construction.
