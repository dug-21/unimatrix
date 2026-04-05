# Risk Coverage Report: crt-043 (GH #501 / #502 Bugfix Verification)

## Context

Python-only eval harness bugfix. No production Rust code was changed.
Verified on branch `bugfix/501-502-clean`.

Changed files:
- `product/research/ass-039/build_scenarios.py` — scenario ID collision fix
- `product/research/ass-039/harness/run_eval.py` — snapshot pairing validation
- `product/research/ass-039/harness/scenarios.jsonl` — regenerated (1,761 lines)
- `product/research/ass-039/harness/scenarios_meta.json` — new sidecar file
- `product/research/ass-039/harness/tests/test_build_scenarios.py` — new tests
- `product/research/ass-039/harness/tests/test_run_eval_sidecar.py` — new tests
- `product/research/ass-040/ROADMAP.md`, `product/test/eval-baselines/log.jsonl`,
  `product/test/eval-baselines/README.md`, `docs/testing/eval-harness.md`

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Scenario ID collisions when same session fires multiple context_search calls at same millisecond | `test_no_id_collision_same_session_same_ms`, `test_briefing_no_id_collision_same_session_same_ms`, `test_uniqueness_assertion_fires_on_collision` | PASS | Full |
| R-02 | Sidecar not written by build_scenarios.py — no snapshot pairing possible | `test_sidecar_written` | PASS | Full |
| R-03 | run_eval.py proceeds silently when scenarios.jsonl is stale vs current DB snapshot | `test_mismatch_exits_1`, `test_matching_hash_is_silent` | PASS | Full |
| R-04 | Missing `--allow-snapshot-mismatch` escape hatch blocks intentional cross-snapshot eval runs | `test_allow_snapshot_mismatch_flag_suppresses_error` | PASS | Full |
| R-05 | Absent sidecar treated as hard error, breaking legacy workflows | `test_absent_sidecar_is_warning_not_error` | PASS | Full |
| R-06 | scenarios.jsonl regeneration still contains duplicate IDs after fix | Scenario uniqueness check: Total=1761, Unique=1761 | PASS | Full |
| R-07 | Sidecar missing required fields (source_db_hash, generated_at, scenario_count) | Sidecar field validation | PASS | Full |
| R-08 | Rust regression from Python-only change | cargo test --workspace | PASS (pre-existing failures noted) | Full |

---

## Test Results

### Bug-Specific Python Tests
- Suite: `product/research/ass-039/harness/tests/`
- Total: 8
- Passed: 8
- Failed: 0

Individual results:
| Test | Result |
|------|--------|
| `test_build_scenarios.py::test_no_id_collision_same_session_same_ms` | PASS |
| `test_build_scenarios.py::test_briefing_no_id_collision_same_session_same_ms` | PASS |
| `test_build_scenarios.py::test_uniqueness_assertion_fires_on_collision` | PASS |
| `test_build_scenarios.py::test_sidecar_written` | PASS |
| `test_run_eval_sidecar.py::test_mismatch_exits_1` | PASS |
| `test_run_eval_sidecar.py::test_absent_sidecar_is_warning_not_error` | PASS |
| `test_run_eval_sidecar.py::test_allow_snapshot_mismatch_flag_suppresses_error` | PASS |
| `test_run_eval_sidecar.py::test_matching_hash_is_silent` | PASS |

### Scenario Uniqueness Check
- Total scenarios: 1761
- Unique IDs: 1761
- Result: PASS — no duplicate IDs

### Sidecar Validation
- `source_db_hash`: present (`97bd647c0ff4205d...`)
- `generated_at`: present (`2026-04-03T15:40:59Z`)
- `scenario_count`: present (`1761`)
- Result: PASS — all required fields present

### Rust Unit Tests (cargo test --workspace)
- Total: 2684 passed, 2 failed
- Passed: 2684
- Failed: 2 (both pre-existing, unrelated to this fix)

Pre-existing failures (not caused by this bugfix):
1. `server::tests::test_migration_v7_to_v8_backfill` — pre-existing, unrelated to eval harness
2. `uds::listener::tests::col018_topic_signal_null_for_generic_prompt` — pre-existing embedding model initialization race (noted in project memory)

Neither failing test touches any file changed by this PR. No xfail markers required — these are tracked via existing project awareness.

### Clippy
- Pre-existing warning in `crates/unimatrix-engine/src/auth.rs` (collapsible_if) — last touched in crt-014 / col-006, not modified by this bugfix.
- No new Rust warnings or errors introduced.

### Integration Tests (infra-001)
- Not required — no production Rust code was changed. User explicitly specified no integration testing for this fix.

---

## Gaps

None. All identified risks have full test coverage. The two pre-existing Rust test failures are unrelated to this bugfix and pre-date this branch.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01: No scenario ID collisions in generated scenarios.jsonl | PASS | 1761 total = 1761 unique; `test_no_id_collision_same_session_same_ms` passes |
| AC-02: build_scenarios.py writes scenarios_meta.json sidecar with required fields | PASS | Sidecar present with `source_db_hash`, `generated_at`, `scenario_count`; `test_sidecar_written` passes |
| AC-03: run_eval.py exits non-zero on snapshot hash mismatch | PASS | `test_mismatch_exits_1` passes |
| AC-04: Absent sidecar is a warning, not a hard error | PASS | `test_absent_sidecar_is_warning_not_error` passes |
| AC-05: `--allow-snapshot-mismatch` flag suppresses hash mismatch error | PASS | `test_allow_snapshot_mismatch_flag_suppresses_error` passes |
| AC-06: No Rust regressions | PASS | 2684 Rust tests pass; 2 pre-existing failures unrelated to this change |
