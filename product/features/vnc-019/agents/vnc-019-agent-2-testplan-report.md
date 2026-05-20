# Agent Report: vnc-019-agent-2-testplan

## Phase: Test Plan Design (Stage 3a)

## Outputs

All test plan files produced under `product/features/vnc-019/test-plan/`:

| File | Lines | Coverage |
|------|-------|----------|
| `OVERVIEW.md` | ~130 | Risk→test mapping, AC coverage table, integration harness plan |
| `graph_read.md` | ~130 | GraphParams extension, SubgraphResponse serialization, validate regressions, tool description string |
| `graph_read_subgraph.md` | ~230 | All BFS logic: validation (A), traversal (B), correctness (C), metadata (D), edge types (E), hydration (F) |
| `graph_read_neighbors.md` | ~55 | Visibility-only change — compile gate + existing test pass verification |
| `tools.md` | ~95 | Tool description review checklist + string assertions + tool count non-regression |

## Risk Coverage Mapping

| Risk ID | Test Function(s) |
|---------|-----------------|
| R-01 (Critical) | `test_bfs_resolve_supersessions_before_visited_check`, `test_bfs_resolve_supersessions_dedup_via_multiple_paths`, `test_bfs_resolve_supersessions_false_includes_deprecated` |
| R-02 (Critical) | `test_bfs_direction_both_single_edge_no_duplicate`, `test_bfs_direction_both_canonical_direction_on_edge_record`, `test_bfs_direction_both_multihop_no_duplicates` |
| R-03 (Critical) | `test_bfs_seed_count_exceeds_max_nodes_truncated`, `test_bfs_seed_count_exactly_at_max_nodes_truncated`, `test_bfs_seed_partial_budget_bfs_expands_remainder` |
| R-04 (High) | `test_bfs_no_edges_skips_metadata_query`, `test_bfs_empty_graph_cold_start_no_error` |
| R-05 (High) | `test_validate_subgraph_mode_recognized`, `test_validate_subgraph_rejects_from_id`, `test_validate_subgraph_rejects_to_id`, `test_validate_chain/current/neighbors_rejects_max_depth`, `test_validate_current_rejects_seed_ids` |
| R-06 (High) | `test_bfs_circular_supersession_terminates`, `test_bfs_supersession_chain_50_hops_terminates` |
| R-07 (High) | `test_validate_max_nodes_above_200_rejected`, `test_validate_max_nodes_zero_rejected`, `test_validate_max_nodes_200_accepted`, `test_bfs_max_nodes_cap_during_bfs_truncates` |
| R-08 (High) | `test_bfs_depth_reached_full_traversal`, `test_bfs_depth_reached_under_truncation`, `test_bfs_depth_reached_zero_no_edges`, `test_bfs_depth_reached_bounded_by_max_depth` |
| R-09 (High) | `test_bfs_hydration_missing_entry_does_not_panic` |
| R-10 (Med) | Compile gate: `cargo build --workspace` |
| R-11 (High) | `test_tool_description_contains_staleness_disclosures` + code review checklist |
| R-12 (Med) | `test_bfs_first_discovery_wins_depth`, `test_bfs_two_paths_to_same_node_single_edge` |
| R-13 (Med) | `test_bfs_follow_to_current_none_fallback_to_original` |
| R-14 (Med) | `test_validate_edge_types_absent_defaults_to_all_non_supersedes`, `test_all_non_supersedes_types_count` |
| R-15 (Med) | `test_bfs_malformed_metadata_returns_none`, `test_bfs_null_metadata_column_returns_json_null`, `test_bfs_valid_metadata_json_parsed` |
| R-16 (Low) | Covered by AC-13 tool description review |

## Integration Harness Plan

Suites to run: `smoke` (mandatory gate), `tools`, `protocol`, `lifecycle`, `edge_cases`, `security`.

New integration tests planned:

**test_tools.py** (18 new tests):
- Response shape, node shape, edge record fields
- Validation: empty seed_ids, max_depth=0/11, max_nodes=201, from_id on subgraph, unknown edge_type
- Behavioral: direction="both" dedup, direction="outgoing" on all edges, unknown seed empty result, metadata populated
- Regressions: chain/neighbors reject seed_ids and max_depth, unrecognized mode lists subgraph

**test_lifecycle.py** (6 new tests):
- Topology traversal with known edge structure (AC-14)
- depth_reached accuracy (full and truncated)
- Supersession integration (AC-09)
- 201-seeds truncation (AC-05)
- Cold-start empty result (may need xfail for tick timing)

**test_protocol.py** (1 new test):
- Tool description mentions "subgraph" and staleness terms

## Critical Delivery Notes for Stage 3b

1. **Update existing test**: `test_validate_unrecognized_mode_fires_before_field_check` in `graph_read_tests.rs` currently uses `mode="subgraph"` as its probe — must be changed to `"walk"` after vnc-019 delivery, and a new test must assert `"subgraph"` passes validation.

2. **`set_test_graph` injection helper** must be created before BFS unit tests can run. See pattern #4501.

3. **Tool description constant**: Expose as `pub(crate) const CONTEXT_GRAPH_DESCRIPTION` for unit test access.

4. **Dangling-edge filter test** (`test_bfs_dangling_edges_removed_after_truncation`) is a correctness invariant test — every edge in resp.edges must have both source_id and target_id present in resp.nodes. This is a universal postcondition assertion that can be reused across all BFS behavioral tests.

## Open Questions

1. Does `TypedGraphState` already have a test injection API, or must one be added? (Delivery agent must check before writing tests.)
2. Is `GRAPH_EDGES.metadata` stored as `Option<String>` or always populated? (Affects test setup for R-15.)
3. Cold-start integration test may be inherently racy with the tick — recommend xfail with documented rationale if tick timing is non-deterministic.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 11 entries: 4 vnc-019 ADRs (#4490, #4491, #4492, #4493), 4 vnc-018 ADRs, 1 scope-risk pattern. All highly relevant; confirmed ADR decisions align with test plan design.
- Queried: `context_search(vnc-019 architectural decisions)` — confirmed 3 vnc-019 ADRs directly.
- Queried: `context_search(graph read MCP tool testing patterns)` — surfaced lesson #4437 (tool count assertion) and pattern #4066 (BFS behavioral direction pairing). Both applied to this plan.
- Stored: entry #4501 "BFS subgraph unit tests require a TypedGraphState injection helper to bypass the tick" via `/uni-store-pattern`
