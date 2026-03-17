# Gate 3a Report: vnc-005

> Gate: 3a (Component Design Review)
> Date: 2026-03-17
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 7 components map 1:1 to ARCHITECTURE.md decomposition; ADR decisions followed |
| Specification coverage | PASS | All 20 FRs and 9 NFRs addressed; no scope additions detected |
| Risk coverage | PASS | All 18 risks (R-01 through R-18) have test plan scenarios; all 5 Critical risks have multiple scenarios |
| Interface consistency | PASS | Shared types in OVERVIEW.md match per-component usage; data flow is coherent |
| Critical constraints (C-01, C-04, C-05, C-07, C-10) | PASS | All five spawn-prompt constraints explicitly enforced in pseudocode |
| Knowledge stewardship compliance | PASS | Spec and risk agent reports contain Queried/Stored entries |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**:

All seven components defined in ARCHITECTURE.md are represented as pseudocode files:

| Architecture Component | Pseudocode File | Alignment |
|------------------------|-----------------|-----------|
| Component 1: Daemonizer (`infra/daemon.rs`) | `pseudocode/daemonizer.md` | PASS — `run_daemon_launcher` + `prepare_daemon_child` exactly match ARCH description; spawn-new-process with no `fork()` |
| Component 2: MCP Session Acceptor (`uds/mcp_listener.rs`) | `pseudocode/mcp_listener.md` | PASS — single acceptor task + per-connection spawns; `retain(is_finished)` sweep; `MAX_CONCURRENT_SESSIONS = 32` |
| Component 3: UnimatrixServer Clone (`server.rs` refactor) | `pseudocode/server_refactor.md` Part 1 | PASS — confirms `#[derive(Clone)]` already present; no new `ServiceLayer` per session; documents single-construction guarantee |
| Component 4: Shutdown Signal Router (`infra/shutdown.rs`) | `pseudocode/shutdown.md` | PASS — `CancellationToken` daemon token; `mcp_socket_guard` + `mcp_acceptor_handle` added to `LifecycleHandles`; single `graceful_shutdown` call site |
| Component 5: Feature-Cycle Accumulator (`server.rs`) | `pseudocode/server_refactor.md` Part 2 | PASS — `FeatureBucket` + two-level `HashMap<String, FeatureBucket>` exactly matches ADR-004; all three eviction triggers present |
| Component 6: Bridge Client (`bridge.rs`) | `pseudocode/bridge.md` | PASS — auto-start sequence with stale PID check; `copy_bidirectional` via `tokio::io::copy` select approach; correct 250ms poll interval |
| Component 7: `unimatrix stop` (`main.rs`) | `pseudocode/stop_cmd.md` | PASS — synchronous; reads PID file; calls `terminate_and_wait(pid, 15s)`; exit codes 0/1/2 match ADR-006 |

ADR compliance confirmed:
- ADR-001 (spawn-new-process): `daemonizer.md` uses `std::process::Command::new(current_exe())` with `--daemon-child`; no `fork()`.
- ADR-002 (session/daemon lifetime): `shutdown.md` documents daemon `CancellationToken` model; session tasks receive child tokens; `graceful_shutdown` called only after daemon token fires.
- ADR-003 (clone model): `server_refactor.md` explicitly prohibits constructing `ServiceLayer` inside session spawn closures.
- ADR-004 (accumulator key): `server_refactor.md` uses `HashMap<String, FeatureBucket>` with `HashMap<u64, EntryAnalysis>` inner map; matches WARN-01 resolution in IMPLEMENTATION-BRIEF.
- ADR-005 (accept loop): `mcp_listener.md` implements counter-based cap with `Arc<AtomicUsize>`, `retain(is_finished)` sweep before each accept.
- ADR-006 (stop subcommand): `stop_cmd.md` is synchronous; reuses `terminate_and_wait`; 15-second timeout.

`ProjectPaths::mcp_socket_path` is correctly introduced in OVERVIEW.md as a Wave 1 dependency. All interface signatures in OVERVIEW.md match the integration surface table in ARCHITECTURE.md.

**One minor observation (not a FAIL)**: The `run_bridge` signature in IMPLEMENTATION-BRIEF uses `(mcp_socket_path: &Path, log_path: &Path)` while `pseudocode/bridge.md` uses `run_bridge(paths: &ProjectPaths)`. The pseudocode approach is architecturally superior (passes full `ProjectPaths` giving access to `pid_path` needed for the stale PID check). This is consistent with ARCHITECTURE.md's `run_bridge(mcp_socket_path)` notation being illustrative. No conflict.

---

### Check 2: Specification Coverage

**Status**: PASS

**Evidence**:

Functional requirement coverage:

| FR | Pseudocode Coverage |
|----|---------------------|
| FR-01 (daemon subcommand) | `daemonizer.md` `run_daemon_launcher` + `stop_cmd.md` `Serve { daemon: true }` dispatch |
| FR-02 (daemon exclusivity) | `daemonizer.md` notes PidGuard `flock(LOCK_EX|LOCK_NB)` enforces this; `stop_cmd.md` `T-DAEMON-I-02` tests second-daemon rejection |
| FR-03 (stdio preserved) | `stop_cmd.md` `tokio_main_stdio` with `mcp_socket_guard: None, mcp_acceptor_handle: None` |
| FR-04 (bridge default) | `bridge.md` `run_bridge`; `stop_cmd.md` `None =>` dispatch arm |
| FR-05 (auto-start sequence) | `bridge.md` Steps 1-5; 500ms retry + 250ms polling + 5s timeout |
| FR-06 (session isolation) | `shutdown.md` documents `QuitReason::Closed` does NOT call `graceful_shutdown`; `mcp_listener.md` `run_session` exits without shutdown |
| FR-07 (SIGTERM shutdown) | `shutdown.md` daemon token pattern; `graceful_shutdown` sequence unchanged |
| FR-08 (stop subcommand) | `stop_cmd.md` `run_stop`; exit codes 0/1/2 |
| FR-09 (two sockets) | OVERVIEW.md; `mcp_listener.md` binds `unimatrix-mcp.sock` distinct from `unimatrix.sock` |
| FR-10 (MCP-over-UDS acceptor) | `mcp_listener.md` `start_mcp_uds_listener` + `run_mcp_acceptor` + `run_session` |
| FR-11 (concurrent sessions) | `mcp_listener.md`; `server_refactor.md` documents Arc-shared state correctness |
| FR-12 (session cap 32) | `mcp_listener.md` `MAX_CONCURRENT_SESSIONS = 32`; counter-based drop |
| FR-13 (socket 0600) | `mcp_listener.md` Step 4: `set_permissions` before accept loop |
| FR-14 (stale socket detection) | `mcp_listener.md` Step 2: `handle_stale_socket` called before bind |
| FR-15 (daemon logging) | `daemonizer.md` Step 2-3: log file opened in append mode; child stdout/stderr redirected |
| FR-16 (accumulator refactor) | `server_refactor.md` Part 2; OVERVIEW.md shared types section |
| FR-17 (stale bucket eviction) | `server_refactor.md` `evict_stale` method; background tick caller documented |
| FR-18 (hook IPC unaffected) | `stop_cmd.md` confirms `Command::Hook` arm unchanged; OVERVIEW.md data flow shows hook path separate |
| FR-19 (UdsSession exemption scope) | `server_refactor.md` C-07 exemption site documented; `mcp_listener.md` integration notes confirm |
| FR-20 (socket path length) | `mcp_listener.md` `validate_socket_path_length` — 103-byte limit; null byte check |

Non-functional requirement coverage:
- NFR-01 (bridge latency): `bridge.md` bridge carries no heap allocation beyond stream handles.
- NFR-02 (auto-start 5s): `bridge.md` 5s timeout; `daemonizer.md` same 5s poll timeout.
- NFR-03 (memory baseline): No new static allocations in daemon mode beyond session task stacks.
- NFR-04 (session overhead): `mcp_listener.md` one task per session; Arc increment only.
- NFR-05 (tick continuity): `stop_cmd.md` `tokio_main_daemon` documents tick spawned before daemon token wait.
- NFR-06 (30s shutdown): `shutdown.md` 30s join timeout per session; 35s abort timeout for acceptor.
- NFR-07 (no log rotation): Documented as accepted; no pseudocode needed.
- NFR-08 (Linux/macOS only): `daemonizer.md` `#[cfg(not(unix))]` Windows error arm.
- NFR-09 (additive Cargo): No new crate versions in any pseudocode.

No scope additions detected. All pseudocode maps to specified functionality.

---

### Check 3: Risk Coverage

**Status**: PASS

**Evidence**:

All 18 risks from RISK-TEST-STRATEGY.md have test plan coverage. Critical risks:

| Risk | Priority | Test Coverage |
|------|----------|---------------|
| R-01 (Arc::try_unwrap fails) | Critical | `T-SERVER-U-02` (count=1 unit), `T-SHUT-I-05` (4 sessions SIGTERM), `T-SHUT-I-03` (SIGTERM + data persistence) |
| R-02 (SIGTERM/spawn race) | Critical | `T-LISTEN-U-05` (token cancellation breaks loop), `T-LISTEN-U-06` (spawn race concurrency stress) |
| R-03 (session EOF calls graceful_shutdown) | Critical | `T-SHUT-U-01` (grep: single call site), `T-SHUT-U-02` (QuitReason::Closed branch), `T-SHUT-I-01/I-02` (daemon survives disconnect) |
| R-04 (double-daemon on macOS) | Critical | `T-DAEMON-I-02` (second daemon rejects), `T-BRIDGE-U-03/U-04/U-05` (stale PID handling) |
| R-12 (stdio regression) | Critical | `T-SHUT-I-07` (stdio exits on stdin close), `T-SHUT-I-08` (SIGTERM on stdio) — both in test-plan/shutdown.md |

High-priority risks:
- R-05 (concurrent drain/upsert): `T-ACCUM-C-01` (concurrent 1000 ops), `T-ACCUM-I-01/I-02` (integration)
- R-06 (socket perms): `T-LISTEN-U-01` (unit perm check), `T-LISTEN-I-01` (integration stat)
- R-07 (exemption boundary): `T-SERVER-U-04` (grep C-07/W2-2 comment), `T-SERVER-U-05` (unit test non-UDS gets rate-limited)
- R-08 (auto-start timeout): `T-BRIDGE-U-02` (timeout+stderr), `T-BRIDGE-I-04` (integration timeout)
- R-09 (stale socket blocks bind): `T-LISTEN-I-03` (stale socket unlinked), `T-DAEMON-I-05` (via daemon-child path)
- R-10 (Vec grows unbounded): `T-LISTEN-U-03` (retain sweep bounds), `T-LISTEN-E-02` (100 cycles)
- R-11 (33rd silently dropped): `T-LISTEN-U-04` (unit: 33rd dropped), `T-LISTEN-I-04` (integration: 32+1)

Medium/Low risks R-13 through R-18 all have at minimum one test scenario each in the relevant component test plans. The OVERVIEW.md risk-to-test mapping table is complete and consistent.

The risk-to-test mapping table in `test-plan/OVERVIEW.md` directly references all 18 risks and their associated test files, with no gaps in the Critical or High priority bands.

---

### Check 4: Interface Consistency

**Status**: PASS

**Evidence**:

Shared types in `pseudocode/OVERVIEW.md` are used consistently across component pseudocode:

**`ProjectPaths::mcp_socket_path`** (`PathBuf`):
- Defined in OVERVIEW.md as `data_dir.join("unimatrix-mcp.sock")`
- Used in `daemonizer.md` as `paths.mcp_socket_path` (poll loop)
- Used in `mcp_listener.md` as `path: &Path` parameter
- Used in `bridge.md` as `paths.mcp_socket_path`
- Used in `stop_cmd.md` via `compute_paths_sync` → `paths`

**`LifecycleHandles` new fields**:
- `mcp_socket_guard: Option<SocketGuard>` and `mcp_acceptor_handle: Option<JoinHandle<()>>`
- Defined in OVERVIEW.md and `shutdown.md`
- Populated in `stop_cmd.md` `tokio_main_daemon` with `Some(...)` values
- Populated as `None` in `tokio_main_stdio` path — correct
- Dropped in correct order (mcp_socket_guard → socket_guard) in `shutdown.md` Step 0a/0c

**`PendingEntriesAnalysis` refactored type**:
- `FeatureBucket` struct defined in OVERVIEW.md and `server_refactor.md`
- `upsert(feature_cycle: &str, analysis: EntryAnalysis)` signature matches IMPLEMENTATION-BRIEF
- `drain_for(feature_cycle: &str) -> Vec<EntryAnalysis>` matches ARCHITECTURE.md integration surface
- `evict_stale(now_unix_secs: u64, ttl_secs: u64)` added correctly
- Callers in `server_refactor.md` (`context_retrospective`, `context_cycle`, UDS listener) all use the new signature

**`start_mcp_uds_listener` signature**:
- `async fn start_mcp_uds_listener(path: &Path, server: UnimatrixServer, shutdown_token: CancellationToken) -> Result<(JoinHandle<()>, SocketGuard), ServerError>`
- Matches ARCHITECTURE.md integration surface table exactly
- Called correctly in `stop_cmd.md` `tokio_main_daemon`

**`run_bridge` signature divergence** (WARN, not FAIL):
- ARCHITECTURE.md states `run_bridge(mcp_socket_path)` taking a single `&Path`
- IMPLEMENTATION-BRIEF states `run_bridge(mcp_socket_path: &Path, log_path: &Path)`
- `pseudocode/bridge.md` uses `run_bridge(paths: &ProjectPaths)` — takes full `ProjectPaths`
- The `ProjectPaths` form is the richest and most correct: it gives access to `pid_path` needed for the stale PID check in Step 2. The ARCHITECTURE.md form was illustrative. No inconsistency between pseudocode files — all bridge-related pseudocode uses `ProjectPaths`. No action required by implementation.

**Data flow coherence**:
- Wave dependency order in OVERVIEW.md correctly sequences `project.rs` → `server.rs` → `shutdown.rs` → `daemon.rs` (Wave 1), then `mcp_listener.rs` → `bridge.rs` → `main.rs` (Wave 2).
- C-04 joint gate (server clone + shutdown decoupling) explicitly flagged in OVERVIEW.md.

---

### Check 5: Critical Constraint Verification (Spawn-Prompt Specific)

**Status**: PASS

**Evidence for each critical constraint**:

**C-01 (setsid before runtime)**:
`daemonizer.md` `prepare_daemon_child` pseudocode is explicitly `fn` (not `async fn`), calls `nix::unistd::setsid()` under `#[cfg(unix)]`, and is called from `main.rs` in the `stop_cmd.md` dispatch BEFORE `tokio_main_daemon` is entered:
```
if cli.daemon_child:
    infra::daemon::prepare_daemon_child()?
    return tokio_main_daemon(cli)  // Tokio runtime initializes here, after setsid
```
The comment `// C-01: setsid must happen before tokio_main_daemon enters the #[tokio::main] context` is present at the call site in `stop_cmd.md`. PASS.

**C-04 (coordinated refactor)**:
`server_refactor.md` explicitly opens with "These are ONE implementation task, reviewed as ONE unit. The implementation agent must not merge either change without the other (C-04)." OVERVIEW.md repeats the joint gate flag. PASS.

**C-05 (single graceful_shutdown call site)**:
`shutdown.md` enumerates:
- Stdio path: one call site in `tokio_main_stdio` after `running.waiting().await`
- Daemon path: one call site in `tokio_main_daemon` after `daemon_token.cancelled().await`
The document explicitly states "grep for `graceful_shutdown(` finds exactly two call sites after refactor: one in the stdio branch, one in the daemon branch." `T-SHUT-U-01` tests this as a grep assertion. The `run_session` pseudocode in `mcp_listener.md` explicitly comments `// ADR-002 / C-04: Do NOT call graceful_shutdown here.` PASS.

**C-07 (UdsSession exemption boundary)**:
Both `server_refactor.md` and `mcp_listener.md` document the required comment:
```rust
// C-07: UDS is filesystem-gated (0600 socket) — rate-limit exemption is
// local-only. When HTTP transport is introduced (W2-2), the HTTP CallerId
// variant MUST NOT inherit this exemption.
```
`T-SERVER-U-04` is a grep test that asserts this comment is present post-implementation. PASS.

**C-10 (hook path before async)**:
`stop_cmd.md` `main()` dispatch explicitly shows `Command::Hook` as the first match arm with comment `// SYNC — no tokio (ADR-002 from vnc-001; R-13 regression gate)`, before `Command::Stop` (sync), before `Command::Serve` (async dispatch), before `None` (async bridge). `bridge.md` Integration Notes state "No Tokio runtime is initialized before the `Hook` arm is reached (C-10)." PASS.

---

### Check 6: Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

The specification agent report (`SPECIFICATION.md` includes a `## Knowledge Stewardship` section at the bottom):
> - Queried: /uni-query-patterns for daemon mode UDS MCP transport session lifecycle — found established patterns #1897, #1898, and #300. All three patterns are consistent with and reinforced by this specification.
> - No stale or contradictory entries found.

The risk strategy document (`RISK-TEST-STRATEGY.md`) includes a `## Knowledge Stewardship` section:
> - Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection daemon UDS socket" — found #245, #211, #300.
> - Queried: `/uni-knowledge-search` for "outcome rework graceful shutdown Arc try_unwrap" — found #81/#33, #312.
> - Queried: `/uni-knowledge-search` for risk patterns on concurrent session/cancellation — found #1367, #731, #735.
> - Queried: `/uni-knowledge-search` for SQLite concurrent access — found #735, #328.
> - Stored: nothing novel to store — patterns observed are codebase-specific to vnc-005 design decisions, not cross-feature generalizations visible across 2+ features at this time.

Both active-storage agents (architect and risk-strategist) have `Queried:` entries and valid `Stored:` / "nothing novel to store" entries with reasons. The pseudocode agent files do not have separate stewardship sections — this is acceptable since pseudocode agents are read-only consumers per the gate 3a check set. No missing stewardship blocks.

---

## Minor Observations (WARN)

None of these block progress.

**WARN-1: `run_bridge` signature variation**
The `bridge.md` pseudocode uses `run_bridge(paths: &ProjectPaths)` while the IMPLEMENTATION-BRIEF function signature table shows `run_bridge(mcp_socket_path: &Path, log_path: &Path)`. The `ProjectPaths` form is strictly superior (enables stale PID check). Implementation agent should use the `ProjectPaths` form and the IMPLEMENTATION-BRIEF table entry should be understood as an approximation.

**WARN-2: `T-SHUT-U-01` grep assertion language**
The test plan asserts "exactly one match" for `graceful_shutdown` call sites, but `shutdown.md` correctly notes there will be two call sites (stdio branch + daemon branch). The test plan's `T-SHUT-U-01` description says "exactly one" but the context makes clear it means the daemon path only. The implementation should verify both branches during Stage 3c. No pseudocode gap — this is a test plan wording precision issue.

**WARN-3: `upsert` merge semantics discrepancy**
`pseudocode/server_refactor.md` `upsert` shows the merge logic summing `rework_flag_count + rework_session_count + success_session_count`. `test-plan/server_refactor.md` `T-ACCUM-U-02` tests that a second upsert of the same entry ID produces `entry_v2` (overwrite), not a sum. These are inconsistent. The test plan's overwrite assertion is better aligned with the ARCHITECTURE.md intent ("if the same entry is stored multiple times across sessions via correction, only the latest `EntryAnalysis` is kept"). Implementation agent should implement overwrite (replace, not sum) semantics and align the pseudocode comment. This does not block progress — the test plan governs.

---

## Rework Required

None. Gate result is PASS.

---

## Knowledge Stewardship

- Stored: nothing novel to store — gate findings are feature-specific observations about vnc-005 design artifacts, not cross-feature patterns materializing across 2+ features yet. The WARN-3 discrepancy (upsert merge vs. overwrite semantics) is a one-off precision gap, not a systemic pattern.
