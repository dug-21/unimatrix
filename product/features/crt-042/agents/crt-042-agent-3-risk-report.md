# Agent Report: crt-042-agent-3-risk

## Status: Complete

## Output

- `/workspaces/unimatrix/product/features/crt-042/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 2 |
| High | 7 |
| Med | 8 |
| Low | 1 |
| **Total** | **17** |

## Top Risks for Tester Attention

1. **R-02 (Critical / High)** — S1/S2 Informs edges are single-direction in GRAPH_EDGES. Outgoing-only traversal silently sees half the graph. This is a **blocking gate before Phase 0 code** — confirmed from `graph_enrichment_tick.rs` line 92 (`t2.entry_id > t1.entry_id`). The tester must verify bidirectionality via a SQL query before accepting any Phase 0 implementation.

2. **R-01 (Critical / Med)** — Flag-off regression. `ppr_expander_enabled = false` must produce bit-identical results to pre-crt-042 for all existing test cases. Any Phase 0 code that runs outside the flag guard is a silent regression across all search queries.

3. **R-03 (High / Low)** — Quarantine bypass. Quarantine check is the caller's responsibility in `search.rs`, not inside `graph_expand`. If the check is missing, reordered, or applied to the wrong field, quarantined entries enter results. Requires an explicit fixture test with a quarantined graph-reachable entry.

4. **R-04 (High / Med)** — O(N) latency at full expansion (200 × O(7000) = 1.4M comparisons). Timing instrumentation (AC-24) is mandatory and must be tested with a tracing subscriber — do not defer (entry #3935 documents the gate failure when tracing tests were deferred).

5. **R-08 (High / High)** — InferenceConfig hidden test sites. Historical pattern (entries #4044, #2730, #4013): new fields miss test literal constructions. Three new fields require grep-verified coverage of all `InferenceConfig {` literal sites.

## Non-Negotiable Tests (do not defer)

- AC-01: flag-off regression (existing suite unchanged)
- AC-14: quarantine bypass fixture
- AC-24: timing instrumentation emission (tracing subscriber test)
- AC-25: cross-category behavioral regression (the core feature proof)
- AC-18/19/20/21: config validation tests (unconditional, four tests)

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"lesson-learned failures gate rejection"` — entries #3579, #2758, #3935 directly inform non-negotiable test requirements
- Queried: `/uni-knowledge-search` for `"risk pattern search pipeline PPR graph traversal"` — entries #3730, #3740, #3744, #3896, #3950 inform integration and direction risks
- Queried: `/uni-knowledge-search` for `"SQLite graph edges directionality migration back-fill"` — entries #3889, #3891 inform R-02 back-fill scenario
- Queried: `/uni-knowledge-search` for `"get_embedding O(N) latency"` — entries #1712, #1724 inform R-15
- Queried: `/uni-knowledge-search` for `"InferenceConfig validation hidden test sites"` — entries #4044, #2730, #3817, #3769, #4013 inform R-08
- Stored: nothing novel to store — all identified risk patterns already exist in Unimatrix; no cross-feature pattern newly visible from this feature alone
