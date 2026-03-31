# Agent Report: crt-036-agent-3-risk

## Output

- Produced: `product/features/crt-036/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 3 |
| High | 8 |
| Medium | 4 |
| Low | 1 |
| **Total** | **16** |

## Top Risks for Tester Attention

1. **R-01 (Critical)** — Legacy 60-day DELETE surviving at either status.rs or tools.rs after delivery. Two independent grep assertions required; a combined grep is insufficient. Entry #3579 confirms this class of omission is the most common delivery gate failure.

2. **R-02 (Critical)** — Cascade delete order violation (sessions deleted before injection_log). The mutation test (intentional inversion + verify test failure) is mandatory — without it, the AC passes even if the ordering guard is absent.

3. **R-04 (Critical)** — Per-cycle transaction not released between cycles, holding the write pool for the full multi-cycle batch. Concurrent-write integration test required alongside structural code review. Entries #2249 and #2159 document this exact failure mode from prior delivery.

4. **R-03 (High)** — mark_signals_purged() using INSERT OR REPLACE instead of targeted UPDATE, silently overwriting summary_json. The summary_json content preservation check is the only way to detect this at test time.

5. **R-09 (High)** — Two-hop subquery full-table scan at 152 MB scale. EXPLAIN QUERY PLAN assertion is mandatory; without it the index assumption is untested until production load.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"lesson-learned failures gate rejection"` -- entries #3579, #2758, #2577, #3766 applied
- Queried: `/uni-knowledge-search` for `"SQLite write pool connection transaction background tick"` -- entries #2249, #2269, #2159 applied
- Queried: `/uni-knowledge-search` for `"cycle_review_index observations deletion session cascade"` -- entries #3914, #3793 applied
- Queried: `/uni-knowledge-search` for `"risk pattern"` -- no directly applicable patterns found for this feature's specific combination
- Stored: nothing novel to store — relevant patterns already captured in Unimatrix (#2249, #2159, #3914); audit_log timestamp unit risk is feature-specific and not yet recurrent across 2+ features
