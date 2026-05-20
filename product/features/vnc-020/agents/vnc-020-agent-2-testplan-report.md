# Agent Report: vnc-020-agent-2-testplan

**Phase**: Test Plan Design (Stage 3a)
**Agent ID**: vnc-020-agent-2-testplan

---

## Output Files

All files written to `product/features/vnc-020/test-plan/`:

| File | Lines | Purpose |
|------|-------|---------|
| `OVERVIEW.md` | ~100 | Overall strategy, risk-to-test mapping, integration harness plan, coverage matrix |
| `graph_read.md` | ~80 | Wire types + dispatch + centralized validation tests (AC-22–26, R-04, R-07) |
| `graph_read_inverse.md` | ~90 | Antijoin handler unit tests + AC-27/AC-28 integration specs |
| `graph_read_filter.md` | ~100 | Correlated subquery handler unit tests + AC-29/AC-30 integration specs |
| `graph_read_path.md` | ~110 | BFS handler unit tests + AC-31/AC-32 integration specs; R-03 double-enqueue fixture |
| `tools.md` | ~55 | AC-19 manual inspection checklist — exact phrases to look for |

---

## Risk Coverage Mapping

| Risk ID | Priority | Test Function(s) |
|---------|----------|-----------------|
| R-01 | Critical | AC-19 manual inspection (tools.md checklist); `test_handle_path_from_id_not_in_snapshot_returns_not_found` |
| R-02 | Critical | `test_context_graph_filter_max_edge_count_zero` (AC-29); `test_filter_max_edge_count_zero_uses_lte_binding` |
| R-03 | Critical | `test_handle_path_bfs_visited_set_keyed_on_resolved_id` (forked-deprecated fixture) |
| R-04 | Critical | 8-field rejection matrix (see OVERVIEW.md table); `test_graph_unrecognized_mode_error_lists_all_seven_modes` (AC-26) |
| R-05 | High | `test_context_graph_inverse_and_semantics` (4-state fixture, AC-28) |
| R-06 | High | `test_handle_path_resolve_supersessions_from_id_reflected`; `test_handle_path_resolve_supersessions_to_id_reflected`; AC-20/AC-21 integration |
| R-07 | High | `test_depth_rejected_on_{chain,current,subgraph,inverse,filter}_mode` (5 tests, AC-25) |
| R-08 | High | `test_filter_both_edge_count_bounds_two_subqueries_in_sql`; combined-bounds integration (AC-30 extended) |
| R-09 | High | `test_handle_path_from_id_not_in_snapshot_returns_not_found` (AC-15); `test_context_graph_path_no_path_disconnected` (AC-14) — SEPARATE fixtures |
| R-10 | High | `test_context_graph_inverse_single_type` (deprecated entry excluded); `test_inverse_sql_includes_status_guard_n1/n3` |
| R-11 | Medium | `test_handle_filter_category_only_no_validation_error` |
| R-12 | Medium | `test_handle_path_1hop_from_id_not_in_hops`; `test_handle_path_2hop_from_id_not_in_hops` |
| R-13 | Low | `test_handle_inverse_limit_zero/501_returns_error`; `test_handle_filter_limit_zero/501_returns_error` |
| R-14 | Low | `test_handle_inverse_unrecognized_edge_type_returns_error` (covers from_str wildcard arm) |
| IR-04 | Integration | `test_filter_multi_type_edge_types_push_bind_pattern`; `test_context_graph_filter_max_edge_count_zero` with multi-type edge_types |

---

## Integration Suite Plan

**Suites that apply**: `tools` (primary), `lifecycle` (path BFS end-to-end), `edge_cases` (boundary values).

**New tests for infra-001/suites/test_tools.py** (all use `server` fixture):

| AC-ID | Test Function Name |
|-------|-------------------|
| AC-27 | `test_context_graph_inverse_single_type` |
| AC-28 | `test_context_graph_inverse_and_semantics` |
| AC-29 | `test_context_graph_filter_max_edge_count_zero` |
| AC-30 | `test_context_graph_filter_min_edge_count_gte2` |
| AC-31 | `test_context_graph_path_found` |
| AC-32 | `test_context_graph_path_self_loop_returns_not_found` |

**Tick dependency note**: AC-31 (path mode) must account for tick-window staleness —
documented in pattern #4517 and in graph_read_path.md. Unit-level tests via TypedGraphState
injection (pattern #4501) are preferred for BFS correctness; AC-31 validates wire format.

---

## Open Questions

1. **Tick-force mechanism**: Does infra-001 have a mechanism to force a graph tick rebuild
   between context_edge writes and a path mode call? If not, AC-31 must either use a
   sleep/retry or be documented as a best-effort integration test. The unit tests (R-03,
   R-09) do not have this problem.

2. **AC-28 edge setup**: Adding incoming edges (to set up inverse mode test data) requires
   knowing the `context_edge` call direction. Confirm: `context_edge("add", source_id, "Cites", target_id)` adds an edge FROM source TO target, meaning target receives an incoming Cites edge. The test fixture in graph_read_inverse.md assumes this direction.

3. **SQL shape verification**: The unit tests for R-02 and R-08 that inspect SQL structure
   require either (a) an exposed test helper that returns the SQL string before execution,
   or (b) behavioral verification via actual DB results. Behavioral verification (option b)
   is always sufficient; SQL inspection is a bonus. The delivery agent should choose whichever
   is simpler given the implementation structure.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 10 entries. Key entries used:
  #4494 (BFS visited-set keyed on resolved ID — directly informed R-03 fixture design),
  #4501 (TypedGraphState injection for unit tests — referenced in graph_read_path.md),
  #4502–4507 (vnc-020 ADRs — confirmed module split and rejection matrix).
- Queried: `context_search` for BFS visited-set pattern — confirmed #4494 already exists.
- Queried: `context_search` for push_bind / IN clause — confirmed #4058 referenced in
  RISK-TEST-STRATEGY.md; no duplicate storage needed.
- Stored: entry #4517 "Integration tests for path mode BFS must account for tick-window
  staleness before asserting results" via `context_store` (pattern). This gap was not
  covered by existing pattern #4501 (which addresses unit-level injection, not integration
  layer tick dependency).
