# Agent Report: crt-043-agent-3-risk

## Output

Produced: `product/features/crt-043/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 1 |
| High | 6 |
| Med | 5 |
| Low | 2 |
| **Total** | **13** |

## Top Risks for Architect/Spec Attention

**R-01 (Critical)** — INSERT/UPDATE residual race. ADR-002 chosen Option 1 and accepted the race. The test strategy requires an integration test that awaits both spawned tasks and confirms non-NULL goal_embedding in the DB. The concurrent-load scenario (20 simultaneous CycleStart events) is flagged as a recommended slow-test.

**R-03 (High)** — Missing write site. Four independent observation write sites must each capture phase pre-spawn_blocking. This is the highest-likelihood implementation failure — each site is an independent omission opportunity. All four require read-back DB tests.

**R-10 (High)** — Embed service unavailable path. warn must be emitted; cycle start must not block. Historically (entry #735, #771), fire-and-forget failure paths are where silent failures accumulate. Test must verify warn is captured and the call returns without awaiting the embed.

## Non-Negotiable Tests (gate-3b)

1. Round-trip `encode_goal_embedding` → `decode_goal_embedding` (R-02, AC-14)
2. Real v19 database through `Store::open()` → both columns present, schema_version = 20 (R-05, AC-01/AC-07)
3. Phase written and read back for all four observation write sites (R-03, AC-09/AC-10)
4. Embed-service-unavailable: warn emitted, cycle start not blocked, goal_embedding NULL (R-10, AC-04a)
5. Empty/absent goal: no embed task spawned, no warn (R-09, AC-04b)
6. Migration idempotency: `Store::open()` on v20 completes without error (R-06, AC-11)

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — #3579, #2758, #2577 (gate-3b test omission patterns, non-negotiable test validation discipline)
- Queried: `/uni-knowledge-search` for "risk pattern SQLite migration embedding serialization" — #4065 (bincode helper pairing discipline, already stored)
- Queried: `/uni-knowledge-search` for "tokio spawn fire-and-forget race" — #735, #771, #1673 (pool saturation, blocking store patterns, supervisor)
- Stored: nothing novel to store — all relevant patterns are already in Unimatrix (#4065, #735, #771)
