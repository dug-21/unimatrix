# Agent Report: crt-044-agent-3-risk

**Role**: Architecture-Risk Strategist
**Mode**: architecture-risk
**Feature**: crt-044 — Bidirectional S1/S2/S8 Edge Back-fill and graph_expand Security Comment

## Output

- `product/features/crt-044/RISK-TEST-STRATEGY.md` — written

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 3 |
| High     | 4 |
| Medium   | 2 |
| Low      | 1 |
| **Total** | **10** |

## Top Risks

1. **R-02 (Critical) — Delivery sequencing conflict with crt-043**: crt-043 is currently in delivery and treats v20 as its migration baseline. If it merges before crt-044, the `< 20` block never runs — S1/S2/S8 edges remain forward-only with no error surface. This is the highest-probability failure mode and requires a pre-merge reviewer gate, not just a test.

2. **R-01 (Critical) — Statement A uses wrong `relation_type`**: The crt-035 back-fill template uses `'CoAccess'`; Statement A needs `'Informs'`. A copy-paste error here silently back-fills nothing for S1/S2 because no row matches. Migration appears successful — no error, just zero inserts.

3. **R-03 (Critical) — One tick function omits the second `write_graph_edge` call**: Three separate functions must each independently add the symmetric second call. Missing one produces source-specific graph asymmetry that only per-source integration tests catch. Entry #4076 confirms this failure mode (zero mandatory tests shipped) has occurred in adjacent features.

## Concerns

- **R-02 has no test that can catch it if crt-043 ships first** — it requires a coordination protocol (reviewer confirms base branch `CURRENT_SCHEMA_VERSION = 19` before merging crt-044). This should be an explicit gate item for Gate 3b/3c.
- **AC-01 and AC-02 in the spec assert total Informs edge count equals bidirectional count** — these are strong assertions that will catch R-01 in a realistic DB fixture but require the fixture to be non-empty. Empty-fixture tests alone are insufficient for these ACs.
- The three-call pattern in `run_s8_tick` (valid_ids check, then two `write_graph_edge` calls) is structurally different from S1/S2 because S8 operates over a constructed pairs set, not direct query rows. The implementation agent should verify the counter variable being incremented is `pairs_written`, not `edges_written`, for S8.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #4076 (zero mandatory tests gate failure), directly elevated R-03 to Critical.
- Queried: `/uni-knowledge-search` for "risk pattern migration graph edge bidirectional" — found #4078, #3889, #4066. All directly applicable.
- Queried: `/uni-knowledge-search` for "SQLite migration schema version delivery sequencing conflict" — found #3894 (schema version cascade checklist). Informed R-10 and R-02.
- Queried: `/uni-knowledge-search` for "write_graph_edge budget counter return value false boolean" — found #4041. Directly informed R-04 and R-05.
- Stored: nothing novel to store — the concurrent migration version conflict (R-02) is feature-specific. If this pattern recurs across 2+ features a pattern entry would be warranted.
