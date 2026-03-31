# Agent Report: crt-037-agent-3-risk

## Task
Architecture-Risk mode: produce RISK-TEST-STRATEGY.md for crt-037 (Informs edge type).

## Artifacts Produced
- `/workspaces/unimatrix/product/features/crt-037/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count | Risk IDs |
|----------|-------|----------|
| Critical | 4 | R-01 (CHECK constraint), R-02 (PPR direction), R-03 (composite guard), R-20 (missing tests at gate) |
| High | 8 | R-04, R-05, R-06, R-07, R-08, R-09, R-10, R-16 |
| Medium | 6 | R-11, R-12, R-13, R-14, R-17, R-19 |
| Low | 2 | R-15, R-18 |
| **Total** | **20** | |

## Top Risks for Tester Attention

**R-20 (Critical):** The gate-3b missing-tests pattern (entry #3579) applies directly here. AC-13–AC-23 are 11 tick integration tests covering Phase 4b/8b. History shows implementation waves can deliver correct production code with zero phase-specific tests. The tester must verify all 11 are present and passing before gate submission — not just that the file exists.

**R-02 (Critical):** PPR direction for `Informs` must be tested by asserting that the *specific lesson node* receives PPR mass when a decision node is seeded — not that any node has non-zero score. Entry #3754 documents that a direction error survived two gate checks in crt-030 because the test asserted non-zero rather than the specific node. AC-05 must be written with this in mind.

**R-03 (Critical):** The Phase 8b composite guard has five independent predicates. Each must have its own negative integration test (AC-14, AC-14b, AC-15, AC-16, AC-17, plus neutral boundary). The AC-13 happy-path test does not substitute for individual guard tests.

**R-04 (High):** Both cross-route failure modes must be tested: an `Informs` pair not written by Phase 8 (Supports path), and a `SupportsContradict` pair not written by Phase 8b (Informs path). Without both, the discriminator routing could be inverted without a test failure.

**R-01 (Critical — pre-implementation gate):** OQ-S1 (CHECK constraint on `relation_type`) must be resolved before Phase C begins. This is a delivery prerequisite, not a test-time check. If the column has a constraint, the feature is blocked by C-01 (no schema migration).

## Open Questions Factored In

- OQ-S1: Elevated R-01 to Critical; DDL inspection is a delivery gate prerequisite.
- OQ-S2: R-07 includes test scenarios for neutral residual noise; FR-11 mutual exclusion check surfaces as R-19.
- OQ-S3: R-13 identifies secondary-lookup latency as an NF-01 threat; requires in-memory `entry_meta` map from `all_active`.
- OQ-S4 / Gap-2 (FR-11): R-19 covers the mutual exclusion case where entailment > threshold AND neutral > 0.5 simultaneously.

## Architecture-Specific Risks Not in Scope Risk Doc

All four were incorporated:
- Discriminator tag routing correctness → R-04 (compiler enforcement verified insufficient alone; origin filter logic error is a runtime risk)
- Phase 4b → Phase 7 batch merge flow → R-05 (metadata survival data-flow test)
- PPR direction lesson-node assertion → R-02 (specific node mass, not aggregate non-zero)
- Cap priority sequencing → R-06 (Supports fully fills before Informs takes remainder — sequential reservation invariant)

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for risk patterns — found entry #2800 (cap logic testability), #3579 (gate-3b missing tests), #3675 (tick source-candidate bounds), #3754 (direction semantics lesson), #3744 (PPR Outgoing pattern), #3937 (neutral-zone detection pattern)
- Stored: nothing novel to store — all applicable patterns already in Unimatrix; crt-037-specific risks live in RISK-TEST-STRATEGY.md
