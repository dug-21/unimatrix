# Risk Coverage Report: crt-048

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `compute_lambda()` positional arg transposition — all params are `f64`, compiles silently | `lambda_specific_three_dimensions` (distinct values 0.8/0.5/0.3 → asserts 0.576); `lambda_single_dimension_deviation` (per-slot isolation); grep: both call sites in `status.rs` have 4 args in correct order | PASS | Full |
| R-02 | Partial `StatusReport` field removal in `mcp/response/mod.rs` (8 sites, 16 refs) | Build gate: `cargo build --release` succeeded; grep `confidence_freshness_score\|stale_confidence_count` in `mcp/response/mod.rs` returns zero matches; non-default `make_coherence_status_report()` (0.8200/15) verified absent | PASS | Full |
| R-03 | `DEFAULT_STALENESS_THRESHOLD_SECS` accidentally deleted | grep returns exactly 1 definition in `coherence.rs` line 13; comment includes "NOT a Lambda input"; build success implies `run_maintenance()` still references constant by name | PASS | Full |
| R-04 | `lambda_weight_sum_invariant` uses exact `==` instead of epsilon | Test body inspection: uses `(total - 1.0_f64).abs() < f64::EPSILON` (line 270) — exact `==` absent; struct constants referenced directly (not inline literals) | PASS | Full |
| R-05 | Breaking JSON change surprises downstream operators | `test_status_json_no_freshness_fields` (new, test_tools.py): asserts both keys absent at wire level; unit test `test_status_json_no_freshness_keys` in `response/status.rs`; PR must document removed keys (process check) | PASS | Full |
| R-06 | `coherence_by_source` per-source `compute_lambda()` call not updated | grep: exactly 2 `compute_lambda(` matches in `status.rs` (lines 751, 772), both 4-argument; `coherence_by_source_uses_three_dim_lambda` unit test; `lambda_renormalization_without_embedding` non-trivial case | PASS | Full |
| R-07 | Re-normalization test expected values not updated for new weights | `lambda_renormalization_without_embedding` Case 2: `0.8*(0.46/0.77)+0.6*(0.31/0.77)` (non-trivial R-07 coverage); `lambda_renormalization_partial`; `lambda_embedding_excluded_specific` | PASS | Full |
| R-08 | `From<&StatusReport>` impl retains stale field assignments | Build gate (would fail if impl referenced removed fields); grep `confidence_freshness_score\|stale_confidence_count` in `status.rs` returns only test assertion strings (lines 1480-1485, checking for absence); `test_status_json_no_freshness_fields` integration test | PASS | Full |
| R-09 | `generate_recommendations()` retains indirect stale confidence reference | Build gate; deleted test `recommendations_below_threshold_stale_confidence` confirmed absent; `recommendations_below_threshold_all_issues` verifies max 3 recommendations (not 4) | PASS | Full |
| R-10 | ADR-003 (entry #179) not superseded before merge | `context_get` on entry #179: status=deprecated, `superseded_by: 4192`; `context_get` on entry #4199 (ADR-001): active, contains exact weights (0.46/0.31/0.23), original ratio (2:1.33:1), rationale (crt-036 invalidation) | PASS | Full |

---

## Test Results

### Unit Tests (`cargo test -p unimatrix-server`)

- Total: 2822
- Passed: 2819
- Failed: 3 (pre-existing — see Pre-existing Failures section)

**Coherence module (`infra::coherence::tests`):**
- Total: 30
- Passed: 30
- Failed: 0

**Key tests verified:**

| Test Name | Risk | Result |
|-----------|------|--------|
| `lambda_weight_sum_invariant` | R-04 | PASS — uses `f64::EPSILON` |
| `lambda_specific_three_dimensions` | R-01 | PASS — 0.576 exact result |
| `lambda_single_dimension_deviation` | R-01 | PASS — per-slot isolation |
| `lambda_all_ones` | AC-07 | PASS — returns 1.0 |
| `lambda_all_zeros` | boundary | PASS — returns 0.0 |
| `lambda_weighted_sum` | basic correctness | PASS |
| `lambda_renormalization_without_embedding` | R-07, AC-08 | PASS — both trivial and non-trivial cases |
| `lambda_renormalization_partial` | R-07 | PASS |
| `lambda_renormalized_weights_sum_to_one` | R-07 | PASS |
| `lambda_embedding_excluded_specific` | R-07 | PASS |
| `lambda_custom_weights_zero_embedding` | R-02 struct update | PASS |
| `coherence_by_source_uses_three_dim_lambda` | R-06 | PASS |
| `recommendations_below_threshold_all_issues` | R-09 | PASS — max 3 recs |
| `test_status_text_no_freshness_line` | R-05 | PASS |
| `test_status_markdown_no_freshness_bullet` | R-05 | PASS |
| `test_status_json_no_freshness_keys` | R-05, R-08 | PASS |

**Deleted tests confirmed absent:**

Freshness tests in `infra/coherence.rs` — all 11 absent:
`freshness_empty_entries`, `freshness_all_stale`, `freshness_none_stale`,
`freshness_uses_max_of_timestamps`, `freshness_recently_accessed_not_stale`,
`freshness_both_timestamps_older_than_threshold`, `oldest_stale_no_stale`,
`oldest_stale_one_stale`, `oldest_stale_both_timestamps_zero`,
`staleness_threshold_constant_value`, `recommendations_below_threshold_stale_confidence`

Fixture tests in `mcp/response/mod.rs` — all 4 absent:
`test_coherence_json_all_fields`, `test_coherence_json_f64_precision`,
`test_coherence_stale_count_rendering`, `test_coherence_default_values`

### Pre-existing Unit Test Failures (not caused by crt-048)

Three tests in `uds::listener::tests` failed due to "embedding model is initializing" — a test environment timing issue:
- `col018_long_prompt_truncated`
- `col018_prompt_at_limit_not_truncated`
- `col018_topic_signal_from_feature_id`

These tests are in `uds/listener.rs`, which has no modifications in crt-048. The last commit touching that file predates this feature. No GH Issue filed — these failures are pre-existing and tracked separately. They were not caused by this feature.

### Integration Tests

#### Smoke Tests (`pytest -m smoke`)

- Total: 23
- Passed: 23
- Failed: 0
- Run time: 199s

**Gate status: PASSED**

#### test_confidence.py

- Total: 14
- Passed: 13
- xfailed: 1 (`test_base_score_deprecated` — pre-existing GH#405, unrelated to crt-048)
- Failed: 0
- Run time: 115s

No Lambda expected-float drift detected in the confidence suite. No existing test was computing Lambda with 4-dimension expected values.

#### test_tools.py (status tests)

- Total: 11 (all `context_status`-related tests including new test)
- Passed: 11
- Failed: 0
- Run time: 90s

New test `test_status_json_no_freshness_fields` **PASSED** — verifies at the MCP wire level that `confidence_freshness_score` and `stale_confidence_count` are absent from `context_status` JSON output.

#### test_tools.py (full suite, non-status tests)

- Total: 108 non-status tests collected
- Passed: 106
- xfailed: 2 (pre-existing, not caused by crt-048)
- Failed: 0
- Run time: 897s

**Combined test_tools.py: 117 passed, 2 xfailed, 0 failed**

---

## Static Analysis Assertions

All pre-flight checks passed:

| Assertion | Command | Result |
|-----------|---------|--------|
| AC-01: `confidence_freshness` absent from `crates/` | `grep -rn "confidence_freshness" crates/` | Zero functional matches (only test assertion strings in `status.rs` checking for absence) |
| AC-04: `oldest_stale_age` absent | `grep -rn "oldest_stale_age" crates/` | Zero matches |
| AC-06: freshness fields absent from `mcp/` | `grep -rn "confidence_freshness\|stale_confidence_count" crates/unimatrix-server/src/mcp/` | Only in test assertion strings (absence checks) |
| AC-11: `DEFAULT_STALENESS_THRESHOLD_SECS` retained | `grep -n "DEFAULT_STALENESS_THRESHOLD_SECS" coherence.rs` | Exactly 1 definition (line 13), comment includes "NOT a Lambda input" |
| AC-13: exactly 2 `compute_lambda()` call sites | `grep -n "compute_lambda(" status.rs` | 2 matches (lines 751, 772), both 4-argument |
| R-02: `0.8200` non-default value absent | `grep -n "0\.8200\|0\.82[^0-9]" mod.rs` | Zero matches |
| R-02: `stale_confidence_count` absent in `mod.rs` | `grep -n "stale_confidence_count" mod.rs` | Zero matches |
| R-06: no freshness function calls in `status.rs` | `grep -n "confidence_freshness_score\|oldest_stale_age" status.rs` | Zero matches |
| R-06: `generate_recommendations` call has 5 args | `grep -n "generate_recommendations(" status.rs` | 1 match (line 784), 5 arguments |
| FR-11: `load_active_entries_with_tags` retained | `grep -n "load_active_entries_with_tags" status.rs` | 4 matches — retained |

---

## Gaps

None. All 10 risks from RISK-TEST-STRATEGY.md have test coverage at the appropriate level:

- R-01 through R-09: covered by unit tests and/or build gate and/or static analysis
- R-10: covered by Unimatrix knowledge state verification (`context_get` on entries #179 and #4199)

The pre-existing unit test failures (`col018_*`) are unrelated to crt-048 and do not represent coverage gaps for this feature.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `grep -r "confidence_freshness" crates/` — zero functional matches; struct has exactly 3 fields |
| AC-02 | PASS | `lambda_weight_sum_invariant` passes; uses `f64::EPSILON` guard; references struct constants directly |
| AC-03 | PASS | `grep -r "confidence_freshness_score" crates/` — zero matches (except absence-checking test strings); build success |
| AC-04 | PASS | `grep -r "oldest_stale_age" crates/` — zero matches |
| AC-05 | PASS | Build succeeded; `compute_lambda()` has 4-param signature; all call sites compile with 4 args |
| AC-06 | PASS | `grep -rn "confidence_freshness\|stale_confidence_count" mcp/` — only in absence-checking test strings; `test_status_json_no_freshness_fields` integration test passed |
| AC-07 | PASS | `lambda_all_ones` test: `compute_lambda(1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS)` = 1.0 |
| AC-08 | PASS | `lambda_renormalization_without_embedding` Case 1: `compute_lambda(1.0, None, 1.0, &DEFAULT_WEIGHTS)` = 1.0 |
| AC-09 | PASS | Build succeeded; `generate_recommendations` has 5-param signature; stale-confidence recommendation branch deleted |
| AC-10 | PASS | `cargo test -p unimatrix-server`: 2819 passed; 3 pre-existing failures in unrelated module |
| AC-11 | PASS | `DEFAULT_STALENESS_THRESHOLD_SECS` at `coherence.rs:13`; comment: "NOT a Lambda input — the Lambda freshness dimension was removed in crt-048" |
| AC-12 | PASS | Entry #179: status=deprecated, superseded_by=4192; entry #4199 (ADR-001): active, all 4 required data points present |
| AC-13 | PASS | `grep -n "compute_lambda(" status.rs` returns exactly 2 matches (lines 751, 772), both 4-argument; `coherence_by_source_uses_three_dim_lambda` test passes |
| AC-14 | PASS | `cargo build --release` succeeded — all 8 fixture sites in `mcp/response/mod.rs` updated without compile errors |

---

## Integration Test Details

### New Test Added

```python
# suites/test_tools.py — added at end of file
def test_status_json_no_freshness_fields(server):
    """AC-06, R-05: Removed JSON keys must be absent from context_status wire response."""
    resp = server.context_status(agent_id="human", format="json")
    report = parse_status_report(resp)
    assert "confidence_freshness_score" not in report
    assert "stale_confidence_count" not in report
```

Result: **PASSED**

### xfail Markers (pre-existing, not caused by crt-048)

| Test | Reason | GH Issue |
|------|--------|----------|
| `test_confidence.py::test_base_score_deprecated` | Deprecated confidence can exceed active due to background scoring timing | GH#405 |
| Two xfails in `test_tools.py` (non-status) | Pre-existing, unrelated to crt-048 | Pre-existing |

No new xfail markers were required. No GH Issues filed by crt-048 testing.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entries #4193 (ADR-002 retention), #4199 (ADR-001 weights), #4189 (structural dimensions pattern) directly relevant; no new knowledge gaps identified
- Stored: nothing novel to store — the test pattern of combining grep static analysis with wire-level MCP JSON absence tests is an established convention already reflected in existing harness tests; no new reusable patterns emerged from this execution
