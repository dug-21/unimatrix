# Agent Report: crt-040-agent-2-testplan

**Phase:** Test Plan Design (Stage 3a)
**Date:** 2026-04-02

## Files Produced

- `/workspaces/unimatrix/product/features/crt-040/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/crt-040/test-plan/store-constant.md`
- `/workspaces/unimatrix/product/features/crt-040/test-plan/inference-config.md`
- `/workspaces/unimatrix/product/features/crt-040/test-plan/write-graph-edge.md`
- `/workspaces/unimatrix/product/features/crt-040/test-plan/path-c-loop.md`

## Risk Coverage Mapping Summary

| Risk ID | Priority | Coverage Status | Test File |
|---------|----------|-----------------|-----------|
| R-01 | Critical | Full — 3 independent scenarios (HashMap lookup, None branch, disallowed category) | path-c-loop.md TC-01/04/05 |
| R-02 | High | Full — two independent unit tests: `write_nli_edge` still writes 'nli'; `write_graph_edge` writes 'cosine_supports' | write-graph-edge.md TC-01/02 |
| R-03 | High | Full — three independent assertions: backing fn, impl Default, serde path | inference-config.md TC-01/02/03 |
| R-04 | Medium | Full — AC-17 grep gate + AC-18 serde forward-compat test | inference-config.md TC-11/12 |
| R-05 | Medium | Full — AC-15 unit test: inferred_edge_count unchanged after cosine_supports write | path-c-loop.md TC-15 |
| R-06 | Medium | Full — AC-19: observability log fires with zero counts (flagged delivery decision) | path-c-loop.md TC-12/13 |
| R-07 | Medium | Full — budget counter not incremented on false return; no warn on false return | write-graph-edge.md TC-04; path-c-loop.md TC-08 |
| R-08 | Low | Full — reversed pair produces at most one edge | path-c-loop.md TC-17 |
| R-09 | Low | Full — NaN and Inf variants; guard fires before threshold comparison | path-c-loop.md TC-09/10/11 |
| R-10 | Medium | Code review gate (HashMap vs linear scan; not unit-testable mechanically) | path-c-loop.md (delivery gate note) |
| R-11 | Medium | Code review gate (file line count; extract helper if tick body > 150 lines) | path-c-loop.md (delivery gate note) |
| R-12 | High | Eval gate — `python product/research/ass-039/harness/run_eval.py` | OVERVIEW.md |
| R-13 | Medium | Full — two config merge tests (project overrides default; global wins when project == default) | inference-config.md TC-09/10 |

## Integration Suite Plan

**Mandatory:** `smoke`
**Required:** `lifecycle` (supports_edge_count visibility), `tools` (AC-06/AC-07 regression)
**Not required:** `confidence`, `contradiction`, `security`, `volume`, `edge_cases`

**New tests to add to `suites/test_lifecycle.py`:**
- `test_context_status_supports_edge_count_increases_after_tick`
- `test_inferred_edge_count_unchanged_by_cosine_supports`
Both are xfail-able if CI lacks an embedding model.

## Open Questions for Stage 3b (Delivery Agent)

1. **Early-return / AC-19 conflict**: The existing early-return `if candidate_pairs.is_empty() && informs_metadata.is_empty()` fires before Path C. When both are empty, Path C's observability log cannot fire. The delivery agent must decide: emit the log before the early-return, or accept that AC-19 is only testable when at least one list is non-empty. Document the decision in the implementation.

2. **category_map reuse**: A `category_map: HashMap<u64, &str>` (from `all_active`) is already built in Phase 5 for the candidate_pairs sort. Path C may reuse this. If it does, the value type is `&str` (not `String`), which limits lifetime. The delivery agent must decide whether to reuse the existing map or build a separate `HashMap<u64, String>`. Either is correct; the test plans are agnostic.

3. **candidate_pairs truncation**: `candidate_pairs` is already truncated to `config.max_graph_inference_per_tick` (default 10) in Phase 5 before Path C runs. AC-12 tests 60 qualifying pairs → 50 edges. If `max_graph_inference_per_tick < 50`, Path C will see fewer than 50 candidates. TC-07 must use a config where `max_graph_inference_per_tick >= 60` to exercise the 50-cap meaningfully.

4. **write_graph_edge SQL error injection (TC-05)**: Injecting a real SQL error into a test is non-trivial. If the testing infrastructure does not support write pool failure simulation, TC-05 should be specified as a code inspection gate in Stage 3c rather than a runtime test.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 14 entries returned. Most relevant: #4028 (ADR-002, dual-site default), #4027 (ADR-001, write_graph_edge sibling), #3817 (InferenceConfig dual-site pattern), #4013 (hidden test sites for config changes), #3713 (threshold calibration lesson). All directly informed the test plans.
- Queried: `context_search("InferenceConfig serde default test pattern impl Default")` — returned #3817, #4013, #2730, #3774, #3743. Confirmed the three-independent-assertion pattern for AC-16 and the TOML flat-field convention.
- Queried: `context_search("graph_edges INSERT OR IGNORE test integration tick")` — returned #3600, #3884, #3883, #3889, #3913. Confirmed write pool pattern and INSERT OR IGNORE idempotency semantics.
- Stored: nothing novel — the category-data-from-candidate-pairs test pattern is crt-040-specific and was flagged as not yet cross-feature. If it recurs in Group 4 (PPR expander), store it as a pattern then. The three-independent-assertion pattern for impl Default / serde / backing fn is already stored as #3817 / #3774.
