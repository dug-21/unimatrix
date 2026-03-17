# vnc-005: Daemon Mode — Persistent Background Server via UDS MCP Transport
## Specification

---

## Objective

Transform Unimatrix from a per-session stdio process into a long-lived background daemon
that accepts multiple MCP connections over a Unix Domain Socket (UDS), surviving client
disconnection so background intelligence work (tick loop, confidence refresh, vector
compaction, co-access cleanup) continues uninterrupted between sessions. A thin
stdio-to-UDS bridge process replaces the current single-shot stdio server as the default
binary behavior, keeping the existing `.mcp.json` invocation unchanged while routing
each Claude Code session to the persistent daemon.

---

## Functional Requirements

### FR-01: Daemon Subcommand
`unimatrix serve --daemon` starts the server process as a detached background daemon:
performs the fork/setsid sequence before the Tokio runtime is initialized, redirects
stdout/stderr to the log file, writes the PID file, binds both UDS sockets, then begins
the background tick loop and MCP session accept loop.

### FR-02: Daemon Exclusivity
`unimatrix serve --daemon` exits non-zero immediately if a healthy daemon is already
running for this project, determined by: PID file present AND `flock(LOCK_EX | LOCK_NB)`
fails AND `is_unimatrix_process(pid)` returns true.

### FR-03: Stdio Subcommand (Preserved)
`unimatrix serve --stdio` starts the server in foreground stdio mode, identical in
behavior to the current no-subcommand stdio server path. No behavioral regression from
the pre-vnc-005 default.

### FR-04: Bridge Mode as Default Invocation
`unimatrix` (no subcommand) operates as a bridge client:
1. Attempt connection to `unimatrix-mcp.sock`.
2. If connection succeeds: enter bidirectional copy loop (stdin → socket, socket → stdout).
3. If connection fails: execute auto-start sequence (FR-05), then bridge.
4. On auto-start failure: print a diagnostic message to stderr and exit non-zero.

### FR-05: Auto-Start Sequence
When no daemon socket is reachable, the bridge:
1. Reads the PID file (if present) and calls `is_unimatrix_process(pid)`. If the process
   is healthy, skip spawn — the socket may be transiently unavailable; retry connection
   once after 500 ms.
2. If no healthy daemon: spawn `unimatrix serve --daemon` as a child process (detached).
3. Poll for `unimatrix-mcp.sock` to appear, with 250 ms intervals, for up to 5 seconds
   total.
4. On socket appearance: proceed to bridge (FR-04 step 2).
5. On timeout: print a diagnostic error to stderr (including the log file path for
   investigation) and exit with code 1.

### FR-06: Session Isolation — Daemon Continues on Client Disconnect
When a bridge client disconnects (stdin closes / TCP-equivalent stream EOF on the UDS
side), only that session's MCP task exits. The daemon process, background tick loop,
vector index, all in-memory caches, and all other active sessions are unaffected.
`graceful_shutdown` is NOT invoked on session close.

### FR-07: Daemon Shutdown via Signal
SIGTERM or SIGINT on the daemon process triggers graceful shutdown: drain pending writes,
dump vector index, save adaptation state, compact the DB — the same sequence as the
current stdio shutdown. The daemon exits 0 after shutdown completes.

### FR-08: Stop Subcommand
`unimatrix stop` reads the PID file, verifies the process via `is_unimatrix_process(pid)`,
sends SIGTERM, waits for the process to exit (polling PID file and `/proc/{pid}` for up
to 10 seconds), and exits 0. Exits non-zero with a clear message if no daemon is running
or the PID file is absent/stale.

### FR-09: MCP UDS Socket — Separate from Hook IPC
The daemon binds a second socket, `unimatrix-mcp.sock`, for MCP JSON-RPC sessions.
The existing hook IPC socket, `unimatrix.sock`, is unchanged. The two sockets coexist
under `~/.unimatrix/{hash}/`. No protocol discriminator or multiplexing is introduced.

### FR-10: MCP-over-UDS Session Acceptor
The daemon runs a continuous accept loop on `unimatrix-mcp.sock`. For each incoming
`UnixStream` connection, the daemon spawns a Tokio task that wraps the stream as an
`rmcp` transport (using `transport-async-rw`) and calls `server.serve(stream)` for
that session. The `UnimatrixServer` handler is either Clone-derived or constructed
per-session from shared `Arc` references; all mutable state is accessed via the existing
`Arc`-wrapped handles.

### FR-11: Concurrent Sessions
Multiple MCP sessions may be active simultaneously against a single daemon. All shared
state (Store, VectorIndex, EmbedServiceHandle, AgentRegistry, AuditLog, SessionRegistry,
ConfidenceStateHandle, SupersessionStateHandle) is accessed via the existing `Arc`-wrapped
references. SQLite single-writer constraint is already enforced by `Mutex<Connection>` in
the store; no additional serialization is required.

### FR-12: Session Concurrency Cap
The daemon enforces a maximum of 32 concurrent MCP sessions. A 33rd connection attempt
is accepted, receives a JSON-RPC error response indicating the session limit, and the
stream is closed. This prevents unbounded task spawn under pathological reconnect loops.

### FR-13: MCP Socket Permissions
`unimatrix-mcp.sock` is created with `0600` permissions (owner-read/write only) at bind
time, before the accept loop starts and before any connections are accepted.

### FR-14: Stale MCP Socket Detection
At daemon startup, if `unimatrix-mcp.sock` exists and no healthy daemon is running
(same stale check as the hook IPC socket via `handle_stale_socket`), the file is
unconditionally unlinked before binding. This mirrors the existing `handle_stale_socket`
behavior for `unimatrix.sock`.

### FR-15: Daemon Logging
The daemon redirects stdout and stderr to `~/.unimatrix/{hash}/unimatrix.log` after
the fork/setsid sequence. Log file is opened in append mode. Existing log content is
preserved across daemon restarts.

### FR-16: `pending_entries_analysis` Accumulator
The in-memory `pending_entries_analysis` structure is refactored from a single
`Vec<EntryRecord>` to a two-level `HashMap<String, HashMap<u64, EntryAnalysis>>`. The
outer key is `feature_cycle: String`; the inner key is `entry_id: u64`. Using entry ID
as the inner key preserves upsert semantics — if the same entry is stored multiple times
across sessions (e.g., via correction), only the latest `EntryAnalysis` is kept. Each
`context_store` call upserts the stored entry into the bucket for the active
`feature_cycle`. When `context_retrospective` is called with a `topic` argument, it
drains the bucket for the matching `feature_cycle` key, consuming all entries accumulated
across all sessions for that feature cycle. *(Authoritative type: ARCHITECTURE.md
Component 5, ADR-004. Supersedes the `Vec<EntryRecord>` description in SCOPE.md OQ-05.)*

### FR-17: Stale Accumulator Bucket Eviction
A `feature_cycle` bucket in `pending_entries_analysis` is evicted when:
- `context_retrospective` drains it (normal path), OR
- `context_cycle` is called with the same feature cycle key (explicit cycle close), OR
- The bucket's oldest entry timestamp exceeds 72 hours (TTL eviction, checked during the
  background maintenance tick).
TTL eviction logs a warning to the daemon log with the feature cycle key and bucket size.

### FR-18: Hook IPC Unaffected
`unimatrix hook SessionStart`, `unimatrix hook SessionStop`, and all other hook
subcommands continue to operate identically against a running daemon. The hook UDS socket
(`unimatrix.sock`), the `HookRequest`/`HookResponse` wire protocol, UID-based peer
credential auth, and the `UDS_CAPABILITIES` fixed capability set are all unchanged.

### FR-19: `CallerId::UdsSession` Rate-Limit Exemption Scope
The existing `CallerId::UdsSession` exemption from rate limiting applies to MCP sessions
received over `unimatrix-mcp.sock`. This exemption is valid because UDS is local-only
(file-system-permission-gated). This exemption MUST NOT be extended to any HTTP transport
caller when HTTP transport (W2-2) is introduced.

### FR-20: Socket Path Length Validation
At startup, the daemon computes the full absolute path of `unimatrix-mcp.sock` and
asserts it is 107 bytes or fewer (the minimum `sun_path` limit across Linux and macOS).
If the path exceeds this limit, the daemon logs a fatal error and exits non-zero with
a message indicating the home directory path is too long for UDS.

---

## Non-Functional Requirements

### NFR-01: Bridge Startup Latency
The bridge process must connect to a running daemon and pass the first MCP byte within
500 ms of invocation on an unloaded system. The bridge itself allocates no significant
heap (no Store, no VectorIndex, no embedding model).

### NFR-02: Auto-Start Timeout
The auto-start wait window is 5 seconds. The daemon must bind `unimatrix-mcp.sock` and
be ready to accept connections within 5 seconds of process start on an unloaded system.
(Embedding model initialization is lazy and does not block socket readiness.)

### NFR-03: Daemon Memory Baseline
The daemon's steady-state memory footprint (no active sessions, tick loop idle) must not
exceed the current stdio server's footprint at equivalent load. No new background
allocation is introduced by daemon mode itself.

### NFR-04: Concurrent Session Overhead
Each additional active MCP session adds one Tokio task plus the session-local stack.
Shared state (Arc references) is not duplicated per session.

### NFR-05: Background Tick Continuity
The 15-minute background tick continues to fire regardless of whether zero or N MCP
sessions are active. Session connect and disconnect events do not reset the tick timer.

### NFR-06: Graceful Shutdown Drain Time
`graceful_shutdown` on SIGTERM/SIGINT must complete within 30 seconds. If DB compaction
exceeds this limit, the daemon logs a warning and exits anyway to avoid blocking system
shutdown.

### NFR-07: Log File Growth
The daemon does not rotate the log file. Operators are responsible for managing log file
size. This is acceptable for dev-workspace scope (W0-0). Log rotation is Wave 2 scope.

### NFR-08: Platform Targets
Linux and macOS only. UDS and `fork`/`setsid` are POSIX; Windows is explicitly out of
scope. The `unimatrix serve --daemon` subcommand must return a clear error on Windows
rather than silently failing.

### NFR-09: Cargo.toml Additive Changes Only
No existing crate dependency versions are changed. The `nix` crate (already present) is
used for `fork`/`setsid`. No new major dependencies are introduced.

---

## Acceptance Criteria

### AC-01: Daemon Start and Detach
**Requirement**: `unimatrix serve --daemon` starts the server as a background process,
creates `unimatrix-mcp.sock`, and the launcher process exits 0.
**Verification**: Invoke `unimatrix serve --daemon`; assert exit code 0 within 5 seconds;
assert `unimatrix-mcp.sock` exists under `~/.unimatrix/{hash}/`; assert a process matching
the daemon PID is alive.

### AC-02: Socket Permissions
**Requirement**: `unimatrix-mcp.sock` is created with `0600` permissions before the first
connection is accepted.
**Verification**: After daemon start, `stat unimatrix-mcp.sock` reports mode `0600`.
Group and other bits are zero.

### AC-03: Bridge Connects to Running Daemon
**Requirement**: `unimatrix` (no subcommand, daemon already running) connects stdin/stdout
to the daemon's MCP socket and passes MCP JSON-RPC traffic bidirectionally.
**Verification**: Start daemon; invoke `unimatrix` with a piped MCP `initialize` request;
assert a valid MCP `initialize` response is received from the daemon.

### AC-04: Daemon Survives Client Disconnect
**Requirement**: When the bridge client terminates (stdin EOF), the daemon process
continues running with all background state intact.
**Verification**: Start daemon; connect bridge; send MCP `initialize`; close bridge stdin;
assert daemon PID is still alive; assert a second bridge connection 1 second later
successfully initializes; assert the daemon's tick counter (or uptime) shows continuity.

### AC-05: Auto-Start on Missing Socket
**Requirement**: `unimatrix` (no subcommand, no daemon running) spawns
`unimatrix serve --daemon`, waits up to 5 seconds for `unimatrix-mcp.sock`, then bridges.
**Verification**: Ensure no daemon is running; invoke `unimatrix` with a piped MCP
`initialize` request; assert a valid response is received within 8 seconds total;
assert `unimatrix-mcp.sock` was created during the invocation.

### AC-06: Auto-Start Stale PID Check
**Requirement**: The bridge calls `is_unimatrix_process(pid)` on the PID file before
spawning a new daemon. A healthy running daemon is never duplicated.
**Verification**: Start daemon; invoke `unimatrix` (bridge); assert only one daemon
process exists after the bridge connects (no duplicate spawn).

### AC-07: Duplicate Daemon Rejected
**Requirement**: `unimatrix serve --daemon` exits non-zero when a healthy daemon is
already running for this project.
**Verification**: Start daemon; invoke `unimatrix serve --daemon` a second time; assert
exit code is non-zero; assert only one daemon process remains; assert error output
indicates a daemon is already running.

### AC-08: Graceful Shutdown on SIGTERM
**Requirement**: SIGTERM on the daemon triggers graceful shutdown: vector dump, adapt
save, DB compaction complete before the process exits.
**Verification**: Start daemon, store one entry, wait for ack; send SIGTERM; assert daemon
exits within 30 seconds; assert the entry is retrievable in a subsequent fresh daemon
start (verifying DB was flushed).

### AC-09: SIGINT Triggers Graceful Shutdown
**Requirement**: SIGINT on the daemon triggers the same graceful shutdown sequence as
SIGTERM.
**Verification**: Same as AC-08 with SIGINT substituted for SIGTERM.

### AC-10: `unimatrix stop` Stops the Daemon
**Requirement**: `unimatrix stop` sends SIGTERM to the daemon and exits 0 after the
daemon process is gone.
**Verification**: Start daemon; invoke `unimatrix stop`; assert exit code 0; assert
daemon PID is no longer alive within 10 seconds.

### AC-11: `unimatrix stop` When No Daemon Running
**Requirement**: `unimatrix stop` exits non-zero with a clear message when no daemon
is running.
**Verification**: Ensure no daemon is running; invoke `unimatrix stop`; assert non-zero
exit code; assert stderr contains a message indicating no daemon was found.

### AC-12: Stdio Path Preserved
**Requirement**: `unimatrix serve --stdio` works as a drop-in replacement for the
pre-vnc-005 default `unimatrix` invocation — full stdio MCP server, no auto-start
behavior.
**Verification**: Invoke `unimatrix serve --stdio` with a piped MCP `initialize` request;
assert a valid response; assert no `unimatrix-mcp.sock` is created; assert the process
exits when stdin closes (same lifecycle as before vnc-005).

### AC-13: Hook IPC Unaffected
**Requirement**: `unimatrix hook SessionStart` works correctly against a running daemon.
**Verification**: Start daemon; invoke `unimatrix hook SessionStart --feature test-001`;
assert exit code 0; assert hook IPC socket (`unimatrix.sock`) is present and served by
the daemon.

### AC-14: Concurrent Sessions — No Data Corruption
**Requirement**: Multiple simultaneous MCP sessions can store and retrieve entries
without data corruption.
**Verification**: Start daemon; open 4 concurrent bridge connections; each connection
calls `context_store` with a unique key then `context_get` for that key; assert all
4 retrievals return the correct entry; assert no SQLite error or panic in daemon logs.

### AC-15: Bridge Failure on Auto-Start Timeout
**Requirement**: If the daemon socket does not appear within 5 seconds of auto-start
spawn, the bridge exits 1 and prints a diagnostic message to stderr that includes the
log file path.
**Verification**: Simulate a daemon that never creates `unimatrix-mcp.sock` (e.g., a
stub binary that sleeps); invoke `unimatrix`; assert exit code 1 within 7 seconds;
assert stderr contains the log file path.

### AC-16: Stale MCP Socket Unlinked at Startup
**Requirement**: If `unimatrix-mcp.sock` exists from a crashed prior daemon (no healthy
process), `unimatrix serve --daemon` unlinks it and binds fresh.
**Verification**: Create a stale `unimatrix-mcp.sock` file with no listening process;
invoke `unimatrix serve --daemon`; assert startup succeeds (exit 0 from launcher);
assert a new socket is bound and accepts connections.

### AC-17: `pending_entries_analysis` Per-Feature-Cycle Accumulation
**Requirement**: Entries stored across multiple bridge sessions under the same
`feature_cycle` value accumulate in the same bucket and are all returned by a single
`context_retrospective` drain call.
**Verification**: Start daemon; open session A, store 2 entries with `feature_cycle=fnc-test-001`; close session A; open session B, store 1 entry with `feature_cycle=fnc-test-001`; call `context_retrospective` with `topic=fnc-test-001`; assert all 3 entries are present in the result.

### AC-18: `pending_entries_analysis` Drain Clears Bucket
**Requirement**: After `context_retrospective` drains a `feature_cycle` bucket, a
subsequent `context_retrospective` call for the same `feature_cycle` returns an empty
accumulator (no duplicate entries).
**Verification**: Drain bucket as in AC-17; call `context_retrospective` again with the
same topic; assert the accumulator section is empty.

### AC-19: Fork Before Runtime Init (SR-02 Compliance)
**Requirement**: The `fork`/`setsid` daemonization sequence executes before any Tokio
runtime is initialized in the daemon process. The child process initializes the Tokio
runtime fresh after `setsid`.
**Verification**: Code review — confirm no `tokio::runtime::Runtime` or `#[tokio::main]`
initialization precedes the `nix::unistd::fork()` call in the daemon startup path.

### AC-20: Session Concurrency Cap Enforced
**Requirement**: A 33rd concurrent MCP connection receives a JSON-RPC error and the
stream is closed; the 32 existing sessions are unaffected.
**Verification**: Open 32 concurrent sessions against the daemon; attempt a 33rd; assert
the 33rd receives a JSON-RPC error response and the connection closes; assert all 32
existing sessions remain active.

---

## Domain Models

### Daemon
A long-lived OS process running `unimatrix serve --daemon`. Owns all shared application
state. Identified by the PID file at `~/.unimatrix/{hash}/unimatrix.pid`. Lifetime is
bounded by SIGTERM/SIGINT, not by any MCP client session. At most one daemon per project
hash is permitted.

### Project Hash
A short (16-character) identifier derived from the project root path. Used as the
directory name under `~/.unimatrix/` that scopes all daemon artifacts (PID file, sockets,
log file) to one project. Already established in vnc-004.

### MCP Socket (`unimatrix-mcp.sock`)
A Unix Domain Socket file at `~/.unimatrix/{hash}/unimatrix-mcp.sock`. Speaks MCP
JSON-RPC (newline-delimited). Created by the daemon at startup with `0600` permissions.
Distinct from the hook IPC socket. Replaced by an HTTP listener in W2-2.

### Hook IPC Socket (`unimatrix.sock`)
The existing Unix Domain Socket at `~/.unimatrix/{hash}/unimatrix.sock`. Speaks the
custom length-framed binary `HookRequest`/`HookResponse` protocol. Unchanged by this
feature.

### Bridge Process
A short-lived process launched by Claude Code via `.mcp.json`. Its sole function is to
connect to the MCP Socket and copy bytes bidirectionally between its own stdin/stdout
and the daemon's UDS stream. Contains no application logic beyond stream copying and
the auto-start sequence.

### MCP Session
A single connection from a bridge process to the daemon's MCP Socket. Has an independent
lifecycle from the daemon. Represented as a Tokio task inside the daemon. A session begins
when the `UnixStream` is accepted and ends when the stream reaches EOF or errors. Session
state is local to the task; all persistent state is daemon-wide.

### Session Acceptor
The daemon's continuous loop that calls `accept()` on `unimatrix-mcp.sock` and spawns
a new Tokio task per incoming connection. Bounded by the session concurrency cap (FR-12).

### Auto-Start Sequence
The bridge's procedure for starting a daemon when none is reachable. Defined fully in
FR-05. Uses `is_unimatrix_process` to avoid double-spawning.

### `pending_entries_analysis`
A daemon-wide in-memory two-level map: `HashMap<String, HashMap<u64, EntryAnalysis>>`.
Outer key: `feature_cycle: String`. Inner key: `entry_id: u64`. The inner map provides
upsert semantics — repeated stores of the same entry (via correction) overwrite rather
than duplicate. Accumulates entries contributed by `context_store` calls from any
session. Drained by `context_retrospective`. Subject to TTL eviction (FR-17).
*(Authoritative type from ARCHITECTURE.md Component 5 / ADR-004.)*

### Feature Cycle Key
A `String` value identifying a feature's active work cycle, e.g., `"vnc-005"`. Provided
by the calling agent on `context_store` and `context_retrospective` requests. The
`pending_entries_analysis` HashMap is keyed by this value.

### Graceful Shutdown
The daemon's shutdown sequence: drain write queue, dump vector index to disk, save
adaptation state, compact SQLite DB. Triggered only by SIGTERM or SIGINT on the daemon
process. Never triggered by a session disconnect.

### `is_unimatrix_process(pid)`
An existing function (vnc-004) that reads `/proc/{pid}/cmdline` on Linux (with fallback
on macOS) to verify a PID corresponds to a running Unimatrix process. Used to distinguish
a live daemon from a stale PID file.

### PidGuard
The existing RAII guard (vnc-004) that holds an `flock(LOCK_EX | LOCK_NB)` on the PID
file and cleans up on drop. Enforces one-daemon-per-project at the OS level.

---

## User Workflows

### Workflow 1: Normal Development Session (Daemon Already Running)
1. Claude Code opens a new session; `.mcp.json` spawns `unimatrix` (no subcommand).
2. Bridge process connects to `unimatrix-mcp.sock` (daemon already running).
3. Bridge enters bidirectional copy; MCP `initialize` handshake completes.
4. Agent calls tools; daemon processes requests using persistent state.
5. Session ends; Claude Code closes stdin on the bridge process.
6. Bridge detects EOF; closes the UDS stream; exits.
7. Daemon's session task detects EOF; exits the task. Daemon continues running.

### Workflow 2: First Session of the Day (Daemon Not Running)
1. Claude Code opens a session; `.mcp.json` spawns `unimatrix` (no subcommand).
2. Bridge attempts connection to `unimatrix-mcp.sock` — fails (no socket).
3. Bridge checks PID file: absent or stale.
4. Bridge spawns `unimatrix serve --daemon` (detached child process).
5. Daemon forks, setsid, initializes runtime, loads Store, VectorIndex, starts tick,
   binds both sockets, writes PID.
6. Bridge polls for `unimatrix-mcp.sock` appearance; socket appears within 5 seconds.
7. Bridge connects and enters bidirectional copy; session proceeds normally.
8. Session ends as in Workflow 1; daemon continues running overnight.

### Workflow 3: Manual Daemon Stop
1. Developer invokes `unimatrix stop`.
2. Stop command reads PID file, verifies via `is_unimatrix_process`.
3. Sends SIGTERM to daemon PID.
4. Daemon runs graceful shutdown sequence; exits 0.
5. `unimatrix stop` detects process gone; exits 0.

### Workflow 4: Multi-Session Retrospective
1. Session A stores entries with `feature_cycle=vnc-005`; session A closes.
2. Session B stores more entries with `feature_cycle=vnc-005`; session B remains open.
3. Session B calls `context_retrospective` with `topic=vnc-005`.
4. Daemon drains the `pending_entries_analysis` bucket for `"vnc-005"`, returning all
   entries from both sessions.
5. Retrospective analysis runs on the complete cross-session entry set.

### Workflow 5: Daemon Restart After Crash
1. Daemon crashes (no SIGTERM received); `unimatrix-mcp.sock` and PID file remain.
2. Claude Code opens a new session; bridge attempts connection — fails.
3. Bridge reads PID file; `is_unimatrix_process(pid)` returns false (stale).
4. Bridge spawns a new daemon.
5. New daemon detects stale `unimatrix-mcp.sock` and stale PID file; unlinks both.
6. New daemon initializes and binds fresh sockets.
7. Session proceeds normally.

### Workflow 6: Development/Test Using Stdio Mode
1. Developer invokes `unimatrix serve --stdio` (no daemon involved).
2. Server starts in stdio mode; behaves identically to the pre-vnc-005 default.
3. Session ends when stdin closes; server runs graceful shutdown and exits.

---

## Constraints

### C-01: Fork Before Tokio Init (SR-02)
`nix::unistd::fork()` and `setsid()` MUST be called before any Tokio runtime is
initialized. The launcher process (before fork) contains no async code, no
`#[tokio::main]`, and no `Runtime::new()`. The child process initializes the Tokio
runtime fresh after `setsid()`. Violation is undefined behavior per Tokio's own
documentation on post-fork safety.

### C-02: rmcp Pinned at `=0.16.0`
No version change to `rmcp`. The UDS MCP transport uses `transport-async-rw`, already
activated by the existing `server` feature. No new rmcp features are added to Cargo.toml.

### C-03: `#![forbid(unsafe_code)]`
All crates retain `#![forbid(unsafe_code)]`. The `nix` crate's `fork`/`setsid` wrappers
are used for daemonization — these are safe Rust wrappers over libc. `SO_PEERCRED` for
MCP session auth is explicitly out of scope (tracked separately).

### C-04: `UnimatrixServer` Refactor Scope (SR-06)
The `UnimatrixServer` Clone/Arc refactor and the `graceful_shutdown` decoupling are
treated as a single coordinated refactor, not two independent tasks. Both must be designed
and reviewed together. Partial implementation of one without the other is a gate failure
condition.

### C-05: Graceful Shutdown Caller Mapping (SR-07)
Before implementation, the architect MUST enumerate every call site of `graceful_shutdown`
and every write queue that must drain before it. The implementation must demonstrate that
no write path is orphaned by the session/daemon lifetime split.

### C-06: No New Capability Escalation
The bridge process carries no Unimatrix capabilities. All auth is enforced by the daemon's
per-session tool handler, unchanged from today. The UDS peer-credential UID check applies
to MCP sessions the same way it applies to hook IPC sessions.

### C-07: `CallerId::UdsSession` Exemption Boundary
The `CallerId::UdsSession` rate-limit exemption is explicitly documented in code as
local-only. When HTTP transport is introduced (W2-2), this exemption MUST NOT be inherited
by the HTTP `CallerId` variant. A code comment referencing this constraint is required at
the exemption site.

### C-08: Socket Path Length (FR-20)
UDS `sun_path` is 104 bytes on macOS, 108 bytes on Linux. The computed socket path must
be validated at startup. The path `~/.unimatrix/{16-char-hash}/unimatrix-mcp.sock`
expands to approximately `{home}/.unimatrix/{16}/unimatrix-mcp.sock`. A home directory
of 60 characters or fewer produces a valid path on both platforms.

### C-09: SQLite Single-Writer
The daemon's `Mutex<Connection>` serializes all SQLite writes. Multi-session concurrency
is already safe. No additional locking is introduced for session concurrency.

### C-10: Hook Subcommand Remains Synchronous
`unimatrix hook` is a synchronous subcommand with a sub-50ms execution budget. No Tokio
runtime is initialized in the hook path. The bridge client path is async but is a
distinct CLI entry point that does not affect the hook path.

### C-11: No systemd / Launchd Integration
Service management via systemd or launchd is Wave 2 scope. The daemon is started manually
or via auto-start. No unit files or plist files are created.

### C-12: Windows Not Supported
`unimatrix serve --daemon` and bridge mode are POSIX-only. On Windows, the subcommand
exits non-zero with a clear message. `unimatrix serve --stdio` continues to work on
Windows.

### C-13: Two-Socket Design is Final (OQ-01)
`unimatrix-mcp.sock` and `unimatrix.sock` are separate, permanent. No discriminator
multiplexing is introduced. This resolves OQ-01 and is not re-opened. The rationale is
W2-2 migration: MCP socket becomes the HTTP listener; hook IPC socket is orthogonal.

---

## Dependencies

### Existing Crates (No Version Change)
- `nix` — already in `Cargo.toml`; provides `unistd::fork`, `unistd::setsid` for
  daemonization without unsafe code.
- `rmcp = "=0.16.0"` with features `["server", "transport-io", "macros"]` — `transport-async-rw`
  is already activated by the `server` feature; wraps `UnixStream` as MCP transport.
- `tokio` — `tokio::net::UnixListener`, `tokio::net::UnixStream` for the accept loop.
- `fs2` — existing `flock` support in PidGuard (vnc-004).
- `nix` — `signal::kill` for `unimatrix stop`.

### Existing Internal Components
- `PidGuard` (vnc-004) — one-daemon-per-project enforcement; reused unchanged.
- `is_unimatrix_process(pid)` (vnc-004) — stale PID detection; reused unchanged.
- `handle_stale_socket` (vnc-004 pattern) — extended to cover `unimatrix-mcp.sock`.
- `SocketGuard` — extended to manage both sockets; must drop both on daemon shutdown.
- `graceful_shutdown` — refactored to decouple from transport close (C-04, C-05).
- `ServiceLayer`, `SessionRegistry`, `ConfidenceStateHandle`, `SupersessionStateHandle`,
  `AgentRegistry`, `AuditLog` — all accessed via existing `Arc`-wrapped references;
  no structural change.
- Background tick loop (`background.rs`) — continues to run while daemon is alive;
  no change to tick logic required.

### External (None New)
No new external services, crates with version bumps, or network dependencies are
introduced by this feature.

---

## NOT in Scope

- **HTTP transport** — W2-2 in product roadmap. `unimatrix-mcp.sock` is the surface that
  becomes an HTTP listener in W2-2, but the HTTP implementation is not part of vnc-005.
- **TLS, OAuth, or any network authentication** — no network surface is exposed.
- **Windows UDS support** — UDS on Windows is explicitly excluded.
- **Multi-project daemon** — one daemon per project hash; no shared daemon across projects.
- **systemd / Launchd / container integration** — Wave 2 operational scope.
- **SO_PEERCRED-based MCP session identity** — tracked as a separate issue; orthogonal.
- **Log file rotation** — dev-workspace scope accepts manual log management.
- **rmcp upgrade** — `=0.16.0` is pinned; no upgrade path is in scope.
- **Hook IPC protocol changes** — `unimatrix.sock`, `HookRequest`/`HookResponse`, UID
  peer credential auth, and `UDS_CAPABILITIES` are all frozen for this feature.
- **`.mcp.json` changes** — the `.mcp.json` command entry stays as `"command":
  "unimatrix", "args": []`. No user-visible config change is required.
- **Changing tick interval or tick logic** — daemon mode enables continuous ticking but
  does not alter tick frequency or behavior.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for daemon mode UDS MCP transport session lifecycle --
  found established patterns #1897 (thin stdio bridge required for Claude Code
  compatibility), #1898 (hook IPC and MCP sessions on distinct sockets), and #300
  (UDS fixed capability set / transport-level authorization boundary). All three patterns
  are consistent with and reinforced by this specification. No stale or contradictory
  entries found.
