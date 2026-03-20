# Agent Report: nan-007-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Agent ID**: nan-007-agent-2-testplan

---

## Deliverables Produced

All 9 test plan files created at `product/features/nan-007/test-plan/`:

| File | Size | Components / ACs |
|------|------|-----------------|
| `OVERVIEW.md` | 9.0K | Full risk-to-test mapping, integration harness plan, suite selection |
| `snapshot.md` | 5.6K | AC-01, AC-02, AC-15; R-01, R-02, R-06 |
| `eval-profile.md` | 6.7K | AC-05, AC-16; R-01, R-02, R-09, R-10 |
| `eval-scenarios.md` | 6.7K | AC-03, AC-04; R-02, R-11, R-16 |
| `eval-runner.md` | 8.4K | AC-05, AC-06, AC-07, AC-16; R-01, R-03, R-08, R-11, R-15 |
| `eval-report.md` | 7.7K | AC-08, AC-09; R-12, R-17 |
| `uds-client.md` | 8.3K | AC-10, AC-11; R-04, R-07, R-14 |
| `hook-client.md` | 8.9K | AC-12, AC-13, AC-14; R-05, R-07, R-13 |
| `cli-wiring.md` | 6.9K | AC-15; R-11, C-09/C-10 |

---

## Risk Coverage Mapping

| Risk ID | Priority | Test File | Key Test Method |
|---------|----------|-----------|-----------------|
| R-01 | Critical | `eval-profile.md` + `eval-runner.md` + offline integration | SHA-256 snapshot hash before/after; `AnalyticsMode::Suppressed` unit assertion |
| R-02 | High | `eval-profile.md` + `eval-scenarios.md` | `PRAGMA user_version` unchanged; write attempt returns `SQLITE_READONLY` |
| R-03 | High | `eval-runner.md` | `test_kendall_tau_reachable_from_eval_runner` — compile-time guard |
| R-04 | High | `uds-client.md` | `test_uds_framing_newline_delimited` — raw byte capture |
| R-05 | High | `hook-client.md` | `test_hook_framing_big_endian_byte_order` — byte-level assertion |
| R-06 | High | `snapshot.md` | Symlink test, relative path test, canonicalize-fails test |
| R-07 | High | `OVERVIEW.md` | Separate test files; offline suite passable without daemon |
| R-08 | High | `eval-runner.md` | `test_pak_soft_ground_truth_*` + `test_pak_hard_labels_*` — both branches |
| R-09 | Med | `eval-profile.md` | `test_confidence_weights_invariant_*` — message content asserted |
| R-10 | Med | `eval-profile.md` | `test_from_profile_returns_model_not_found` |
| R-11 | Med | `cli-wiring.md` | subprocess exit code 0 on valid input (no runtime panic) |
| R-12 | Med | `eval-report.md` | `test_zero_regression_check_mrr_regression_only` + `_pak_regression_only` |
| R-13 | Med | `hook-client.md` | `test_hook_oversized_payload_rejected_before_send` (mocked socket) |
| R-14 | Med | `uds-client.md` | `test_uds_path_too_long_rejected` + `_exactly_103_accepted` |
| R-15 | Med | `eval-runner.md` | `test_eval_run_two_profiles_completes` |
| R-16 | Med | `eval-scenarios.md` | `test_run_scenarios_length_parity` |
| R-17 | Low | `eval-report.md` | `test_report_contains_all_five_sections` |
| R-18 | Low | Documented only; SQLite WAL guarantee, not unit-tested |

All 16 ACs mapped:
- **AC-01**: `snapshot.md` → `test_snapshot_creates_valid_sqlite`
- **AC-02**: `snapshot.md` → symlink + relative path + direct path tests
- **AC-03**: `eval-scenarios.md` → `test_run_scenarios_produces_valid_jsonl` + parity check
- **AC-04**: `eval-scenarios.md` → source filter mcp/uds/all tests
- **AC-05**: `eval-runner.md` + `eval-profile.md` → SHA-256 integrity test
- **AC-06**: `eval-runner.md` → `test_output_json_schema_completeness`
- **AC-07**: `eval-runner.md` → dual-mode P@K tests (both branches)
- **AC-08**: `eval-report.md` → `test_report_contains_all_five_sections`
- **AC-09**: `eval-report.md` → OR semantics tests + empty indicator test
- **AC-10**: `uds-client.md` → parity test + all-12-tools test
- **AC-11**: `uds-client.md` → context manager test
- **AC-12**: `hook-client.md` → `test_hook_ping_pong`
- **AC-13**: `hook-client.md` → `test_hook_session_visible_in_status`
- **AC-14**: `hook-client.md` → `test_hook_oversized_payload_rejected_before_send`
- **AC-15**: `cli-wiring.md` → `--help` subprocess tests for all subcommands
- **AC-16**: `eval-runner.md` + `eval-profile.md` → live DB path guard tests

---

## Integration Harness Plan (Summary)

Three test files to create (two are D5/D6 deliverables):

| File | Location | Group | Fixture |
|------|----------|-------|---------|
| `test_eval_offline.py` | `tests/` | Group 1 (no daemon) | subprocess only |
| `test_eval_uds.py` | `tests/` | Group 2 (D5 deliverable) | `daemon_server` |
| `test_eval_hooks.py` | `tests/` | Group 2 (D6 deliverable) | `daemon_server` |

Existing suites to run at Stage 3c: `smoke` (mandatory gate) + `protocol` + `tools` (verify 12 tools still listed after main.rs changes).

---

## Key Design Decisions in Test Plans

1. **Offline/live separation enforced via file boundaries** — `test_eval_offline.py` has zero `daemon_server` fixture imports. R-07 is covered structurally.
2. **R-03 covered by compile-time guard** — `test_kendall_tau_reachable_from_eval_runner` calls `kendall_tau()` directly from the `eval` module. Removing the `test-support` feature causes a compile error at this exact test.
3. **R-12 OR semantics have dedicated tests for each branch** — MRR-only regression and P@K-only regression are tested independently. A combined-AND implementation would pass one and fail the other.
4. **Framing tests use raw byte capture** — both R-04 and R-05 require byte-level assertions, not just round-trip success. A wrong-framing bug would pass a round-trip test if the server happens to tolerate malformed input.
5. **Size guard mocks the socket send** — R-13 requires proving zero bytes were sent. This requires mocking `socket.sendall` or equivalent; a simple `ValueError` assertion alone does not confirm pre-send placement.

---

## Open Questions

1. **`context_quarantine` and `context_enroll` in AC-10**: These require admin capabilities. The test plan assumes an `admin_server` fixture variant is available (per USAGE-PROTOCOL.md fixture table). If not, these two of the 12 tools may need to be tested via a daemon started with admin credentials or skipped with a documented reason.

2. **`test_uds_query_logged_as_source_uds`**: Accessing `query_log` from a live DB requires either direct SQLite access (bypassing MCP) or a `context_status` path that surfaces query log counts. The exact verification method depends on what `context_status` exposes for `query_log`. If it does not expose this, a direct SQLite read of the live DB is needed.

3. **`daemon_server` fixture socket path fields**: The plan assumes the fixture yields both `socket_path` (hook IPC) and `mcp_socket_path` (MCP UDS). If only one path is yielded, the D6 tests need to derive the other path via `ProjectPaths` convention. Implementer should check conftest.py before coding.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "nan-007 architectural decisions" (category: decision) — found all 5 ADRs (#2585–#2588, #2602). Used to inform dispatch ordering (ADR-005), analytics suppression (ADR-002), and read-only pool approach (ADR-001).
- Queried: `/uni-knowledge-search` for "evaluation harness testing patterns edge cases" — found entry #1204 (test plan must cross-reference pseudocode) and #157 (cumulative test infrastructure). Informed the decision to structure framing tests as byte-level captures rather than round-trip tests.
- Stored: entry #2603 "Two-socket / two-framing test patterns for dual-socket daemon harnesses" via `/uni-store-pattern` — the byte-capture + wrong-socket + size-guard-mock + offline-file-separation pattern is novel and generalizable to any future feature that adds a second daemon socket.
