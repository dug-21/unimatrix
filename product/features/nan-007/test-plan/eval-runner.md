# Test Plan: `eval/runner.rs` (D3)

**Component**: `crates/unimatrix-server/src/eval/runner.rs`
**Function under test**: `run_eval(db: &Path, scenarios: &Path, configs: &[PathBuf], k: usize, out: &Path) -> Result<(), Box<dyn Error>>`
**AC coverage**: AC-05 (read-only), AC-06 (JSON output), AC-07 (P@K labels), AC-16 (live-DB guard)
**Risk coverage**: R-01 (analytics suppression), R-03 (kendall_tau feature), R-08 (P@K dual-mode), R-11 (block_export_sync), R-15 (multi-profile memory)

---

## Unit Tests

Location: `crates/unimatrix-server/src/eval/runner.rs` (inline `#[cfg(test)]`)

### Test: `test_kendall_tau_reachable_from_eval_runner`

**Purpose**: R-03 — `kendall_tau()` from `unimatrix_engine::test_scenarios` is callable from the `eval` module. This test must call the function directly so that removing the `test-support` feature from `Cargo.toml` causes a compile error here.
**Arrange**: Two known ranking lists: `[1, 2, 3]` (baseline) and `[1, 3, 2]` (candidate).
**Act**: Call `unimatrix_engine::test_scenarios::kendall_tau(&[1, 2, 3], &[1, 3, 2])`.
**Assert**: Returns a value in `[-1.0, 1.0]`. For `[1,2,3]` vs `[1,3,2]`, exactly one inversion → expected tau ≈ 0.333.
**Risk**: R-03 (explicit compile-time guard)

### Test: `test_pak_soft_ground_truth_query_log_scenario`

**Purpose**: AC-07, R-08 — `expected = null` scenario uses `baseline.entry_ids` for P@K.
**Arrange**: Scenario with `expected = null`, `baseline.entry_ids = [10, 20, 30]`. Profile returns `[10, 20, 50]` (2/3 baseline entries in top 3).
**Act**: Compute `p_at_k` via the eval runner's P@K function with `k = 3`.
**Assert**: `p_at_k ≈ 0.667` (2 hits out of 3 returned, all 3 in baseline = 2/3).
**Risk**: R-08 (AC-07)

### Test: `test_pak_hard_labels_hand_authored_scenario`

**Purpose**: AC-07, R-08 — `expected = [id1, id2]` scenario uses `expected` for P@K, not `baseline`.
**Arrange**: Scenario with `expected = [10, 20]`, `baseline = null`. Profile returns `[10, 30, 20]` (10 and 20 are in expected).
**Act**: Compute P@K with `k = 3`.
**Assert**: `p_at_k = 2/3 ≈ 0.667` (2 expected IDs in top-3 results out of k=3).
**Risk**: R-08 (AC-07)

### Test: `test_pak_hard_labels_not_confused_with_baseline`

**Purpose**: R-08 — when `expected` is non-null, `baseline.entry_ids` is NOT used for P@K even if present.
**Arrange**: Scenario with `expected = [10]`, `baseline.entry_ids = [20, 30]` (disjoint from expected). Profile returns `[10, 20, 30]`.
**Act**: Compute P@K with `k = 3`.
**Assert**: `p_at_k = 1/3 ≈ 0.333` (only entry 10 matches `expected`). If baseline were mistakenly used, result would be 2/3.
**Risk**: R-08 critical

### Test: `test_pak_at_k1_known_result`

**Purpose**: P@K@1 = 1.0 when the first result is in ground truth.
**Arrange**: Hand-authored scenario, `expected = [99]`. Profile returns `[99, 1, 2]`.
**Act**: Compute P@K@1.
**Assert**: `p_at_k = 1.0`.
**Risk**: R-08

### Test: `test_mrr_known_result`

**Purpose**: MRR computation is correct for a known case.
**Arrange**: Ground truth = `{10, 20}`. Result list = `[5, 10, 20]`. First relevant at rank 2.
**Act**: Compute MRR.
**Assert**: `mrr = 1/2 = 0.5`.
**Risk**: FR-19

### Test: `test_kendall_tau_single_element_no_panic`

**Purpose**: Edge case — Kendall tau for a single-element list must not panic or produce NaN.
**Arrange**: Two single-element lists: `[5]` vs `[5]`.
**Act**: `kendall_tau(&[5], &[5])`.
**Assert**: Returns 1.0 (or a defined convention). Does NOT panic or return NaN.
**Risk**: Edge case (from RISK-TEST-STRATEGY.md)

### Test: `test_output_json_schema_completeness`

**Purpose**: AC-06 — per-scenario result JSON contains all required fields.
**Arrange**: Minimal scenario + 2 profiles.
**Act**: Call `run_eval` on a temp snapshot with the test scenario.
**Assert**: Each result JSON file contains: `scenario_id`, `query`, `profiles` (keyed by profile name, each with `entries`, `latency_ms`, `p_at_k`, `mrr`), `comparison` (with `kendall_tau`, `rank_changes`, `mrr_delta`, `p_at_k_delta`, `latency_overhead_ms`). All values are numeric where required.
**Risk**: AC-06

### Test: `test_profile_name_collision_rejected`

**Purpose**: Two profiles with the same `[profile] name` produce a structured error before replay.
**Arrange**: Two profile TOML files both having `[profile] name = "baseline"`.
**Act**: `run_eval(&snapshot, &scenarios, &[profile1, profile2], 5, &out)`.
**Assert**: Returns `Err(...)` with message naming the duplicate profile name. No result files written.
**Risk**: Edge case (from RISK-TEST-STRATEGY.md)

### Test: `test_k_zero_rejected`

**Purpose**: `--k 0` returns a user-readable error.
**Arrange**: Valid scenario file and profiles.
**Act**: `run_eval(&snapshot, &scenarios, &[profile], 0, &out)`.
**Assert**: Returns `Err(...)` with message mentioning `k >= 1`.
**Risk**: Edge case (from RISK-TEST-STRATEGY.md)

### Test: `test_empty_scenarios_produces_empty_results`

**Purpose**: Edge case — empty `scenarios.jsonl` → empty results directory, exit 0.
**Arrange**: Empty JSONL file (0 bytes or 0 lines).
**Act**: `run_eval`.
**Assert**: Returns `Ok(())`. `out` directory exists but contains 0 JSON files.
**Risk**: Edge case

### Test: `test_eval_run_metric_reproducibility`

**Purpose**: NFR-05 — identical input produces identical numeric results across repeated calls.
**Arrange**: Fixed scenario, fixed profile, fixed snapshot.
**Act**: Call `run_eval` twice in succession.
**Assert**: Output JSON files are byte-for-byte identical (or at minimum, all numeric fields have identical values). If float tie-breaking exists, document it with a comment.
**Risk**: NFR-05

---

## Integration Tests (Python Subprocess)

Location: `product/test/infra-001/tests/test_eval_offline.py`

### Test: `test_eval_run_readonly_sha256`

**Purpose**: AC-05 — snapshot SHA-256 is unchanged after `eval run`.
**Act**:
1. Compute `sha256sum <snapshot>`.
2. `unimatrix eval run --db <snapshot> --scenarios <jsonl> --configs baseline.toml,candidate.toml --out <results_dir>`.
3. Compute `sha256sum <snapshot>` again.
**Assert**: Exit code 0. Both hashes are identical.
**Risk**: R-01 (Critical), AC-05

### Test: `test_eval_run_output_schema`

**Purpose**: AC-06 — one JSON result file per scenario with all required fields.
**Act**: `unimatrix eval run` with a 3-scenario JSONL and 2 profile configs.
**Assert**: `results/` directory contains 3 JSON files. Each parses correctly and contains `profiles`, `comparison.kendall_tau`, `comparison.mrr_delta`, `comparison.p_at_k_delta`, `comparison.latency_overhead_ms`. All values numeric.
**Risk**: AC-06

### Test: `test_eval_run_refuses_live_db`

**Purpose**: AC-16 — live DB path guard via subprocess.
**Act**: `unimatrix eval run --db <live-db-path> --scenarios <file> --configs <profile> --out <dir>`.
**Assert**: Exit code != 0. stderr contains both resolved paths.
**Risk**: R-06 (AC-16)

### Test: `test_eval_run_pak_soft_ground_truth`

**Purpose**: AC-07 — subprocess-level P@K mode dispatch.
**Arrange**: Scenarios file with one query-log-sourced scenario (`expected: null`).
**Assert**: The result JSON has a `p_at_k` value computed against `baseline.entry_ids`. (Verify by checking that the reported P@K matches the expected calculation using the scenario's baseline list.)
**Risk**: R-08

### Test: `test_eval_run_pak_hard_labels`

**Purpose**: AC-07 — hand-authored scenario P@K mode via subprocess.
**Arrange**: Scenarios file with one hand-authored scenario (`expected: [id1, id2]`, `baseline: null`).
**Assert**: Result `p_at_k` is computed against the `expected` list.
**Risk**: R-08

### Test: `test_eval_run_two_profiles_completes`

**Purpose**: R-15 (NFR-03) — 2-profile eval run completes without OOM.
**Arrange**: Snapshot with representative entry count (at least 100 entries for test purposes). Two profile configs (baseline + one candidate).
**Assert**: Exit code 0. Results directory contains one JSON per scenario.
**Risk**: R-15

---

## Edge Cases from Risk Strategy

- Empty scenarios file: `run_eval` must exit 0 and produce an empty results directory.
- Profile name collision: structured error before any replay, no partial results written.
- `--k 0` or negative: rejected with user-readable error before construction.
- Single-scenario eval: Kendall tau for single-element lists must not panic or produce NaN.
- Metric reproducibility: identical input → identical numeric output (NFR-05).
- Profile TOML with `[inference]` section pointing to missing model: `EvalError::ModelNotFound` propagated as non-zero exit with descriptive message.

---

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness testing patterns edge cases" — found entries #1204 (Test Plan Must Cross-Reference Pseudocode for Edge-Case Behavior Assertions), #729 (Intelligence pipeline requires cross-crate integration tests), #157 (Test infrastructure is cumulative)
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — found ADR-002 (analytics suppression at construction), ADR-003 (test-support feature flag for kendall_tau reuse), ADR-001 (live-DB path guard, block_export_sync)
Queried: /uni-query-patterns for "integration test harness patterns infra" — found entries #238 (Testing Infrastructure Convention), #748 (TestHarness Server Integration Pattern), #128 (Risk drives testing)
Stored: nothing novel to store — test plan agents are read-only; patterns are consumed not created
