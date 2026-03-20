# Risk Coverage Report: nan-007 (W1-3 Evaluation Harness)

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `AnalyticsMode::Suppressed` not applied at `EvalServiceLayer` construction | `eval/profile/tests::test_from_profile_analytics_mode_is_suppressed`, `eval/scenarios/tests::test_run_scenarios_does_not_write_to_snapshot`, `test_eval_offline.py::TestEvalRunReadOnly::test_sha256_unchanged_after_eval_run` (subprocess SHA-256) | PASS | Full |
| R-02 | `SqlxStore::open()` accidentally called on snapshot, triggering migration | `eval/snapshot.rs::test_snapshot_no_sqlx_store_open_in_snapshot` (structural invariant), `eval/scenarios/tests::test_run_scenarios_does_not_write_to_snapshot` | PASS | Full |
| R-03 | `test-support` feature removed, silently compiling out `kendall_tau` | `eval/runner/tests_metrics::test_kendall_tau_reachable_from_eval_runner` | PASS | Full |
| R-04 | `UnimatrixUdsClient` framing mismatch (length-prefix vs newline-delimited) | `test_eval_uds.py::TestFramingProtocol::test_send_newline_delimited`, `test_send_no_length_prefix` | PASS | Full |
| R-05 | `UnimatrixHookClient` byte order or field ordering wrong in 4-byte BE prefix | `test_eval_hooks.py::TestFramingByteOrder::test_send_uses_big_endian_length_prefix`, `test_send_ping_wire_bytes`, `test_recv_reads_big_endian_header`, `test_recv_16_byte_body`, `test_be_header_differs_from_le` | PASS | Full |
| R-06 | Snapshot path canonicalization bypass via symlink | `snapshot.rs::test_snapshot_path_guard_symlink`, `test_snapshot_path_guard_same_path`, `test_snapshot_path_guard_missing_parent_returns_error`; `test_eval_offline.py::TestSnapshotRefusesLiveDb::test_snapshot_refuses_symlink_to_live_db` (subprocess) | PASS | Full |
| R-07 | Offline/live acceptance paths conflated during delivery | Structural: `tests/test_eval_uds.py` and `tests/test_eval_hooks.py` marked `@pytest.mark.integration`; `tests/test_eval_offline.py` (31 subprocess tests, all passing without daemon); offline unit tests in both files run without daemon (39 passed, 14 deselected) | PASS | Full |
| R-08 | P@K dual-mode semantics inverted | `eval/runner/tests_metrics::test_pak_soft_ground_truth_query_log_scenario`, `test_pak_hard_labels_hand_authored_scenario`, `test_pak_hard_labels_not_confused_with_baseline`, `test_determine_ground_truth_prefers_expected`, `test_determine_ground_truth_falls_back_to_baseline` | PASS | Full |
| R-09 | `ConfidenceWeights` sum invariant produces opaque serde error | `eval/profile/tests::test_confidence_weights_invariant_sum_low_fails`, `test_confidence_weights_invariant_sum_high_fails`, `test_confidence_weights_invariant_boundary_pass_within_tolerance`, `test_confidence_weights_invariant_boundary_fail_outside_tolerance`, `test_confidence_weights_invariant_message_names_fields` | PASS | Full |
| R-10 | `EvalServiceLayer::from_profile()` panics on missing inference model path | `eval/profile/tests::test_from_profile_invalid_weights_returns_config_invariant` (ConfigInvariant path exercised); `EvalError::ModelNotFound` display test; construction from missing snapshot returns `Io` variant | PASS | Partial (ModelNotFound path exercised via display test and profile validation; no model path test because ONNX not present in CI) |
| R-11 | `eval run` nested runtime panic from missing `block_export_sync` wrapper | `main.rs` dispatch confirmed pre-tokio (structural review); `eval/runner/tests::test_k_zero_rejected` exercised sync dispatch path; all 31 `test_eval_offline.py` subprocess invocations exercise the block_export_sync bridge path end-to-end; no runtime-nesting panic observed in any test run | PASS | Full |
| R-12 | Zero-regression check silently omits partial regressions (MRR-only or P@K-only) | `eval/report/tests::test_zero_regression_check_mrr_regression_only`, `test_zero_regression_check_pak_regression_only`, `test_zero_regression_check_both_regression`, `test_zero_regression_check_exact_equal_metrics_not_regression` | PASS | Full |
| R-13 | `UnimatrixHookClient` size guard fires after send, not before | `test_eval_hooks.py::TestPayloadSizeGuard::test_oversized_payload_rejected_before_send`, `test_pre_tool_use_large_input_rejected`, `test_client_still_usable_after_size_rejection`, `test_payload_exactly_at_limit_accepted` | PASS | Full |
| R-14 | UDS socket path > 103 bytes bypasses validation | `test_eval_uds.py::TestPathLengthValidation::test_uds_path_too_long_rejected`, `test_uds_path_exactly_103_accepted`, `test_uds_path_1_byte_accepted`, `test_uds_path_validation_uses_utf8_byte_count`, `test_uds_path_104_bytes_raises_valueerror` | PASS | Full |
| R-15 | Vector index memory exhaustion for multi-profile eval | No dedicated OOM test — not reliably unit-testable. `run_eval` accepts multiple config paths structurally; memory threshold documented in CLI help (NFR-03). Integration path deferred to live eval. | N/A | Partial (by design; NFR-03 threshold documented) |
| R-16 | Mismatched `baseline.entry_ids` / `baseline.scores` lengths | `eval/scenarios/tests::test_run_scenarios_length_parity` (truncates to min), `test_run_scenarios_produces_valid_jsonl` (asserts equal lengths in output) | PASS | Full |
| R-17 | Report missing section headers | `eval/report/tests::test_report_contains_all_five_sections`, `test_report_empty_results_dir`; `test_eval_offline.py::TestEvalReportSections::test_report_contains_all_five_sections` (subprocess) | PASS | Full |
| R-18 | WAL checkpoint during snapshot causes inconsistency | SQLite guarantee; documented in `snapshot.rs` module doc and RISK-TEST-STRATEGY.md. Not unit-tested. | N/A | Documented-only (by design) |

---

## Test Results

### Unit Tests (Rust — `cargo test --workspace --lib`)

- **Total**: 2675
- **Passed**: 2675
- **Failed**: 0
- **Ignored**: 18 (pre-existing; unrelated to nan-007)

Note: 1 pre-existing doctest failure in `crates/unimatrix-server/src/infra/config.rs` (GH#303 scope). Excluded via `--lib` flag as established project practice.

#### nan-007-specific unit tests (86 total)

| Module | Tests | Scope |
|--------|-------|-------|
| `eval/profile/tests` | 21 | AnalyticsMode, EvalError display, `validate_confidence_weights`, `parse_profile_toml`, `EvalServiceLayer::from_profile` |
| `eval/runner/tests` | 8 | k=0 rejection, profile name collision, empty scenarios, blank-line skipping, missing file error, write_scenario_result sanitization, JSON schema completeness |
| `eval/runner/tests_metrics` | 17 | `kendall_tau` reachability (R-03), P@K dual-mode (R-08), MRR, `determine_ground_truth`, `compute_rank_changes`, `compute_tau_safe`, reproducibility |
| `eval/scenarios/tests` | 16 | ScenarioSource filters, JSONL schema validity (AC-03), length parity (R-16), source filter mcp/uds/all (AC-04), empty log, limit, expected=null, unique IDs, read-only enforcement (R-01/R-02), unicode, null baseline |
| `eval/report/tests` | 15 | Five sections (AC-08/R-17), MRR-only regression (R-12), P@K-only regression (R-12), both-regression, no-regression indicator (AC-09), equal-metrics not regression, C-07 always Ok, empty dir, malformed JSON skipped, summary table, latency distribution, entry-level analysis, `compute_aggregate_stats`, `find_regressions` sort, `compute_latency_buckets` |
| `snapshot/tests` | 7 | Path guard same path (AC-02/R-06), path guard symlink (AC-02/R-06), missing parent, canonicalize fails on source, `canonicalize_or_parent` existing, non-existent file in existing parent, missing parent error, structural SqlxStore invariant |
| `eval/profile/error` | 2 | `EvalError::ModelNotFound`, `EvalError::ConfigInvariant` display content |

### Integration Tests (Python — infra-001)

#### Smoke Gate (mandatory)
- **Suites**: `suites/ -m smoke`
- **Total**: 20
- **Passed**: 20
- **Failed**: 0
- **xfailed**: 0

#### Protocol Suite
- **Suite**: `suites/test_protocol.py`
- **Total**: 13
- **Passed**: 13
- **Failed**: 0

#### Tools Suite
- **Suite**: `suites/test_tools.py`
- **Total**: 73
- **Passed**: 72
- **xfailed**: 1 (GH#305, pre-existing — `test_retrospective_baseline_present`)
- **Failed**: 0

#### D1–D4 Offline Subprocess Tests (new — rework1)
- **File**: `tests/test_eval_offline.py`
- **Total run**: 31
- **Passed**: 31
- **Failed**: 0
- **xfailed**: 0 (no xfail markers in this file)
- **Coverage**: AC-01 (snapshot SQLite tables via `sqlite3.connect`), AC-02 (snapshot refuses live DB + symlink, subprocess), AC-04 (`eval scenarios --source` filter variants, subprocess), AC-05 (SHA-256 unchanged after `eval run`, subprocess), AC-06 (result JSON fields present, subprocess), AC-08 (five Markdown section headers, subprocess), AC-15 (`--help` subcommand visibility, subprocess), AC-16 (`eval run` refuses active DB, subprocess); also covers R-01, R-06, R-07, R-11, R-17

#### UDS Client Unit Tests (offline, no daemon)
- **File**: `tests/test_eval_uds.py -m "not integration"`
- **Total run**: 16
- **Passed**: 16
- **Deselected** (integration-only, require daemon): 0 from this subset
- **Coverage**: R-04 (framing), R-14 (path length), AC-11 (context manager), failure modes

#### Hook Client Unit Tests (offline, no daemon)
- **File**: `tests/test_eval_hooks.py -m "not integration"`
- **Total run**: 23
- **Passed**: 23
- **Deselected** (integration-only, require daemon): 14
- **Coverage**: R-05 (BE framing), R-13 (size guard before send), AC-14 (ValueError subclass), failure modes, typed method wire format

#### Integration Tests (require live daemon — deferred)
- 14 tests across `TestUdsIntegration` and `TestHookIntegration` marked `@pytest.mark.integration`
- **Not run** in this environment (no live daemon available)
- These test AC-10, AC-11 (live), AC-12, AC-13, AC-14 (live), FR-37, FR-40 against a running daemon
- Risk triage: this is R-07 mitigation in action — offline acceptance (D1–D4) is fully verified; live acceptance (D5–D6) requires daemon fixture, separated correctly

### Total Integration Test Count

| Suite | Run | Passed | xfailed | Deselected/Skipped |
|-------|-----|--------|---------|-------------------|
| Smoke | 20 | 20 | 0 | — |
| Protocol | 13 | 13 | 0 | — |
| Tools | 73 | 72 | 1 (GH#305) | — |
| D1–D4 offline subprocess (new) | 31 | 31 | 0 | — |
| UDS client (unit) | 16 | 16 | 0 | — |
| Hook client (unit) | 23 | 23 | 0 | — |
| UDS+Hook integration (live) | 0 | — | — | 14 (need daemon) |
| **Total run** | **176** | **175** | **1** | **14** |

---

## Gaps

### R-10: `EvalError::ModelNotFound` from construction path

The `EvalError::ModelNotFound` variant is tested via its `Display` impl but the full code path in `EvalServiceLayer::from_profile()` when a non-existent model path is specified cannot be exercised in this environment — the ONNX inference model path validation only fires when `[inference] nli_model` is configured in a profile TOML and the ONNX runtime is present. The display test (`test_eval_error_display_model_not_found`) confirms the error type is correctly defined and formatted. The missing-path validation code path in `layer.rs` is structurally complete. Coverage is partial for this specific error variant.

### R-15: Multi-profile memory exhaustion

OOM testing for 2 profiles × 50k entries is not feasible in unit tests. The architecture decision (one VectorIndex per profile) is consistent with CLI help text. The NFR-03 threshold (2 profiles, 50k entries, 8 GB RAM) is documented. Functional multi-profile `run_eval` path is exercised by `test_profile_name_collision_rejected` which parses two profiles. Full live integration test deferred.

### R-18: WAL checkpoint during snapshot

WAL isolation during concurrent snapshot is a SQLite guarantee. Documented in `snapshot.rs` module doc, RISK-TEST-STRATEGY.md, and ACCEPTANCE-MAP.md (SR-02). Not unit-testable; accepted.

### Live daemon integration tests (D5–D6, AC-10–AC-13)

14 integration tests (`@pytest.mark.integration`) require a live daemon via `daemon_server` pytest fixture. Not available in this CI environment. These are correctly separated from offline tests (R-07 mitigation). AC-10 (tool parity via UDS), AC-12 (ping/pong), AC-13 (session visible in status) require live daemon. The offline framing and client unit tests provide high confidence in correct implementation; live tests are the acceptance gate for Group 2 (D5/D6).

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_eval_offline.py::TestSnapshotCreatesValidSqlite::test_snapshot_contains_expected_tables` opens snapshot via `sqlite3.connect`, runs `SELECT name FROM sqlite_master WHERE type='table'`, asserts all 17 expected tables present; 4 tests in class all pass |
| AC-02 | PASS | `snapshot.rs::test_snapshot_path_guard_same_path`, `test_snapshot_path_guard_symlink`, `test_snapshot_parent_dir_missing` (Rust unit); `test_eval_offline.py::TestSnapshotRefusesLiveDb` (4 subprocess tests including symlink on Unix) |
| AC-03 | PASS | `eval/scenarios/tests::test_run_scenarios_produces_valid_jsonl` asserts all required fields; `test_run_scenarios_length_parity` asserts `len(entry_ids) == len(scores)` |
| AC-04 | PASS | `test_run_scenarios_source_filter_mcp`, `test_run_scenarios_source_filter_uds`, `test_run_scenarios_source_filter_all` (Rust unit); `test_eval_offline.py::TestEvalScenariosSourceFilter` (4 subprocess tests with `--source mcp|uds|all` against snapshot with known rows of both types) — Note: binary uses `--source` flag, spec says `--retrieval-mode`; naming deviation from FR-08 carried forward from Gate 3b (WARN, not FAIL) |
| AC-05 | PASS | `eval/scenarios/tests::test_run_scenarios_does_not_write_to_snapshot` (Rust unit); `test_eval_offline.py::TestEvalRunReadOnly::test_sha256_unchanged_after_eval_run` (subprocess SHA-256 comparison before/after `eval run`); `test_from_profile_analytics_mode_is_suppressed` confirms `AnalyticsMode::Suppressed` at construction |
| AC-06 | PASS | `eval/runner/tests::test_output_json_schema_completeness` (Rust unit); `test_eval_offline.py::TestEvalRunResultJson` (4 subprocess tests: top-level fields, comparison fields, numeric profile metrics, numeric comparison values) |
| AC-07 | PASS | `test_pak_hard_labels_not_confused_with_baseline` (expected != baseline), `test_determine_ground_truth_prefers_expected`, `test_determine_ground_truth_falls_back_to_baseline` — both branches verified |
| AC-08 | PASS | `eval/report/tests::test_report_contains_all_five_sections` (Rust unit); `test_eval_offline.py::TestEvalReportSections` (4 subprocess tests: full pipeline report, minimal hand-crafted JSON, empty results dir, always-zero exit) |
| AC-09 | PASS | `test_zero_regression_check_mrr_regression_only`, `test_zero_regression_check_pak_regression_only`, `test_zero_regression_check_no_regressions_empty_indicator` — OR semantics and empty indicator verified |
| AC-10 | PARTIAL | Framing verified (unit tests), parity test exists (integration-only, deferred) |
| AC-11 | PARTIAL | Context manager `__exit__` returns False, disconnect-on-exit verified (unit); live connect/disconnect deferred to integration |
| AC-12 | PARTIAL | Ping request structure verified (unit: `test_ping_request_structure`); live pong response deferred to integration |
| AC-13 | PARTIAL | Session request structures verified (unit: `test_session_start_request_structure`, `test_session_stop_request_structure`); live session visibility deferred |
| AC-14 | PASS | `test_oversized_payload_rejected_before_send` (no bytes sent), `test_payload_too_large_raises_as_value_error` (is ValueError), `test_client_still_usable_after_size_rejection` — all three AC-14 requirements pass |
| AC-15 | PASS | `test_eval_offline.py::TestHelpVisibility` (6 subprocess tests): `unimatrix --help` contains `snapshot` and `eval`; `unimatrix eval --help` contains `scenarios`, `run`, `report` individually and combined; all pass |
| AC-16 | PASS | `eval/profile/layer.rs` implements `LiveDbPath` guard; `test_from_profile_returns_live_db_path_error_for_same_path` (Rust unit); `test_eval_offline.py::TestEvalRunRefusesLiveDb` (3 subprocess tests: non-zero exit, descriptive error message, symlink via canonicalize); tests skip gracefully if workspace active DB absent |

---

## xfail Registry

| Test | Reason | GH Issue |
|------|--------|----------|
| `suites/test_tools.py::test_retrospective_baseline_present` | Pre-existing: baseline_comparison null when synthetic features lack delivery counter registration | GH#305 |

No new xfail markers introduced by nan-007.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "testing procedures gate verification integration test triage" — found entries #553 (worktree isolation procedure), #296 (service extraction procedure), #487 (workspace tests without hanging), #750 (pipeline validation tests), #2579 (cross-cutting infrastructure migration procedure). No directly applicable procedures for eval harness testing found; proceeded without.
- Stored: nothing novel to store — the offline/live test separation pattern (R-07 mitigation via `@pytest.mark.integration`) and the two-socket client unit test architecture (mocked socket send capture) are specific to nan-007's design. Will evaluate for promotion to Unimatrix knowledge after feature delivery is confirmed.
