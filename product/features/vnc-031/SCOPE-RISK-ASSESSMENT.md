# Scope Risk Assessment: vnc-031

Mode: scope-risk. Sources: SCOPE.md, PRODUCT-VISION.md. Historical: #4938 (genesis lesson), #4826 (install-surface test sensitivity), #4811 (vnc-027 ADR-004), #4926 (nan-016 ADR-003), #706 (crt-052 replacement source).

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | "Compare against the fresh `newHookEntry.command`" keep-rule (OQ-1) can mis-classify: if the managed-group repoint composes a command that does not byte-equal what Step 3 actually wrote (whitespace, `LD_LIBRARY_PATH` prefix, arg ordering), the kept entry gets pruned and the event loses ALL uni hooks. | High | Med | Architect: make the keep-target the entry-identity from Step 3's repoint (the object it mutated), not a re-derived string compare. Spec an invariant: every managed event ends with exactly one uni hook (AC-02 must fail loud, not silently zero). |
| SR-02 | The prune trusts `isUnimatrixHook` as the sole ownership signal across foreign groups it never touched before. Any false-positive in `UNIMATRIX_PATTERNS` (Non-Goal 2 forbids changing it) now deletes a user hook it previously only ignored — blast radius widens from one group to all groups. | High | Low | Architect/spec: treat the prune's correctness as fully bounded by `isUnimatrixHook` precision. Add a regression asserting a near-miss foreign hook (uni-looking but not classified) survives in a non-managed group (extends AC-03). |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | OQ-2/OQ-3 partition (managed events cross-group prune vs Step 3b opt-out prune for non-registered events) is the load-bearing seam. If the two paths overlap or leave a gap for an event that is registered-this-run but mid-rename, a stale hook survives or a fresh one is double-pruned. | Med | Med | Spec must state the exact event partition: managed-event prune covers `events`; Step 3b opt-out covers `HOOK_EVENTS \ events`; union = all, intersection = empty. Confirm OQ-3 = registered-only before design. |
| SR-04 | Goal 4 retires `scripts/dogfood-switchover.sh`'s `PRUNE_FRAGMENT`. If the source prune does not subsume every case the script's whole-shell-token matcher handled (e.g. `.bak`/old-client-dir tokens, the rollback dirname-level match), retiring the script regresses promote/rollback even though `mergeSettings` tests pass. | High | Med | Architect: enumerate every case `pruneStaleUniHooks` handled and map each to the source behavior BEFORE deleting it (the #4938 "enumerate what the primitive does/doesn't manage" discipline). Do not retire the script until parity is proven on real legacy input. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | Cross-consumer blast radius: behavior change lands simultaneously for `init` (string arm), `init --remote` (object arm), and dogfood switchover. Consumer init tests that iterate `HOOK_EVENTS` or assert event/group counts are sensitive to install-surface shape changes (#4826) and may false-fail or, worse, codify the new shape without intent. | High | High | Spec: AC-06/AC-10 must exercise BOTH call arms on real legacy-shaped input. Re-run all `init`/`initRemote` consumer tests; any fixture touch must be justified as intended, not rubber-stamped. |
| SR-06 | vnc-027 ADR-004 (#4811) matcher-narrowing adjacency: an over-broad prune could remove the retained cycle-event `context_cycle` frame under `PRETOOLUSE_CYCLE_MATCHER`, or perturb the SubagentStop opt-in matrix. | High | Low | Spec: explicit AC that `PRETOOLUSE_CYCLE_MATCHER` retains its fresh entry and SubagentStop opt-in/opt-out behavior is byte-unchanged (AC-07 covers opt-out; add opt-in retention). |
| SR-07 | Idempotency regression: the new prune runs after Step 3 on already-merged input. A second pass must be a no-op (`deepStrictEqual`). If the prune's "fresh entry" identification differs between first run (stale `"*"` present) and second run (already clean), round-trip breaks. | Med | Med | AC-05 must include the stale-`"*"`-on-first-run case explicitly. Treat idempotency as a gate, not an afterthought. |

## Assumptions

- **(SCOPE Dependencies)** crt-052 #706 + vnc-027 #4811 are merged on `main`, so the replacement observation source already backs the broad-hook prune. If either is NOT actually on the delivery base branch, vnc-031 prunes telemetry with no live replacement. Architect must verify both are present on the branch this delivers from, not just "on main as of writing."
- **(OQ-1)** No consumer relies on a stale uni hook surviving. SCOPE found none except the nan-016 harness assertion (updated by AC-09). If any other harness/runbook asserts survival, it breaks silently.
- **(OQ-4 / Goal 1)** Any `isUnimatrixHook===true` entry is reconcilable Unimatrix-owned state — a user cannot legitimately pin a uni-pattern hook under a non-managed matcher. This is the one genuine behavioral change (survive → pruned) and rests on this assumption being acceptable to the human.

## Design Recommendations

1. **(SR-01, SR-07)** Make the keep-target the Step 3 repointed entry's identity, not a string re-compare; the writer knows which object it kept. Add a hard invariant: managed event → exactly one uni hook, never zero.
2. **(SR-04)** Do NOT retire `dogfood-switchover.sh`'s prune until source parity is proven on REAL legacy-shaped input (#4938: verify on legacy input, never a pre-narrowed seed). Enumerate the script's token/dirname cases and map each.
3. **(SR-05)** Both call arms + all consumer init tests exercised on real legacy input; fixture changes justified, not rubber-stamped (#4826).
4. **(OQ-4 / SR-02)** Surface the survive→pruned behavioral change for explicit human sign-off; it is the lone behavioral call in #728.
