# Gate 3b Report: col-029

> Gate: 3b (Code Review)
> Date: 2026-03-26
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | Three documented deviations, all justified and test-verified |
| Architecture compliance | PASS | ADR-001 through ADR-004 all satisfied |
| Interface implementation | PASS | All six `StatusReport` fields, `GraphCohesionMetrics`, `EDGE_SOURCE_NLI` correct |
| Test case alignment | PASS | 8 store unit tests + 4 format/struct tests; all test plan scenarios covered |
| Code quality | WARN | `read.rs` is 2046 lines (exceeds 500-line rule); pre-existing and out-of-scope per architecture |
| Security | PASS | Parameterless read-only SQL; no user input; no injection surface |
| Knowledge stewardship | PASS | All three agent reports contain stewardship sections with Queried/Stored entries |

---

## Detailed Findings

### Pseudocode Fidelity

**Status**: PASS

**Evidence**: The implementation deviates from pseudocode in three documented ways (agent-3 report, Deviations 1–3):

1. **Query 2 restructured from per-entry outer loop to four scalar sub-queries.** The pseudocode specified an outer `FROM entries e LEFT JOIN graph_edges ge` with a `SUM(CASE ...)` for `cross_category_edge_count`. This double-counted every cross-category edge (each edge A→B appeared once when the outer row is A, once when B). The fix uses a scalar sub-query scanning `graph_edges` directly with INNER JOINs to both active endpoints. All tests pass with the corrected form.

2. **`connected_entry_count` UNION requires both endpoints active.** The pseudocode UNION collected all `source_id` and `target_id` from `bootstrap_only=0` edges and then joined to `entries` with `status=0` only on the collected ID itself. This erroneously marked an active entry as "connected" if it only edges to a deprecated entry. The fix INNER JOINs both `src_a.status=0` and `tgt_a.status=0` before collecting IDs, matching the semantic intent of R-08 in the risk strategy.

3. **`mean_entry_degree` uses `active_active_edge_count` instead of `total_edges`.** The pseudocode used all `bootstrap_only=0` edges for the mean degree formula. The test `test_graph_cohesion_mixed_connectivity` (active→deprecated edge) demonstrated this over-counts degree. The fix computes only active→active edge count, consistent with PPR semantics.

All three deviations are justified by test-verified functional correctness issues. The agent exhausted the pseudocode as a starting point and corrected bugs discovered through test execution, which is appropriate behaviour for a delivery agent.

**Query count**: Two `fetch_one` calls on `read_pool()` — ADR-002 two-query cap is satisfied. Query 1 (two aggregate SUM columns, one `fetch_one`), Query 2 (four scalar sub-query columns, one `fetch_one`).

### Architecture Compliance

**Status**: PASS

**ADR-001 — EDGE_SOURCE_NLI constant**: `pub const EDGE_SOURCE_NLI: &str = "nli"` present at line 1563 of `read.rs`. Re-exported from `lib.rs` line 37: `ContradictEdgeRow, EDGE_SOURCE_NLI, GraphCohesionMetrics, GraphEdgeRow, StatusAggregates`. PASS.

**ADR-002 — Two SQL queries**: Exactly two `fetch_one` calls on `read_pool()` within `compute_graph_cohesion_metrics()`. The "Query 2 drives from entries" formulation in the pseudocode is replaced with four scalar sub-queries inside a single `SELECT ... FROM` (no outer FROM clause), which is semantically equivalent and SQLite-valid. ADR-002 says "two round-trips to SQLite" — two round-trips confirmed. PASS.

**ADR-003 — read_pool() for both queries**: Both `fetch_one` calls use `self.read_pool()`. `write_pool_server()` does not appear anywhere in the `compute_graph_cohesion_metrics` function body (lines 1012–1139). The two occurrences of `write_pool_server` in `read.rs` (lines 1377, 1408) are in pre-existing functions `query_bootstrap_contradicts` and `get_content_via_write_pool`, unrelated to this feature. ADR-003 comment present at call site in `services/status.rs` line 679: `// ADR-003: read_pool() — WAL snapshot semantics intentional, bounded staleness accepted.` PASS.

**ADR-004 — Cross-category SQL with explicit LEFT JOIN and IS NOT NULL guards**: The cross-category sub-query at line 1078–1086 uses INNER JOINs (tighter than LEFT JOIN) to `src_e` and `tgt_e` with `AND src_e.status = 0` / `AND tgt_e.status = 0`, plus explicit `IS NOT NULL` guards on both categories before the inequality. The INNER JOIN approach is stricter than the ADR-004 LEFT JOIN + `ge.id IS NOT NULL` guard and produces identical results (edges to deprecated entries never match the INNER JOIN). The NULL guards on category are present. PASS.

**AC-15 — Single call site**: `compute_graph_cohesion_metrics` appears exactly once in production code — `services/status.rs` line 681, inside `compute_report()`. The 8 occurrences in `read.rs` are all within the `#[cfg(test)]` block. No call in `load_maintenance_snapshot()` or `maintenance_tick()`. PASS.

**AC-17 — write_pool_server() absent from function body**: Confirmed by code review. PASS.

**R-11 — ADR-003 comment at call site**: Present at `services/status.rs` line 679. PASS.

### Interface Implementation

**Status**: PASS

**GraphCohesionMetrics struct** (read.rs lines 1584–1598): Six fields with correct types — `connectivity_rate: f64`, `isolated_entry_count: u64`, `cross_category_edge_count: u64`, `supports_edge_count: u64`, `mean_entry_degree: f64`, `inferred_edge_count: u64`. Matches architecture specification exactly.

**StatusReport six new fields** (response/status.rs lines 70–86): All six appended after `graph_compacted: bool`, with correct types per FR-10 and AC-01.

**StatusReport::default()** (lines 151–157): All six fields explicitly listed with correct zero values — `graph_connectivity_rate: 0.0`, `isolated_entry_count: 0`, `cross_category_edge_count: 0`, `supports_edge_count: 0`, `mean_entry_degree: 0.0`, `inferred_edge_count: 0`. R-04 satisfied. Compile confirmed.

**Service call site** (services/status.rs lines 678–691): All six fields assigned in the `Ok(gcm)` arm. Error arm uses `tracing::warn!("graph cohesion metrics failed: {e}")` only — no early return, no error propagation. Non-fatal pattern matches architecture and Phase 4 co-access precedent.

**Format output — Summary** (response/status.rs lines 254–267): Conditional `if report.isolated_entry_count > 0 || report.cross_category_edge_count > 0 || report.inferred_edge_count > 0`. Format string: `"\nGraph cohesion: {:.1}% connected, {} isolated, {} cross-category, {} inferred"`. Matches pseudocode exactly.

**Format output — Markdown** (response/status.rs lines 475–508): `"\n#### Graph Cohesion\n"` always present inside `### Coherence` block. All six metric labels present: Connectivity, Isolated entries, Cross-category edges, Supports edges, Mean entry degree, Inferred (NLI) edges. Matches pseudocode. `#### ` (four hashes) used — correct per architecture (sub-section of Coherence).

**lib.rs re-export**: `EDGE_SOURCE_NLI` and `GraphCohesionMetrics` both appear on line 37 of `lib.rs`. R-09 satisfied.

### Test Case Alignment

**Status**: PASS

**Store layer tests** (8 tests, all in `read.rs` `#[cfg(test)]` block, all `#[tokio::test]`):

| Test | AC/Risk Coverage | Result |
|------|-----------------|--------|
| `test_graph_cohesion_all_isolated` | AC-02, AC-07, R-05 | pass |
| `test_graph_cohesion_all_connected` | AC-03, AC-06, AC-07, R-01 (chain) | pass |
| `test_graph_cohesion_mixed_connectivity` | AC-08, R-01, R-08 (deprecated endpoint) | pass |
| `test_graph_cohesion_cross_category` | AC-04, R-02 (NULL guard) | pass |
| `test_graph_cohesion_same_category_only` | AC-04 (same-category exclusion) | pass |
| `test_graph_cohesion_nli_source` | AC-05, R-03 (bootstrap NLI) | pass |
| `test_graph_cohesion_bootstrap_excluded` | AC-16, R-03 | pass |
| `test_graph_cohesion_empty_store` | R-05 (denominator=0) | pass |

**Test plan called for 7 mandatory tests (AC-13)**; implementation delivers 8, adding `test_graph_cohesion_empty_store` as the explicit denominator=0 case noted as "recommended additional" in the test plan. R-01 chain topology (entry B as both source and target) is present in `test_graph_cohesion_all_connected`.

**Server layer tests** (4 tests in `response/status.rs`):

| Test | AC/Risk Coverage | Result |
|------|-----------------|--------|
| `test_status_report_default_cohesion_fields` | AC-12, R-04 | pass |
| `test_format_summary_graph_cohesion_present` | AC-09, R-10 | pass |
| `test_format_summary_graph_cohesion_absent` | AC-09 (zero case) | pass |
| `test_format_markdown_graph_cohesion_section` | AC-10 | pass |

All test plan scenarios from `store-cohesion-query.md`, `status-report-fields.md`, and `format-output.md` have corresponding tests.

### Code Quality

**Status**: WARN (pre-existing file size only; no new quality issues)

**Compilation**: `cargo build --workspace` completes with zero errors. 12 warnings on `unimatrix-server` (pre-existing, not introduced by this feature).

**No stubs or placeholders**: Grep across modified files returns no `todo!()`, `unimplemented!()`, `TODO`, or `FIXME`.

**No `.unwrap()` in non-test code**: No `.unwrap()` calls introduced in `compute_graph_cohesion_metrics()` or the format/service additions.

**File size**: `read.rs` is 2046 lines. The 500-line rule flags this as WARN. This is pre-existing (architecture documented it as 1570 lines before this feature; the 500-line housekeeping concern is explicitly noted in both the architecture doc and the `EDGE_SOURCE_NLI` docstring at line 1560). The architecture states "splitting to `read_graph.rs` is out of scope for col-029." This is a known housekeeping item, not a blocking issue.

`status.rs` (response, 1292 lines) and `services/status.rs` (1786 lines) also exceed the 500-line rule but are pre-existing. No new excess introduced relative to scope.

**All tests pass**: Full workspace test run shows all 20 test suites passing with zero failures. Test counts include 2163 unit tests in `unimatrix-server`, 422 in `unimatrix-store`, and 152 in other crates.

### Security

**Status**: PASS

`compute_graph_cohesion_metrics()` is a parameterless read-only function. All SQL queries use literal predicates (`bootstrap_only = 0`, `status = 0`, `source = 'nli'`, `relation_type = 'Supports'`) — no user-supplied values interpolated into query strings. No injection surface. No writes. Per RISK-TEST-STRATEGY.md Security Risks section: no security risks identified for this feature.

### Knowledge Stewardship

**Status**: PASS

All three rust-dev agent reports contain `## Knowledge Stewardship` sections:

- **agent-3-store-cohesion-query**: Queried `/uni-query-patterns` (entries #3028, #2744, #2058). Attempted to store a novel pattern but lacked Write capability (anonymous agent). Pattern documented in report for retrospective. This is an environment constraint, not a process failure — the agent fulfilled the obligation.
- **agent-4-status-report-fields**: Queried `/uni-query-patterns` (entries #952, #298, #276, #307, #94). Stored entry #3603 "StatusReport struct literal locations" via `/uni-store-pattern`.
- **agent-5-service-call-site**: Queried `/uni-query-patterns` (ADR entries #3591–#3595). Stored: nothing novel, with explicit reason: "non-fatal match/warn pattern is the established Phase 4 co-access precedent already documented."

All three stewardship blocks include both `Queried:` entries (evidence of pre-implementation lookup) and `Stored:` entries or explicit "nothing novel — {reason}" explanations. No missing blocks.

---

## Rework Required

None.

---

## Notes

**Query 1 missing `COUNT(*) AS total_edges`**: The pseudocode specified Query 1 as `SELECT COUNT(*), SUM(supports), SUM(nli)` to provide `total_edges` for `mean_entry_degree`. The implementation omits the `COUNT(*)` from Query 1 and adds a fourth scalar sub-query to Query 2 for `active_active_edge_count` instead. This is semantically correct — the `total_edges` value in the pseudocode was intended for `mean_entry_degree` computation, and the implementation correctly substitutes the semantically tighter `active_active_edge_count` (which the agent determined via test failure is the right operand). The ADR-002 two-query cap is maintained.

**`cargo audit` not available in environment**: `cargo audit` is not installed in the worktree. No new dependencies were introduced (NFR-04 confirmed: no changes to `Cargo.toml`), so no new CVE surface exists.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the ADR deviation documentation pattern (where an implementation correctly deviates from pseudocode due to test-found bugs and documents the reasoning in the agent report) is already captured as a general delivery lesson. No new cross-feature pattern visible from col-029 gate 3b alone.
