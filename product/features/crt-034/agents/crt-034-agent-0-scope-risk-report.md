# Agent Report: crt-034-agent-0-scope-risk

## Output

- Produced: `product/features/crt-034/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary

| Severity | Count |
|----------|-------|
| High | 1 (SR-05) |
| Medium | 2 (SR-01, SR-02) |
| Low | 3 (SR-03, SR-04, SR-06) |

**Total: 6 risks**

## Top 3 for Architect/Spec Writer Attention

1. **SR-05 (High)** — Hard sequencing dependency on GH #409. If #409 prunes co_access rows before crt-034 is deployed, qualifying pairs are permanently lost with no error signal. Recommend: add first-run detectability (warn! if qualifying pairs = 0 on early ticks post-bootstrap).

2. **SR-02 (Med)** — Per-pair INSERT OR IGNORE + conditional UPDATE is two write-pool round-trips. Under SQLite write contention this silently skips pairs until the next tick. Recommend: evaluate a single UPSERT/CTE to reduce write pool hold time per pair.

3. **SR-01 (Med)** — Separate MAX(count) query runs every tick on the full co_access table before the batch fetch. Recommend: fold MAX into the batch query as a subquery to avoid a second read competing with the write sequence.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #3579, #2758, #1203, #1686 (delivery/gate process lessons, not directly applicable to this scope)
- Queried: `/uni-knowledge-search` for "risk pattern background tick graph edges" — found #3821 (directly applicable: confirmed write_pool_server() and insertion point pattern), #1542 (background tick error semantics), #1616 (dedup flag ordering)
- Queried: `/uni-knowledge-search` for "co_access GRAPH_EDGES promotion PPR tick ordering" — confirmed #3821 and ADR entries #3785, #3735
- Queried: `/uni-knowledge-search` for "outcome rework background tick infallible" — found #3723 (tick observability lesson), #1542, #1366
- Stored: entry #3822 "Promotion tick idempotency: near-threshold pair oscillation must be explicitly specified" via context_store (pattern, background-tick topic) — novel pattern not covered by any existing entry; SR-03 in this assessment is its first instance
