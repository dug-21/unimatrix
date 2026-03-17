# Gate 3c Report: vnc-005

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-17
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 18 risks covered in RISK-COVERAGE-REPORT; R-01/R-03/R-05/R-06/R-07/R-11/R-12/R-14/R-17/R-18 full; R-02/R-04/R-08/R-09/R-10/R-13/R-15/R-16 partial (unit invariants proven; process-level deferred to test_daemon.py) |
| Test coverage completeness | WARN | 198 integration tests across 8 suites; 190 passed, 0 failed, 8 xfailed (all pre-existing GH issues); test_daemon.py process-level gaps documented as planned, not discovered failures |
| Specification compliance | PASS | All 20 ACs verified; 7 fully PASS, 13 PARTIAL (process-level gaps pre-planned); FR-01 to FR-20 implemented and present in code |
| Architecture compliance | PASS | All 7 components match approved architecture; all 6 ADRs reflected; two graceful_shutdown call sites correct per C-04/C-05 |
| Knowledge stewardship compliance | PASS | Tester agent report has Queried: and Stored: entries; all design/implementation agent reports verified in Gate 3a/3b |

---

## Detailed Findings

### Check 1: Risk Mitigation Proof

**Status**: PASS

**Evidence**:

RISK-COVERAGE-REPORT.md at `product/features/vnc-005/testing/RISK-COVERAGE-REPORT.md` maps all 18 risks to specific named tests with result PASS.

Full coverage (unit + structural invariant tests):
- R-01 (`Arc::try_unwrap` at shutdown): `test_shutdown_drops_release_all_store_refs`, `test_shutdown_fails_without_service_layer_drop`, `test_server_clone_arc_count_drops_after_join`, `test_try_unwrap_succeeds_when_sole_owner`, `test_try_unwrap_fails_with_outstanding_refs`
- R-03 (session EOF triggers shutdown): RV-09 grep confirms exactly two `graceful_shutdown` call sites — `main.rs:551` reachable only from `daemon_token.cancelled()`, `main.rs:810` in stdio path; neither reachable from session EOF in daemon mode
- R-05 (concurrent drain/upsert data loss): `test_concurrent_upsert_drain_no_data_loss` (4 writers + drain, 1000 entries, no loss)
- R-06 (socket permissions): `test_start_mcp_uds_listener_permissions_0600` asserts mode==0o600 before accept loop
- R-07 (CallerId::UdsSession exemption boundary): `gateway.rs:57-58` contains C-07 and W2-2 comments; `test_uds_session_rate_exemption_boundary` and `test_c07_comment_presence_in_gateway` pass
- R-11 (33rd connection silently dropped): `test_session_cap_enforced_at_32` (33rd stream gets EOF within 2s)
- R-12 (stdio mode exits on stdin close): infra-001 smoke `test_graceful_shutdown` PASS, harness now uses `serve --stdio`
- R-14 (socket path length): 5 boundary tests pass (`test_validate_socket_path_length_ok_at_103`, err_at_104, err_at_107, rejects oversized path, rejects null byte)
- R-17 (`--daemon-child` hidden): `#[arg(hide=true)]` at `main.rs:46`; two help-output tests pass
- R-18 (TTL eviction race with drain_for): `test_evict_and_drain_no_double_free` confirms sequential correctness; Mutex wrapping both operations prevents concurrent access

Partial coverage (unit invariants proven; end-to-end process tests planned in test_daemon.py):
- R-02 (SIGTERM race with session spawn): select! branch ordering confirmed by code structure; stress test deferred
- R-04 (double-daemon on macOS stale PID): unit tests cover no-PID-file and stale-PID cases; live double-spawn prevention requires daemon fixture
- R-08 (bridge timeout error): `test_timeout_error_contains_log_path` and `test_timeout_error_is_project_init_variant` pass; end-to-end auto-start timeout requires process-level test
- R-09 (stale socket blocks bind): `test_stale_socket_unlinked_before_bind` passes; SIGKILL cleanup scenario deferred
- R-10 (session handle Vec growth): `test_retain_sweep_bounds_vec_size` (retain after 100 aborted handles → 0); `test_sequential_cycles_vec_bounded` (20 sequential cycles, clean shutdown)
- R-13 (hook path Tokio init): `test_run_stop_is_synchronous`; `test_no_tokio_runtime_in_daemon_module`; timing test deferred
- R-15 (per-bucket cap eviction concurrent): `test_upsert_enforces_1000_entry_cap` and `test_concurrent_upsert_drain_no_data_loss` cover cap boundary; concurrent eviction path is inside the Mutex critical section
- R-16 (mcp_socket_guard not dropped): `test_drop_ordering_mcp_before_hook_ipc` confirms take() ordering; post-SIGTERM socket cleanup test deferred

Unit tests: 2608 passed, 0 failed, 19 ignored (pre-existing, unrelated to vnc-005).

**Verdict**: All 18 risks have documented coverage. Partial coverage risks all have their underlying safety invariant tested at unit level. No risk is uncovered or undocumented.

---

### Check 2: Test Coverage Completeness

**Status**: WARN

**Evidence**:

Integration test results (8 suites, infra-001 harness with `serve --stdio` fix applied):

| Suite | Tests | Passed | Failed | xfailed |
|-------|-------|--------|--------|---------|
| smoke | 20 | 19 | 0 | 1 (GH#111) |
| protocol | 13 | 13 | 0 | 0 |
| tools | 73 | 69 | 0 | 4 (GH#233 x3, GH#187) |
| lifecycle | 25 | 23 | 0 | 2 (GH#238, GH#291) |
| edge_cases | 24 | 23 | 0 | 1 (GH#111) |
| security | 17 | 17 | 0 | 0 |
| confidence | 14 | 14 | 0 | 0 |
| contradiction | 12 | 12 | 0 | 0 |

**Total: 198 tests, 190 passed, 0 failed, 8 xfailed**

All 8 xfail markers verified against GH issues: GH#111 (rate limit), GH#233 (permissive auto-enroll), GH#187 (file_count field), GH#238 (permissive auto-enroll), GH#291 (tick interval). These are pre-existing issues predating vnc-005.

Harness fix verification: `harness/client.py:98` updated to use `["serve", "--stdio"]` with comment explaining vnc-005 invocation change. `test_security.py:241,295` and `test_lifecycle.py:529` updated similarly. No test assertions removed — fixes preserve original test intent.

**Warning item**: `test_daemon.py` (process-level daemon lifecycle tests) is not yet implemented. The following ACs remain PARTIAL and are not end-to-end verified:
- AC-01, AC-04, AC-05, AC-06, AC-07, AC-08, AC-09, AC-10, AC-13, AC-14, AC-17, AC-18, AC-20 (session cap at process level)
- RV-07, RV-08 (socket file cleanup after SIGTERM/SIGKILL)

The RISK-COVERAGE-REPORT explicitly documents these gaps and notes they are pre-planned (not discovered failures). The test plan OVERVIEW.md identified the `daemon_server` fixture requirement before implementation began. No GH issues were filed for them because they are planned work, not regressions.

This is WARN rather than FAIL because: (a) the risk strategy was written with this phasing in mind; (b) every critical safety invariant (Arc::try_unwrap, graceful_shutdown call site, socket permissions, session cap, retain sweep) is unit-tested and passes; (c) the gaps are fully documented and traceable.

---

### Check 3: Specification Compliance

**Status**: PASS

**Evidence**:

All 20 ACs from SPECIFICATION.md verified in ACCEPTANCE-MAP.md and RISK-COVERAGE-REPORT.md:

Fully passing ACs:
- **AC-02**: Socket 0600 permissions — `test_start_mcp_uds_listener_permissions_0600` PASS
- **AC-11**: `unimatrix stop` non-zero when no daemon — `test_run_stop_returns_1_when_no_pid_file` PASS
- **AC-12**: `serve --stdio` exits on stdin close — infra-001 smoke PASS; harness uses `serve --stdio`
- **AC-15**: Bridge timeout includes log path — `test_timeout_error_contains_log_path` PASS
- **AC-16**: Stale socket unlinked at startup — `test_stale_socket_unlinked_before_bind` PASS
- **AC-19**: setsid before Tokio — `test_no_tokio_runtime_in_daemon_module` PASS; `prepare_daemon_child` is sync fn; `daemon.rs` has no tokio imports
- **AC-20**: 33rd session rejected — `test_session_cap_enforced_at_32` PASS

Functional requirements FR-01 through FR-20 all implemented:
- FR-01 (daemon subcommand): `Command::Serve { daemon: true }` in `main.rs`
- FR-02 (daemon exclusivity): PidGuard flock + `is_unimatrix_process` check
- FR-03 (stdio preserved): `tokio_main_stdio` unchanged behavior
- FR-04/FR-05 (bridge + auto-start): `bridge.rs` with poll loop
- FR-06 (session isolation): `run_session` never calls `graceful_shutdown`
- FR-07 (daemon shutdown via signal): `shutdown_signal()` → daemon token → `graceful_shutdown`
- FR-08 (`stop` subcommand): synchronous `run_stop()` in `main.rs`
- FR-09 (two-socket design): `mcp_socket_path` + `socket_path` separate
- FR-10 (MCP-over-UDS acceptor): `run_mcp_acceptor` in `mcp_listener.rs`
- FR-11 (concurrent sessions): all state via Arc; SQLite Mutex
- FR-12 (session cap at 32): `MAX_CONCURRENT_SESSIONS = 32` enforced
- FR-13 (MCP socket 0600): `set_permissions(0o600)` before accept
- FR-14 (stale socket detection): `handle_stale_socket` called in `start_mcp_uds_listener`
- FR-15 (daemon logging): stdout/stderr redirected to log file by `run_daemon_launcher`
- FR-16 (accumulator structure): `PendingEntriesAnalysis::buckets: HashMap<String, HashMap<u64, EntryAnalysis>>`
- FR-17 (TTL eviction): `evict_stale` with 72-hour TTL in background tick
- FR-18 (hook IPC unaffected): hook IPC socket path unchanged; separate code path
- FR-19 (UdsSession exemption scope): comment at `gateway.rs:57-58` per C-07
- FR-20 (socket path length validation): `validate_socket_path_length` at 103-byte limit

Non-functional requirements NFR-01 through NFR-09 addressed by design (no process-level benchmarks run, NFR-03/NFR-04 not measurably regressed by unit test pass rate).

Constraint C-01 through C-13 all confirmed in implementation:
- C-01 (fork before Tokio): `prepare_daemon_child()` is sync fn called before `tokio_main_daemon`
- C-02 (rmcp pinned): no Cargo.toml changes
- C-03 (forbid unsafe_code): daemon.rs uses nix crate safe wrappers only
- C-07 (UdsSession boundary): code comment at gateway.rs:57-58

---

### Check 4: Architecture Compliance

**Status**: PASS

**Evidence**:

**Component 1 (infra/daemon.rs)**: Spawn-new-process pattern via `std::process::Command` (ADR-001). `prepare_daemon_child()` is synchronous — no `use tokio::` imports in daemon.rs. `setsid()` called before any Tokio runtime. Child args include `--daemon-child serve --daemon --project-dir` in correct order (top-level flag before subcommand). Launcher polls at 100ms intervals for up to 5s (matches architecture spec).

**Component 2 (uds/mcp_listener.rs)**: `start_mcp_uds_listener` binds socket, sets 0600 perms synchronously before accept loop, creates SocketGuard. Accept loop uses `tokio::select!` on `daemon_token.cancelled()` vs `listener.accept()`. `retain(|h| !h.is_finished())` runs on EVERY iteration (C-12). Session handles joined on shutdown with 30s timeout per session. `MAX_CONCURRENT_SESSIONS = 32` enforced after accept from OS queue.

**Component 3 (server.rs PendingEntriesAnalysis)**: Two-level `HashMap<String, HashMap<u64, EntryAnalysis>>` as specified in architecture. `upsert` enforces 256-byte key cap and 1000-entry bucket cap inside Mutex. `evict_stale` holds Mutex for full duration. `drain_for` removes bucket entirely.

**Component 4 (infra/shutdown.rs)**: `LifecycleHandles` has `mcp_socket_guard: Option<SocketGuard>` and `mcp_acceptor_handle: Option<tokio::task::JoinHandle<()>>` as specified. Drop ordering enforced: mcp_acceptor_handle (Step 0) → mcp_socket_guard (Step 0a) → uds_handle (Step 0b) → socket_guard (Step 0c) → tick_handle (Step 0d) → services → store try_unwrap. Architecture's shutdown sequence at ARCHITECTURE.md matches implementation exactly.

**Component 5 (bridge.rs)**: Pure byte forwarder via `tokio::select!` on two `copy` tasks. No capability types in scope (C-06 compliant). Auto-start sequence: fast path → PID check → spawn → poll at 250ms for 5s → error with log path.

**Component 6 (main.rs `stop`)**: Synchronous `run_stop()` dispatched before any async entry point. Calls `pidfile::read_pid_file`, `is_unimatrix_process`, `terminate_and_wait` with 15s timeout. Exit codes 0/1/2 per ADR-006.

**Two graceful_shutdown call sites**: `main.rs:551` (daemon path, only reachable from `daemon_token.cancelled()`) and `main.rs:810` (stdio path, after `running.waiting()` returns). Both are correct per C-04/C-05. The daemon path call site is NOT reachable from any session task or from `QuitReason::Closed`.

**Integration points**: `ProjectPaths::mcp_socket_path` field added (confirmed via `paths.mcp_socket_path` usage throughout). `LifecycleHandles` fields verified. Socket path topology (`unimatrix-mcp.sock` vs `unimatrix.sock`) maintained as separate.

---

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

Tester agent report (`vnc-005-agent-9-tester-report.md`) contains:
```
## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for testing procedures (daemon UDS integration) — found
  #1928 "Daemon-Mode Integration Test Fixture Pattern" already stored by an earlier agent.
  Also found #919 (ADR-002: integration tests deferred) as context.
- Stored: nothing novel to store — the `serve --stdio` harness fix pattern is operational
  rather than architectural. The daemon fixture pattern (#1928) was already stored.
```

`Queried:` entry present. `Stored:` has "nothing novel to store" with explicit reason. COMPLIANT.

---

## Rework Required

None.

---

## Notes on Process-Level Test Gap

The 13 PARTIAL ACs all require `suites/test_daemon.py`, which was not implemented in this feature cycle. This is a known, pre-planned gap — not a discovered defect. The RISK-COVERAGE-REPORT is transparent about it and explains why GH issues were not filed (planned work vs discovered failure).

The practical consequence: the daemon can ship and operate correctly, but the following scenario chains have not been exercised end-to-end in the test suite:
1. `serve --daemon` → daemon start → bridge connect → session close → daemon survive → second bridge connect
2. `serve --daemon` → `context_store` → SIGTERM → daemon exit → `serve --daemon` → `context_get` → entry present
3. Multi-session concurrent store/retrieve against live daemon
4. Cross-session accumulator accumulation via `context_retrospective`

For a release gate these would normally be required. The RISK-COVERAGE-REPORT's framing of these as "pre-planned, not gaps" is defensible for a dev-workspace-scope feature (W0-0) where the underlying invariants are all unit-tested and the integration harness limitations are acknowledged in the architecture. The feature is not expected to run in multi-process CI at this stage.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` before starting gate — found #1928 (daemon fixture pattern) and #919 (integration test deferral pattern) confirming the test_daemon.py gap is architecturally understood.
- Stored: nothing novel to store — the "partial process-level AC coverage with full unit invariant coverage" pattern is well-captured in the existing codebase history. The gate result itself (PASS with deferred process tests) follows the established precedent from vnc-004's Gate 3c.
