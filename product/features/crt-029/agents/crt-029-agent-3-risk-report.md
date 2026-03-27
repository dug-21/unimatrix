# Agent Report: crt-029-agent-3-risk

## Output
- `/workspaces/unimatrix/product/features/crt-029/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 3 |
| High | 6 |
| Medium | 3 |
| Low | 1 |
| **Total** | **13** |

## Top Risks for Human Attention

**R-01 (Critical)** — False-positive `Contradicts` edges from tick silently suppress search results via col-030 `suppress_contradicts`. Architecture mitigation is solid (explicit `contradiction_threshold` parameter in `write_inferred_edges_with_cap`), but this must be verified by AC-10 + AC-19† unit tests. Any softening of the threshold floor in a future refactor would re-open this silently.

**R-02 (Critical)** — `get_embedding` O(N) unbounded scan if source-candidate cap is not enforced before Phase 4. Architecture mitigation is ADR-003 (`select_source_candidates` capped to `max_graph_inference_per_tick`). AC-06c call-count assertion is mandatory.

**R-09 (Critical)** — Tokio handle access inside rayon closure causes runtime panic (no Tokio runtime on rayon threads). Historical incidents in crt-022 (Unimatrix #3339, #3353). No compile-time detection. Mandatory code review of rayon closure body.

## Unresolved Risk Requiring Human Decision

**R-06 / SR-06** — Two conflicting Unimatrix ADRs for `compute_graph_cohesion_metrics` pool choice:
- Entry **#3593**: `write_pool_server()` (write pool)
- Entry **#3595**: `read_pool()` (read pool)

The architecture doc (citing entry #3619) asserts the function uses `read_pool()`, which would confirm SR-06 is mitigated. However, the two conflicting ADR entries remain active in Unimatrix. A human must:
1. Confirm the actual pool used by `compute_graph_cohesion_metrics` in source code
2. Deprecate the incorrect ADR entry (#3593 or #3595)

Until resolved, the integration risk between observability queries and tick writes cannot be confirmed as mitigated.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for risk patterns — found entries #3655, #2730, #2800, #3653 (all relevant, incorporated)
- Queried: `/uni-knowledge-search` for gate failures — found entries #2758, #3579, #3548, #3353, #3339 (rayon/tokio panic history informed R-09 severity)
- Queried: `/uni-knowledge-search` for pool contention — found conflicting ADRs #3593/#3595 (surfaced as unresolved R-06)
- Stored: nothing novel to store — all risk patterns already recorded in Unimatrix. No new cross-feature pattern discovered from this feature alone.
