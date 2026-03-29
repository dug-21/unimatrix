# Risk Coverage Report: crt-032

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Inconsistent default between two definition sites | `test_inference_config_weight_defaults_when_absent` (serde path), `test_inference_config_default_weights_sum_within_headroom` (Default::default() path) | PASS | Full |
| R-02 | Default-assertion test left asserting 0.10 | `test_inference_config_weight_defaults_when_absent` (updated assertion), grep scan confirms no `w_coac default must be 0.10` | PASS | Full |
| R-03 | Intentional fixture tests in search.rs changed | search.rs `FusionWeights { w_coac: 0.10 }` count unchanged (15 occurrences); `test_inference_config_validate_accepts_sum_exactly_one` unchanged | PASS | Full |
| R-04 | Stale doc comments | Grep: no `Default: 0.10`, `Defaults sum to 0.95`, `0.95 + 0.02 + 0.05 = 1.02`, `default 0.10` on w_coac line in search.rs | PASS | Full |
| R-05 | CO_ACCESS_STALENESS_SECONDS accidentally modified | Present at definition + 3 call sites (search.rs:972, status.rs:625, status.rs:1147) | PASS | Full |
| R-06 | compute_search_boost or compute_briefing_boost removed | Both defined in `crates/unimatrix-engine/src/coaccess.rs` — unchanged; `compute_search_boost(` call site in search.rs:991 present | PASS | Full |
| R-07 | Partial-TOML test comment not updated | Comment at config.rs line 4883 reads `0.00` and `0.90` | PASS | Full |

## Test Results

### Unit Tests

- Total: 2379+ (unimatrix-server lib) + additional crates
- Passed: all (0 failures on clean runs)
- Failed: 0
- Note: `col018_topic_signal_null_for_generic_prompt` is a pre-existing transient flaky test (embedding model initialization timing); documented across crt-030, col-031, bugfix-434, col-027. Unrelated to crt-032.

### Integration Tests

- Smoke suite: 20 tests
- Passed: 20
- Failed: 0
- Suites run: smoke (all 9 suites represented — protocol, tools, lifecycle, volume, security, confidence, contradiction, edge_cases, adaptation)

### New Tests Written

None. All risks covered by updated existing tests. No new test functions added.

## Gaps

None. All 7 risks have full coverage.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `default_w_coac()` body: `0.0` (config.rs lines 621–623) |
| AC-02 | PASS | `InferenceConfig::default()` struct literal: `w_coac: 0.0` (config.rs line 549) |
| AC-03 | PASS | `cargo test --workspace`: 2379+ passed, 0 failed; smoke: 20 passed |
| AC-04 | PASS | No `w_coac default must be 0.10` or `w_coac - 0.10` assertion in config.rs |
| AC-05 | PASS | 0.25+0.35+0.15+0.00+0.05+0.05 = 0.85 ≤ 1.0 (arithmetic verified) |
| AC-06 | PASS | `architecture/ADR-001-w_coac-zero-default.md` exists (64 lines); Unimatrix entry #3785 |
| AC-07 | PASS | `CO_ACCESS_STALENESS_SECONDS` at definition + 3 call sites unchanged |
| AC-08 | PASS | `fn compute_search_boost` and `fn compute_briefing_boost` in unimatrix-engine/src/coaccess.rs; call site in search.rs:991 |
| AC-09 | PASS | No new migration files; CO_ACCESS table schema unchanged |
| AC-10 | PASS | `pub w_coac: f64` field present in `InferenceConfig` (config.rs line 360) |
| AC-11 | PASS | Field doc: `Default: 0.0` (config.rs line 358) |
| AC-12 | PASS | w_prov doc: `Defaults sum to 0.85` (config.rs line 367) |
| AC-13 | PASS | w_phase_explicit doc: `0.85 + 0.02 + 0.05 = 0.92` (config.rs line 381) |
| AC-14 | PASS | FusionWeights.w_coac comment: `default 0.0 (zeroed in crt-032; PPR subsumes co-access signal via GRAPH_EDGES.CoAccess)` (search.rs line 118) |
| AC-15 | PASS | `make_weight_config()` helper: `w_coac: 0.0` (config.rs line 4729) |
| AC-16 | PASS | Partial-TOML test comment: `0.40 + 0.25 + 0.15 + 0.00 + 0.05 + 0.05 = 0.90` (config.rs line 4883) |

## Knowledge Stewardship

- Queried: no queries performed (Task tool unavailable; Delivery Leader executed tester role directly)
- Stored: nothing novel to store — this is a default-value change with no new test patterns. The pre-existing transient failure pattern for `col018_topic_signal_null_for_generic_prompt` is already documented across multiple prior features.
