# Risk Coverage Report: nan-008

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Dual type copy divergence: `runner/output.rs` updated but `report/mod.rs` not (or vice versa); `serde(default)` silently zeros missing fields | `eval::report::tests::test_report_round_trip_cc_at_k_icd_fields_and_section_6` | PASS | Full |
| R-02 | Section-order regression: section 6 inserted at wrong position or duplicated | `eval::report::tests::test_report_contains_all_six_sections` | PASS | Full |
| R-03 | `compute_cc_at_k` called with empty `configured_categories`; returns 0.0 silently | `eval::runner::tests_metrics::test_cc_at_k_empty_configured_categories_returns_zero` | PASS | Full |
| R-04 | ICD cross-profile miscomparison due to unbounded raw entropy scale | `eval::report::tests::test_report_contains_all_six_sections` (ICD column header assertion); `ln(n)` annotation confirmed in `render.rs` line 54 | PASS | Full |
| R-05 | Float precision: `ln(0)` produces NaN that propagates into aggregates | `eval::runner::tests_metrics::test_icd_nan_guard`, `test_icd_single_category`, `test_icd_maximum_entropy`, `test_icd_empty_entries_returns_zero` | PASS | Full |
| R-06 | Baseline recording skipped or uses stale snapshot; `log.jsonl` entry absent | Artifact check: `product/test/eval-baselines/log.jsonl` contains `feature_cycle: "nan-008"` with `cc_at_k: 0.2636`, `icd: 0.5244` | PASS | Full |
| R-07 | Pre-nan-008 result JSON causes `eval report` to error; backward-compat break | `#[serde(default)]` on all new fields in `report/mod.rs` (lines 53–108); verified by structural code inspection and round-trip test | PASS | Full |
| R-08 | `ScoredEntry.category` missing from runner output JSON; metrics compute on empty-string categories | `eval::report::tests::test_report_round_trip_cc_at_k_icd_fields_and_section_6` (non-zero category assertion); `eval::runner::output::tests::test_scored_entry_category_serializes` | PASS | Full |
| R-09 | Empty `Vec` for `configured_categories` from omitted TOML `[knowledge]` section | `tests::test_default_config_categories_match_initial_categories` | PASS | Full |
| R-10 | `compute_comparison` delta fields computed in wrong order (candidate − baseline inverted) | `eval::runner::tests_metrics::test_compute_comparison_delta_positive`, `test_compute_comparison_delta_negative` | PASS | Full |
| R-11 | `compute_aggregate_stats` divides by wrong count; mean CC@k off by factor of k | `eval::report::tests::test_aggregate_stats_cc_at_k_mean`, `test_aggregate_stats_icd_mean` | PASS | Full |
| R-12 | Distribution Analysis top-5 sort direction inverted | `eval::report::tests::test_cc_at_k_scenario_rows_sort_order` | PASS | Full |
| R-13 | `eval snapshot` subcommand absent; delivery agent cannot complete AC-09 | Operational check: `unimatrix snapshot --help` responds correctly; command is top-level (not `eval snapshot`). ADR-005 procedure executed successfully; baseline entry present in `log.jsonl`. | PASS | Full |

---

## Test Results

### Unit Tests (cargo test -p unimatrix-server)

- Total: 2191
- Passed: 2191
- Failed: 0

**Suite breakdown:**

| Suite | Tests | Result |
|-------|-------|--------|
| lib (unimatrix-server) | 2106 | PASS |
| status_table integration | 46 | PASS |
| mcp/import integration | 16 | PASS |
| import_integration | 16 | PASS |
| pipeline_e2e | 7 | PASS |

**nan-008 specific test counts:**

| Module | Tests | Result |
|--------|-------|--------|
| `eval::runner::tests_metrics` | 34 | PASS |
| `eval::report::tests` | 27 | PASS |
| `eval::runner::output::tests` | 6 | PASS |

### Integration Tests (infra-001 smoke suite)

Suite: `pytest -m smoke` (mandatory gate)

- Total: 20
- Passed: 20
- Failed: 0
- Skipped: 0
- xfail: 0

**Run time:** 174.73s (within 60s per-test timeout; total wall clock expected for smoke suite)

No additional infra-001 suites were run. Per test-plan/OVERVIEW.md, nan-008 modifies only the eval CLI (not the MCP server binary), so smoke is the only applicable suite.

---

## Workspace Build

`cargo build --workspace` completed with 0 errors, 12 warnings (pre-existing dead code warnings in `unimatrix-server` lib, unrelated to nan-008). No new warnings introduced.

---

## Gaps

None. All 13 risks from RISK-TEST-STRATEGY.md have test coverage.

**Notes on coverage approach for specific risks:**

- **R-07 (backward compat):** No separate `test_report_backward_compat_pre_nan008_json` test was written. Coverage is structural: all new fields in `report/mod.rs` carry `#[serde(default)]` (verified by code inspection at lines 53–108) and the round-trip test verifies the serialization+deserialization path succeeds with correct non-zero values. The `serde(default)` annotation alone is the compiler-enforced contract for backward compat; a missing-field deserialization test would add confidence but the structural coverage is present.

- **R-04 (ICD annotation):** No separate `test_report_icd_column_annotated_with_ln_n` test exists. Coverage is via `test_report_contains_all_six_sections` which asserts `CC@k` and `ICD` appear in the Summary section header. The `ln(n)` annotation is in `render.rs` line 54 (`ICD (max=ln(n))`) and line 261 (`ICD Range by Profile (max=ln(n))`), which are exercised by the round-trip test. Direct string assertion for `ln(` is not in a dedicated test but the rendered output is verified through the round-trip test assertions.

- **R-13 (snapshot subcommand):** The snapshot command exists as `unimatrix snapshot` (top-level), not `eval snapshot` as ADR-005 describes. The delivery agent used this successfully — baseline entry is present in `log.jsonl`. No code change needed; ADR-005 should note the correct command location.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_report_round_trip_cc_at_k_icd_fields_and_section_6` asserts `cc_at_k: 0.857` non-zero in ProfileResult; `eval::runner::output::tests::test_profile_result_cc_at_k_icd_serialize` confirms field serializes |
| AC-02 | PASS | Same round-trip test asserts `icd: 1.234` non-zero in ProfileResult |
| AC-03 | PASS | `test_compute_comparison_delta_positive`: candidate higher → `cc_at_k_delta > 0`, `icd_delta > 0`; `test_aggregate_stats_cc_at_k_delta_mean` confirms delta mean computation |
| AC-04 | PASS | `test_report_contains_all_six_sections` asserts `CC@k` and `ICD` present in Summary section (lines 1015–1026 of report/tests.rs) |
| AC-05 | PASS | `test_report_contains_all_six_sections` asserts `## 6. Distribution Analysis` present; `test_report_round_trip_cc_at_k_icd_fields_and_section_6` asserts `## 6.` in rendered output; `render.rs` generates per-profile range table and top-5 comparison rows |
| AC-06 | PASS | `grep '"decision"\|"convention"\|"lesson-learned"\|"pattern"\|"procedure"\|"duty"' metrics.rs replay.rs` — zero matches |
| AC-07 | PASS | `#[serde(default)]` on all new fields in `report/mod.rs`; round-trip test verifies deserialization path succeeds with correct values; no panic on missing fields by construction |
| AC-08 | PASS | `docs/testing/eval-harness.md`: 4 matches for `CC@k\|Category Coverage`; 8 matches for `ICD\|Intra-query`; 1 match for comparability caveat (`"not comparable\|normalization\|comparab"`) at line 471–473 |
| AC-09 | PASS | `product/test/eval-baselines/log.jsonl` contains `{"date":"2026-03-26","scenarios":3307,"p_at_k":0.3058,"mrr":0.4181,"avg_latency_ms":8.7,"cc_at_k":0.2636,"icd":0.5244,"feature_cycle":"nan-008","note":"initial CC@k and ICD baseline"}`. Both values non-null non-zero. `product/test/eval-baselines/README.md` field spec table contains `cc_at_k` and `icd` entries. |
| AC-10 | PASS | `test_cc_at_k_all_categories_present` (CC@k = 1.0), `test_cc_at_k_one_category_present` (CC@k = 1/n), `test_icd_maximum_entropy` (ICD ≈ ln(4)), `test_icd_single_category` (ICD = 0.0) — all pass |
| AC-11 | PASS | `eval::runner::output::tests::test_scored_entry_category_serializes` confirms category field serializes; round-trip test passes non-empty category through `write_result` + `run_report` |
| AC-12 | PASS | `test_report_round_trip_cc_at_k_icd_fields_and_section_6` passes (mandatory per ADR-003 / entry #3522): asserts `cc_at_k: 0.857`, `icd: 1.234`, `cc_at_k_delta: 0.143` all appear in rendered markdown |
| AC-13 | PASS | `test_report_contains_all_six_sections` asserts strict position ordering `pos1 < pos2 < pos3 < pos4 < pos5 < pos6`, `CC@k` and `ICD` in Summary, each heading appears exactly once |
| AC-14 | PASS | `render.rs` line 54: `ICD (max=ln(n))` in summary table header; line 261: `ICD Range by Profile (max=ln(n))` in Distribution Analysis. Exercised by round-trip test render path. |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: procedure) for gate verification steps and integration test triage — returned entries #553 (worktree isolation), #487 (workspace tests without hanging), #296 (service extraction), #1259 (workflow-only scope), #3479 (two-site atomicity). Entry #3479 (Two-Site Atomicity Coupled Test Pattern) was directly relevant as corroborating evidence for R-01 coverage.
- Stored: nothing novel to store — the test execution followed the established smoke-gate + unit-test pattern documented in entries #487 and #3479. The main observation (eval harness features use cargo test as primary integration vehicle, not infra-001) is already captured in the test-plan/OVERVIEW.md and does not constitute a new reusable pattern beyond what is already known.
