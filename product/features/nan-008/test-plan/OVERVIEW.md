# nan-008 Test Strategy Overview

## Feature Summary

nan-008 adds CC@k (Category Coverage at k) and ICD (Intra-query Category Diversity)
to the eval harness runner output, eval report markdown, documentation, and baseline log.
All changes are additive. The primary risk is the dual type copy architecture
(`runner/output.rs` and `report/mod.rs` share no compile-time link), which means a
missing field produces silent zero-valued metrics rather than a compile error.

---

## Test Strategy by Layer

### Unit Tests (`cargo test --workspace`)

Pure functions in `runner/metrics.rs` are fully testable without I/O or database access.
Tests live in `crates/unimatrix-server/src/eval/runner/tests_metrics.rs` (extended) and
`crates/unimatrix-server/src/eval/report/tests.rs` (extended).

Focus areas:
- Metric boundary values: CC@k at 0.0, 1/n, 1.0; ICD at 0.0 and ln(n)
- Float safety: NaN guard in `compute_icd`, empty-slice guards in both functions
- Delta sign: `compute_comparison` with known baseline/candidate values
- Aggregate accumulation: `compute_aggregate_stats` with known scenario counts
- Sort order: `compute_cc_at_k_scenario_rows` descending by delta
- Backward compat: deserialization of JSON missing all new fields defaults to 0.0
- Round-trip: end-to-end serialization through `run_report` with non-zero values

### Integration Tests (report/tests.rs round-trip)

The most important integration test is the round-trip in `report/tests.rs`:
`test_report_round_trip_cc_at_k_icd_fields_and_section_6`. It exercises both the
runner serialization and report deserialization paths without a running binary, and
asserts non-zero values appear in the rendered markdown. It is the primary guard
against R-01 (dual type copy divergence) and R-02 (section-order regression).

### infra-001 Integration Harness

nan-008 modifies the eval harness, not the MCP server binary or any tool logic.
The infra-001 harness tests the `unimatrix-server` binary through MCP JSON-RPC.
The eval harness is a separate CLI command (`eval run`, `eval report`) with no
MCP interface. Therefore the infra-001 harness is NOT the primary integration
vehicle for this feature.

**Minimum gate:** `pytest -m smoke` must pass to confirm the MCP server binary
is unbroken by any compilation-time side effects of the eval changes.

**No additional infra-001 suites are needed.** The eval runner and report modules
are tested by `cargo test`, not by MCP JSON-RPC.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Test Location | Test Name(s) |
|---------|----------|---------------|--------------|
| R-01 | Critical | report/tests.rs | `test_report_round_trip_cc_at_k_icd_fields_and_section_6` |
| R-02 | High | report/tests.rs | `test_report_contains_all_six_sections` |
| R-03 | High | tests_metrics.rs | `test_cc_at_k_empty_configured_categories_returns_zero` |
| R-04 | High | report/tests.rs | `test_report_icd_column_annotated_with_ln_n` |
| R-05 | High | tests_metrics.rs | `test_icd_single_category`, `test_icd_maximum_entropy`, `test_icd_empty_entries_returns_zero`, `test_icd_two_entries_one_category_each` |
| R-06 | High | Manual/artifact | Parse `log.jsonl` for `feature_cycle: "nan-008"` with non-zero values |
| R-07 | High | report/tests.rs | `test_report_backward_compat_pre_nan008_json` |
| R-08 | High | report/tests.rs | `test_report_round_trip_cc_at_k_icd_fields_and_section_6` (non-empty category assertion) |
| R-09 | Med | tests_metrics.rs (or config test) | `test_knowledge_config_default_populates_initial_categories` |
| R-10 | Med | tests_metrics.rs | `test_compute_comparison_delta_positive`, `test_compute_comparison_delta_negative` |
| R-11 | Med | report/tests.rs | `test_aggregate_stats_cc_at_k_mean`, `test_aggregate_stats_icd_mean` |
| R-12 | Med | report/tests.rs | `test_cc_at_k_scenario_rows_sort_order` |
| R-13 | Low | Manual/operational | Run `eval --help`; confirm snapshot subcommand; no code test |

---

## Cross-Component Test Dependencies

```
tests_metrics.rs
  imports: runner/metrics.rs (compute_cc_at_k, compute_icd)
  imports: runner/output.rs  (ScoredEntry with new category field)

report/tests.rs
  imports: report/mod.rs (run_report, ScenarioResult, ProfileResult, ComparisonMetrics, ScoredEntry, default_comparison)
  imports: report/aggregate.rs (compute_aggregate_stats, compute_cc_at_k_scenario_rows)
  uses:    tempfile::TempDir (already in dev-deps)
  uses:    serde_json (already in dev-deps)
```

The `report/tests.rs` tests exercise the JSON deserialization boundary between runner
and report, which is the Integration Surface for R-01. These tests MUST write real
JSON to disk and call `run_report` to cover the full path.

---

## Acceptance Criteria Coverage

| AC-ID | Test(s) | Verification Type |
|-------|---------|-------------------|
| AC-01 | `test_report_round_trip_cc_at_k_icd_fields_and_section_6` | Unit/integration |
| AC-02 | `test_report_round_trip_cc_at_k_icd_fields_and_section_6` | Unit/integration |
| AC-03 | `test_compute_comparison_delta_positive` | Unit |
| AC-04 | `test_report_summary_table_has_cc_at_k_and_icd_columns` | Unit/integration |
| AC-05 | `test_report_contains_all_six_sections` | Unit/integration |
| AC-06 | grep check (no hardcoded category strings) | Static |
| AC-07 | `test_report_backward_compat_pre_nan008_json` | Unit/integration |
| AC-08 | grep check on docs/testing/eval-harness.md | Static |
| AC-09 | log.jsonl artifact parse | Manual delivery step |
| AC-10 | `test_cc_at_k_all_categories_present`, `test_cc_at_k_one_category_present`, `test_icd_maximum_entropy`, `test_icd_single_category` | Unit |
| AC-11 | `test_report_round_trip_cc_at_k_icd_fields_and_section_6` (category assertion) | Unit/integration |
| AC-12 | `test_report_round_trip_cc_at_k_icd_fields_and_section_6` | Unit/integration |
| AC-13 | `test_report_contains_all_six_sections` | Unit/integration |
| AC-14 | `test_report_icd_column_annotated_with_ln_n` | Unit/integration |

---

## Integration Harness Plan (infra-001)

### Suite Selection

nan-008 modifies no MCP tool logic, no server binary behavior, no storage schema,
and no confidence or contradiction logic. It extends the eval CLI commands only.

| Suite | Run? | Reason |
|-------|------|--------|
| smoke | YES — mandatory gate | Confirms binary compiles and MCP server still functions |
| tools | No | No tool logic changes |
| protocol | No | No protocol changes |
| lifecycle | No | No store/retrieval changes |
| confidence | No | No confidence system changes |
| contradiction | No | No contradiction logic changes |
| security | No | No security boundary changes |
| volume | No | No schema changes |
| edge_cases | No | No MCP edge case changes |

**Run command (Stage 3c):**
```bash
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60
```

### New Integration Tests Needed

No new infra-001 integration tests are needed. The eval harness is exercised through
`cargo test` (unit + in-process integration tests in `report/tests.rs`). The MCP
binary interface is unchanged.

**Rationale:** The features tested here (CC@k formula, ICD formula, section 6 rendering,
dual-copy deserialization) are all reachable via `run_report` and `compute_cc_at_k` /
`compute_icd` direct function calls. There is no new MCP tool or MCP-visible behavior
to validate through JSON-RPC.

### Existing Coverage

The `tools` suite includes tests for eval-adjacent behavior (e.g., `context_search`
used by the eval runner). Those existing tests provide sufficient coverage for the
MCP layer; no extension is needed for nan-008.

---

## Execution Order

1. `cargo test --workspace 2>&1 | tail -30` — verify all unit and in-process tests pass
2. `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60` — mandatory gate
3. Manual artifact checks:
   - AC-06: grep for hardcoded category strings in metrics.rs and replay.rs
   - AC-08: grep for CC@k and ICD subsections in eval-harness.md
   - AC-09: parse log.jsonl for nan-008 baseline entry
   - R-13: run `eval --help` to confirm snapshot subcommand
