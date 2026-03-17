# Gate 3b Report: vnc-005

> Gate: 3b (Code Review) — rework iteration 1
> Date: 2026-03-17
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All components match pseudocode; upsert overwrite semantics correctly resolves WARN-3 from gate-3a |
| Architecture compliance | PASS | All ADRs followed; spawn-new-process, CancellationToken, Clone model all implemented correctly |
| Interface implementation | PASS | All function signatures match; ProjectPaths, LifecycleHandles, PendingEntriesAnalysis all correct |
| Test case alignment | PASS | All test plan scenarios have corresponding tests across all 6 components |
| Code quality | WARN | server.rs is 2996 lines (pre-existing violation; vnc-005 added ~514 lines to already-over-limit file) |
| Security | PASS | No hardcoded secrets; 0600 socket perms; path length validated; no command injection |
| Knowledge stewardship | PASS | All 6 rust-dev agents have filed reports with Queried + Stored/declined entries |

## Detailed Findings

### Check 1: Pseudocode Fidelity

**Status**: PASS

**Evidence**:

- `infra/daemon.rs`: `run_daemon_launcher` and `prepare_daemon_child` match pseudocode exactly. Child args ordering (`--daemon-child` before `serve`) correctly resolved at implementation time.
- `uds/mcp_listener.rs`: `start_mcp_uds_listener` → validate path → handle stale → bind → chmod 0600 → SocketGuard → spawn acceptor. Matches pseudocode step-for-step.
- `run_mcp_acceptor`: `retain(|h| !h.is_finished())` runs on EVERY iteration (line 138) before `select!`. R-10 enforced.
- `run_session`: Does NOT call `graceful_shutdown`. Bridges `child_token` into rmcp cancellation token. ADR-002 / C-04 compliant.
- `server.rs` `PendingEntriesAnalysis`: Two-level `HashMap<String, FeatureBucket>` with `upsert`, `drain_for`, `evict_stale`. WARN-3 from gate-3a correctly resolved: implementation uses **overwrite** semantics (`bucket.entries.insert(analysis.entry_id, analysis)`) matching the specification (FR-16: "overwrite rather than duplicate") and test plan (T-ACCUM-U-02).
- `infra/shutdown.rs`: `mcp_socket_guard` and `mcp_acceptor_handle` added to `LifecycleHandles`. Explicit drop sequence in `graceful_shutdown` matches pseudocode.
- `bridge.rs`: `run_bridge(paths: &ProjectPaths)` — uses full `ProjectPaths` (correct resolution of WARN-1 from gate-3a). Auto-start, PID check, poll loop, and timeout error all match pseudocode.
- `main.rs`: Dispatch ordering matches pseudocode (Hook → Export → Import → Version → ModelDownload → Stop → Serve → None/bridge). C-01 enforced: `prepare_daemon_child()` called at lines 213 and 237 before `tokio_main_daemon()`.

**One deviation (accepted)**: The pseudocode `run_daemon_launcher` shows `--project-dir` passed conditionally. The implementation always passes it — more conservative and correct; the pseudocode note itself says "always pass it to avoid misdetection."

### Check 2: Architecture Compliance

**Status**: PASS

**Evidence**:

- **ADR-001 (spawn-new-process)**: No `fork()`. `std::process::Command::new(current_exe)` in `run_daemon_launcher`. `nix::unistd::setsid()` in `prepare_daemon_child` before any Tokio init. `daemon.rs` has no tokio imports; `#[tokio::main]` only in `tokio_main_daemon()` which runs after `prepare_daemon_child()` returns.
- **ADR-002 (session/daemon lifetime)**: `CancellationToken` created at daemon startup via `shutdown::new_daemon_token()`. Sessions receive child tokens. `daemon_token.cancelled().await` is the daemon lifetime boundary. Session tasks do not cancel the daemon token.
- **ADR-003 (UnimatrixServer Clone)**: `server.clone()` in the spawn closure (mcp_listener.rs). `ServiceLayer::new` called exactly once per process path. Never inside a session task.
- **ADR-004 (accumulator)**: `HashMap<String, FeatureBucket>` with `entries: HashMap<u64, EntryAnalysis>`. 1000-entry cap with lowest-rework eviction. 72h TTL eviction in background tick.
- **ADR-005 (accept loop topology)**: Single acceptor task, per-connection spawned tasks, `AtomicUsize` counter-based cap at 32, `retain` sweep every iteration.
- **ADR-006 (stop subcommand)**: Synchronous, no Tokio, exit codes 0/1/2, 15s timeout.
- **Component boundaries**: `unimatrix-engine/src/project.rs` has `mcp_socket_path: PathBuf` field; `infra/daemon.rs`, `uds/mcp_listener.rs`, `bridge.rs` are all new modules in correct locations.

### Check 3: Interface Implementation

**Status**: PASS

**Evidence**:

All critical checks from the IMPLEMENTATION-BRIEF verified:

| Check | Verified |
|-------|---------|
| C-01: `setsid()` before Tokio runtime | `prepare_daemon_child()` at main.rs lines 213/237; `tokio_main_daemon` entered only after return |
| C-04/C-05: Exactly one `graceful_shutdown` call per process path | Two call sites total: daemon path (after `daemon_token.cancelled().await`) and stdio path (after `running.waiting().await`). Zero calls in session tasks. |
| C-07: UdsSession exemption comment references "C-07" and "W2-2" | Present in gateway.rs lines 57-60 |
| C-10: Hook dispatch before async code | First match arm in `main()`; no Tokio runtime before `Command::Hook` |
| Drop ordering | `graceful_shutdown` explicit: mcp_acceptor_handle → mcp_socket_guard → uds_handle → socket_guard → tick_handle → ServiceLayer → store. PidGuard drops last as main() local. |
| R-10: retain sweep every iteration | `session_handles.retain(|h| !h.is_finished())` at mcp_listener.rs line 138, inside loop before `select!` |
| Upsert: OVERWRITE semantics | `bucket.entries.insert(analysis.entry_id, analysis)`. Test `test_upsert_overwrites_existing_entry` confirms. |
| RV-03: `--daemon-child` hidden from `--help` | `#[arg(long, hide = true)]`. Two tests in main_tests.rs confirm absence from output. |

All interface signatures verified:

| Interface | Spec | Implementation |
|-----------|------|----------------|
| `ProjectPaths::mcp_socket_path` | `PathBuf`, `data_dir.join("unimatrix-mcp.sock")` | project.rs: `let mcp_socket_path = data_dir.join("unimatrix-mcp.sock")` |
| `LifecycleHandles::mcp_socket_guard` | `Option<SocketGuard>` | shutdown.rs |
| `LifecycleHandles::mcp_acceptor_handle` | `Option<JoinHandle<()>>` | shutdown.rs |
| `PendingEntriesAnalysis::buckets` | `HashMap<String, FeatureBucket>` | server.rs |
| `upsert(feature_cycle, analysis)` | overwrite semantics | server.rs |
| `drain_for(feature_cycle)` | returns `Vec<EntryAnalysis>`, removes bucket | server.rs |
| `start_mcp_uds_listener(path, server, token)` | `async fn -> Result<(JoinHandle<()>, SocketGuard), ServerError>` | mcp_listener.rs |
| `run_bridge(paths)` | `async fn(&ProjectPaths) -> Result<(), ServerError>` | bridge.rs |
| `Cli::Command::Stop` | new variant | main.rs |
| `--daemon-child` hidden | `#[arg(long, hide = true)]` | main.rs |

### Check 4: Test Case Alignment

**Status**: PASS

**Evidence**:

All test plan scenarios have corresponding tests. Summary by component:

**Daemonizer**: T-DAEMON-U-01, T-DAEMON-U-04, T-DAEMON-E-01 — all covered in daemon.rs and main_tests.rs.

**MCP Listener**: T-LISTEN-U-01 through T-LISTEN-U-05, SR-01 — all covered in mcp_listener.rs.

**Server Refactor**: T-ACCUM-U-01 through T-ACCUM-U-08, T-SERVER-U-04 (C-07 comment presence), T-SERVER-U-05 (UdsSession only) — all covered in server.rs test module.

**Shutdown**: T-SHUT-U-03, T-SHUT-U-04, MCP acceptor abort — all covered in shutdown.rs.

**Bridge**: T-BRIDGE-U-02, T-BRIDGE-U-03, T-BRIDGE-E-01, T-BRIDGE-E-02 — all covered in bridge.rs.

**Stop Subcommand**: T-STOP-U-02, T-STOP-U-03, T-STOP-U-06 — all covered in main.rs and main_tests.rs.

Integration tests (T-SHUT-I-*, T-DAEMON-I-*, T-ACCUM-I-*) require a running daemon and are deferred to gate-3c. This is documented in test plans as "Stage 3c" form.

**Test run (rework iteration 1)**: All 2578+ tests pass. Zero failures across workspace.

### Check 5: Code Quality

**Status**: WARN

**Evidence**:

- Build: `cargo build --workspace` completes cleanly — 0 errors, 6 warnings (all suggestions in `unimatrix-server` lib, no errors). `Finished dev profile [unoptimized + debuginfo] target(s) in 0.26s`
- Tests: All test result lines show `ok. N passed; 0 failed` across all workspace crates.
- No `todo!()`, `unimplemented!()`, `TODO`, or `FIXME` in any of the 6 new/modified files.
- No `.unwrap()` in non-test production code in any of the 6 vnc-005 files.

**WARN — `server.rs` line count**:
- `server.rs` is 2996 lines, exceeding the 500-line gate limit.
- Pre-existing violation: server.rs was already 2482 lines before vnc-005. vnc-005 added ~514 lines of production + test code.
- The new additions (PendingEntriesAnalysis, FeatureBucket, methods, tests) total approximately 200 production + 300 test lines — a bounded, cohesive addition.
- `project.rs`: 455 lines (within limit). All other new/modified files within limit.
- Not a regression introduced by this feature; flagged as WARN only.

### Check 6: Security

**Status**: PASS

**Evidence**:

- No hardcoded secrets, API keys, or credentials in any vnc-005 files.
- **0600 socket permissions**: `std::fs::set_permissions(path, Permissions::from_mode(0o600))` at mcp_listener.rs, set synchronously after bind, before accept loop starts. No permission window.
- **Path length validation**: `validate_socket_path_length` rejects paths > 103 bytes and paths with null bytes. Called before bind.
- **Stale socket unlink**: `handle_stale_socket` called before bind. Reuses existing vnc-004 function.
- **No command injection**: `run_daemon_launcher` and `bridge.rs` use `std::process::Command::new(exe_path).args(...)` with explicit argument vectors — no shell interpolation.
- **feature_cycle key length cap**: `upsert` silently drops keys > 256 bytes (C-16).
- **C-06 bridge no capabilities**: `bridge.rs` contains no `CallerId`, capability token, or auth field. Pure byte pipe.
- **C-07 exemption boundary**: Comment in gateway.rs explicitly warns against HTTP transport inheriting the UdsSession exemption.
- `cargo audit` not available in this environment. Not blocking (tool not installed, not a code defect).

### Check 7: Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

All six rust-dev agent reports present in `product/features/vnc-005/agents/`:

| Agent | Component | Stewardship |
|-------|-----------|-------------|
| vnc-005-agent-3-daemonizer-report.md | `infra/daemon.rs` | Queried + Stored (ADR-001 via entry #1911) |
| vnc-005-agent-4-server-refactor-report.md | `server.rs` refactor | Queried ADR-004 (entry #1914) + "No new entries — pattern fully captured in ADR-004" |
| vnc-005-agent-5-shutdown-report.md | `infra/shutdown.rs` | Queried + Stored |
| vnc-005-agent-6-mcp-listener-report.md | `uds/mcp_listener.rs` | Queried ADR entries #1911–#1916 + "No new patterns beyond ADR-005 entry #1915" |
| vnc-005-agent-7-bridge-report.md | `bridge.rs` | Queried "bridge stdio UDS" — no existing pattern; "No new entries — bridge specific to this feature, documented in ADR-001 entry #1911" |
| vnc-005-agent-8-main-report.md | `main.rs` / stop subcommand | Queried + Stored |

All three previously-missing reports (agent-4, agent-6, agent-7) now filed with proper `## Knowledge Stewardship` sections containing both `Queried:` and `Stored:` (or "nothing novel — {reason}") entries. Rework requirement from previous report satisfied.

---

## Self-Check

- [x] Correct gate check set used (3b)
- [x] All 7 checks in the gate's check set evaluated (none skipped)
- [x] Glass box report written to `reports/gate-3b-report.md`
- [x] No FAILs — no fix recommendations required
- [x] Cargo output was truncated (`tail -3` and `grep "test result"`)
- [x] Gate result accurately reflects findings (PASS — only outstanding item is pre-existing WARN on server.rs line count)
- [x] Knowledge Stewardship report block included

---

## Knowledge Stewardship

- Stored: nothing novel to store — entry #1267 ("Agent reports omit Knowledge Stewardship section unless structurally enforced") already captures the recurring pattern observed in this gate. The pattern resolved cleanly in rework iteration 1 without requiring a second iteration, consistent with the documented pattern.
