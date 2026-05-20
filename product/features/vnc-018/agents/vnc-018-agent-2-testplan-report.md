# Agent Report: vnc-018-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Status**: Complete

## Files Written

| File | Lines | Purpose |
|------|-------|---------|
| `product/features/vnc-018/test-plan/OVERVIEW.md` | ~170 | Strategy, risk-to-test mapping, integration harness plan |
| `product/features/vnc-018/test-plan/graph_read.md` | ~260 | All graph_read.rs functions + node_index_for accessor |
| `product/features/vnc-018/test-plan/tools_dispatch.md` | ~95 | Capability check, dispatch wiring, P-03 update |
| `product/features/vnc-018/test-plan/store_queries.md` | ~170 | query_supersession_chain, query_direct_neighbors, schema cascade |
| `product/features/vnc-018/test-plan/ppr_bfs.md` | ~110 | Advances/Motivates PPR + BFS additions, regression guard |
| `product/features/vnc-018/test-plan/migration.md` | ~150 | All 7 v26→v27 cascade touch points, AC-19 migration test file |

## Risk Coverage Mapping

All 21 risks mapped:
- Critical (R-01, R-02, R-03, R-04, R-05, R-06, R-19, R-20): Full coverage
- High (R-08, R-09, R-10, R-11, R-12, R-13, R-14, R-21): Full coverage
- Medium (R-15, R-16, R-17, R-18): Adequate coverage
- Low/Resolved (R-07): Unit test on node_index_for accessor

All 20 ACs (AC-01 through AC-20) mapped to specific named test expectations.

## Integration Harness Plan Summary

**Suites to run**: `smoke` (gate), `protocol`, `tools`, `lifecycle`, `edge_cases`

**test_protocol.py change (mandatory)**:
- `test_list_tools_returns_thirteen` → rename to `test_list_tools_returns_fourteen`; assert 14 tools + `context_graph` present

**New Python tests required** (all in `test_tools.py`, `server` fixture):

| Test name | Covers | Priority |
|-----------|--------|----------|
| `test_graph_chain_basic` | AC-20 chain mode | Mandatory |
| `test_graph_current_resolves_deprecated` | AC-20 current mode | Mandatory |
| `test_graph_neighbors_outgoing_depth1` | AC-20 neighbors mode | Mandatory |
| `test_graph_current_nonexistent_id_returns_error` | AC-05a / R-21 | Critical |
| `test_graph_chain_nonexistent_id_returns_empty` | AC-04 / R-21 pair | Critical |
| `test_graph_current_orphaned_deprecated_returns_error` | R-20 | Critical |
| `test_graph_neighbors_depth1_sees_fresh_write` | R-03 depth=1 | Critical |
| `test_graph_neighbors_depth2_does_not_see_fresh_write` | R-03 staleness | Critical |
| `test_graph_neighbors_edgerecord_metadata_is_null` | R-15 | Medium |
| `test_graph_neighbors_supersedes_silently_excluded_no_warning_field` | R-06 / AC-10a | Critical |

## Non-Negotiable Tests

The following must be present before Gate 3c or will be gate failures:
1. P-03 asserts 14 tools (AC-16) — lesson #4437 precedent
2. `migration_v26_to_v27.rs` asserts all 4 index names (AC-19)
3. AC-03b raw JSON wire shape of `truncated` field (not deserialized)
4. R-03 staleness test with comment "expected behavior, not a bug"
5. R-20 orphaned-deprecated test — only guard against omitted `AND e.status='Active'`
6. AC-05a / R-21 matched pair with comment stating asymmetry is intentional

## Open Questions

None. All open questions from SPECIFICATION.md resolved per IMPLEMENTATION-BRIEF.md:
- OQ-01: neighbors non-existent anchor returns empty (consistent with chain mode)
- OQ-02: depth validated to 1..=10 (error outside range)

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 10 entries; most relevant were ADR entries #4481 (four missing indexes), #4475 (SQL CTE decision), #4482 (BFS node_index_for accessor), #4474 (multi-mode asymmetry pattern), #4437 (missing tool count lesson)
- Queried: `context_search` for vnc-018 ADRs — returned #4477, #4479, #4480 (ADR-003, ADR-005, ADR-006)
- Queried: `context_search` for MCP integration testing patterns — returned #1369 (6-step handler pipeline), #4437 (tool count gate failure lesson), #4210 (normalization replication), #4474 (behavioral asymmetry pattern)
- Stored: nothing novel to store — the test planning patterns used here (matched asymmetry pair tests with explicit comments, wire-format JSON inspection, cold-start SQL path tests) are well-established in the codebase. The R-20 orphaned-deprecated test pattern is feature-specific to this CTE filter risk and not yet a cross-feature reusable pattern.
