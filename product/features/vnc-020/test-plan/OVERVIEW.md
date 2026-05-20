# vnc-020 Test Plan Overview

## Feature Summary

vnc-020 adds three modes to `context_graph`: `inverse` (SQL antijoin), `filter` (correlated
subquery), and `path` (in-memory BFS). All dispatch through the existing 14th MCP tool.
The feature adds no schema migration and no new tool.

---

## Overall Test Strategy

### Unit Tests

All unit tests live in the Rust crate: `crates/unimatrix-server/src/mcp/`.

| Module | File | Test Focus |
|--------|------|------------|
| graph_read.rs | `#[cfg(test)]` mod inside graph_read.rs | Validation dispatch, rejection matrix, unrecognized-mode error text |
| graph_read_inverse.rs | `#[cfg(test)]` mod inside graph_read_inverse.rs | handle_inverse validation, SQL shape, dynamic builder N=1/2/3 |
| graph_read_filter.rs | `#[cfg(test)]` mod inside graph_read_filter.rs | handle_filter validation, SQL shape, edge-count subquery structure |
| graph_read_path.rs | `#[cfg(test)]` mod inside graph_read_path.rs | handle_path validation, BFS termination, visited-set invariant, self-path |

All async tests use `#[tokio::test]`. `TypedGraphState` injection uses pattern #4501 (bypass
the tick by passing a pre-populated `Arc<RwLock<TypedGraphState>>` directly to `handle_path`).

### Integration Tests

New tests added to `product/test/infra-001/suites/test_tools.py` under a dedicated
`# === vnc-020: context_graph inverse/filter/path modes ===` section after the existing
vnc-019 block.

### Test Count Target

- Unit tests: ~45 new (across 4 modules)
- Integration tests: 6 new (AC-27 through AC-32)

---

## Risk-to-Test Mapping

| Risk ID | Priority | Mitigation Tests |
|---------|----------|-----------------|
| R-01 | Critical | AC-19 (manual inspection), `test_handle_path_from_id_not_in_snapshot_returns_not_found` (unit), `test_inverse_filter_tool_description_no_staleness_language` (manual check) |
| R-02 | Critical | `test_context_graph_filter_max_edge_count_zero` (integration AC-29), `test_filter_sql_uses_lte_binding_for_max_edge_count_zero` (unit) |
| R-03 | Critical | `test_handle_path_bfs_visited_set_deduplicates_resolved_id` (unit — forked deprecated graph), `test_handle_path_double_enqueue_fork_gives_unique_hops` (unit) |
| R-04 | Critical | 8 rejection unit tests (one per new field × one wrong-mode minimum), see rejection matrix below |
| R-05 | High | `test_context_graph_inverse_and_semantics` (integration AC-28), `test_inverse_single_missing_type_returns_partial_match` (unit) |
| R-06 | High | AC-20 integration test (`test_context_graph_path_resolve_supersessions_from_id`), AC-21 integration test, `test_handle_path_to_id_resolution_reflected_in_response` (unit) |
| R-07 | High | `test_depth_rejected_on_chain/current/subgraph/inverse/filter` (5 unit tests, AC-25) |
| R-08 | High | `test_filter_both_edge_count_bounds_two_subqueries_in_sql` (unit), combined-bounds integration (AC-30 extended) |
| R-09 | High | `test_handle_path_from_id_not_in_snapshot_returns_not_found` (unit AC-15), `test_context_graph_path_no_path_disconnected` (integration AC-14) — separate fixtures |
| R-10 | High | `test_context_graph_inverse_single_type` with deprecated entry (integration AC-27), `test_inverse_sql_always_has_status_guard` (unit, N=1/2/3 types) |
| R-11 | Medium | `test_filter_category_only_returns_all_active` (unit), `test_context_graph_filter_category_only` (integration) |
| R-12 | Medium | AC-31 (2-hop path hops content), `test_handle_path_1hop_from_id_not_in_hops` (unit) |
| R-13 | Low | `test_inverse_limit_zero_rejected`, `test_inverse_limit_501_rejected`, `test_filter_limit_zero_rejected`, `test_filter_limit_501_rejected` (unit AC-05/AC-11) |
| R-14 | Low | `test_relation_type_from_str_all_16_variants` (unit), `test_relation_type_from_str_wildcard_fires_last` (unit) |
| IR-04 | Integration | `test_context_graph_filter_max_edge_count_zero` with two edge_types, `test_filter_edge_types_multi_type_push_bind` (unit) |

### Rejection Matrix Coverage (R-04)

One wrong-mode rejection test per new field (8 minimum per R-04/SR-08):

| New Field | Test Name | Wrong Mode Tested |
|-----------|-----------|-------------------|
| `category` | `test_category_rejected_on_path_mode` | path |
| `missing_edge_types` | `test_missing_edge_types_rejected_on_filter_mode` | filter |
| `limit` | `test_limit_rejected_on_chain_mode` | chain |
| `min_age_days` | `test_min_age_days_rejected_on_path_mode` | path |
| `min_confidence` | `test_min_confidence_rejected_on_subgraph_mode` | subgraph |
| `max_confidence` | `test_max_confidence_rejected_on_current_mode` | current |
| `min_edge_count` | `test_min_edge_count_rejected_on_inverse_mode` | inverse |
| `max_edge_count` | `test_max_edge_count_rejected_on_neighbors_mode` | neighbors |

Additional from AC-22/AC-23/AC-24:
- `test_from_id_rejected_on_filter_mode` (pre-existing stub now actively rejected)
- `test_missing_edge_types_rejected_on_chain/current/neighbors/subgraph/path` (AC-23)
- `test_filter_params_rejected_on_inverse` (AC-24, one per filter-only param)

---

## Integration Harness Plan

### Existing Suites That Apply

| Suite | Reason |
|-------|--------|
| `tools` | New modes extend the 14th tool; all AC-27 through AC-32 land in test_tools.py |
| `lifecycle` | Path mode uses in-memory BFS — lifecycle patterns (store→graph_edge→path) |
| `edge_cases` | from_id==to_id (AC-32), empty DB calls, boundary values |

**Smoke subset**: existing smoke tests cover the base context_graph tool path. No new smoke
marker is added for vnc-020 integration tests; the existing smoke gate is the minimum.

### New Integration Tests Required

All 6 tests are added to `product/test/infra-001/suites/test_tools.py`, fixture `server`
(fresh DB per test), using the existing `server.context_graph()`, `server.context_edge()`,
`server.context_store()`, and `server.context_correct()` helper methods.

| AC-ID | Test Function Name | Suite File | Fixture |
|-------|-------------------|------------|---------|
| AC-27 | `test_context_graph_inverse_single_type` | test_tools.py | `server` |
| AC-28 | `test_context_graph_inverse_and_semantics` | test_tools.py | `server` |
| AC-29 | `test_context_graph_filter_max_edge_count_zero` | test_tools.py | `server` |
| AC-30 | `test_context_graph_filter_min_edge_count_gte2` | test_tools.py | `server` |
| AC-31 | `test_context_graph_path_found` | test_tools.py | `server` |
| AC-32 | `test_context_graph_path_self_loop_returns_not_found` | test_tools.py | `server` |

Note: AC-32 is a self-path (from_id==to_id) that can be verified as a unit test (preferred)
or as an integration test. Given its dependency on the BFS handler logic, both a unit and
integration test are recommended. The integration test is included for completeness.

### No New Suite Files

All tests extend the existing `test_tools.py` vnc-020 block. No new suite files are needed —
behavior is visible through existing `context_graph` MCP interface.

---

## Cross-Component Test Dependencies

| Dependency | Risk | Addressed By |
|------------|------|-------------|
| `validate_no_unsupported_params` centralized in graph_read.rs; all 3 new handlers depend on it | R-04 | Rejection tests in graph_read.md call validate_no_unsupported_params directly |
| `follow_to_current` pub(super) from graph_read_neighbors.rs; used by graph_read_path.rs | R-03, R-06 | Path unit tests mock follow_to_current via TypedGraphState injection |
| BFS over TypedRelationGraph (in-memory, tick-window) vs live SQL (inverse/filter) | R-01, R-09 | Separate test fixtures — never mix SQL-mode and path-mode assertions in one test |
| `push_bind` pattern for IN-clause binding (IR-04) | IR-04 | Unit test asserts SQL parameter count; integration test with multi-type edge_types |

---

## Constraints Verification

| Constraint | Verification |
|------------|-------------|
| C5 — graph_read.rs <= 500 lines | Code review gate: `wc -l graph_read.rs` in Stage 3c |
| C3 — CURRENT_SCHEMA_VERSION stays at 27 | Unit test: assert schema version constant unchanged |
| C7 — tool count = 14 | Existing integration test `test_list_tools_returns_14` still passes |
