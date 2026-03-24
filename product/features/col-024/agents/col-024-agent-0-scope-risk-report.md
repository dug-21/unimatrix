# Agent Report: col-024-agent-0-scope-risk

## Output

Produced: `product/features/col-024/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary

| Severity | Count |
|----------|-------|
| High | 2 (SR-01, SR-06) |
| Medium | 3 (SR-02, SR-03, SR-04) |
| Low | 2 (SR-05, SR-07) |

## Top 3 Risks for Architect Attention

1. **SR-01 (High/High)** — `cycle_events.timestamp` (seconds) vs `observations.ts_millis` (milliseconds) unit mismatch is a silent correctness bug. Every window comparison is wrong if the conversion is missed. Mandate a named helper before any SQL is written.

2. **SR-06 (High/Med)** — When topic_signal enrichment was never applied, `load_cycle_observations` returns empty and falls through to legacy silently. The empty result is indistinguishable between "no cycle_events rows" and "rows exist but no attributed observations." Add a structured log event on fallback activation to make attribution gaps detectable.

3. **SR-03 (Med/Med)** — Open-ended windows (`cycle_start` with no `cycle_stop`) use `unix_now_secs()` as the stop boundary. A force-abandoned feature without a stop event will over-include observations from subsequent work in the same session. The spec must define behavior for abandoned cycles.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — no directly relevant results; found gate procedure (#167) and convention (#141), not applicable.
- Queried: `/uni-knowledge-search` for "outcome rework observation attribution session" — found #866 (ADR-003 col-020 attribution on report) and #759 (ADR-017-002 HashMap accumulator for topic signals), both informative for understanding the attribution design context.
- Queried: `/uni-knowledge-search` for "risk pattern" (category: pattern) — no applicable cross-feature risk patterns found.
- Stored: nothing novel to store — the timestamp unit mismatch risk (SR-01) is specific to this feature's schema boundary, not a cross-feature pattern visible across 2+ features yet.
