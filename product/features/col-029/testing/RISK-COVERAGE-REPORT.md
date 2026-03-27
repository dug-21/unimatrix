# Risk Coverage Report: col-029

GH Issue: #413 — Graph Cohesion Metrics in context_status

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `connected_entry_count` double-count (chain A→B→C) | `test_graph_cohesion_all_connected`, `test_graph_cohesion_mixed_connectivity` | PASS | Full |
| R-02 | `cross_category_edge_count` NULL guard — deprecated endpoint | `test_graph_cohesion_cross_category`, `test_graph_cohesion_mixed_connectivity` | PASS | Full |
| R-03 | `bootstrap_only=1` NLI edge leaks into `inferred_edge_count` | `test_graph_cohesion_nli_source`, `test_graph_cohesion_bootstrap_excluded` | PASS | Full |
| R-04 | `StatusReport::default()` missing field — compile error | `test_status_report_default_cohesion_fields`, `cargo check` | PASS | Full |
| R-05 | Division-by-zero on empty store (NaN/inf) | `test_graph_cohesion_all_isolated`, `test_graph_cohesion_empty_store` | PASS | Full |
| R-06 | Call from maintenance tick (NFR-01 violation) | grep static check (AC-15) | PASS | Full |
| R-07 | ~~write_pool_server() contention~~ | — (eliminated by ADR-003) | N/A | Eliminated |
| R-08 | Rust HashSet logic error for connected_entry_count | `test_graph_cohesion_mixed_connectivity` (deprecated-endpoint edge) | PASS | Full |
| R-09 | `EDGE_SOURCE_NLI` not re-exported from `lib.rs` | grep check + `cargo check` | PASS | Full |
| R-10 | Summary line omitted on empty store, no confirmation | `test_format_summary_graph_cohesion_present` (zero sub-test) | PASS | Full |
| R-11 | WAL read staleness with `read_pool()` | ADR-003 comment at call site (lines 1005-1006 of `read.rs`) | PASS | Full (design mitigation only — no runtime test required) |

---

## Test Results

### Unit Tests

**Total across workspace:** All passing.

| Crate | Tests | Passed | Failed |
|-------|-------|--------|--------|
| unimatrix-store | 152 | 152 | 0 |
| unimatrix-server | 2163 | 2163 | 0 |
| All other crates | 597 | 570 | 0 (27 ignored) |
| **Workspace total** | ~2912 | ~2885 | **0** |

**col-029 specific unit tests — 7 mandatory (AC-13) + 1 additional:**

| Test Function | Location | AC Coverage | Result |
|--------------|----------|-------------|--------|
| `test_graph_cohesion_all_isolated` | `read::tests` | AC-02, AC-07, R-05 | PASS |
| `test_graph_cohesion_all_connected` | `read::tests` | AC-03, AC-06, AC-07, R-01 | PASS |
| `test_graph_cohesion_mixed_connectivity` | `read::tests` | AC-08, R-01, R-08 | PASS |
| `test_graph_cohesion_cross_category` | `read::tests` | AC-04, R-02 | PASS |
| `test_graph_cohesion_same_category_only` | `read::tests` | AC-04 | PASS |
| `test_graph_cohesion_nli_source` | `read::tests` | AC-05, R-03 | PASS |
| `test_graph_cohesion_bootstrap_excluded` | `read::tests` | AC-16, R-03 | PASS |
| `test_graph_cohesion_empty_store` *(additional)* | `read::tests` | R-05 (zero-active branch) | PASS |

**Server response unit tests:**

| Test Function | Location | AC Coverage | Result |
|--------------|----------|-------------|--------|
| `test_status_report_default_cohesion_fields` | `mcp::response::status::tests` | AC-12, R-04 | PASS |
| `test_format_summary_graph_cohesion_present` | `mcp::response::status::tests` | AC-09, R-10 | PASS |
| `test_format_summary_graph_cohesion_absent` | `mcp::response::status::tests` | R-10 (zero suppression) | PASS |
| `test_format_markdown_graph_cohesion_section` | `mcp::response::status::tests` | AC-10 | PASS |

### Integration Tests

**Smoke gate (mandatory):** 20/20 PASS — run time 174s.

**Status-specific tools suite:** 8/8 PASS — run time 66s.

| Suite | Selected Tests | Passed | Failed | Relevance |
|-------|---------------|--------|--------|-----------|
| smoke (`-m smoke`) | 20 | 20 | 0 | Mandatory gate — `test_status_empty_db` validates zero-state R-05 path end-to-end |
| `test_tools.py` (status filter) | 8 | 8 | 0 | `test_status_all_formats`, `test_status_with_entries`, `test_status_empty_db` + 5 others |
| `test_lifecycle.py` (restart/persistence) | 3 | 3 | 0 | Restart persistence with new fields |
| `test_edge_cases.py` (empty) | 2 | 2 | 0 | Empty DB zero-field guard (R-05) |

**Total integration tests executed:** 33 (20 smoke + 8 tools + 3 lifecycle + 2 edge_cases; smoke overlaps with others)

No integration test failures. No `xfail` markers added. No GH Issues filed.

---

## Static Checks

### AC-01 — Six fields present in `StatusReport`

```
grep -E "graph_connectivity_rate|isolated_entry_count|..." status.rs
```

All six field names appear in: struct definition, `default()` block, `StatusReportJson` struct, format output functions. **PASS.**

### AC-11 — Six field assignments in `services/status.rs`

All six assignment lines (`report.graph_connectivity_rate = gcm.connectivity_rate;` etc.) confirmed at lines 683-688 of `services/status.rs`. **PASS.**

### AC-14 — Dual LEFT JOIN aliases with `status = 0`

`src_e` and `tgt_e` aliases confirmed at `read.rs` lines 1080-1081 with `AND src_e.status = 0` / `AND tgt_e.status = 0`. **PASS.**

### AC-15 — Exactly one production call site for `compute_graph_cohesion_metrics`

`grep -rn "compute_graph_cohesion_metrics" crates/` returns:
- `read.rs:1012` — function definition
- `read.rs:1807, 1849, 1884, 1916, 1941, 1965, 1993, 2019` — all inside `#[cfg(test)]` block
- `services/status.rs:681` — **the one production call site** in `compute_report()`
- `mcp/response/status.rs:72, 83` — doc comments only

No call in `maintenance_tick()` or `load_maintenance_snapshot()`. **PASS.**

### AC-17 — `read_pool()` used, `write_pool_server()` absent in function body

`compute_graph_cohesion_metrics()` calls `self.read_pool()` at lines 1025 and 1095. The `write_pool_server()` references in `read.rs` are in unrelated functions. The function docstring at line 1005 explicitly references ADR-003 and WAL snapshot semantics. **PASS.**

### R-09 — `EDGE_SOURCE_NLI` re-exported from `lib.rs`

`grep "EDGE_SOURCE_NLI" crates/unimatrix-store/src/lib.rs` returns the `pub use` re-export. **PASS.**

### R-11 — ADR-003 comment at call site

Lines 1005-1006 of `read.rs`:
```
/// Uses `read_pool()` — consistent with `compute_status_aggregates()` (ADR-003 col-029).
/// WAL snapshot semantics are intentional: bounded staleness is acceptable for this
```
**PASS.**

---

## Implementation Notes

### Query 2 Deviation from Spec

The implementation deviates from the IMPLEMENTATION-BRIEF Query 2 template in one deliberate way: rather than using a `COUNT(*)` outer join + `connected_raw` approach, it uses a UNION-based sub-query for `connected_entry_count` (matching ADR-002's preferred approach). Additionally, `mean_entry_degree` uses `active_active_edge_count` (edges between two active endpoints) rather than raw `total_edges` from Query 1. This is more correct: edges to deprecated entries would inflate the degree of their active endpoint under the raw-edges approach. The unit tests (`test_graph_cohesion_mixed_connectivity`) confirm this behavior is correct.

### Summary Format Conditional

The Summary line is conditionally suppressed when `isolated + cross_category + inferred = 0`. A store that has only `supports_edge_count > 0` (and all other fields zero) would suppress the Summary line while still showing the Markdown sub-section. This is a documented trade-off, not a bug. `test_format_summary_graph_cohesion_absent` verifies the suppression.

---

## Gaps

None. All 10 active risks (R-01 through R-11, excluding eliminated R-07) have full coverage:

- R-01, R-02, R-03, R-05: Direct unit test coverage
- R-04, R-09: Compile-time verification (`cargo check`) + grep
- R-06: Static grep (single call site, not in tick path)
- R-08: Covered by `test_graph_cohesion_mixed_connectivity` (deprecated-endpoint edge)
- R-10: `test_format_summary_graph_cohesion_present` + zero sub-test
- R-11: Design mitigation only (ADR-003 comment verified) — no runtime test required per RISK-TEST-STRATEGY.md

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | All six field names appear in `status.rs` struct definition, `default()`, `StatusReportJson`, and format functions |
| AC-02 | PASS | `test_graph_cohesion_all_isolated`: `connectivity_rate=0.0`, `isolated_entry_count=3`, all others 0 |
| AC-03 | PASS | `test_graph_cohesion_all_connected`: `connectivity_rate=1.0`, `isolated_entry_count=0`, `mean_entry_degree=4/3` |
| AC-04 | PASS | `test_graph_cohesion_cross_category` (cross-category=1, deprecated endpoint excluded) + `test_graph_cohesion_same_category_only` (cross-category=0) |
| AC-05 | PASS | `test_graph_cohesion_nli_source`: `inferred_edge_count=1` (bootstrap NLI excluded) |
| AC-06 | PASS | `test_graph_cohesion_all_connected`: `supports_edge_count=2` (both Supports edges counted) |
| AC-07 | PASS | `test_graph_cohesion_all_isolated`: `mean_entry_degree=0.0`; `test_graph_cohesion_all_connected`: `mean_entry_degree=4/3` |
| AC-08 | PASS | `test_graph_cohesion_mixed_connectivity`: `connectivity_rate=0.5` (C with deprecated-target edge counts as isolated) |
| AC-09 | PASS | `test_format_summary_graph_cohesion_present`: output contains `"Graph cohesion:"`, `"75.0%"`, isolated and cross-category values |
| AC-10 | PASS | `test_format_markdown_graph_cohesion_section`: `"#### Graph Cohesion"` present with all six labels; placed after `"### Coherence"` |
| AC-11 | PASS | `services/status.rs` line 681: call site; lines 683-688: all six field assignments in `Ok(gcm)` arm |
| AC-12 | PASS | `test_status_report_default_cohesion_fields`: all six defaults verified; `cargo check` compiles cleanly |
| AC-13 | PASS | All 7 mandatory functions present and passing: `test_graph_cohesion_all_isolated`, `test_graph_cohesion_all_connected`, `test_graph_cohesion_mixed_connectivity`, `test_graph_cohesion_cross_category`, `test_graph_cohesion_same_category_only`, `test_graph_cohesion_nli_source`, `test_graph_cohesion_bootstrap_excluded` |
| AC-14 | PASS | `read.rs` lines 1080-1081: `JOIN entries src_e ON src_e.id = ge.source_id AND src_e.status = 0` and `JOIN entries tgt_e ON tgt_e.id = ge.target_id AND tgt_e.status = 0` |
| AC-15 | PASS | Single production call site at `services/status.rs:681`; not in `maintenance_tick` or `load_maintenance_snapshot` |
| AC-16 | PASS | `test_graph_cohesion_bootstrap_excluded`: NLI source + `bootstrap_only=1` edge gives `inferred_edge_count=0` |
| AC-17 | PASS | `compute_graph_cohesion_metrics()` uses `self.read_pool()` at lines 1025 and 1095; `write_pool_server()` absent from function body |

**All 17 AC verified. All PASS.**

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"gate verification steps testing procedure unit tests integration"` — found #750 (pipeline validation tests), #2326 (async pattern verification), #3479 (two-site atomicity pattern). No novel procedure discovered.
- Queried: `/uni-knowledge-search` for `"create_graph_edges_table test helper unit tests read.rs"` — found #3600 (critical pitfall: `create_graph_edges_table()` is for pre-v13 schema only; new cohesion tests must not call it). Confirmed the implementation followed this correctly — the seven unit tests use `open_test_store()` without calling `create_graph_edges_table()`.
- Stored: nothing novel to store. The primary testing patterns for this feature (UNION dedup for connectivity, `open_test_store()` without `create_graph_edges_table()`, dual-endpoint JOIN status filters) are either already captured (#3600, #1043, #1044) or are feature-specific mechanics rather than reusable patterns.
