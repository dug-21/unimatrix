# Risk Coverage Report: vnc-005

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `Arc::try_unwrap(store)` fails at graceful shutdown | `test_shutdown_drops_release_all_store_refs`, `test_shutdown_fails_without_service_layer_drop`, `test_server_clone_arc_count_drops_after_join`, `test_try_unwrap_succeeds_when_sole_owner`, `test_try_unwrap_fails_with_outstanding_refs` | PASS | Full |
| R-02 | Race between SIGTERM and session task spawning | `test_shutdown_token_breaks_accept_loop`, `test_sequential_cycles_vec_bounded` (accept loop join drain), `test_session_cap_enforced_at_32` | PASS (unit) | Partial — stress race test requires `test_daemon.py` (process-level) |
| R-03 | `graceful_shutdown` called on session EOF kills daemon | `test_lifecycle_handles_has_vnc005_fields`, `test_drop_ordering_mcp_before_hook_ipc`, RV-09 grep (2 call sites: daemon path + stdio path, each correct) | PASS | Full — daemon path call site is only reachable from `daemon_token.cancelled()`, never from session EOF |
| R-04 | Double-daemon on macOS stale PID | `test_run_stop_returns_1_when_stale_pid`, `test_run_stop_returns_1_when_no_pid_file` (unit) | PASS (unit) | Partial — full double-daemon prevention test requires `test_daemon.py` |
| R-05 | Concurrent drain/upsert data loss | `test_concurrent_upsert_drain_no_data_loss` (4 writers + drain, 1000 entries, no loss) | PASS | Full |
| R-06 | MCP socket created without 0600 permissions | `test_start_mcp_uds_listener_permissions_0600` (asserts mode==0o600, group-read==0, other-read==0) | PASS | Full |
| R-07 | `CallerId::UdsSession` exemption boundary missing | `test_uds_session_rate_exemption_boundary`, `test_c07_comment_presence_in_gateway`, RV-11 grep (C-07 and W2-2 at `gateway.rs:57-58`) | PASS | Full |
| R-08 | Bridge auto-start timeout with no actionable error | `test_timeout_error_contains_log_path` (bridge.rs), `test_timeout_error_is_project_init_variant`, `test_bridge_constants_match_spec` | PASS | Full (unit); process-level end-to-end in `test_daemon.py` |
| R-09 | Stale MCP socket blocks new daemon bind | `test_stale_socket_unlinked_before_bind` (pre-creates stale file, asserts bind succeeds) | PASS | Full |
| R-10 | Session handle Vec grows unboundedly | `test_retain_sweep_bounds_vec_size` (100 aborted handles → 0 after retain), `test_sequential_cycles_vec_bounded` (20 cycles, clean shutdown) | PASS | Full |
| R-11 | 33rd connection silently dropped — client retry | `test_session_cap_enforced_at_32` (32 live + 33rd gets EOF within 2s timeout) | PASS | Full |
| R-12 | `unimatrix serve --stdio` no longer exits on stdin close | `test_graceful_shutdown` (infra-001 smoke, PASS), `test_server_process_cleanup` (PASS) | PASS | Full (integration) — stdio path has separate `graceful_shutdown` call site (line 810) distinct from daemon path (line 551) |
| R-13 | Hook path accidentally initializes Tokio runtime | `test_run_stop_is_synchronous`, `test_no_tokio_runtime_in_daemon_module` (compile check) | PASS | Full (unit) |
| R-14 | Socket path exceeds 104-byte macOS limit | `test_validate_socket_path_length_ok_at_103`, `test_validate_socket_path_length_err_at_104`, `test_validate_socket_path_length_err_at_107`, `test_start_mcp_uds_listener_rejects_oversized_path`, `test_validate_socket_path_null_byte_rejected` | PASS | Full |
| R-15 | Per-bucket 1000-entry cap eviction under concurrent upsert | `test_concurrent_upsert_drain_no_data_loss` (1000-entry cap path exercised), cap boundary within `upsert` (evicts min rework_flag_count inside Mutex) | PASS | Full |
| R-16 | `mcp_socket_guard` not dropped before `graceful_shutdown` returns | `test_drop_ordering_mcp_before_hook_ipc` (verifies take() ordering), `test_lifecycle_handles_has_vnc005_fields` (both fields are Option) | PASS (unit) | Partial — SIGTERM socket-cleanup end-to-end requires `test_daemon.py` |
| R-17 | `--daemon-child` visible in help output | `test_daemon_child_hidden_from_help`, `test_daemon_child_hidden_from_serve_help`, `#[arg(hide=true)]` verified in source | PASS | Full |
| R-18 | TTL eviction races with `drain_for` | `test_evict_and_drain_no_double_free` (evict then drain → empty, no panic), Mutex invariant: both ops use `lock().unwrap_or_else()` wrapping | PASS | Full |

---

## Test Results

### Unit Tests

- Total: 2608
- Passed: 2608
- Failed: 0
- Ignored: 19 (pre-existing — unrelated to vnc-005)

All unit tests pass across the full workspace.

### Integration Tests (infra-001)

**Harness fix applied**: `harness/client.py` updated to invoke `unimatrix serve --stdio` instead of bare `unimatrix` (no-subcommand now defaults to bridge mode per vnc-005 design). Two additional direct subprocess invocations in `suites/test_security.py` (S-31, S-32) and one in `suites/test_lifecycle.py` also updated. These are tests caused by the feature's invocation change — not pre-existing bugs.

| Suite | Tests | Passed | Failed | xfailed |
|-------|-------|--------|--------|---------|
| smoke | 20 | 19 | 0 | 1 (GH#111, pre-existing) |
| protocol | 13 | 13 | 0 | 0 |
| tools | 73 | 69 | 0 | 4 (pre-existing) |
| lifecycle | 25 | 23 | 0 | 2 (pre-existing) |
| edge_cases | 24 | 23 | 0 | 1 (pre-existing) |
| security | 17 | 17 | 0 | 0 |
| confidence | 14 | 14 | 0 | 0 |
| contradiction | 12 | 12 | 0 | 0 |

**Integration Total: 198 tests run, 190 passed, 0 failed, 8 xfailed (all pre-existing)**

---

## Static Verification Results (RV Items)

| RV-ID | Check | Result |
|-------|-------|--------|
| RV-03 | `--daemon-child` not in `--help` or `serve --help` | PASS — `test_daemon_child_hidden_from_help`, `test_daemon_child_hidden_from_serve_help` pass; `#[arg(hide=true)]` present at `main.rs:46` |
| RV-09 | `graceful_shutdown` call sites: exactly one daemon path call, not from `QuitReason::Closed` | PASS — Two call sites: line 551 (`tokio_main_daemon`, reachable only from `daemon_token.cancelled()`) and line 810 (`tokio_main_stdio`). Session EOF in daemon path reaches `running.waiting().await` only; daemon continues. Correct per IMPLEMENTATION-BRIEF C-05. |
| RV-11 | `CallerId::UdsSession` exemption comment references C-07 and W2-2 | PASS — `services/gateway.rs:57`: `// C-07 (vnc-005): UdsSession exemption is local-only (UDS file-system gated).` and line 58: `// Must NOT extend to HTTP transport callers (W2-2).` |

---

## Gaps

### Process-Level Tests Requiring `suites/test_daemon.py` (Not Yet Implemented)

The following ACs and RVs require a live daemon process and cannot be exercised by the current infra-001 harness (which uses stdio transport). These gaps are **known and planned** — the test plan OVERVIEW.md explicitly identifies them as requiring a new `suites/test_daemon.py` module.

| Gap | AC/RV | Risk | Status |
|-----|-------|------|--------|
| Daemon start/stop lifecycle (socket exists, PID alive) | AC-01, AC-07 | R-04 | Not yet implemented — requires `daemon_server` fixture |
| Bridge auto-start end-to-end (no daemon → spawns → connects) | AC-05, AC-06 | R-08, R-04 | Not yet implemented |
| Daemon survival after client disconnect | AC-04 | R-03 | Not yet implemented (unit-level covered by R-03 grep) |
| Concurrent multi-session store+retrieve | AC-14 | R-01, R-05 | Not yet implemented |
| Cross-session accumulator accumulation (AC-17/18) | AC-17, AC-18 | R-05, R-18 | Not yet implemented |
| Session cap at 32 (end-to-end with real daemon) | AC-20 | R-11 | Covered at unit level (`test_session_cap_enforced_at_32`) |
| Socket file cleanup after SIGTERM | RV-07 | R-16 | Not yet implemented (unit drop ordering covered) |
| SIGKILL + stale file → next start cleans up | RV-08 | R-09, R-16 | Not yet implemented |
| `unimatrix stop` exit codes | AC-10, AC-11 | — | AC-11 covered at unit level; AC-10 needs live daemon |
| Hook IPC unaffected by daemon | AC-13 | R-13 | Not yet implemented |
| `serve --stdio` no MCP socket created | AC-12 | R-12 | Covered indirectly by smoke (no daemon mode in existing harness) |

**GH Issues are NOT filed for these gaps** because they are pre-planned implementation tasks for `suites/test_daemon.py`, not discovered pre-existing failures. The unit-level coverage for the underlying invariants (socket permissions, Arc::try_unwrap, retain sweep, cap enforcement) is comprehensive.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PARTIAL | Daemon start verified at unit level (poll constants, arg construction); process-level socket creation requires `test_daemon.py` |
| AC-02 | PASS | `test_start_mcp_uds_listener_permissions_0600` — asserts mode==0o600, group-read==0, other-read==0 |
| AC-03 | PARTIAL | Bridge byte forwarding verified (unit: `test_do_bridge_returns_ok_when_peer_closes`, `test_try_connect_succeeds_against_live_listener`); full MCP JSON-RPC roundtrip requires `test_daemon.py` |
| AC-04 | PARTIAL | Daemon path does NOT call `graceful_shutdown` on session EOF (RV-09 verified); process-level daemon survival requires `test_daemon.py` |
| AC-05 | PARTIAL | Auto-start logic verified at unit level (bridge timeout error, PID check, launcher constants); end-to-end requires `test_daemon.py` |
| AC-06 | PARTIAL | PID check logic covered (`test_run_stop_returns_1_when_stale_pid`); no-duplicate-spawn end-to-end requires `test_daemon.py` |
| AC-07 | PARTIAL | PidGuard flock prevents second daemon; CLI parse verified (`test_serve_daemon_subcommand_parsed`); process-level requires `test_daemon.py` |
| AC-08 | PARTIAL | `test_compact_succeeds_after_unwrap`, `test_shutdown_drops_release_all_store_refs` (unit); SIGTERM + data persistence end-to-end requires `test_daemon.py` |
| AC-09 | PARTIAL | Signal handling shares same `shutdown_signal()` as AC-08 path; SIGINT end-to-end requires `test_daemon.py` |
| AC-10 | PARTIAL | `run_stop` synchronous verified (`test_run_stop_is_synchronous`); live daemon stop requires `test_daemon.py` |
| AC-11 | PASS | `test_run_stop_returns_1_when_no_pid_file`, `test_run_stop_returns_1_when_stale_pid` (unit) |
| AC-12 | PASS | infra-001 smoke `test_graceful_shutdown` PASS (stdio mode exits cleanly); `test_server_process_cleanup` PASS; harness now invokes `serve --stdio` |
| AC-13 | PARTIAL | Hook path is synchronous (`test_run_stop_is_synchronous`, `test_no_tokio_runtime_in_daemon_module`); live hook IPC against daemon requires `test_daemon.py` |
| AC-14 | PARTIAL | Concurrent accumulator correctness verified (`test_concurrent_upsert_drain_no_data_loss`); multi-session against live daemon requires `test_daemon.py` |
| AC-15 | PASS | `test_timeout_error_contains_log_path`, `test_timeout_error_is_project_init_variant` — error contains log path, timeout seconds, "daemon log" direction |
| AC-16 | PASS | `test_stale_socket_unlinked_before_bind` — pre-created stale file removed, bind succeeds |
| AC-17 | PARTIAL | `drain_for` semantics verified at unit level (`test_evict_and_drain_no_double_free`); cross-session accumulation requires `test_daemon.py` |
| AC-18 | PARTIAL | Second drain returns empty verified (`test_evict_and_drain_no_double_free`); sequential bridge sessions require `test_daemon.py` |
| AC-19 | PASS | `test_no_tokio_runtime_in_daemon_module` (compile check); `prepare_daemon_child` is a sync `fn`; `daemon.rs` has no `use tokio::` imports; setsid called before `tokio_main_daemon` |
| AC-20 | PASS | `test_session_cap_enforced_at_32` — 33rd stream sees EOF within 2s, 32 existing sessions unaffected |

---

## Harness Changes Made (This Report)

These changes were caused by vnc-005's default invocation change (no-subcommand → bridge mode):

1. `product/test/infra-001/harness/client.py` — Added `"serve", "--stdio"` to subprocess args (line 96)
2. `product/test/infra-001/suites/test_security.py` — Updated 2 raw subprocess invocations (S-31, S-32) to use `serve --stdio`
3. `product/test/infra-001/suites/test_lifecycle.py` — Updated 1 raw subprocess invocation to use `serve --stdio`

None of these changes remove or modify test assertions — they preserve the original test intent while adapting to the new CLI routing.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for testing procedures (daemon UDS integration) — found #1928 "Daemon-Mode Integration Test Fixture Pattern" already stored by an earlier agent in this feature session. No duplicate to store.
- Stored: nothing novel to store — the harness `serve --stdio` fix pattern is already captured in the implementation behavior, and the fixture pattern for `daemon_server` was already stored as #1928. The key insight (raw subprocess invocations in test suites must be updated when default binary invocation changes) is a minor operational note rather than a reusable pattern.
