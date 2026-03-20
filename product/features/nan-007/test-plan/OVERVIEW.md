# nan-007 Test Plan — Overview

## Test Strategy

nan-007 delivers two independent acceptance groups:

- **Group 1 (Offline, D1–D4)**: `snapshot`, `eval scenarios`, `eval run`, `eval report`. No running daemon. Validated via Rust unit tests and subprocess integration tests.
- **Group 2 (Live, D5–D6)**: `UnimatrixUdsClient`, `UnimatrixHookClient`. Require a live daemon. Validated via Python integration tests using the `daemon_server` pytest fixture.

These groups must remain independent: a Group 2 daemon fixture failure must not block Group 1 acceptance (R-07, SR-04).

### Test Layers

| Layer | Tool | Scope |
|-------|------|-------|
| Unit | `cargo test --workspace` | Logic in `eval/profile.rs`, `eval/runner.rs`, `eval/report.rs`, `snapshot.rs` |
| Integration (Rust) | `cargo test` with `#[tokio::test]` | `EvalServiceLayer` construction, read-only enforcement, block_export_sync path |
| Integration (Python — offline) | `pytest tests/test_eval_offline.py` | subprocess invocations of `unimatrix snapshot`, `eval scenarios`, `eval run`, `eval report` |
| Integration (Python — live) | `pytest tests/test_eval_uds.py tests/test_eval_hooks.py` | `UnimatrixUdsClient` and `UnimatrixHookClient` against `daemon_server` fixture |
| Smoke gate | `pytest -m smoke` | Minimum gate; must pass before Stage 3c sign-off |

---

## Risk-to-Test Mapping

| Risk ID | Description | Priority | Test File / Location | AC Coverage |
|---------|-------------|----------|----------------------|-------------|
| R-01 | Analytics suppression not applied at construction | Critical | `eval/profile.rs` unit tests + `test_eval_offline.py` SHA-256 check | AC-05 |
| R-02 | `SqlxStore::open()` called on snapshot (migration) | High | `eval/profile.rs` unit test (grep assertion) + post-eval schema check | AC-05 |
| R-03 | `test-support` feature removed from Cargo.toml | High | `eval/runner.rs` unit test calling `kendall_tau()` directly | — |
| R-04 | UDS client framing mismatch (length-prefix vs newline) | High | `test_eval_uds.py` framing byte capture + AC-10 parity test | AC-10, AC-11 |
| R-05 | Hook client length-prefix byte order wrong | High | `test_eval_hooks.py` byte-level framing + AC-12, AC-13 | AC-12, AC-13 |
| R-06 | Snapshot path canonicalization bypass via symlink | High | `test_eval_offline.py` symlink test + relative path test | AC-02 |
| R-07 | Offline/live acceptance paths conflated | High | Separate test files; offline suite runs without daemon | — |
| R-08 | P@K dual-mode semantics inverted | High | `eval/runner.rs` unit tests — both branches | AC-07 |
| R-09 | `ConfidenceWeights` validation opaque serde error | Med | `eval/profile.rs` unit tests — message content asserted | — |
| R-10 | Panic on missing inference model path | Med | `eval/profile.rs` unit test — `EvalError::ModelNotFound` | — |
| R-11 | Missing `block_export_sync` wrapper (nested runtime) | Med | `test_eval_offline.py` subprocess exit code 0 on valid input | — |
| R-12 | Zero-regression check misses partial regressions (OR) | Med | `eval/report.rs` unit tests — MRR-only and P@K-only cases | AC-09 |
| R-13 | Hook client size guard fires after send, not before | Med | `test_eval_hooks.py` — oversized payload, socket write mock | AC-14 |
| R-14 | UDS path > 103 bytes bypasses validation | Med | `uds-client.md` unit tests — boundary at 103/104 bytes | — (FR-31) |
| R-15 | Vector index memory exhaustion (multi-profile) | Med | `test_eval_offline.py` — 2-profile eval run completes | — (NFR-03) |
| R-16 | Mismatched `baseline.entry_ids` / `baseline.scores` | Med | `eval/scenarios.rs` unit test + `test_eval_offline.py` JSONL validation | AC-03 |
| R-17 | Report missing section headers | Low | `eval/report.rs` unit test — all five headers present | AC-08 |
| R-18 | WAL checkpoint during snapshot | Low | Documented; not unit-tested (SQLite guarantee) | — |

---

## Cross-Component Test Dependencies

1. `snapshot.rs` produces `snapshot.db` consumed by `eval/scenarios.rs` integration tests.
2. `eval/scenarios.rs` output (`scenarios.jsonl`) consumed by `eval/runner.rs` integration tests.
3. `eval/runner.rs` output (`results/*.json`) consumed by `eval/report.rs` integration tests.
4. `UnimatrixUdsClient` (D5) is used within `test_eval_hooks.py` to verify session visibility (AC-13), making D5 a dependency for D6 live tests.
5. The `daemon_server` pytest fixture (entry #1928) gates all D5/D6 tests.

---

## Integration Harness Plan

### Existing Suites Applicable to nan-007

nan-007 does not touch any existing server tools, confidence system, or schema. However, the new Python harness files extend the test infrastructure.

| Suite | Applicability |
|-------|---------------|
| `smoke` | Mandatory gate — runs against the existing server after nan-007 changes to `main.rs` to verify nothing regressed |
| `protocol` | Verify `tools/list` still returns all 12 tools after CLI additions to `main.rs` |
| `tools` | Verify all 12 existing tools still work after Cargo.toml `test-support` feature addition |

Run command for gate:
```bash
cargo build --release
cd /workspaces/unimatrix/product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60
```

### New Integration Test Files

Two new Python files implement D5 and D6 deliverables directly (they are the deliverable, not just test wrappers):

#### `tests/test_eval_offline.py` (Group 1)

Tests for D1–D4 via subprocess invocations. Runs without a daemon. Pytest marks: no `daemon_server` fixture dependency.

Planned tests:
- `test_snapshot_creates_valid_sqlite` — AC-01
- `test_snapshot_refuses_live_db_path` — AC-02
- `test_snapshot_refuses_symlink_to_live_db` — AC-02 (R-06)
- `test_snapshot_refuses_relative_path_to_live_db` — AC-02 (R-06)
- `test_snapshot_parent_dir_missing` — failure mode
- `test_eval_scenarios_jsonl_schema` — AC-03
- `test_eval_scenarios_length_parity` — R-16
- `test_eval_scenarios_source_filter_mcp` — AC-04
- `test_eval_scenarios_source_filter_uds` — AC-04
- `test_eval_scenarios_source_filter_all` — AC-04
- `test_eval_scenarios_empty_query_log` — edge case
- `test_eval_run_readonly_sha256` — AC-05 (R-01)
- `test_eval_run_output_schema` — AC-06
- `test_eval_run_pak_soft_ground_truth` — AC-07 (R-08)
- `test_eval_run_pak_hard_labels` — AC-07 (R-08)
- `test_eval_run_refuses_live_db` — AC-16 (R-06)
- `test_eval_run_two_profiles_completes` — R-15 (NFR-03)
- `test_eval_run_empty_scenarios` — edge case
- `test_eval_run_k_zero_rejected` — edge case
- `test_eval_report_five_sections` — AC-08 (R-17)
- `test_eval_report_zero_regression_mrr_only` — AC-09, R-12
- `test_eval_report_zero_regression_pak_only` — AC-09, R-12
- `test_eval_report_empty_regression_indicator` — AC-09
- `test_eval_report_empty_results_dir` — edge case
- `test_cli_help_snapshot_visible` — AC-15
- `test_cli_help_eval_subcommands_visible` — AC-15

#### `tests/test_eval_uds.py` (Group 2, D5)

Tests for `UnimatrixUdsClient` against live daemon via `daemon_server` fixture.

Planned tests:
- `test_uds_connection_lifecycle` — FR-32
- `test_uds_context_manager` — AC-11
- `test_uds_tool_parity_search` — AC-10
- `test_uds_all_12_tools_callable` — AC-10 (FR-34)
- `test_uds_framing_newline_delimited` — R-04 (raw byte capture)
- `test_uds_concurrent_clients` — FR-35
- `test_uds_query_logged_as_source_uds` — FR-35 (source field)
- `test_uds_path_too_long_rejected` — R-14 (FR-31), boundary 104 bytes
- `test_uds_path_exactly_103_accepted` — R-14 (FR-31), boundary 103 bytes
- `test_uds_socket_not_found_connection_error` — failure mode

#### `tests/test_eval_hooks.py` (Group 2, D6)

Tests for `UnimatrixHookClient` against live daemon via `daemon_server` fixture.

Planned tests:
- `test_hook_ping_pong` — AC-12 (R-05)
- `test_hook_framing_big_endian` — R-05 (byte-level)
- `test_hook_session_lifecycle` — AC-13
- `test_hook_session_visible_in_status` — AC-13 (via UDS client)
- `test_hook_session_keywords_populated` — FR-40 (col-022)
- `test_hook_oversized_payload_rejected_before_send` — AC-14 (R-13)
- `test_hook_oversized_payload_client_still_usable` — R-13
- `test_hook_pre_post_tool_use` — FR-37
- `test_hook_invalid_payload_malformed_json` — FR-40

### Fixture Selection

| Test Group | Fixture | Rationale |
|-----------|---------|-----------|
| `test_eval_offline.py` | None (subprocess) | No daemon needed; invoke binary as subprocess |
| `test_eval_uds.py` | `daemon_server` | Yields `mcp_socket_path` for `UnimatrixUdsClient` |
| `test_eval_hooks.py` | `daemon_server` | Yields `socket_path` for `UnimatrixHookClient` and `mcp_socket_path` for verification via UDS |

### Suite Selection for Stage 3c

| Command | Purpose |
|---------|---------|
| `pytest -m smoke` | Minimum gate — existing server unbroken |
| `pytest suites/test_protocol.py suites/test_tools.py -v` | Verify 12 tools still listed after main.rs changes |
| `pytest tests/test_eval_offline.py -v --timeout=120` | Group 1 offline AC verification |
| `pytest tests/test_eval_uds.py tests/test_eval_hooks.py -v --timeout=60` | Group 2 live AC verification |

All commands run from `/workspaces/unimatrix/product/test/infra-001/`.

---

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness testing patterns edge cases" — found entries #1204 (Test Plan Must Cross-Reference Pseudocode for Edge-Case Behavior Assertions), #729 (Intelligence pipeline testing requires cross-crate integration tests), #157 (Test infrastructure is cumulative), #229 (Tester Role Duties)
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — found ADR-001 through ADR-005: snapshot sqlx + block_on wrapper, analytics suppression, test-support feature, eval crate placement, nested eval subcommand via clap
Queried: /uni-query-patterns for "integration test harness patterns infra" — found entries #238 (Testing Infrastructure Convention), #729 (Intelligence pipeline integration tests), #157 (Test infrastructure is cumulative), #129 (Concrete assertions)
Queried: /uni-query-patterns for "snapshot database testing patterns" — found entries #748 (TestHarness Server Integration Pattern), #238 (Testing Infrastructure Convention), #2326 (Bug fix verification async pattern), #128 (Risk drives testing)
Stored: nothing novel to store — test plan agents are read-only; patterns are consumed not created
