# col-006: Hook Transport Layer ("Cortical Implant")

## Problem Statement

Unimatrix has a sophisticated knowledge engine (redb storage, HNSW vectors, confidence evolution, contradiction detection) but agents must explicitly call MCP tools to benefit. Most agents never call `context_briefing`. The knowledge engine is invisible unless agents cooperate.

Claude Code provides lifecycle hooks (SessionStart, UserPromptSubmit, PreCompact, PostToolUse, Stop, etc.) that fire automatically on every agent interaction. By connecting Unimatrix to these hooks, knowledge delivery becomes automatic -- every prompt enriched, compaction resilient, confidence feedback closed-loop -- without agent cooperation.

col-006 is the foundation: the transport layer that connects hook processes to the running Unimatrix MCP server. All subsequent hook-driven features (col-007 through col-011) build on this transport. Without it, no automatic delivery is possible.

The core technical constraint driving the design: redb v3.1.x takes an exclusive file lock. No second process can open the database while the MCP server is running. Every hook process must communicate with the server via IPC. This constraint, discovered in ASS-014 (RQ-2), shapes the entire architecture.

## Goals

1. Establish a Unix domain socket (UDS) listener in the MCP server as a secondary transport alongside stdio
2. Implement a `hook` subcommand on the existing `unimatrix-server` binary that dispatches Claude Code hook events to the running server via UDS
3. Define and implement a `Transport` trait with a `LocalTransport` (UDS) implementation using length-prefixed JSON wire protocol
4. Extract shared business logic from `unimatrix-server` into a new `unimatrix-engine` crate so both MCP tools and hook handlers use the same query pipeline
5. Implement layered authentication (filesystem permissions, SO_PEERCRED/getpeereid, process lineage verification) with zero configuration
6. Implement graceful degradation (event queue for when server is unavailable, silent skip for injection hooks) ensuring zero regression to existing MCP functionality
7. Validate the transport end-to-end with SessionStart and Stop hooks as smoke tests, proving connectivity and round-trip latency under 50ms

## Non-Goals

- **Context injection logic** -- col-007 implements UserPromptSubmit knowledge injection
- **Compaction resilience** -- col-008 implements PreCompact knowledge preservation
- **Confidence feedback** -- col-009 implements implicit helpfulness signals
- **Session lifecycle tracking** -- col-010 implements session registration, SESSIONS table, and schema v4 migration
- **Agent routing** -- col-011 implements semantic agent matching
- **Telemetry tables** -- SESSIONS, INJECTION_LOG, SIGNAL_QUEUE tables are deferred to col-010 (the feature that first writes to them)
- **Schema v4 migration** -- Deferred to col-010 which introduces the telemetry tier
- **Daemon architecture** -- Phase 2 optimization where the hook process becomes a long-lived daemon; col-006 uses ephemeral hook processes
- **Remote/HTTPS transport** -- Future centralized deployment; col-006 is local-only UDS
- **Windows named pipe transport** -- P2 platform; col-006 targets Linux and macOS via UDS
- **`unimatrix init` auto-configuration** -- alc-003 scope; col-006 documents manual `.claude/settings.json` configuration
- **Replacing existing observation hooks** -- The current bash observation hooks for col-002 coexist; col-006 adds new hooks alongside them

## Background Research

### ASS-014 Spike (Complete)

The cortical implant architecture was fully researched in ASS-014 across five research questions:

- **RQ-1 (Data Model)**: Two-tier model -- knowledge tier (existing 14 tables) + telemetry tier (3 new tables). Telemetry is never embedded, never in search results, own GC. Data model deferred to features that need it.
- **RQ-2 (Access Pattern)**: redb exclusive file lock forces all hook-to-server communication through IPC. No direct database access from hook processes. The `unimatrix-engine` crate extraction is the key architectural addition.
- **RQ-3 (Impact Assessment)**: Identified 4 affected subsystems (PidGuard socket lifecycle, embed handle sharing, category allowlist, co-access pipeline). Zero-regression guarantee via comprehensive integration test suite (1025 unit + 174 integration tests).
- **RQ-4 (Transport & Security)**: Transport trait with sync public API, length-prefixed JSON over UDS, layered auth (filesystem + kernel credentials + process lineage). Threat model covers privilege escalation, injection via socket, and DoS.
- **RQ-5 (Distribution)**: Bundled subcommand (`unimatrix-server hook <EVENT>`) is the clear winner. Zero additional installation, automatic version coupling, single distribution target.

### Key Architecture Decisions from ASS-014

- **D1**: Two-tier data model (knowledge + telemetry), strict isolation
- **D2**: IPC-only access from hook processes (redb constraint)
- **D3**: UDS as primary transport with Transport trait abstraction
- **D4**: Bundled subcommand, not separate binary
- **D5**: Layered auth (filesystem + SO_PEERCRED + lineage), no tokens/passwords
- **D6**: Server-side session state (not hook-side)
- **D7**: Fire-and-forget for writes, synchronous for reads
- **D8**: Graceful degradation ladder (IPC > queue > skip)

### Existing Codebase Patterns

- **PidGuard (vnc-004)**: RAII lifecycle for PID file with flock + identity check. Socket lifecycle follows the same pattern.
- **Crate dependency graph**: embed -> store -> vector -> core -> server. The `unimatrix-engine` crate inserts between core and server.
- **spawn_blocking pattern**: Async wrappers in core use `spawn_blocking` for synchronous redb/HNSW operations. The UDS handler follows the same pattern.
- **Project discovery**: `project.rs` in server computes project hash from cwd. Hook subcommand needs the same logic.

## Proposed Approach

### 7 Build Components

**1. UDS Listener in MCP Server**
Add a tokio task that binds `UnixListener` on `~/.unimatrix/{project_hash}/unimatrix.sock`. Socket mode 0o600 (owner-only). Accept connections, spawn handler per connection. Length-prefixed JSON wire protocol (4-byte big-endian length + JSON payload). Socket lifecycle follows PidGuard pattern: create after PidGuard, remove on shutdown, stale detection on startup.

**2. Hook Subcommand (`unimatrix-server hook <EVENT>`)**
Clap subcommand with positional EVENT argument. Reads JSON from stdin (Claude Code hook format). Parses `hook_event_name`, `session_id`, `cwd`, and event-specific fields. Instance discovery: compute project hash from cwd, find unimatrix.sock. Connect to UDS, send request, receive response (or fire-and-forget). Exit codes: 0 = success, 1 = error (logged to stderr, non-blocking).

**3. Transport Trait + LocalTransport**
`Transport` trait with `request()`, `fire_and_forget()`, `is_connected()`, `connect()`, `disconnect()`. `LocalTransport` implementation using UDS with length-prefixed JSON. Request/Response enums covering the operations needed by col-006 smoke tests (Ping/Pong, SessionRegister, SessionClose) plus extension points for future features. `TransportError` enum: Unavailable, Timeout, Rejected, Codec, Transport.

**4. `unimatrix-engine` Crate Extraction**
Extract shared business logic from `unimatrix-server`: `confidence.rs`, `coaccess.rs`, `project.rs`. New modules: `search.rs` (embed + HNSW + re-rank + co-access boost pipeline), `query.rs` (index-based lookup with filtering). Both MCP server and UDS handler call into `unimatrix-engine`. Incremental extraction: move one module at a time, verify all integration tests pass after each move.

**5. Layered Authentication**
SO_PEERCRED extraction (Linux) / getpeereid (macOS). UID verification (same user as server). Process lineage check (`/proc/{pid}/cmdline` for "unimatrix-server" on Linux, fallback to UID-only on macOS). Pre-enroll `cortical-implant` as Internal trust agent in `bootstrap_defaults()`. No tokens, no passwords, no configuration files.

**6. Graceful Degradation**
Local event queue: `~/.unimatrix/{hash}/event-queue/pending-{ts}.jsonl`. Queue replay on successful connection to server. Size limits: 1000 events/file, 10 files max, 7-day pruning. On hook invocation: try UDS connect; if unavailable, queue fire-and-forget events and skip synchronous queries (return empty, exit 0).

**7. Hook Configuration Documentation**
Document manual `.claude/settings.json` configuration for the hook subcommand. Initially register: SessionStart, Stop (minimal set for transport validation). Provide copy-paste JSON block in feature documentation.

### Key Design Choices

- **Ephemeral hook processes** (not daemon): Each hook invocation is a new process. ~3ms startup overhead is acceptable within the 50ms budget. Daemon architecture deferred to future optimization.
- **Length-prefixed JSON** (not JSON-RPC): Simpler than full JSON-RPC for the internal protocol. 4-byte big-endian length prefix + JSON payload. Type-tagged request/response enums handle routing.
- **Incremental engine extraction**: Move modules one at a time from server to engine crate. Run full test suite after each move. This minimizes risk of breaking existing MCP tools.

## Acceptance Criteria

- AC-01: The MCP server starts a UDS listener on `~/.unimatrix/{project_hash}/unimatrix.sock` alongside its existing stdio transport, with socket permissions 0o600
- AC-02: The UDS listener accepts connections and handles length-prefixed JSON requests concurrently without blocking the stdio MCP transport
- AC-03: `unimatrix-server hook <EVENT>` subcommand reads Claude Code hook JSON from stdin, connects to the UDS, and dispatches the event to the server
- AC-04: A Ping request sent via UDS returns a Pong response with round-trip latency under 50ms (measured end-to-end including process startup)
- AC-05: The `Transport` trait is defined with `request()`, `fire_and_forget()`, `is_connected()`, `connect()`, `disconnect()` methods and `LocalTransport` implements it over UDS
- AC-06: The `unimatrix-engine` crate exists with `confidence`, `coaccess`, and `project` modules extracted from `unimatrix-server`, and all existing integration tests (174+) pass without modification
- AC-07: The UDS handler authenticates connections via UID verification (SO_PEERCRED on Linux, getpeereid on macOS) and rejects connections from different users
- AC-08: When the MCP server is not running (no socket), the hook subcommand exits with code 0 (non-blocking), queues fire-and-forget events to the local event queue, and skips synchronous queries
- AC-09: Stale socket files (from crashed server) are detected and cleaned up on server startup, following the same pattern as stale PID file handling
- AC-10: Socket cleanup occurs on graceful server shutdown (before compaction), and the socket file does not persist after clean exit
- AC-11: The `cortical-implant` agent is pre-enrolled as Internal trust level in `bootstrap_defaults()`
- AC-12: The event queue respects size limits (1000 events/file, 10 files max) and 7-day pruning
- AC-13: SessionStart and Stop hooks successfully round-trip through the transport as smoke tests, proving the full chain: hook stdin -> parse -> UDS connect -> server dispatch -> response -> hook stdout/exit

## Constraints

### Hard Constraints

- **redb exclusive file lock**: Hook processes cannot open the database. All data access must go through IPC to the running MCP server. This is non-negotiable with redb v3.1.x.
- **50ms latency budget**: Claude Code hooks that produce stdout content (synchronous) must complete within 50ms including process startup, IPC round-trip, and response formatting.
- **Zero regression**: All existing MCP tools must continue to work identically. The 1025 unit + 174 integration tests must pass without modification after engine extraction.
- **Single binary**: The hook subcommand is part of the existing `unimatrix-server` binary. No separate binary to distribute.

### Soft Constraints

- **Linux + macOS only**: UDS is the transport. Windows support (named pipes) is deferred to P2.
- **Manual hook configuration**: Users manually edit `.claude/settings.json`. Automated configuration is alc-003 scope.
- **No new redb tables**: col-006 does not add telemetry tables. Those are deferred to col-010.

### Dependencies

- **Existing**: `unimatrix-server` (vnc-001 through vnc-004), all core crates (store, vector, embed, core)
- **New crate**: `unimatrix-engine` must be created by extracting from server
- **No dependency on col-007 through col-011**: col-006 is the foundation; all subsequent features depend on it

## Open Questions

1. **Claude Code hook stdin format stability**: The hook JSON format (`hook_event_name`, `session_id`, `cwd`) is documented but Anthropic has not made explicit stability guarantees. Should we parse defensively with `#[serde(default)]` and `#[serde(flatten)]` for unknown fields? (Recommendation from ASS-014: yes.)

2. **Session identity**: Claude Code may or may not expose a `session_id` in the hook JSON. If not available, should we use parent PID as a session proxy, or generate our own session ID on first hook invocation? (ASS-014 recommends parent PID as session proxy with env var fallback.)

3. **Engine extraction granularity**: Should `search.rs` and `query.rs` (new modules) be part of the initial engine extraction in col-006, or deferred to col-007 which first needs them? (Recommendation: include `search.rs` and `query.rs` stubs in col-006 engine extraction, with full implementation in col-007.)

4. **Event queue format**: Should the event queue use JSONL (one event per line) or individual files per event? JSONL is simpler but risks partial-line corruption on crash. Individual files are safer but create more filesystem overhead. (ASS-014 recommends JSONL with 1000-event file rotation.)

## Tracking

https://github.com/dug-21/unimatrix/issues/63
