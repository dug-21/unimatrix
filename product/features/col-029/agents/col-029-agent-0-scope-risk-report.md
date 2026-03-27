# Agent Report: col-029-agent-0-scope-risk

## Output
- Produced: `/workspaces/unimatrix-col-029/product/features/col-029/SCOPE-RISK-ASSESSMENT.md`
- Risks: 6 total (2 High Technology, 2 High Scope Boundary, 2 Integration — one Med, one High)

## Risk Summary
| Severity | Count |
|----------|-------|
| High | 4 (SR-01, SR-03, SR-05, SR-06) |
| Med | 2 (SR-02, SR-04) |
| Low | 0 |

## Top 3 for Architect Attention
1. **SR-03 / SR-06** (High) — The caching path is ambiguous. Scope says "cached in maintenance report struct" but `maintenance_tick` uses a thin `MaintenanceDataSnapshot`, not `StatusReport`. Without an explicit cache handle the six metrics will either never reach `compute_report` or will force a re-query on every status call. Recommend the `Arc<RwLock<Option<GraphCohesionSnapshot>>>` pattern (Unimatrix #1560).
2. **SR-01** (High) — `inferred_edge_count` is defined as `source='nli'`. This string is not a named constant. When #412 ships its inference pass, silent divergence is a real risk. Architect should define a shared constant before implementation.
3. **SR-04** (Med) — `cross_category_edge_count` is not derivable from a single JOIN — it requires joining ENTRIES twice. The SQL design needs explicit architect review to avoid a cartesian product.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for risk patterns -- found #1560 (background-tick Arc<RwLock<T>> cache pattern) directly applicable; no novel pattern to store (this is a well-established risk in this codebase)
- Stored: nothing novel to store -- the caching pattern and maintenance-tick scope risk type are already captured in Unimatrix (#1560, #1777)
