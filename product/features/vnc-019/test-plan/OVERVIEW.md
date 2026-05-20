# vnc-019 Test Plan: OVERVIEW

## Feature Summary

vnc-019 adds `subgraph` mode to the existing `context_graph` MCP tool (14 tools total,
unchanged). The mode performs bounded BFS from one or more seed entries over the
in-memory `TypedRelationGraph`, returning both discovered nodes and typed edges.

New files: `graph_read_subgraph.rs`, `graph_read_subgraph_tests.rs`.
Modified files: `graph_read.rs`, `graph_read_neighbors.rs`, `tools.rs`.
No new crate, table, migration, or MCP tool.

---

## Test Levels

| Level | Location | Tool | Scope |
|-------|----------|------|-------|
| Unit | `graph_read_subgraph_tests.rs` | `cargo test` | BFS logic, validation, edge cases |
| Unit | `graph_read_tests.rs` (updated) | `cargo test` | `validate_no_unsupported_params` extensions, `SubgraphResponse` serialization |
| Unit | `graph_read_neighbors_tests.rs` (unchanged) | `cargo test` | `pub(super)` visibility compile check |
| Integration | infra-001 `test_tools.py` (new tests) | pytest | End-to-end subgraph call through MCP JSON-RPC |
| Integration | infra-001 `test_lifecycle.py` (new tests) | pytest | Topology traversal, truncation, metadata hydration |
| Smoke | infra-001 `-m smoke` | pytest | Mandatory minimum gate |

---

## Risk-to-Test Mapping

| Risk ID | Priority | Test Location | Test Function(s) |
|---------|----------|---------------|------------------|
| R-01 | Critical | subgraph_tests.rs | `test_bfs_resolve_supersessions_before_visited_check`, `test_bfs_resolve_supersessions_dedup_via_multiple_paths`, `test_bfs_resolve_supersessions_false_includes_deprecated` |
| R-02 | Critical | subgraph_tests.rs | `test_bfs_direction_both_single_edge_no_duplicate`, `test_bfs_direction_both_canonical_direction_on_edge_record`, `test_bfs_direction_both_multihop_no_duplicates` |
| R-03 | Critical | subgraph_tests.rs | `test_bfs_seed_count_exceeds_max_nodes_truncated`, `test_bfs_seed_count_exactly_at_max_nodes_truncated`, `test_bfs_seed_partial_budget_bfs_expands_remainder` |
| R-04 | High | subgraph_tests.rs | `test_bfs_no_edges_skips_metadata_query`, `test_bfs_empty_graph_cold_start_no_error` |
| R-05 | High | graph_read_tests.rs | `test_validate_subgraph_rejects_from_id`, `test_validate_subgraph_rejects_to_id`, `test_validate_chain_rejects_seed_ids`, `test_validate_chain_rejects_max_depth`, `test_validate_current_rejects_max_depth`, `test_validate_neighbors_rejects_max_depth`, `test_validate_subgraph_recognized_mode` |
| R-06 | High | subgraph_tests.rs | `test_bfs_circular_supersession_terminates`, `test_bfs_supersession_chain_50_hops_terminates` |
| R-07 | High | subgraph_tests.rs | `test_validate_max_nodes_above_200_rejected`, `test_validate_max_nodes_zero_rejected`, `test_validate_max_nodes_200_accepted` |
| R-08 | High | subgraph_tests.rs | `test_bfs_depth_reached_full_traversal`, `test_bfs_depth_reached_under_truncation`, `test_bfs_depth_reached_zero_no_edges`, `test_bfs_depth_reached_bounded_by_max_depth` |
| R-09 | High | subgraph_tests.rs (review) | `test_bfs_hydration_missing_entry_does_not_panic` |
| R-10 | Med | compile | `cargo build --workspace` (compile gate) |
| R-11 | High | tools.rs review | `test_tool_description_contains_staleness_disclosures` (string match) |
| R-12 | Med | subgraph_tests.rs | `test_bfs_first_discovery_wins_depth`, `test_bfs_two_paths_to_same_node_single_edge` |
| R-13 | Med | subgraph_tests.rs | `test_bfs_follow_to_current_none_fallback_to_original` |
| R-14 | Med | subgraph_tests.rs | `test_validate_edge_types_absent_defaults_to_all_non_supersedes`, `test_validate_edge_types_empty_defaults_to_all_non_supersedes` |
| R-15 | Med | subgraph_tests.rs | `test_bfs_malformed_metadata_returns_none`, `test_bfs_null_metadata_column_returns_json_null`, `test_bfs_valid_metadata_json_parsed` |
| R-16 | Low | tools.rs review | Covered by AC-13 tool description review |

---

## Acceptance Criteria Coverage

| AC-ID | Test Location | Test Function(s) |
|-------|---------------|------------------|
| AC-01 | infra-001/test_tools.py | `test_graph_subgraph_basic_response_shape` |
| AC-02 | infra-001/test_tools.py | `test_graph_subgraph_node_shape_matches_entry_record` |
| AC-03 | infra-001/test_tools.py | `test_graph_subgraph_edge_record_fields` |
| AC-04 | subgraph_tests.rs | `test_bfs_isolated_seed_present_in_nodes` |
| AC-05 | infra-001/test_lifecycle.py | `test_graph_subgraph_201_seeds_truncated_at_200` |
| AC-06 | subgraph_tests.rs | `test_validate_max_depth_boundary_values` |
| AC-07 | subgraph_tests.rs | `test_validate_seed_ids_absent_error`, `test_validate_seed_ids_empty_error` |
| AC-08 | subgraph_tests.rs | `test_validate_unknown_edge_type_error`, `test_validate_edge_types_absent_defaults_to_all_non_supersedes` |
| AC-09 | subgraph_tests.rs + infra-001 | `test_bfs_resolve_supersessions_before_visited_check`, `test_graph_subgraph_supersession_integration` |
| AC-10 | subgraph_tests.rs | `test_bfs_resolve_supersessions_false_includes_deprecated` |
| AC-11 | graph_read_tests.rs | `test_validate_chain_rejects_seed_ids`, `test_validate_current_rejects_seed_ids`, `test_validate_neighbors_rejects_seed_ids` (existing), + new `mode="current"` |
| AC-12 | subgraph_tests.rs + infra-001 | `test_bfs_direction_both_single_edge_no_duplicate`, `test_graph_subgraph_direction_both_dedup` |
| AC-13 | graph_read_tests.rs | `test_tool_description_contains_staleness_disclosures` |
| AC-14 | infra-001/test_lifecycle.py | `test_graph_subgraph_topology_traversal` |
| AC-15 | subgraph_tests.rs + infra-001 | `test_bfs_depth_reached_*`, `test_graph_subgraph_depth_reached_accuracy` |
| AC-16 | graph_read_tests.rs | `test_validate_chain_rejects_max_depth`, `test_validate_current_rejects_max_depth`, `test_validate_neighbors_rejects_max_depth` |
| AC-17 | subgraph_tests.rs | `test_bfs_seed_absent_from_graph_empty_result` |
| AC-18 | subgraph_tests.rs + infra-001 | `test_bfs_valid_metadata_json_parsed`, `test_graph_subgraph_metadata_populated` |
| AC-19 | subgraph_tests.rs | `test_bfs_no_edges_skips_metadata_query` |

---

## Integration Harness Plan

### Which Existing Suites Apply

| Suite | Applies | Reason |
|-------|---------|--------|
| `smoke` | YES (mandatory gate) | Any change requires smoke gate |
| `tools` | YES | New mode on context_graph tool — tool parameter and response validation |
| `protocol` | YES | Tool count must remain 14; tool discovery must reflect updated description |
| `lifecycle` | YES | Multi-step flows: write entries + edges, call subgraph, verify topology |
| `edge_cases` | YES | Cold-start (empty graph), duplicate seeds, max_depth boundaries |
| `security` | YES | Input validation boundaries (max_nodes > 200, unknown edge_type, direction) |
| `confidence` | NO | No confidence system changes |
| `contradiction` | NO | No contradiction changes |
| `volume` | OPTIONAL | Only if 200-node cap test requires volume-scale DB |

### Existing Suite Coverage Gaps

The existing `test_tools.py` suite covers the 14 tools but has no subgraph-mode tests.
The existing `test_lifecycle.py` suite has no multi-seed topology traversal tests.
Both gaps require new tests added as part of vnc-019 delivery.

### New Integration Tests to Add

#### In `suites/test_tools.py`

```python
# Fixture: server (fresh DB per test)

def test_graph_subgraph_basic_response_shape(server):
    # AC-01: subgraph call returns nodes, edges, truncated, seed_ids, depth_reached
    # Arrange: store 2 entries, write an edge between them
    # Act: context_graph(mode="subgraph", seed_ids=[id1], edge_types=["Supports"],
    #       direction="outgoing", max_depth=2)
    # Assert: response has keys nodes, edges, truncated, seed_ids, depth_reached

def test_graph_subgraph_node_shape_matches_entry_record(server):
    # AC-02: each node is a full EntryRecord shape (id, title, content, category, etc.)

def test_graph_subgraph_edge_record_fields(server):
    # AC-03: edge has source_id, target_id, relation_type, direction="outgoing", depth, metadata

def test_graph_subgraph_empty_seed_ids_rejected(server):
    # AC-07: seed_ids=[] → validation error with exact message

def test_graph_subgraph_max_depth_boundary_0_rejected(server):
    # AC-06: max_depth=0 → validation error

def test_graph_subgraph_max_depth_boundary_11_rejected(server):
    # AC-06: max_depth=11 → validation error

def test_graph_subgraph_max_nodes_above_200_rejected(server):
    # R-07: max_nodes=201 → validation error

def test_graph_subgraph_from_id_rejected(server):
    # R-05: from_id on subgraph mode → validation error

def test_graph_subgraph_unknown_edge_type_rejected(server):
    # AC-08: edge_types=["BogusType"] → validation error naming the type

def test_graph_subgraph_direction_both_dedup(server):
    # AC-12: single A→B edge; call with seed=[A,B], direction="both"; len(edges)==1

def test_graph_subgraph_direction_outgoing_on_all_edge_records(server):
    # AC-03: direction field is always "outgoing" on all returned EdgeRecords

def test_graph_subgraph_unknown_seed_empty_result(server):
    # AC-17: non-existent seed → nodes=[], edges=[], truncated=false, depth_reached=0

def test_graph_subgraph_metadata_populated(server):
    # AC-18: edge with non-null metadata → EdgeRecord.metadata is parsed JSON

def test_graph_subgraph_chain_mode_rejects_seed_ids(server):
    # AC-11 / R-05 regression: mode="chain", seed_ids=[1] → validation error

def test_graph_subgraph_chain_mode_rejects_max_depth(server):
    # AC-16 / R-05 regression: mode="chain", max_depth=2 → error with exact message

def test_graph_subgraph_neighbors_mode_rejects_max_depth(server):
    # AC-16 regression: mode="neighbors", max_depth=2 → validation error

def test_graph_subgraph_mode_listed_in_unrecognized_error(server):
    # FR-20: unrecognized mode error lists "subgraph" in supported modes
```

#### In `suites/test_lifecycle.py`

```python
# Fixture: server (fresh DB per test)

def test_graph_subgraph_topology_traversal(server):
    # AC-14: write 5 entries with typed edges forming a known topology; call subgraph;
    # assert returned node IDs, edge triples, and depths match expected values exactly

def test_graph_subgraph_depth_reached_accuracy(server):
    # AC-15: A→B→C chain; max_depth=10; assert depth_reached=2

def test_graph_subgraph_truncation_depth_reached(server):
    # AC-15b: max_nodes=2 on chain A→B→C; assert truncated=true; depth_reached=1

def test_graph_subgraph_supersession_integration(server):
    # AC-09: store A (active), B (deprecated, superseded_by=C), C (active);
    # write edge A→B; call with resolve_supersessions=true;
    # assert B absent from nodes, C present

def test_graph_subgraph_201_seeds_truncated_at_200(server):
    # AC-05: store 201 entries; call with all 201 as seed_ids, max_nodes default;
    # assert len(nodes)==200 and truncated==true and depth_reached==0

def test_graph_subgraph_cold_start_empty_result(server):
    # IR-05: call before any tick; graph is empty; assert empty response, no error
    # (Note: this may be an xfail candidate if tick timing is non-deterministic in harness)
```

### Protocol Suite Note

The existing `test_protocol.py` includes `test_list_tools_returns_N` (currently 14).
vnc-019 does NOT add a new tool, so this count must remain 14. The test needs no update.
However, tool description content changes require verifying the `context_graph` tool
description in the `tools/list` response includes the subgraph mode text. Add:

```python
def test_graph_tool_description_includes_subgraph(server):
    # Verify tools/list response for context_graph mentions "subgraph" mode
    # and contains staleness disclosure terms
```

### Fixture Choices

All new tests use the `server` fixture (fresh DB per test) to avoid state leakage.
The topology test `test_graph_subgraph_topology_traversal` is standalone and
self-contained — it writes its own graph state.
The 201-seed test uses `server` with explicit setup, not `populated_server` (which
populates 50 entries, not 201).

---

## Cross-Component Test Dependencies

| Dependency | Test Impact |
|------------|-------------|
| `pub(super)` on `follow_to_current` | Compile-time — if missing, `cargo build` fails before any test runs |
| `pub(super)` on `all_non_supersedes_types` | Already `pub(super)` per architecture; no change needed |
| `SubgraphResponse` defined in `graph_read.rs` | `use super::SubgraphResponse` in `graph_read_subgraph_tests.rs` |
| vnc-018 schema v27 indexes | OR-chain metadata query performance; infra-001 DB must have migrated schema |
| TypedRelationGraph tick | Integration tests require at least one tick to fire; subgraph tests on warm graph |

---

## Open Questions for Stage 3b

1. **`test_validate_unrecognized_mode_fires_before_field_check`** in `graph_read_tests.rs` currently uses `mode="subgraph"` as the unrecognized-mode probe (see existing test). After vnc-019 delivery, `"subgraph"` is a recognized mode — this test will break. The delivery agent must update it to use a different unrecognized mode string (e.g., `"walk"`) and add a separate test confirming `"subgraph"` passes `validate_no_unsupported_params`. (R-05)

2. **Cold-start test timing**: `test_graph_subgraph_cold_start_empty_result` may be inherently racy if the infra-001 harness waits for the server to be ready and the tick fires during that wait. May need to be marked `@pytest.mark.xfail` with documentation.

3. **201-seed test performance**: Storing 201 entries in setup may be slow. Consider using `shared_server` scope if test isolation is not critical for this case.
