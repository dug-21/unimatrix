# Gate 3c Report: col-029

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-26
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 10 active risks mapped to passing tests or verified static checks |
| Test coverage completeness | PASS | 8 store unit tests + 4 format/struct tests; all Risk-Test-Strategy scenarios exercised |
| Specification compliance | PASS | All 17 ACs verified; 2 implementation deviations documented and test-confirmed correct |
| Architecture compliance | PASS | ADR-001 through ADR-004 satisfied; single production call site; no tick path |
| Knowledge stewardship | PASS | Tester agent report has Queried and Stored/reason entries |
| Integration test gate | PASS | 20/20 smoke; 8/8 tools; 3/3 lifecycle; 2/2 edge_cases; no xfail added; no tests deleted |
| R-07 elimination | PASS | Correctly marked N/A in RISK-COVERAGE-REPORT; ADR-003 removed write-pool contention |
| R-11 WAL staleness | PASS | Accepted trade-off; ADR-003 comment present at services/status.rs:679 and read.rs:1005-1006 |

---

## Detailed Findings

### 1. Risk Mitigation Proof

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md maps all 10 active risks to test results or verified static checks.

| Risk ID | Mitigation | Verified |
|---------|-----------|---------|
| R-01 (double-count) | `test_graph_cohesion_all_connected` (chain A→B→C, B appears both sides) + `test_graph_cohesion_mixed_connectivity` | Code confirmed: UNION sub-query deduplicates IDs; `connectivity_rate <= 1.0` assertion present |
| R-02 (NULL guard deprecated endpoint) | `test_graph_cohesion_cross_category` — inserts active→deprecated edge, asserts `cross_category_edge_count = 1` (only active–active edge) | SQL uses INNER JOINs with `status = 0` on both `src_e` and `tgt_e`, naturally excluding deprecated endpoints |
| R-03 (bootstrap NLI leak) | `test_graph_cohesion_nli_source` + `test_graph_cohesion_bootstrap_excluded` — both assert `inferred_edge_count` excludes `bootstrap_only=1` | Confirmed; the `bootstrap_excluded` test inserts `source='nli', bootstrap_only=1` edge and asserts `inferred_edge_count = 0` |
| R-04 (missing default field) | `test_status_report_default_cohesion_fields` asserts all six default values | Compile check passed; all six fields listed at `response/status.rs` lines 151-157 |
| R-05 (division by zero) | `test_graph_cohesion_all_isolated` (no edges) + `test_graph_cohesion_empty_store` (no active entries) | `!is_nan()` and `!is_infinite()` assertions present; guards at `read.rs:1115-1125` |
| R-06 (tick call) | Static grep: single production call at `services/status.rs:681` | `grep -rn compute_graph_cohesion_metrics crates/` returns 1 production hit; none in `load_maintenance_snapshot()` or `maintenance_tick()` |
| R-07 | Eliminated (ADR-003) | Correctly marked N/A in RISK-COVERAGE-REPORT |
| R-08 (HashSet logic error) | `test_graph_cohesion_mixed_connectivity` — active→deprecated edge does not count active entry as connected | Implementation uses UNION with INNER JOIN on both endpoints (not HashSet); both endpoints must be `status=0` before IDs are collected |
| R-09 (EDGE_SOURCE_NLI re-export) | `lib.rs:37` contains `pub use` including `EDGE_SOURCE_NLI` | Confirmed; `unimatrix_store::EDGE_SOURCE_NLI` importable from server crate |
| R-10 (summary omission on empty store) | `test_format_summary_graph_cohesion_absent` (zero suppression) + `test_format_summary_graph_cohesion_present` (non-zero line present) | Both tests pass |
| R-11 (WAL staleness) | Design mitigation: ADR-003 comment at `services/status.rs:679` and `read.rs:1005-1006` | No runtime test required per RISK-TEST-STRATEGY; comment confirmed at both sites |

### 2. Test Coverage Completeness

**Status**: PASS

**Store unit tests** (all in `read.rs #[cfg(test)]`; all `#[tokio::test]`; all PASS):

| Test | Risk/AC Coverage |
|------|-----------------|
| `test_graph_cohesion_all_isolated` | AC-02, AC-07, R-05 — no edges, NaN guards |
| `test_graph_cohesion_all_connected` | AC-03, AC-06, AC-07, R-01 — chain topology dedup |
| `test_graph_cohesion_mixed_connectivity` | AC-08, R-01, R-08 — deprecated endpoint exclusion |
| `test_graph_cohesion_cross_category` | AC-04, R-02 — NULL guard + deprecated target |
| `test_graph_cohesion_same_category_only` | AC-04 — zero cross-category |
| `test_graph_cohesion_nli_source` | AC-05, R-03 — bootstrap NLI excluded from inferred count |
| `test_graph_cohesion_bootstrap_excluded` | AC-16, R-03 — all bootstrap_only=1 edges give zero metrics |
| `test_graph_cohesion_empty_store` *(additional)* | R-05 — zero active entries, denominator guards |

The seven mandatory tests (AC-13) are present. The eighth is an additional edge case (empty store with zero active entries vs. all-isolated with active entries but no edges) that strengthens R-05 coverage.

**Server unit tests** (all in `response/status.rs`; all PASS):

| Test | AC/Risk Coverage |
|------|-----------------|
| `test_status_report_default_cohesion_fields` | AC-12, R-04 |
| `test_format_summary_graph_cohesion_present` | AC-09, R-10 |
| `test_format_summary_graph_cohesion_absent` | R-10 zero suppression |
| `test_format_markdown_graph_cohesion_section` | AC-10 — all six labels + placement after `### Coherence` |

**Risk-to-scenario completeness**: All 27 required scenarios from RISK-TEST-STRATEGY.md are addressed. Critical (R-01, 3 scenarios), High (R-02, R-03, R-05, 9 scenarios), Medium (R-04, R-06, R-08, R-09, compile/static checks), Low (R-10, R-11 design mitigation).

**Integration tests**: 20/20 smoke PASS; 8/8 tools PASS; 3/3 lifecycle PASS; 2/2 edge_cases PASS. No xfail markers added by col-029 (the three existing xfail markers — GH#405, GH#305, GH#111 — are pre-existing). No integration tests deleted or commented out. RISK-COVERAGE-REPORT includes integration counts (line 73: "33 unique tests across suites").

### 3. Specification Compliance

**Status**: PASS

All 17 ACs verified. Key verifications:

**AC-01** (six fields in StatusReport): All six field names present in struct definition, `default()`, `StatusReportJson`, and format functions at `response/status.rs` lines 70-86, 151-157.

**AC-02 / AC-03** (isolated and connected cases): Both test functions include explicit `mean_entry_degree` assertions (AC-07). Chain topology in `all_connected` (A→B→C) confirms the UNION dedup resolves R-01.

**AC-05 / AC-16** (bootstrap_only=1 exclusion): `test_graph_cohesion_nli_source` inserts one `bootstrap_only=0` and one `bootstrap_only=1` NLI edge, asserts `inferred_edge_count = 1`. `test_graph_cohesion_bootstrap_excluded` inserts only `bootstrap_only=1` edges including NLI-sourced, asserts all metrics zero.

**AC-08** (deprecated entries excluded): `test_graph_cohesion_mixed_connectivity` inserts entry C→deprecated_entry edge; asserts `connectivity_rate = 0.5` (only A and B count as connected).

**AC-09** (summary format): `test_format_summary_graph_cohesion_present` asserts "Graph cohesion:", "75.0%", "5 cross-category", "4 inferred" appear.

**AC-10** (markdown sub-section): `test_format_markdown_graph_cohesion_section` asserts `#### Graph Cohesion`, all six labels, and placement after `### Coherence`.

**AC-15** (single call site, not in tick): Static grep confirms one production call at `services/status.rs:681`; none in `load_maintenance_snapshot()` or `maintenance_tick()`.

**Noted deviation — mean_entry_degree formula**: The spec FR-08 specifies `(2 * non_bootstrap_edge_count) / active_entry_count` using all non-bootstrap edges. The implementation uses `active_active_edge_count` (edges where both endpoints are active, from Query 2). This is a documented improvement: edges to deprecated entries would inflate active-endpoint degree under the raw formula, misrepresenting PPR-relevant graph connectivity. The gate-3b report accepted this deviation; unit tests verify the corrected semantics. Not a compliance issue.

**NFR-01** (no tick caching): Confirmed by AC-15 static check. `MaintenanceDataSnapshot` unchanged.

**NFR-02** (two-query maximum): Two `fetch_one` calls on `read_pool()` — Query 1 (supports + inferred counts), Query 2 (four scalar sub-queries in a single `SELECT`). Two round-trips total.

**NFR-03** (no schema migration): No `Cargo.toml` changes; no new tables/columns/indexes. Confirmed.

**NFR-06** (no lambda change): Lambda, `graph_quality_score`, and four coherence dimension scores are untouched.

**NFR-07** (read_pool only): Both queries use `self.read_pool()` at lines 1025 and 1095. `write_pool_server()` absent from `compute_graph_cohesion_metrics()` body.

### 4. Architecture Compliance

**Status**: PASS

**ADR-001** (`EDGE_SOURCE_NLI` constant): `pub const EDGE_SOURCE_NLI: &str = "nli"` at `read.rs:1563`. Re-exported from `lib.rs:37`. Docstring at line 1555-1563 explains SR-01 rationale.

**ADR-002** (two SQL queries): Exactly two `fetch_one` calls within `compute_graph_cohesion_metrics`. The four scalar sub-queries in Query 2 are within a single SQL statement — one round-trip.

**ADR-003** (`read_pool()` for both queries): Confirmed at lines 1025 and 1095 of `read.rs`. ADR-003 comment present at `services/status.rs:679` ("WAL snapshot semantics intentional, bounded staleness accepted") and at `read.rs:1005-1006` (docstring). R-07 is correctly marked as eliminated in RISK-COVERAGE-REPORT.

**ADR-004** (cross-category SQL no cartesian product): Implementation uses INNER JOIN to `src_e` and `tgt_e` with `status = 0` predicates. This is stricter than the ADR-004 LEFT JOIN approach but produces equivalent results (edges to deprecated endpoints never match). `IS NOT NULL` guards on both categories present at `read.rs:1083-1085`.

**Component structure**: Three-layer implementation matches architecture — Store function (`read.rs`), StatusReport fields (`response/status.rs`), service call site (`services/status.rs`). Phase 5 placement confirmed.

**R-11 call site comment**: `services/status.rs:679` contains `// ADR-003: read_pool() — WAL snapshot semantics intentional, bounded staleness accepted.` This satisfies R-11's requirement per RISK-TEST-STRATEGY (no runtime test required; design mitigation only).

### 5. Knowledge Stewardship

**Status**: PASS

Tester agent report at `product/features/col-029/agents/col-029-agent-6-tester-report.md` contains a `## Knowledge Stewardship` section with:
- Queried: `/uni-knowledge-search` for testing procedures — found #3600 (pitfall: `create_graph_edges_table()` for pre-v13 schema only)
- Stored: "nothing novel to store" with explicit reason: "patterns already captured in #3600, #1043, #1044"

The implementation correctly avoided calling `create_graph_edges_table()` in the new cohesion tests, confirming the query had operational effect.

---

## Rework Required

None.

---

## Scope Concerns

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store — this gate found no recurring cross-feature failure pattern (all checks passed cleanly; the documented implementation deviations were caught and corrected at implementation time). Feature-specific gate results live in this report, not Unimatrix.
