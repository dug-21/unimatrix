# Agent Report: crt-040-agent-0-scope-risk

**Mode**: scope-risk
**Output**: product/features/crt-040/SCOPE-RISK-ASSESSMENT.md (40 lines)

## Risk Summary

| Severity | Count |
|----------|-------|
| High | 2 (SR-01, SR-04) |
| Med | 4 (SR-02, SR-05, SR-07, SR-08) |
| Low | 3 (SR-03, SR-06, SR-09) |

## Top 3 Risks for Architect/Spec Attention

1. **SR-01 (High)** — Edge writer generalization: `write_nli_edge` hardcodes `'nli'` as source. The new `write_graph_edge` sibling must be distinct; any accidental reuse of the old writer silently retags all Informs edges with wrong source.

2. **SR-04 (High)** — UNIQUE constraint scope unverified: If `graph_edges` UNIQUE includes the `source` column, `INSERT OR IGNORE` cannot deduplicate Path B + Path C collision. This is a correctness invariant that must be confirmed from migration DDL before spec is written.

3. **SR-07 (Med)** — Tick infallibility contract: `run_graph_inference_tick` returns `()` and must never propagate errors. Path C embedding lookups and SQL writes must follow the "log warn, continue" pattern established by entry #3883 — any `unwrap` or `?` silently kills the tick.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection graph edge detection" — found #2800 (cap logic testability), #3579 (mandatory tests), #3645 (boundary case traces)
- Queried: `/uni-knowledge-search` for "risk pattern graph inference tick structural edge writes" — found #3883 (tick write pool pattern), #3884 (INSERT OR IGNORE idempotency), #3822 (near-threshold oscillation)
- Queried: `/uni-knowledge-search` for "outcome rework InferenceConfig cosine threshold" — found #4018, #4019 (crt-039 ADRs confirming cosine floor decisions)
- Stored: nothing novel to store — risks are feature-specific to crt-040; no cross-feature pattern visible yet (pattern visibility requires 2+ features)
