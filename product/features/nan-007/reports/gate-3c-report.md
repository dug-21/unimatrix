# Gate 3c Report: nan-007

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-20
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 18 risks mapped in RISK-COVERAGE-REPORT.md; 15 fully covered, 3 documented partial/N/A with accepted rationale |
| Test coverage completeness | WARN | 14 live integration tests correctly deselected (daemon not available); R-07 mitigation confirmed operational |
| Specification compliance | FAIL | `test_eval_offline.py` absent — subprocess-level AC-01, AC-02 (shell path), AC-04, AC-05 SHA-256, AC-06, AC-08, AC-15, AC-16 have no subprocess verification |
| Architecture compliance | PASS | Component structure matches ARCHITECTURE.md; ADR-001 through ADR-005 all satisfied in code |
| Knowledge stewardship — tester agent | PASS | `## Knowledge Stewardship` section present with `Queried:` and `Stored:` entries in agent report |

---

## Detailed Findings

### Check 1: Risk Mitigation Proof

**Status**: PASS

**Evidence**: `RISK-COVERAGE-REPORT.md` maps all 18 risks from RISK-TEST-STRATEGY.md:

- **Critical (R-01)**: `test_from_profile_analytics_mode_is_suppressed` confirms `AnalyticsMode::Suppressed` at construction; `test_run_scenarios_does_not_write_to_snapshot` confirms byte-for-byte snapshot integrity. `open_readonly()` implementation in `crates/unimatrix-store/src/db.rs` lines 134–168 drops the analytics receiver immediately, eliminating the drain task. AC-05 / NFR-04 satisfied at unit level.

- **High risks (R-02 through R-08)**: All fully covered with passing tests. R-06 path guard symlink test (`test_snapshot_path_guard_symlink`) confirmed in `snapshot.rs` tests. R-08 P@K dual-mode covered by five dedicated tests. R-04/R-05 framing verified by byte-capture unit tests.

- **Medium risks (R-09 through R-16)**: All covered or accepted. R-09 `ConfidenceWeights` error message content asserted. R-10 partial by design (ONNX not in CI). R-11 structural review + `test_k_zero_rejected`. R-12 OR-semantics explicitly tested with MRR-only and P@K-only regression cases. R-13 and R-14 full boundary testing.

- **Low risks (R-17, R-18)**: R-17 section headers test passes. R-18 WAL isolation documented as SQLite guarantee — accepted.

Verified test counts: `cargo test --workspace --lib` returns 2675 passed / 0 failed / 18 ignored (pre-existing), matching RISK-COVERAGE-REPORT.md. nan-007-specific eval tests: 86 pass. Python offline tests: 39 pass.

### Check 2: Test Coverage Completeness

**Status**: WARN

**Evidence**: Risk-to-scenario mapping from Phase 2 is exercised for all 18 risks. The 14 `@pytest.mark.integration` tests (deselected for live daemon) cover:
- AC-10 (UDS tool parity): live parity test present in `TestUdsIntegration`
- AC-12 (hook ping/pong): `TestHookIntegration::test_hook_ping_pong`
- AC-13 (session visible in status): `TestHookIntegration::test_hook_session_visible_in_status`

Integration tests are marked `@pytest.mark.integration` (not `xfail`). No GH issue is required — they are explicitly skipped when no daemon is present. This is the R-07 mitigation design: offline (D1–D4) and live (D5–D6) acceptance paths are separated. The pytest.ini confirms the `integration` marker is defined with the correct description. The offline acceptance gate (D1–D4) is fully validated independently.

**Why WARN and not FAIL**: The RISK-TEST-STRATEGY.md (R-07 section) explicitly states "D5/D6 fixture failure cannot block D1–D4 acceptance." This separation was a Gate 3a-approved design requirement, not a gap. The integration tests exist in source, are properly marked, and will run when a daemon fixture is available.

### Check 3: Specification Compliance

**Status**: FAIL

**Evidence**: `test_eval_offline.py` was specified in RISK-TEST-STRATEGY.md (R-07 coverage requirement: "Test suite must be structured so `pytest product/test/infra-001/tests/test_eval_offline.py` (or equivalent) passes without a daemon") and in the test plan. This file was not produced by Stage 3b. The RISK-COVERAGE-REPORT.md and tester agent report both acknowledge the gap.

The following acceptance criteria lack subprocess-level verification:

| AC | Spec Requirement | Current Coverage | Gap |
|----|-----------------|------------------|-----|
| AC-01 | `sqlite3` table name verification via subprocess | Structural only (VACUUM INTO impl, no subprocess `sqlite3` check) | Subprocess shell test absent |
| AC-02 | Shell: `unimatrix snapshot --out <live-db>` exits non-zero | Unit test `test_snapshot_path_guard_same_path` covers Rust layer | No subprocess exit-code verification |
| AC-04 | Shell: filter variants against a known snapshot | Rust unit test covers filter logic | No subprocess invocation |
| AC-05 | SHA-256 hash unchanged after `eval run` (subprocess) | Unit test confirms no writes; snapshot byte-for-byte integrity at Rust level | No end-to-end subprocess SHA-256 check |
| AC-06 | Parse result JSON from subprocess run | Rust unit test `test_output_json_schema_completeness` covers schema | No subprocess invocation |
| AC-08 | Grep section headers in Markdown output file (subprocess) | Rust unit test `test_report_contains_all_five_sections` | No subprocess invocation |
| AC-15 | `unimatrix --help` contains `snapshot`; `unimatrix eval --help` contains `scenarios`, `run`, `report` | `main.rs` wires the subcommands with correct clap help text; visual inspection confirms | No subprocess `--help` invocation |
| AC-16 | Shell: `eval run --db <live-db>` exits non-zero | Unit test `test_from_profile_returns_live_db_path_error_for_same_path` covers Rust layer | No subprocess exit-code verification |

Critically, AC-05 SHA-256 subprocess verification is the key acceptance criterion for NFR-04 (read-only enforcement). The Rust unit test `test_run_scenarios_does_not_write_to_snapshot` establishes byte-for-byte correctness within the unit test environment, but the subprocess-level integration test that exercises the full binary with a real snapshot file is absent.

**Severity assessment**: REWORKABLE. The Rust unit tests provide high confidence in correct implementation. The missing subprocess tests are a test coverage gap, not an implementation defect. The binary builds, passes all unit tests, and satisfies the spec at the code level. The test plan deliverable is incomplete.

**Note on scope**: R-07 specifically required `test_eval_offline.py` as a deliverable. The tester agent also confirmed this gap. This is not a new finding; it was carried forward from Stage 3b.

### Check 4: Architecture Compliance

**Status**: PASS

**Evidence**:
- **ADR-001 (sqlx + block_export_sync)**: `snapshot.rs` uses `block_export_sync` for async bridge; `sqlx::query("VACUUM INTO ?")` confirmed; no rusqlite import.
- **ADR-002 (AnalyticsMode suppression)**: `open_readonly()` in `db.rs` drops the analytics receiver at construction; `EvalServiceLayer.analytics_mode` field always `AnalyticsMode::Suppressed`.
- **ADR-003 (test-support feature)**: `test_kendall_tau_reachable_from_eval_runner` confirms accessibility; feature present in `Cargo.toml`.
- **ADR-004 (no new workspace crate)**: All eval modules in `crates/unimatrix-server/src/eval/` confirmed.
- **ADR-005 (nested eval subcommand)**: `main.rs` uses `Command::Eval { command: EvalCommand }` with `#[command(subcommand)]`; pre-tokio dispatch confirmed.
- **Component boundaries**: All 7 components present at expected paths. Module tree matches ARCHITECTURE.md exactly.
- **Integration points**: `ProjectPaths.socket_path` and `ProjectPaths.mcp_socket_path` correctly used in client implementations.
- **No architectural drift**: No new tables, no new workspace members, no new Python dependencies beyond stdlib.

One carried-forward WARN from Gate 3b: `open_readonly()` was added to `SqlxStore` API, which ADR-002 Option B's framing discouraged in favor of raw pool only. However, `open_readonly()` satisfies FR-24's intent (no migration, no drain task) and ADR-002's functional invariants. The method is documented with "no migrations, no drain task." This WARN is inherited from 3b and does not block 3c.

### Check 5: Knowledge Stewardship — Tester Agent

**Status**: PASS

**Evidence**: `product/features/nan-007/agents/nan-007-agent-13-tester-report.md` contains a `## Knowledge Stewardship` section with:
- `Queried:` entry: `/uni-knowledge-search` for "testing procedures gate verification integration test triage"
- `Stored:` entry: "nothing novel to store — the offline/live test partitioning pattern and mocked-socket send-capture architecture are nan-007-specific. No cross-feature promotion yet."

Both required fields are present with rationale.

---

## Integration Test Validation

**Smoke tests (mandatory)**: 20/20 passed — gate confirmed.

**Live integration tests (14 deferred)**:
- Not xfail — correctly marked `@pytest.mark.integration` (a skip-when-no-daemon pattern, not a known-failure marker).
- No GH issue required for this pattern. These tests are expected to run in environments with a live daemon.
- The 14 deselected tests cover AC-10, AC-11 (live), AC-12, AC-13, AC-14 (live) — all Group 2 (D5/D6) acceptance criteria. Group 1 (D1–D4) is fully independent.
- xfail registry: one pre-existing xfail (`test_retrospective_baseline_present`, GH#305). Not introduced by nan-007.
- No integration tests were deleted or commented out.
- Integration test counts in RISK-COVERAGE-REPORT.md are accurate (145 run, 144 passed, 1 xfailed, 14 deselected).

**test_eval_offline.py**: Absent. This is the primary FAIL. See Check 3.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| `test_eval_offline.py` absent — R-07 deliverable missing; AC-01, AC-02 (subprocess), AC-04, AC-05 (SHA-256 subprocess), AC-06, AC-08, AC-15, AC-16 lack subprocess-level verification | `uni-rust-dev` or `uni-tester` | Create `product/test/infra-001/tests/test_eval_offline.py` with subprocess invocations of: (1) `unimatrix snapshot --out` producing a valid SQLite file with correct tables (AC-01); (2) `unimatrix snapshot --out <live-db>` returning non-zero exit (AC-02); (3) `unimatrix eval scenarios --retrieval-mode mcp/uds/all` against a known snapshot (AC-04); (4) SHA-256 unchanged after `eval run` (AC-05); (5) result JSON fields present after `eval run` (AC-06); (6) report section headers present (AC-08); (7) `unimatrix --help` / `unimatrix eval --help` contain correct subcommand names (AC-15); (8) `eval run --db <live-db>` returning non-zero exit (AC-16). Tests must pass without a running daemon. Use the compiled binary via subprocess. |

---

## Scope Concerns

None. This is a test coverage gap in a deliverable (test file), not a scope or architecture failure.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` before this validation to check for existing validation patterns — no tool available in this context; proceeded with direct artifact analysis.
- Stored: nothing novel to store — the offline/live separation pattern for eval harness testing is nan-007-specific. If this pattern recurs in future eval-adjacent features, it should be promoted to Unimatrix at that time via the retro.
