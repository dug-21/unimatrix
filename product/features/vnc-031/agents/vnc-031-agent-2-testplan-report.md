# Agent Report — vnc-031-agent-2-testplan (Stage 3a)

## Deliverables
- product/features/vnc-031/test-plan/OVERVIEW.md
- product/features/vnc-031/test-plan/merge-settings-step3c.md
- product/features/vnc-031/test-plan/dogfood-switchover-retire.md
- product/features/vnc-031/test-plan/dogfood-effect-harness.md

Component plans map 1:1 to the IMPLEMENTATION-BRIEF Component Map (merge-settings Step 3c, dogfood-switchover retire, dogfood-effect harness).

## Risk Coverage Mapping
Every R-01..R-15 mapped to ≥1 concrete scenario (OVERVIEW §Risk→Test Mapping); SR-01..SR-07 traced. Two Critical risks drive design:
- R-01 (identity→string-compare degradation): shape-varying near-twin + exact-fresh-command assertions designed so a `command ===`/`includes` reimplementation turns ≥1 test red. This is the load-bearing residual risk — ADR-001 makes SR-01 unrepresentable by construction, so tests guard the *mechanism*, not the happy path.
- R-04 (script-retire parity): P1–P8 proven on REAL legacy input in the dogfood-effect harness, static fragment-absence grep, and a binding commit-ordering gate (parity-proof ≤ deletion).

Critical coverage requested in spawn prompt, all planned:
- AC-02 fail-loud zero invariant (R-02): explicit `assert(count !== 0, ...)` + adversarial no-managed-entry-on-input seed.
- R-01 string-compare discriminator: shape-varying near-twin pruned, keep-target survives by identity.
- R-07 near-miss foreign: `my-unimatrix-wrapper run` survives byte-for-byte.
- AC-05/FR-12 idempotency incl. stale-`"*"`-on-run-1.
- AC-06 both arms identical.
- AC-09 GATE C P1–P8 parity proof procedure (OVERVIEW §GATE C).
- AC-09 GATE B negative control preserved (`assert.throws` on unpruned post-state, repointed reconstruction).

## Integration / Harness Plan
- **infra-001: N/A** — zero Rust/server/MCP surface; smoke gate does not apply (stated explicitly for the 3c report).
- Integration surface is JS-package-internal; validated by `node --test packages/unimatrix/test/` + the two script-level harnesses.
- The dogfood-effect harness (real script → real settings → re-fire) is the closest integration analog and hosts the GATE C parity proof. Self-skips loudly if npm/tar can't stage install — SKIP ≠ PASS.
- GATE A (delivery-base #706/#4811) and GATE B (harness attribution repoint) recorded as delivery-time obligations in the coverage report.

## Key Design Decisions
- Survivor always asserted by **exact** command equality (not `includes`) — structural proxy for object identity (R-01).
- P6 quoted-spaced-path proven at unit level (a spaced install dir isn't realizable in `os.tmpdir()`); harness covers P1–P5/P7/P8 on real install. Flagged as Open Question 2 for the implementer/SM.
- GATE B is an attribution repoint, not a nan-016 rewrite: RUNBOOK carries no surviving-`"*"` assertion (grep-confirmed); `noPrunePromoteContent` must be repointed to a no-Step-3c reconstruction so the negative control stays non-vacuous once `mergeSettings` itself prunes.

## Open Questions
1. `seedWithCrossGroupStale` signature — fresh command asserted per-arm via the arm's own producer (#4263). Confirm with implementer in 3b.
2. P6 unit-vs-harness split for GATE C — confirm identity unit proof + harness real-input proof together satisfy the gate.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get — retrieved vnc-031 ADR-001/002/003 (#4939/#4940/#4941), nan-016 patterns #4930/#4936 (surviving stale `"*"`), nan-016 ADR-003 #4926. Risk Strategy already cites the load-bearing lessons (#4938 parity-on-real-input, #4932 negative-control-reconstruction, #4827 arm-key reconciliation, #4263 fixture derivation, #4826 event-count sensitivity).
- Stored: nothing novel to store — the object-identity-keep-rule-shifts-residual-risk-to-mechanism-degradation pattern is observed in only this one feature; per the ≥2-feature rule, defer storing until a second feature exhibits it (same conclusion the Risk Strategy reached). All other patterns already exist and are referenced.
