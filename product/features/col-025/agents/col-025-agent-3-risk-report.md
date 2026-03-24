# Agent Report: col-025-agent-3-risk

**Role**: Risk Strategist (Architecture-Risk Mode)
**Feature**: col-025 — Feature Goal Signal

## Deliverable

`/workspaces/unimatrix/product/features/col-025/RISK-TEST-STRATEGY.md` — written.

## Risk Summary

| Priority | Count |
|----------|-------|
| High (including elevated) | 5 (R-02, R-04, R-05, R-06, R-07) |
| Medium | 5 (R-01, R-03, R-08, R-09, R-11) |
| Low | 2 (R-10, R-12) |
| **Total** | **12** |

## Key Risks for Human Attention

1. **R-04 — SubagentStart precedence inversion (High)**: The explicit goal branch in `listener.rs` is the only hand-coded precedence logic in this feature. If inverted, goal overrides a non-empty `prompt_snippet` silently. AC-12 (inversion guard) is a non-negotiable test. Historical lesson #2758 confirms that non-negotiable tests must be verified by name at gate 3c.

2. **R-02 — Migration test cascade (High)**: Pattern #2933 is a recurring CI trap. Three existing test files assert `schema_version ≤ 15` and must be updated. A new `migration_v15_to_v16.rs` test must be added. High likelihood because it has happened on every previous migration feature.

3. **R-07 — UDS byte-limit truncation panic (High, elevated from High×Low)**: The UDS path truncates silently rather than rejecting. A non-UTF-8-boundary slice will cause a panic terminating the server. A char-boundary-safe truncation test with a multi-byte character straddling the limit is required.

4. **R-06 — SessionState struct literal compile failures (High)**: Pattern #3180 confirms this fires on every `SessionState` field addition. Test helpers (`make_session_state`, `make_state_with_rework`) must be audited before merge.

5. **R-05 — Topic-signal synthesis removal breaks existing tests (High)**: ADR-002 explicitly notes existing `derive_briefing_query` tests must be updated. These are not behavioral regressions but stale test contracts that will cause CI failures if not addressed proactively.

## Open Questions Raised

- AC-13 specifies 2 048 bytes for the MCP guard; ADR-005 specifies 4 096 bytes for `MAX_GOAL_BYTES`. The SPECIFICATION and ADR use different limits for the same constant. Delivery must resolve which value is authoritative before implementation (spec FR-03 says 2 048; ADR-005 says `MAX_GOAL_BYTES = 4096`). This inconsistency should be flagged to the Design Leader.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #2758 (gate-3c non-negotiable test names), #2800 (circuit breaker testability)
- Queried: `/uni-knowledge-search` for "SQLite migration schema version cascade" — found #2933 (cascade pattern, directly informs R-02), #378 (old-schema DB tests)
- Queried: `/uni-knowledge-search` for "risk pattern SessionState session resume" — found #3180 (field additions require helper updates, directly informs R-06), #3301 (graceful degradation pattern)
- Queried: `/uni-knowledge-search` for "SubagentStart hook session_id lookup" — found #3297 (session_id routing gotcha), #3230 (routing pattern)
- Stored: nothing novel to store — all relevant patterns (#2933, #3180, #3301) already exist in Unimatrix; no new cross-feature pattern visible from this single feature
