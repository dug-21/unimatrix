# col-029-agent-3-store-cohesion-query Report

Agent ID: col-029-agent-3-store-cohesion-query
Feature: col-029 — graph cohesion metrics (GH #413)
Component: store-cohesion-query

---

## Files Modified

1. `crates/unimatrix-store/src/read.rs` — added:
   - `pub const EDGE_SOURCE_NLI: &str = "nli";` (near GraphEdgeRow, ~line 1412)
   - `pub struct GraphCohesionMetrics { ... }` (six fields, after StatusAggregates)
   - `impl Store { pub async fn compute_graph_cohesion_metrics(&self) -> Result<GraphCohesionMetrics> }`
   - Two helper fns `insert_test_entry`, `insert_test_edge` and 8 unit tests in the existing `#[cfg(test)]` block

2. `crates/unimatrix-store/src/lib.rs` — re-exported `EDGE_SOURCE_NLI` and `GraphCohesionMetrics`

3. `crates/unimatrix-store/tests/migration_v16_to_v17.rs` — import reorder only (cargo fmt)

---

## Tests

```
running 8 tests
test read::tests::test_graph_cohesion_all_isolated ... ok
test read::tests::test_graph_cohesion_all_connected ... ok
test read::tests::test_graph_cohesion_mixed_connectivity ... ok
test read::tests::test_graph_cohesion_cross_category ... ok
test read::tests::test_graph_cohesion_same_category_only ... ok
test read::tests::test_graph_cohesion_nli_source ... ok
test read::tests::test_graph_cohesion_bootstrap_excluded ... ok
test read::tests::test_graph_cohesion_empty_store ... ok

test result: ok. 8 passed; 0 failed; 0 ignored
```

Full workspace: all 39 test suites pass with zero new failures. Zero clippy warnings introduced.

---

## Deviations from Pseudocode

### Deviation 1 — Query 2 restructured from per-entry outer loop to three scalar sub-queries

**Pseudocode**: outer `FROM entries e LEFT JOIN graph_edges ge ... WHERE e.status=0` with `SUM(CASE ...)` for `cross_category_edge_count`.

**Problem**: each edge A→B appears twice in the per-entry outer loop — once when the outer entry is A (source), once when B (target). This double-counts every cross-category edge. The failing test: `test_graph_cohesion_cross_category` returned `cross_category_edge_count=2` instead of 1.

**Fix**: `cross_category_edge_count` is a scalar sub-query scanning `graph_edges` directly with INNER JOINs to both endpoints.

### Deviation 2 — `connected_entry_count` UNION sub-query requires both endpoints active

**Pseudocode**: the UNION of all `source_id` and `target_id` from `bootstrap_only=0` edges, joined back to entries with `status=0`.

**Problem**: an active entry C that only edges to deprecated entry E would appear in `source_id` UNION, then pass the `status=0` join (C itself is active), making C appear "connected" even though it has no active neighbour. Test `test_graph_cohesion_mixed_connectivity` returned `connectivity_rate=0.75` instead of 0.5.

**Fix**: UNION sub-query explicitly INNER JOINs both `src_a.status=0` and `tgt_a.status=0` to qualify edges before collecting IDs.

### Deviation 3 — `mean_entry_degree` uses active-active edge count, not total non-bootstrap edges

**Pseudocode**: `mean_entry_degree = (2 * total_edges) / active` where `total_edges` is all `bootstrap_only=0` edges.

**Problem**: the test `test_graph_cohesion_mixed_connectivity` has edge C→E (active→deprecated) and expects this edge NOT to count in `total_edges`. Test returned `mean_entry_degree=1.0` instead of 0.5.

**Fix**: a separate `active_active_edge_count` sub-query (INNER JOIN both endpoints with `status=0`) replaces `total_edges` in the `mean_entry_degree` formula.

### Deviation 4 — `entries.source` NOT NULL column missing from test insert

**Pseudocode/test plan**: the entry insert pattern omitted the `source` column (NOT NULL, no default). Tests panicked on `NOT NULL constraint failed: entries.source`. Added `source=''` to `insert_test_entry` helper.

---

## Issues / Blockers

None. All 8 tests pass, zero new failures across workspace.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-store` — found entry #3028 (read_pool pub(crate) visibility pattern), #2744 (write_pool_server for server tests), #2058 (pool acquire timeouts). Applied ADR-003 read_pool() guidance directly.
- Store attempt: Tried to store pattern "cross_category_edge_count per-edge sub-query" via `/uni-store-pattern` but received `MCP error -32003: Agent 'anonymous' lacks Write capability`. Pattern is documented in this report for the retrospective to capture:

  **Pattern to store**: "When computing edge aggregate counts (cross-category, etc.) from a query whose outer FROM scans entries, each undirected edge appears twice — once per endpoint. Use a scalar sub-query scanning graph_edges directly with INNER JOINs to both endpoints instead of SUM in the outer loop. Also: connected_entry_count UNION must require both endpoints active (not just the collected ID), or active entries with deprecated-only neighbours are falsely counted as connected."
