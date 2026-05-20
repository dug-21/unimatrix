# Agent Report: vnc-019-agent-4-tester (Stage 3c)

## Task

Execute test suite for vnc-019 (context_graph subgraph mode): unit tests, integration smoke tests, and relevant integration suites (tools, protocol, lifecycle, edge_cases, security). Write new integration tests identified in the test plan. Produce RISK-COVERAGE-REPORT.md.

## Results

### Unit Tests

`cargo test --workspace`: **5040 passed, 0 failed** (28 ignored — NLI model tests requiring on-disk models).

vnc-019-specific unit tests verified passing:
- `graph_read_subgraph_bfs_tests.rs`: 19 BFS behavioral tests (validation, BFS contracts, edge cases)
- `graph_read_tests_vnc019.rs`: 18 tests (validate_no_unsupported_params, SubgraphResponse serialization, tool description assertions)
- `graph_read_subgraph_tests.rs`: 5 struct-level param tests
- `graph_subgraph_integration.rs` (crate-level): 3 integration tests
- `graph_read_tests.rs` (existing, no regressions): 16 tests all passing

### Integration Tests

| Suite | Result | Counts |
|-------|--------|--------|
| smoke (mandatory gate) | PASS | 23/23 |
| protocol | PASS | 13/13 |
| tools | PASS | 162 passed, 3 pre-existing xfail |
| lifecycle | PASS | 57 passed, 5 pre-existing xfail, 2 pre-existing xpass |
| edge_cases | PASS | 22 passed, 2 pre-existing xfail (GH#576, GH#111) |
| security | PASS | 20/20 |

### New Integration Tests Added

**`suites/test_tools.py`** — 16 new tests (all passing):
- Response shape: basic, node shape, edge record fields
- Validation: empty seed_ids, max_depth 0/11, max_nodes 201, from_id on subgraph, unknown edge_type
- Correctness: direction=both dedup, direction always outgoing, unknown seed empty result
- Regressions: chain/neighbors reject seed_ids/max_depth, unrecognized mode lists subgraph

**`suites/test_lifecycle.py`** — 3 new tests (all passing):
- `test_graph_subgraph_topology_traversal` — 5-entry graph, dedup + dangling-edge invariants
- `test_graph_subgraph_depth_reached_accuracy` — A→B→C chain depth_reached
- `test_graph_subgraph_truncation_depth_reached` — max_nodes=2 cap, truncation invariant

**`harness/client.py`** — updated `context_graph` method:
- `id` parameter made optional (None default; subgraph mode uses seed_ids, not id)
- `max_depth` parameter added for subgraph mode

### Coverage Gaps

- **R-06 (circular supersession)**: 50-hop guard tested in vnc-018 tests (unchanged). No vnc-019 circular-chain test because the MCP interface cannot insert circular `superseded_by` references.
- **R-09 (missing ENTRIES row)**: No explicit delete-then-hydrate test. Code review confirms `get_many` returns partial results silently.
- **R-12 (multi-path depth non-determinism)**: Dedup invariant tested; exact depth-per-path ordering not testable through MCP.
- **R-13 (follow_to_current None fallback)**: Covered by vnc-018 follow_to_current tests; MCP interface cannot inject broken supersession chains.
- **R-15 (malformed JSON metadata)**: `serde_json::from_str(...).ok()` verified by code review; no MCP path to insert raw malformed metadata.

All Critical (R-01, R-02, R-03) and High (R-04, R-05, R-07, R-08, R-11) risks have full test coverage. All 19 AC items verified.

## Files Modified/Created

- Created: `product/features/vnc-019/testing/RISK-COVERAGE-REPORT.md`
- Modified: `product/test/infra-001/suites/test_tools.py` (16 new subgraph tests appended)
- Modified: `product/test/infra-001/suites/test_lifecycle.py` (3 new subgraph lifecycle tests appended)
- Modified: `product/test/infra-001/harness/client.py` (`max_depth` param + `id` optional)

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned ADR #4493 (vnc-019 staleness/subgraph decisions), ADR #4477 (GraphParams lock), lesson #4473 (gate 3b failure lesson), and related ADRs. All relevant context confirmed prior decisions.
- Stored: nothing novel to store — the test patterns used (BFS unit tests via set_test_graph helper, infra-001 graph mode tests with context_edge setup) follow established conventions already in the codebase. The harness client update (optional `id`, new `max_depth`) is a minor mechanical change, not a reusable pattern.
