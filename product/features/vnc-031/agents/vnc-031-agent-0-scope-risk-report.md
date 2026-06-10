# Agent Report: vnc-031-agent-0-scope-risk

Mode: scope-risk. Produced: `product/features/vnc-031/SCOPE-RISK-ASSESSMENT.md` (38 lines, under 100-line constraint).

## Risk Summary
- High severity: SR-01, SR-02, SR-04, SR-05, SR-06 (5)
- Medium severity: SR-03, SR-07 (2)
- Total: 7 scope-level risks across Technology / Scope-Boundary / Integration.

## Top 3 for Architect/Spec Attention
1. **SR-05 (High/High)** — cross-consumer blast radius. Behavior change hits `init`, `init --remote`, and dogfood switchover at once; install-surface event/count tests are sensitive (#4826). Exercise both call arms on real legacy input.
2. **SR-04 (High/Med)** — retiring the dogfood script prune (Goal 4) can regress promote/rollback if source doesn't subsume the script's whole-token/dirname matching. Prove parity on real legacy input before deleting (#4938).
3. **SR-01 (High/Med)** — the "compare against fresh command" keep-rule (OQ-1) can silently prune the entry it meant to keep, zeroing a managed event. Key off Step 3's repointed entry identity, not a re-derived string.

## Knowledge Stewardship
- Queried: `context_search` for install-surface / merge-settings / idempotency lessons and patterns -- found #4938 (genesis lesson: wrapper must reconcile state the frozen primitive does not manage; verify on real legacy input), #4826 (install-surface test sensitivity, cited throughout SCOPE), #4811 / #4926 / #706 (adjacent decisions and shipped replacement source). All informed SR-01/04/05.
- Stored: nothing novel to store -- the recurring pattern (verify migration tools on real legacy-shaped input; install-surface changes ripple to event-count consumer tests) is already captured in #4938 and #4826. No 2+-feature pattern beyond those exists to add.
