# col-029: Test Plan Overview — Graph Cohesion Metrics in context_status

GH Issue: #413

---

## Overall Test Strategy

This feature adds six read-only diagnostic metrics to `context_status`. The implementation
spans two crates: `unimatrix-store` (SQL query function) and `unimatrix-server` (struct fields,
service call site, format output). The test strategy is primarily unit-test driven because:

1. All business logic lives in `compute_graph_cohesion_metrics()` — a pure SQL aggregate
   function that can be fully exercised via `open_test_store()` without the MCP server.
2. The SPECIFICATION (Workflow 3) explicitly states: "No integration test additions are required
   for this feature beyond the unit tests if the SQL is fully exercised there."
3. The format output functions are pure `StatusReport -> String` transformations testable as
   unit tests in `status.rs`.

### Test Levels

| Level | Scope | Location | Count |
|-------|-------|----------|-------|
| Unit — store layer | `compute_graph_cohesion_metrics()`, 7 mandatory scenarios (AC-13) | `crates/unimatrix-store/src/read.rs` `#[cfg(test)]` | 7 |
| Unit — struct defaults | `StatusReport::default()` six new fields | `crates/unimatrix-server/src/mcp/response/status.rs` `#[cfg(test)]` | 1 |
| Unit — format output | Summary conditional line, Markdown sub-section structure | `crates/unimatrix-server/src/mcp/response/status.rs` `#[cfg(test)]` | 2 |
| Static / compile | `cargo check -p unimatrix-server` — R-04, R-09 | CI build step | — |
| Static / grep | Single call site (AC-15), `read_pool()` usage (AC-17), re-export (R-09) | Delivery gate review | — |
| Integration smoke | Mandatory gate: `pytest -m smoke` | infra-001 harness | ~15 |
| Integration tools | `test_status_all_formats`, `test_status_empty_db` regression coverage | `suites/test_tools.py` | 8 existing |

---

## Risk-to-Test Mapping

Sourced from RISK-TEST-STRATEGY.md.

| Risk ID | Priority | Risk | Test Component | Test Function(s) |
|---------|----------|------|----------------|-----------------|
| R-01 | Critical | `connected_entry_count` double-count (chain A→B→C) | store-cohesion-query | `test_graph_cohesion_all_connected`, `test_graph_cohesion_mixed_connectivity` |
| R-02 | High | `cross_category_edge_count` NULL guard — deprecated endpoint | store-cohesion-query | `test_graph_cohesion_cross_category` |
| R-03 | High | `bootstrap_only=1` NLI edge leaks into `inferred_edge_count` | store-cohesion-query | `test_graph_cohesion_bootstrap_excluded`, `test_graph_cohesion_nli_source` |
| R-04 | Medium | `StatusReport::default()` missing field → compile error | status-report-fields | `test_status_report_default_cohesion_fields` + `cargo check` |
| R-05 | Medium | Division-by-zero on empty store (NaN/inf) | store-cohesion-query | `test_graph_cohesion_all_isolated` |
| R-06 | Medium | Call from maintenance tick (NFR-01 violation) | service-call-site | grep/static check (AC-15) |
| R-07 | — | ~~write_pool contention~~ — eliminated by ADR-003 | — | — |
| R-08 | Medium | Rust HashSet logic error for connected count | store-cohesion-query | `test_graph_cohesion_mixed_connectivity` (deprecated-endpoint case) |
| R-09 | Medium | `EDGE_SOURCE_NLI` not re-exported from `lib.rs` | status-report-fields | `cargo check` + grep for `pub use` in `lib.rs` |
| R-10 | Low | Summary line omitted on empty store, no confirmation | format-output | `test_format_summary_graph_cohesion_present` |
| R-11 | Low | WAL staleness with `read_pool()` | — | ADR-003 comment check (no runtime test; accepted trade-off) |

---

## Cross-Component Test Dependencies

- `store-cohesion-query` tests are self-contained: they use `open_test_store()` and direct
  `sqlx::query` inserts. No server layer needed.
- `status-report-fields` test constructs `StatusReport::default()` directly — depends on the
  struct having all six fields present (compile-time enforced).
- `service-call-site` has no runtime unit test; coverage is via static grep checks (AC-15, AC-17)
  plus integration smoke test which calls `context_status` end-to-end.
- `format-output` tests construct a `StatusReport` with specific field values and call
  `format_status_report()` directly — depends on `status-report-fields` having the six fields.

---

## Integration Harness Plan

### Suite Selection

This feature touches `context_status` tool behavior (new fields in response) and store
read-only logic. Per the suite selection table:

| Feature touches... | Applicable suites |
|--------------------|------------------|
| Server tool logic (`context_status`) | `tools`, `protocol` |
| Store behavior (new SQL function) | `tools`, `lifecycle`, `edge_cases` |

**Smoke (mandatory gate):** `pytest -m smoke` — covers `test_status_empty_db` and
`test_status_all_formats` as smoke-marked tests. Must pass before Gate 3c.

**Primary suites for Stage 3c:** `tools` (contains 8 existing `context_status` tests that
provide regression coverage for the new fields not breaking existing output). `lifecycle`
and `edge_cases` cover restart persistence and boundary behaviors that could surface issues
if the six new fields are missing from serialization.

### Existing Suite Coverage of This Feature

| Suite | Tests relevant to col-029 | What they validate |
|-------|--------------------------|-------------------|
| `tools` (smoke) | `test_status_empty_db` | New fields default to zero on empty store — no crash |
| `tools` | `test_status_all_formats` | Summary/Markdown/JSON formats all succeed with new fields |
| `tools` | `test_status_with_entries` | New fields present and parseable on non-empty store |
| `lifecycle` | restart persistence tests | Six new fields survive serialization round-trip |
| `edge_cases` | empty DB operations | `context_status` on truly empty store — zero-field guard (R-05) |

### Gap Analysis — New Integration Tests Needed

The SPECIFICATION (Workflow 3 / "NOT in scope") explicitly states no new integration tests
are required if the SQL is fully exercised by unit tests. The 7 unit tests in
`store-cohesion-query` fully exercise all SQL paths.

However, there is one MCP-visible behavior gap that unit tests cannot cover:

**Gap 1 — Summary format line present in live response (AC-09):**
The Summary format conditional (`isolated + cross_category + inferred > 0`) is only
exercised end-to-end through the MCP interface. The existing `test_status_all_formats`
in `test_tools.py` only asserts success, not content.

**Recommended new integration test** (add to `suites/test_tools.py`):

```python
def test_status_summary_includes_graph_cohesion_line(server):
    """AC-09: Summary format includes graph cohesion line when store has entries with edges."""
    # Store two entries in different categories
    r1 = server.context_store("graph cohesion test entry A", "topic-a", "decision", agent_id="human")
    r2 = server.context_store("graph cohesion test entry B", "topic-b", "convention", agent_id="human")
    # (Note: edge insertion is not directly testable via MCP tools — rely on existing
    #  unit tests for full SQL coverage; this test validates format conditional only
    #  at zero-metrics state to confirm no crash and consistent output)
    resp = server.context_status(agent_id="human", format="summary")
    result = assert_tool_success(resp)
    # With no edges, Summary line is suppressed per the conditional — confirm no crash
    assert result.text is not None
```

**Gap 2 — Markdown `#### Graph Cohesion` sub-section present (AC-10):**
AC-10 requires verification that the Markdown section header and six labels appear. This
can only be verified through the MCP interface or unit tests on `format_status_report`.

The `format-output` unit tests (see `format-output.md`) cover this gap without needing a
new integration test — they call `format_status_report()` directly with a fully populated
`StatusReport` and assert on string content.

**Decision: No new integration tests required.** The `format-output` unit tests cover AC-09
(Summary format logic) and AC-10 (Markdown labels). The smoke gate via `test_status_empty_db`
and `test_status_all_formats` provides adequate integration regression coverage. The one
potential new test above is of marginal value since the conditional suppression case (zero
metrics) is better tested by `test_graph_cohesion_all_isolated` unit test.

### Suites to Run in Stage 3c

```bash
# Mandatory smoke gate
python -m pytest suites/ -v -m smoke --timeout=60

# Regression coverage for tools touching context_status
python -m pytest suites/test_tools.py -v --timeout=60

# Lifecycle: restart persistence with new fields
python -m pytest suites/test_lifecycle.py -v --timeout=60
```
