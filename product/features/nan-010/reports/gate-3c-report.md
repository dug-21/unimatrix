# Gate 3c — RISK-COVERAGE-REPORT (nan-010)

## Verdict: PASS

---

## Test Execution Summary

| Suite | Run | Passed | Failed |
|-------|-----|--------|--------|
| `cargo test -p unimatrix-server` (full) | 2,183 | 2,183 | 0 |
| `eval::profile::tests` (targeted) | 27 | 27 | 0 |
| `eval::report::tests_distribution_gate` (targeted) | 13 | 13 | 0 |
| `eval::report::render_distribution_gate::tests` (targeted) | 5 | 5 | 0 |
| infra-001 smoke (`-m smoke`) | 20 | 20 | 0 |

**Total unit tests:** 2,183 passed, 0 failed.
**Total integration smoke tests:** 20 passed, 0 failed.

---

## Risk Coverage

| Risk ID | Priority | Tests Run | Outcome |
|---------|----------|-----------|---------|
| R-01 | Critical | Gate-3b static verified (pre-split step; `render_distribution_gate.rs` and `aggregate/distribution.rs` boundary modules exist; compile succeeds) | PASS |
| R-02 | High | `test_parse_distribution_change_profile_valid`, `test_parse_distribution_change_missing_targets` | PASS |
| R-03 | High | `test_distribution_gate_baseline_rejected` | PASS |
| R-04 | High | `test_write_profile_meta_schema` (atomic write + no orphan `.tmp`), `test_write_profile_meta_schema_tmp_not_read_as_sidecar` | PASS |
| R-05 | High | `test_check_distribution_targets_all_pass`, `test_check_distribution_targets_cc_at_k_fail`, `test_check_distribution_targets_icd_fail`, `test_check_distribution_targets_mrr_floor_fail`, `test_distribution_gate_mrr_floor_veto` | PASS |
| R-06 | High | `test_distribution_gate_distinct_failure_modes` (all three failure mode combinations verified) | PASS |
| R-07 | High | `test_distribution_gate_corrupt_sidecar_aborts` (error message contains "profile-meta.json is malformed" and "re-run eval to regenerate") | PASS |
| R-08 | Med | `test_check_distribution_targets_all_pass` (asserts `cc_at_k.actual == 0.65`, `icd.actual == 1.35`, `mrr_floor.actual == 0.50` against fixture — field read path validated) | PASS |
| R-09 | Med | `test_distribution_gate_section_header` (single-profile `## 5.` and multi-profile `### 5.N` both verified) | PASS |
| R-10 | Med | `test_write_profile_meta_schema` (hand-crafted JSON parsed bidirectionally; round-trip serde fidelity confirmed) | PASS |
| R-11 | High | Gate-3b verified: `eval/profile/tests.rs` and `eval/report/tests_distribution_gate.rs` both exist with required test functions present | PASS |
| R-12 | Med | `test_distribution_gate_exit_code_zero` (`run_report` returns `Ok(())` even when regressions are present; no distribution-gate abort path wired to exit) | PASS |
| R-13 | Med | `test_distribution_gate_table_content` (asserts rendered output does not contain "Regressions"; `render_distribution_gate_section` signature does not accept a regressions parameter) | PASS |
| R-14 | Med | `test_check_distribution_targets_mrr_floor_fail` (candidate `mean_mrr = 0.28`, `mrr_floor.actual == 0.28` asserted; if baseline MRR 0.60 were used incorrectly, test would fail) | PASS |
| R-15 | High | `test_report_without_profile_meta_json` (pre-nan-010 `ScenarioResult` JSON without new fields deserializes correctly; zero new fields added to `ScenarioResult`) | PASS |

---

## Test Results

### Unit Tests

- Total: 2,183
- Passed: 2,183
- Failed: 0
- Run: `cargo test -p unimatrix-server 2>&1`

### Integration Tests (infra-001 smoke)

- Total: 20
- Passed: 20
- Failed: 0
- Run: `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60`

### Additional Integration Suite Assessment

nan-010 introduces no new MCP tools and makes no changes to the MCP protocol layer, tool dispatch, or storage schema. The affected code paths are entirely within `eval/` (profile parsing and report rendering), which operate outside the MCP request/response lifecycle. Per the suite selection table, the smoke gate is the appropriate minimum; no additional infra-001 suites are required.

---

## Gaps

None. Every risk in the RISK-TEST-STRATEGY.md has test coverage:

- R-01 (pre-split order): static/structural coverage confirmed at gate-3b; boundary modules compile cleanly.
- All High and Critical risks have dedicated named tests in the mandatory test files.
- All Med risks are covered by the render and aggregation tests.

AC-12 (documentation) is the only manually-verified criterion. The `docs/testing/eval-harness.md` file covers:
- The `distribution_change` flag (lines 405–432, 491–515)
- The `[profile.distribution_targets]` sub-table (lines 409–422)
- Distribution Gate Section 5 behavior (lines 491–521)
- Example TOML for PPR-class features (lines 412–422, syntactically valid)
- Safety constraints table entry (line 781)
- The "Baseline MRR (reference)" informational row is documented in the render section (line 101 of `render_distribution_gate.rs`)

One minor documentation gap: AC-12 calls for "guidance on choosing `cc_at_k_min`/`icd_min`/`mrr_floor` values" and specifically "guidance section references baseline MRR as a calibration reference." The doc provides inline comments on the TOML fields and mentions the reference row in Section 5 behavior, but there is no dedicated guidance subsection on calibration methodology. This is a low-severity documentation gap that does not block the feature gate — all code acceptance criteria pass.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_parse_distribution_change_profile_valid` — round-trip parse confirms `distribution_change = true` and all three target values |
| AC-02 | PASS | `test_parse_distribution_change_missing_targets` — error message contains "distribution_targets" |
| AC-03 | PASS | `test_parse_distribution_change_missing_cc_at_k`, `test_parse_distribution_change_missing_icd`, `test_parse_distribution_change_missing_mrr_floor` — each names the missing field |
| AC-04 | PASS | `test_parse_no_distribution_change_flag` — `distribution_change = false`, `distribution_targets = None` for standard profile |
| AC-05 | PASS | `test_write_profile_meta_schema` — JSON written, schema validated (`version = 1`, correct profiles map), no orphan `.tmp`, bidirectional serde verified |
| AC-06 | PASS | `test_report_without_profile_meta_json` — absent sidecar → "Zero-Regression Check" in report, no "Distribution Gate" text |
| AC-07 | PASS | `test_distribution_gate_section_header` — heading "## 5. Distribution Gate" and "Distribution change declared. Evaluating against CC@k and ICD targets." both present |
| AC-08 | PASS | `test_distribution_gate_table_content` — CC@k table, ICD table, MRR floor table, "Baseline MRR (reference)" row with em-dash, 4dp formatting all verified |
| AC-09 | PASS | `test_distribution_gate_pass_condition` (all PASSED), `test_distribution_gate_mrr_floor_veto` (diversity PASSED, MRR floor FAILED, overall FAILED) |
| AC-10 | PASS | `test_distribution_gate_distinct_failure_modes` — Case A "Diversity targets not met" only, Case B "ranking floor breached" only, Case C both messages present |
| AC-11 | PASS | `test_report_without_profile_meta_json` — absent sidecar returns `Ok(empty map)`, report renders "Zero-Regression Check", exits 0 |
| AC-12 | PARTIAL | `docs/testing/eval-harness.md` covers five of the six required items. Missing: dedicated calibration guidance subsection for `mrr_floor` calibration using baseline MRR. All other items present. Does not block gate. |
| AC-13 | PASS | `test_check_distribution_targets_all_pass`, `_cc_at_k_fail`, `_icd_fail`, `_mrr_floor_fail` — per-metric assertions on `MetricGateRow.actual` values confirm correct field reads |
| AC-14 | PASS | `test_report_without_profile_meta_json` — pre-nan-010 `ScenarioResult` JSON deserializes without error; zero new fields added to `ScenarioResult` |

---

## Notes

- The `eval::report::tests_distribution_gate` module contains 13 tests covering Components 3, 4, and 7 (sidecar write, aggregation gate, report sidecar load).
- The `eval::report::render_distribution_gate::tests` module contains 5 tests covering the Component 6 renderer.
- The `eval::profile::tests` module contains 27 tests (10 existing + 5 nan-010 additions for AC-01 through AC-04).
- No xfail markers were applied. No pre-existing test failures were encountered.
- infra-001 smoke ran in 174.68s (20 tests). No failures, no pre-existing issues.
- AC-12 documentation gap does not block delivery: all functional acceptance criteria and all High/Critical risk tests pass.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "gate verification steps integration test triage" — found entries #553, #487, #296, #1259, #3479. Entry #487 ("How to run workspace tests without hanging") confirmed the `tail -5` / `tail -30` pattern already in use. No new procedure emerged.
- Stored: nothing novel to store — the test execution followed established patterns from RISK-TEST-STRATEGY.md. The targeted test filter approach (`cargo test -p unimatrix-server tests_distribution_gate`) and the three-suite split (profile, gate, render) are documented in the test plan. No cross-feature pattern emerged.
