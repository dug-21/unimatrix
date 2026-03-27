# Agent Report: col-029-agent-2-testplan

Phase: Test Plan Design (Stage 3a)
Feature: col-029 — Graph Cohesion Metrics in context_status (GH #413)

---

## Output Files

- `/workspaces/unimatrix-col-029/product/features/col-029/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix-col-029/product/features/col-029/test-plan/store-cohesion-query.md`
- `/workspaces/unimatrix-col-029/product/features/col-029/test-plan/status-report-fields.md`
- `/workspaces/unimatrix-col-029/product/features/col-029/test-plan/service-call-site.md`
- `/workspaces/unimatrix-col-029/product/features/col-029/test-plan/format-output.md`

---

## Risk Coverage Mapping

| Risk ID | Priority | Coverage | Test File | Method |
|---------|----------|----------|-----------|--------|
| R-01 | Critical | Full | store-cohesion-query | `test_graph_cohesion_all_connected` (chain A→B→C), `test_graph_cohesion_mixed_connectivity`; assert `connectivity_rate == 1.0` and `<= 1.0` |
| R-02 | High | Full | store-cohesion-query | `test_graph_cohesion_cross_category` — deprecated endpoint edge asserts `cross_category_edge_count = 1` not 2 |
| R-03 | High | Full | store-cohesion-query | `test_graph_cohesion_bootstrap_excluded` (NLI+bootstrap_only=1 asserts inferred=0), `test_graph_cohesion_nli_source` |
| R-04 | Medium | Full | status-report-fields | `test_status_report_default_cohesion_fields` + `cargo check` (compile-time) |
| R-05 | Medium | Full | store-cohesion-query | `test_graph_cohesion_all_isolated` — explicit `is_nan()` and `is_infinite()` guards; zero-entry sub-scenario |
| R-06 | Medium | Full | service-call-site | Static grep: `grep -rn "compute_graph_cohesion_metrics" crates/` — exactly one result |
| R-07 | — | Eliminated | — | ADR-003 correction removes write-pool contention risk |
| R-08 | Medium | Full | store-cohesion-query | `test_graph_cohesion_mixed_connectivity` — deprecated-endpoint edge does not count C as connected |
| R-09 | Medium | Full | status-report-fields | `cargo check -p unimatrix-server` + grep for `pub use EDGE_SOURCE_NLI` in `lib.rs` |
| R-10 | Low | Full | format-output | `test_format_summary_graph_cohesion_present` — zero-fields sub-test confirms suppression |
| R-11 | Low | Accepted | service-call-site | ADR-003 comment check at call site; no runtime test (WAL staleness not deterministically testable) |

---

## Integration Suite Plan (Stage 3c)

Suites to run:
1. `pytest -m smoke` — mandatory gate
2. `python -m pytest suites/test_tools.py -v` — 8 existing `context_status` tests provide regression
3. `python -m pytest suites/test_lifecycle.py -v` — restart persistence with new serialized fields

No new integration tests required. The SPECIFICATION Workflow 3 explicitly excludes them,
and the `format-output` unit tests cover the MCP-visible format behaviors (AC-09, AC-10).

---

## Key Design Decisions in Test Plans

1. `open_test_store()` applies all migrations including v13 (GRAPH_EDGES). New cohesion tests
   must NOT call `create_graph_edges_table()` — this is for pre-v13 tests only. Documented as
   pattern #3600 in Unimatrix.

2. The chain topology for R-01 (A→B→C where B is both source and target) is the mandatory
   test case. Star topology is insufficient because it does not exercise the overlap.

3. `service-call-site` has no runtime unit test by design. The match arm structure is trivial
   and compile-verified; integration smoke covers the code path end-to-end.

4. `format-output` tests are synchronous (no `#[tokio::test]`). `format_status_report` is a
   pure function taking `&StatusReport` — no async context needed.

5. The `#### Graph Cohesion` vs `### Graph Cohesion` discrepancy between SPECIFICATION FR-13
   and ARCHITECTURE/IMPLEMENTATION-BRIEF is flagged in `format-output.md`. Implementation
   Brief is authoritative; test asserts `####`.

---

## Open Questions

1. **`open_test_store()` and entries table `category` column:** The entry insert SQL in
   `store-cohesion-query.md` assumes a minimal `entries` row schema. The implementer must
   verify the exact columns required by the schema (nullable vs NOT NULL) to avoid insert
   failures. The pseudocode agent should confirm the minimal viable insert.

2. **UNIQUE constraint on `graph_edges`:** The `(source_id, target_id, relation_type)` UNIQUE
   constraint means the `test_graph_cohesion_nli_source` test must use different `relation_type`
   values for the two A→B edges (one `bootstrap_only=0`, one `bootstrap_only=1`). This is
   called out in the test plan but should be confirmed against the actual schema DDL.

3. **`StatusReportJson` six fields:** The six fields must also appear in `StatusReportJson`
   and the `From<&StatusReport>` impl. The test plan covers this via compile-time check only.
   If the JSON format output diverges (e.g., fields serialized under different names), a
   dedicated JSON format unit test may be needed in Stage 3c.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` category=procedure for "testing procedures gate verification integration test triage" — found entries #487 (workspace tests without hanging), #3479 (two-site atomicity coupled test), #3440 (spec authority before formatter spawn). Applied: confirmed no new procedure needed.
- Queried: `/uni-knowledge-search` for "store layer unit test SQL aggregate read.rs test patterns" — found #726 (SQL Aggregation pattern), #2271 (SqlxStore test setup), #748 (TestHarness integration), #3539 (schema cascade). Applied: confirmed `open_test_store()` usage pattern and schema context.
- Queried: `/uni-knowledge-search` for "col-029 architectural decisions" — found ADRs #3591, #3592, #3594, #3595. Applied: ADR-002 (two queries), ADR-003 (read_pool), ADR-004 (no cartesian product) directly inform test scenarios.
- Stored: entry #3600 "read.rs test helper create_graph_edges_table is for pre-v13 schema only" via `/uni-store-pattern` — novel pitfall for implementers adding post-v13 graph tests.
