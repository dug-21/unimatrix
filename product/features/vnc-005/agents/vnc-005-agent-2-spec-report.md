# Agent Report: vnc-005-agent-2-spec

## Output
- SPECIFICATION.md written to:
  `product/features/vnc-005/specification/SPECIFICATION.md`

## Acceptance Criteria Coverage
20 ACs total (expanded from 12 in SCOPE.md):

| AC-ID | Source | Topic |
|-------|--------|-------|
| AC-01 | SCOPE AC-01 | Daemon start and detach |
| AC-02 | SCOPE AC-02 | Socket permissions 0600 |
| AC-03 | SCOPE AC-03 | Bridge connects to running daemon |
| AC-04 | SCOPE AC-04 | Daemon survives client disconnect |
| AC-05 | SCOPE AC-05 | Auto-start on missing socket |
| AC-06 | SCOPE AC-06 | Auto-start stale PID check |
| AC-07 | SCOPE AC-07 | Duplicate daemon rejected |
| AC-08 | SCOPE AC-08 | Graceful shutdown on SIGTERM |
| AC-09 | New (SIGINT) | Graceful shutdown on SIGINT |
| AC-10 | SCOPE AC-12 | `unimatrix stop` stops daemon |
| AC-11 | New (stop/no-daemon) | `unimatrix stop` when no daemon running |
| AC-12 | SCOPE AC-09 | Stdio path preserved |
| AC-13 | SCOPE AC-10 | Hook IPC unaffected |
| AC-14 | SCOPE AC-11 | Concurrent sessions no data corruption |
| AC-15 | SR-03/SR-04 | Bridge failure on auto-start timeout |
| AC-16 | SR-08 | Stale MCP socket unlinked at startup |
| AC-17 | OQ-05 | `pending_entries_analysis` cross-session accumulation |
| AC-18 | OQ-05 | Drain clears bucket (no duplicates) |
| AC-19 | SR-02 | Fork before runtime init (code review gate) |
| AC-20 | SR-09 | Session concurrency cap enforced |

## Key Constraints Documented
- C-01: Fork before Tokio init (SR-02 critical risk)
- C-04/C-05: Server clone and shutdown decoupling treated as one coordinated refactor (SR-06/SR-07)
- C-07: `CallerId::UdsSession` exemption explicitly bounded to UDS/local only
- C-08: Socket path length validated at startup; home dir must be 60 chars or fewer
- C-13: Two-socket design is final (OQ-01 resolved)

## Key Decisions Made During Spec Authoring
- SCOPE had 12 ACs; expanded to 20 to cover: SIGINT (separate from SIGTERM), `unimatrix stop` failure case, bridge timeout error behavior (SR-03), stale MCP socket detection (SR-08), cross-session accumulation and drain semantics (OQ-05), concurrency cap verification (SR-09), and fork/runtime ordering as a code-review gate (SR-02).
- FR-12 (session concurrency cap = 32) added to address SR-09; SCOPE left this open.
- FR-17 (stale bucket eviction: drain | context_cycle | 72h TTL) added to address SR-05; SCOPE left this open.
- FR-20 (socket path length validation at startup) added to address SCOPE constraint #7.
- AC-19 specified as code-review verification (not runtime test) since the ordering constraint cannot be tested by invoking the binary.

## Open Questions for Architect
- **OQ-A**: SCOPE constraint #6 states `nix::unistd::fork` for daemonization, but the architect must confirm the exact fork pattern: (a) launcher forks and child execs fresh, or (b) launcher forks and child initializes runtime in-place. The specification requires (b) at minimum but does not prescribe the exec variant; SR-02 recommendation says "fork-before-tokio or double-fork-with-exec" — architect must choose.
- **OQ-B**: SCOPE does not specify whether the bridge process uses a small Tokio runtime or a simpler async executor for the bidirectional copy. The specification does not constrain this; architect should confirm that a minimal Tokio runtime for bridge async I/O does not conflict with C-10 (hook path remains synchronous).
- **OQ-C**: `SocketGuard` currently manages one socket. Extending it to manage two (hook IPC + MCP) — should it hold a `Vec<PathBuf>` or should a second guard instance be used? Architect decides; spec requires both are cleaned up (C-04 / SR-08).

## Knowledge Stewardship
- Queried: /uni-query-patterns for daemon mode UDS MCP transport session lifecycle -- found #1897 (thin stdio bridge), #1898 (socket separation), #300 (UDS capability boundary). All consistent with spec. No corrections or deprecations needed.
