# Agent Report: col-029-agent-2-spec

## Output

`product/features/col-029/specification/SPECIFICATION.md`

## Acceptance Criteria Count

- Scope AC-01 through AC-13: all thirteen carried forward verbatim as specified
- AC-14 (new): SR-04 resolution — `cross_category_edge_count` SQL must join `entries`
  twice with `status = 0` on both aliases; deprecated/quarantined endpoints excluded
- AC-15 (new): Per-call only — `compute_graph_cohesion_metrics` must have exactly one
  call site in `compute_report()`, not in the background tick
- AC-16 (new): Bootstrap-only NLI edges excluded — a `source='nli'` row with
  `bootstrap_only=1` must not appear in `inferred_edge_count`

Total: 16 acceptance criteria (13 from scope + 3 additions).

## Key Decisions Made

1. SR-04 resolved in spec: `cross_category_edge_count` FR-06 explicitly requires a
   double-join on `entries` with `status = 0` on both aliases and mandates a CTE or
   subquery to prevent cartesian product. Confirmed as AC-14.

2. SR-03/SR-06 per-call design confirmed: The scope decision to skip tick caching is
   upheld in NFR-01 and AC-15. Lesson #1777 (compute_report tick inflation bug) supports
   this — adding another phase to the tick path risks repeating the same availability
   problem.

3. AC-16 added to close the gap where a bootstrap-only row with `source='nli'` would
   slip through if the `bootstrap_only=0` filter were applied only to the outer query.
   The SQL in FR-09 must include both `source = 'nli'` AND `bootstrap_only = 0`.

4. The `### Graph Cohesion` markdown sub-section is specified as nested within the
   Coherence block (not a top-level section), consistent with scope Decision 3.

5. Summary format uses conditional append guard (non-zero or `total_active > 0`),
   consistent with the existing `graph_stale_ratio > 0.0` pattern in `format_status_report`.

## Open Questions for Architect

1. **SR-01 — Named constant location** for `EDGE_SOURCE_NLI: &str = "nli"`. Three
   candidate locations; architect must decide to prevent #412 divergence.
2. **SR-04 — SQL query plan review** for the double-join on `entries`. Architect must
   confirm CTE approach does not produce a cartesian product on large corpora.

## Knowledge Stewardship

- Queried: /uni-query-patterns for `context_status StatusReport` — found Generic Formatter Pattern (#298), Response Formatting Convention (#307); no established naming convention specific to health sub-metrics, confirmed pattern is append-to-struct with manual Default
- Queried: /uni-query-patterns for `unit test store SQL` — found Test Gateway Pattern (#315), TestHarness (#748); confirmed `open_test_store()` is the correct helper
- Queried: /uni-query-patterns for `graph GRAPH_EDGES maintenance tick cache` — found TypedRelationGraph bootstrap-exclusion pattern (#2476); lesson #1777 confirms per-call design avoids tick inflation regression
- No stale or incorrect entries found; no corrections made
