# Agent Report: crt-036-agent-0-scope-risk

## Output
- Produced: `product/features/crt-036/SCOPE-RISK-ASSESSMENT.md`
- 9 risks identified across three categories
- Document: 38 lines (under 100-line limit)

## Risk Summary
| Severity | Count |
|----------|-------|
| High | 3 (SR-01, SR-02, SR-04) |
| Med | 5 (SR-03, SR-05, SR-06, SR-07, SR-08) |
| Low | 1 (SR-09) |

## Top 3 Risks for Architect/Spec Writer

1. **SR-01/SR-02 (High)** — Write pool deadlock and long-lock risk from multi-cycle DELETE transactions at tick time. Must design GC as per-cycle short transactions with connection release between cycles, not one spanning transaction. Historical evidence: entry #2249 (deadlock pattern).

2. **SR-04 (High)** — Two independent 60-day DELETE sites (`status.rs` + `tools.rs`) that must both be removed. Delivery risk: the `tools.rs` FR-07 path is easy to overlook. Spec writer must make each site an independent verifiable AC.

3. **SR-07 (Med/High likelihood)** — `PhaseFreqTable` lookback (`query_log_lookback_days`) can silently exceed available retained data after K-cycle pruning takes effect. Architect should add a tick-time `tracing::warn!` on mismatch.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" -- found #3579 (gate-3b test omission), #2758 (gate-3c grep check), #1203 (cascading rework); none directly specific to retention GC
- Queried: `/uni-knowledge-search` for "outcome rework retention deletion GC" -- found #3914 (crt-036 two-hop join pattern, already stored); confirmed pre-existing coverage
- Queried: `/uni-knowledge-search` for "risk pattern" (category: pattern) -- no retention-specific patterns beyond #3914
- Queried: `/uni-knowledge-search` for "SQLite background tick transaction atomicity partial delete" -- found #2159 (pool connection identity) and #2249 (deadlock) as directly applicable
- Stored: nothing novel to store -- the write-pool deadlock (#2249) and connection-identity (#2159) patterns are already in Unimatrix and were applied as evidence for SR-02 and SR-03
