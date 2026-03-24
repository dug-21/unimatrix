# Agent Report: col-024-agent-3-risk

**Mode**: architecture-risk
**Output**: `product/features/col-024/RISK-TEST-STRATEGY.md`

## Summary

Produced RISK-TEST-STRATEGY.md with 14 risks (R-01–R-12, plus integration,
edge case, security, and failure mode sections) and 29+ required test scenarios.

## Risk Summary by Priority

| Priority | Count | Key Risks |
|----------|-------|-----------|
| Critical | 2 | R-01 (timestamp conversion bypass), R-02 (enrichment site gap) |
| High | 4 | R-03 (fallback not activated), R-04 (explicit signal overridden), R-05 (double block_sync), R-06 (open-ended window over-inclusion) |
| Med | 6 | R-07–R-10, plus integration and failure mode risks |
| Low | 2 | R-11 (deduplication), R-12 (enrichment scope leak) |

## Top Risks for Human Attention

**R-01** — Timestamp unit mismatch is the highest-stakes correctness risk. Both
columns are `i64`; the compiler cannot catch a missing `* 1000`. ADR-002 mandates
`cycle_ts_to_obs_millis` as the single conversion site. Tests must verify both
inclusion (observation inside window returned) and exclusion (observation outside
window not returned) to catch both under- and over-multiplication.

**R-02** — Four write sites, one helper, but the helper must be wired at each
site independently. A single missed call site means observations from that path
carry `topic_signal: None`, making them invisible to the new primary lookup.
Historical evidence (#981, #756) shows this class of silent attribution miss
has caused feature rework before. Each site must have its own test.

**R-05** — Single `block_sync` entry (ADR-001) is the correct architecture, but
a per-window loop that accidentally calls `block_sync` per iteration panics
inside a Tokio runtime. This was a recurring problem in #735 and #1688. The
multi-window test must run inside `#[tokio::test]` to catch this at the correct
runtime boundary.

## Open Questions

1. **E-03 (malformed event log)**: Multiple `cycle_start` rows without an
   intervening `cycle_stop` is not covered by the spec. The window-pairing
   algorithm's behavior in this case must be defined in implementation and
   documented in a test. Recommend the tester confirm expected behavior with
   the implementor before writing the test.

2. **FM-04 (registry mutex poisoning)**: The spec does not address `get_state`
   returning a poisoned lock result. Recommend the implementor confirm whether
   `enrich_topic_signal` uses `.ok()` / `.unwrap_or(None)` rather than
   `.unwrap()` to avoid panic propagation.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for lesson-learned failures gate rejection -- found #981/#756 (NULL feature_cycle silent failure, directly informs R-02/R-03 severity elevation)
- Queried: /uni-knowledge-search for risk patterns observation attribution sync trait -- found #3367 (topic_signal enrichment pattern, confirms architecture direction)
- Queried: /uni-knowledge-search for timestamp unit mismatch -- found #3372 (ADR-002 col-024)
- Queried: /uni-knowledge-search for spawn_blocking fire-and-forget -- found #735, #1688 (spawn_blocking pool saturation and timeout lessons, elevated R-05 to High)
- Stored: nothing novel to store -- attribution-miss pattern already in #981/#756; timestamp and enrichment patterns already stored as col-024 ADR entries #3372–#3374
