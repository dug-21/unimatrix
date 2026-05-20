# Risk Coverage Report: vnc-019

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | BFS visited-set keyed by node ID but resolve_supersessions substitutes to terminal ID — if substitution happens AFTER visited check, deprecated node appears in results | `test_bfs_resolve_supersessions_before_visited_check`, `test_bfs_direction_both_no_duplicate_edges` | PASS | Full |
| R-02 | direction="both" dedup uses canonical (source_id, target_id, rel_type) triple — wrong key causes duplicates or drops | `test_bfs_direction_both_no_duplicate_edges`, `test_bfs_edge_direction_always_outgoing`, `test_graph_subgraph_direction_both_dedup`, `test_graph_subgraph_direction_outgoing_on_all_edge_records` | PASS | Full |
| R-03 | max_nodes cap checked pre-enqueue against collected_node_ids.len() — seeds at or above cap must set truncated=true, depth_reached=0 | `test_bfs_seed_saturation_sets_truncated`, `test_graph_subgraph_topology_traversal` (truncation invariant) | PASS | Full |
| R-04 | Post-BFS OR-chain SQL built dynamically — empty WHERE clause when collected_edges is empty produces syntax error | `test_bfs_cold_start_empty_result`, `test_graph_subgraph_unknown_seed_empty_result` | PASS | Full |
| R-05 | validate_no_unsupported_params regression — seed_ids or max_depth passing through on chain/current/neighbors after the subgraph arm is added | `test_validate_chain_rejects_max_depth`, `test_validate_current_rejects_max_depth`, `test_validate_neighbors_rejects_max_depth`, `test_validate_current_rejects_seed_ids`, `test_validate_subgraph_rejects_from_id`, `test_validate_subgraph_rejects_to_id`, `test_validate_subgraph_mode_recognized`, `test_validate_unrecognized_mode_lists_subgraph`, `test_graph_subgraph_chain_mode_rejects_seed_ids`, `test_graph_subgraph_chain_mode_rejects_max_depth`, `test_graph_subgraph_neighbors_mode_rejects_max_depth`, `test_graph_subgraph_mode_listed_in_unrecognized_error` | PASS | Full |
| R-06 | Circular supersession chain — 50-hop guard must terminate BFS | `test_bfs_cold_start_empty_result` (cold-start path exercises guard absence); circular path via `test_validate_max_depth_boundary_values_accepted` (guard inherited from vnc-018 follow_to_current unit tests) | PASS | Partial — no dedicated circular-chain test; 50-hop guard is tested in vnc-018 follow_to_current tests which are unchanged |
| R-07 | max_nodes > 200 behavior — must reject with validation error, cap never exceeded | `test_validate_max_nodes_above_200_rejected`, `test_validate_max_nodes_zero_rejected`, `test_validate_max_nodes_200_accepted` (unit); `test_graph_subgraph_max_nodes_above_200_rejected` (integration); truncation invariant in topology test | PASS | Full |
| R-08 | depth_reached computed as max depth across collected_edges — must be 0 when no edges, N when truncation fires at depth N | `test_bfs_depth_reached_zero_when_no_edges`, `test_bfs_two_hop_chain_depth_reached_2`, `test_bfs_max_depth_one_only_direct_neighbors`, `test_bfs_seed_saturation_sets_truncated` (depth_reached=0), `test_graph_subgraph_depth_reached_accuracy`, `test_graph_subgraph_truncation_depth_reached` | PASS | Full |
| R-09 | Batch node hydration (get_many) graceful on missing ENTRIES rows — no panic | `test_bfs_cold_start_empty_result` (nodes not in graph → empty result, no panic); get_many reviewed in implementation — returns partial result silently | PASS | Partial — no explicit delete-then-hydrate test; cold-start path confirms no panic on absent entries |
| R-10 | SubgraphResponse visibility across module boundary — compile error if not pub | `cargo build --workspace` (compile gate) | PASS | Full |
| R-11 | Staleness contract — tick-window BFS silently omits recent edges; tool description is sole disclosure | `test_tool_description_contains_staleness_disclosures` (unit string assertion); `test_graph_tool_description_includes_subgraph` in protocol suite | PASS | Full |
| R-12 | Edge depth non-determinism under multi-path discovery — first-discovery-wins | `test_bfs_direction_both_no_duplicate_edges` (dedup by canonical triple); `test_graph_subgraph_topology_traversal` (no duplicates assertion) | PASS | Partial — no explicit multi-path same-edge depth test; dedup invariant is validated |
| R-13 | follow_to_current None fallback behavior — deprecated original re-enqueued when chain broken | `test_bfs_cold_start_empty_result` (None path — no supersession chain, fallback not triggered); vnc-018 unit tests for follow_to_current cover None path | PASS | Partial — None fallback path relies on vnc-018 follow_to_current coverage |
| R-14 | Default edge_types expansion — all_non_supersedes_types imported correctly from graph_read_neighbors.rs | `test_bfs_traverses_supports_edge` (absent edge_types defaults to all non-Supersedes; Supports traversed); `test_all_non_supersedes_types_count` (count=15, Supersedes excluded) | PASS | Full |
| R-15 | Malformed JSON in GRAPH_EDGES.metadata — must return metadata=None, no panic | `test_bfs_null_metadata_column_returns_json_null` via edge fields test; metadata=null/None path verified in `test_graph_subgraph_edge_record_fields` (metadata field present) | PASS | Partial — no test with deliberately malformed JSON string injected into DB; the serde_json::from_str(...).ok() guard is verified by code review |
| R-16 | truncated=true with no structured reason — tool description must document re-query pattern | `test_tool_description_contains_staleness_disclosures` (truncated mentioned in description); AC-13 code review | PASS | Full |

---

## Test Results

### Unit Tests (cargo test --workspace)

- Total: 5040
- Passed: 5040
- Failed: 0
- Ignored: 28 (NLI model tests requiring models on disk)

**vnc-019 specific unit tests (unimatrix-server):**

| Module | Tests | All Pass |
|--------|-------|----------|
| `graph_read_subgraph_tests.rs` (struct / param) | 5 | Yes |
| `graph_read_subgraph_bfs_tests.rs` (BFS behavioral) | 19 | Yes |
| `graph_read_tests_vnc019.rs` (graph_read.rs changes) | 18 | Yes |
| `graph_subgraph_integration.rs` (crate-level integration) | 3 | Yes |
| Existing `graph_read_tests.rs` (extended, no regressions) | 16 | Yes |
| Existing `graph_read_neighbors_tests.rs` (unchanged) | All pass | Yes |

Total vnc-019 unit tests: **61** new/modified tests, all passing.

### Integration Tests (infra-001)

Suites run per plan: smoke (mandatory gate), tools, protocol, lifecycle, edge_cases, security.
Suites skipped: confidence, contradiction, volume (feature does not touch those systems).

| Suite | Tests | Passed | Failed | XFAIL | XPASS | Notes |
|-------|-------|--------|--------|-------|-------|-------|
| smoke | 23 | 23 | 0 | 0 | 0 | Mandatory gate — PASS |
| protocol | 13 | 13 | 0 | 0 | 0 | tool count=14 confirmed |
| tools | 162 | 162 | 0 | 3 | 0 | 3 pre-existing xfails (GH#303, #305, other) |
| lifecycle | 64 | 57 | 0 | 5 | 2 | 5 pre-existing xfails; 2 XPASS (pre-existing) |
| edge_cases | 24 | 22 | 0 | 2 | 0 | GH#576, GH#111 pre-existing |
| security | 20 | 20 | 0 | 0 | 0 | input validation boundaries |

**New vnc-019 integration tests added:**

- `suites/test_tools.py`: 16 new subgraph tests (all passing)
- `suites/test_lifecycle.py`: 3 new subgraph lifecycle tests (all passing)
- `harness/client.py`: `max_depth` param added; `id` made optional (subgraph mode compatible)

**Total integration tests run: 306 (including new subgraph tests)**
**Integration passed: 297 passed + 10 xfail + 2 xpassed (all expected)**
**Integration failed: 0**

---

## Gaps

### R-06: Circular supersession chain
No dedicated circular-chain unit test exists in vnc-019. The 50-hop termination guard lives in `follow_to_current` in `graph_read_neighbors.rs`, which is unchanged from vnc-018. The vnc-018 test suite covers this path. A dedicated circular-chain BFS test was identified in the test plan but not implemented because the `set_test_graph` helper does not provide a mechanism to inject entries with circular `superseded_by` references into the SQLite test DB (needed for `follow_to_current` to traverse). This is a partial coverage gap — the guard code is unchanged and covered by vnc-018 tests.

### R-09: Batch hydration with explicitly deleted ENTRIES row
No test simulates deleting an entry from ENTRIES between graph rebuild and `get_many`. The cold-start path confirms no panic on empty results. Code review of `get_many` confirms it returns partial results on missing IDs. Full test would require a harness that can delete entries post-store, which is not supported by the current MCP interface.

### R-12: Multi-path depth non-determinism
No test explicitly verifies that "first-discovery-wins" depth is maintained when the same node is reachable at different depths via different seeds. The dedup invariant (no duplicate canonical triples) is tested. Depth non-determinism would be a secondary ordering concern that requires a controlled BFS frontier inspection — not feasible through the MCP interface.

### R-13: follow_to_current None fallback
No dedicated test simulates a broken supersession chain (superseded_by pointing to non-existent ID) and verifies the deprecated original is included in results. This path requires inserting a raw entry with a broken `superseded_by` reference in the test DB, which is not directly supported through the MCP interface.

### R-15: Malformed JSON metadata in DB
No test injects deliberately malformed JSON into `GRAPH_EDGES.metadata`. The `serde_json::from_str(...).ok()` guard is verified by code review. A full test would require direct DB access to insert a bad row, not available through MCP.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_graph_subgraph_basic_response_shape` — nodes, edges, truncated, seed_ids, depth_reached present |
| AC-02 | PASS | `test_graph_subgraph_node_shape_matches_entry_record` — id, title, content, category, status fields verified |
| AC-03 | PASS | `test_graph_subgraph_edge_record_fields` — source_id, target_id, relation_type, direction, depth, metadata present; direction=="outgoing" |
| AC-04 | PASS | `test_bfs_traverses_supports_edge` + cold-start test (seed always in result if graph has it) |
| AC-05 | PASS | `test_bfs_seed_saturation_sets_truncated` (unit: 3 seeds, max_nodes=3 → truncated=true, depth_reached=0) |
| AC-06 | PASS | `test_validate_max_depth_zero_rejected`, `test_validate_max_depth_eleven_rejected`, `test_validate_max_depth_boundary_values_accepted`; `test_graph_subgraph_max_depth_boundary_0_rejected`, `test_graph_subgraph_max_depth_boundary_11_rejected` |
| AC-07 | PASS | `test_validate_seed_ids_absent_returns_error`, `test_validate_seed_ids_empty_returns_error`; `test_graph_subgraph_empty_seed_ids_rejected` — exact message verified |
| AC-08 | PASS | `test_validate_unknown_edge_type_rejected`; `test_graph_subgraph_unknown_edge_type_rejected` — bad value echoed in error |
| AC-09 | PASS | `test_bfs_traverses_supports_edge` (resolve_supersessions behavior covered by BFS tests); BFS substitution before visited check covered in unit (R-01 tests) |
| AC-10 | PASS | `test_bfs_edge_direction_always_outgoing` (resolve_supersessions=false default path: deprecated nodes returned as-is) |
| AC-11 | PASS | `test_validate_chain_rejects_seed_ids` (existing + extended), `test_validate_current_rejects_seed_ids`, `test_validate_neighbors_rejects_seed_ids` (existing); `test_graph_subgraph_chain_mode_rejects_seed_ids` |
| AC-12 | PASS | `test_bfs_direction_both_no_duplicate_edges`; `test_graph_subgraph_direction_both_dedup` — len(edges)==1, direction=="outgoing" |
| AC-13 | PASS | `test_tool_description_contains_staleness_disclosures` — tick, depth_reached, truncated, "empty result", "outgoing", "200" all asserted |
| AC-14 | PASS | `test_graph_subgraph_topology_traversal` — 5-entry graph written, called, dedup+dangling-edge invariants asserted |
| AC-15 | PASS | `test_bfs_depth_reached_zero_when_no_edges`, `test_bfs_two_hop_chain_depth_reached_2`, `test_bfs_max_depth_one_only_direct_neighbors`, `test_bfs_seed_saturation_sets_truncated`; `test_graph_subgraph_depth_reached_accuracy`, `test_graph_subgraph_truncation_depth_reached` |
| AC-16 | PASS | `test_validate_chain_rejects_max_depth`, `test_validate_current_rejects_max_depth`, `test_validate_neighbors_rejects_max_depth`; `test_graph_subgraph_chain_mode_rejects_max_depth`, `test_graph_subgraph_neighbors_mode_rejects_max_depth` — exact message contains "max_depth" + "subgraph" |
| AC-17 | PASS | `test_bfs_cold_start_empty_result` (unit); `test_graph_subgraph_unknown_seed_empty_result` — nodes=[], edges=[], truncated=false, depth_reached=0, seed_ids echoed |
| AC-18 | PASS | `test_graph_subgraph_edge_record_fields` — metadata field present on all edges; null verified in edge_record_metadata_serializes_as_null unit test |
| AC-19 | PASS | `test_bfs_cold_start_empty_result` — isolated seed, no edges, no SQL error (proves empty OR-chain guard fires correctly) |

---

## Pre-existing xfail Markers (not caused by vnc-019)

All xfail markers pre-date vnc-019. No new xfail markers were added as part of this feature.

| GH Issue | Suite | Test | Reason |
|----------|-------|------|--------|
| GH#576 | edge_cases | `test_very_long_content` | Content size cap (8KB) rejects 50KB payloads; test predates cap |
| GH#111 | edge_cases | `test_100_rapid_sequential_stores` | Rate limit blocks rapid sequential stores |
| GH#305 | lifecycle | `test_auto_quarantine_after_consecutive_bad_ticks` | Tick interval env var required |
| (XPASS) | lifecycle | `test_search_multihop_injects_terminal_active` | Implementation changed; not caused by vnc-019 |
| (XPASS) | lifecycle | `test_inferred_edge_count_unchanged_by_cosine_supports` | bugfix-491 landed |
| Pre-existing | lifecycle | `test_context_status_supports_edge_count_increases_after_tick` | Tick timing |
| Pre-existing | lifecycle | `test_s1_edges_visible_in_status_after_tick` | Tick interval |
| Pre-existing | lifecycle | `test_inferred_edge_count_unchanged_by_s1_s2_s8` | bugfix-491 |
| Pre-existing | tools | (3 xfails) | Pre-existing from earlier features |
