# vnc-005: Daemon Mode — Persistent Background Server via UDS MCP Transport

## Problem Statement

Unimatrix currently starts as a new stdio process every time Claude Code opens an MCP
session. When the session ends (stdin closes), the server process exits — taking the
background tick loop, co-access cache, confidence state, adaptation state, and HNSW
vector index with it. There is no continuity between sessions.

This architecture makes every Wave 1+ intelligence feature meaningless: write queue
draining, NLI post-store inference, tick-based confidence refresh, GNN training,
and overnight GGUF synthesis all require a process that is alive between sessions.
The infrastructure for background intelligence already exists (background.rs tick loop,
15-minute intervals, ConfidenceStateHandle, SupersessionStateHandle, etc.). The
process just keeps dying before it can do anything.

The fix is structural: transform Unimatrix into a long-lived daemon that accepts
multiple MCP connections over a Unix Domain Socket. Each session connects and
disconnects; the daemon and all its background work continues.

Secondary problem: the existing UDS socket in `~/.unimatrix/{hash}/unimatrix.sock`
already serves hook IPC (a custom `HookRequest`/`HookResponse` wire protocol). A new
MCP-over-UDS transport layer is needed — separate from and alongside the hook IPC
socket. The daemon needs to multiplex: accept MCP JSON-RPC sessions on one socket while
continuing to accept hook events on the same (or second) socket.

## Goals

1. Add `unimatrix serve --daemon` subcommand that starts the server as a long-lived
   background process listening for MCP connections over UDS.
2. Add a thin stdio-to-UDS bridge mode (the "client" path) that Claude Code spawns per
   session — bridges stdin/stdout to the running daemon's MCP UDS socket.
3. Auto-start: if no daemon is running when the bridge client starts, spawn the daemon
   before connecting.
4. The daemon survives MCP client disconnection — background tick loop, vector index,
   adaptation state, and all in-memory caches persist across sessions.
5. UDS socket file permissions are 0600 (owner-only) from creation.
6. Stale PID detection on auto-start uses the existing `is_unimatrix_process` check
   (vnc-004) — never auto-starts a second daemon if a healthy one is running.
7. The `.mcp.json` `command` path shifts from "start a full server" to "connect to
   daemon or start one, then bridge stdio".

## Non-Goals

- HTTP transport (W2-2 in the product roadmap) — UDS is local-only dev workspace scope.
- TLS, OAuth, or network authentication — no network surface is exposed.
- Windows support — UDS transport on Windows is out of scope; the feature targets Linux
  and macOS development environments only.
- Multi-project daemon — one daemon per project (keyed by project hash), not a single
  daemon serving multiple projects.
- Changing the hook IPC socket or hook wire protocol — the existing
  `HookRequest`/`HookResponse` UDS stays as-is; this feature adds a separate MCP socket
  or multiplexes the existing one with a protocol discriminator.
- Containerization or systemd service integration (Wave 2 scope).
- Replacing or removing the existing stdio server path — stdio mode stays working for
  development and testing.
- SO_PEERCRED-based caller identity for MCP sessions (tracked separately; orthogonal to
  this feature).

## Background Research

### Current Architecture

**Server startup (main.rs `tokio_main`):**
- No-subcommand path initializes the full stack: Store, VectorIndex, EmbedServiceHandle,
  AgentRegistry, AuditLog, ServiceLayer, SessionRegistry, background tick
- Calls `rmcp::ServiceExt::serve(rmcp::transport::io::stdio())` — blocks until stdin
  closes or SIGTERM
- On disconnect: `running.waiting()` returns `QuitReason::Closed`, triggers
  `graceful_shutdown` (vector dump, adapt save, DB compaction), then exits

**This means every session is a cold start**: store open, vector index load, embedding
model load (lazy but triggered early), tick loop initialization — then teardown 30
minutes later when the session ends.

**Existing UDS socket** (`~/.unimatrix/{hash}/unimatrix.sock`):
- Already bound at server startup (line 214 of main.rs, `uds_listener::start_uds_listener`)
- Speaks a custom length-framed binary protocol (`HookRequest`/`HookResponse` via
  `unimatrix_engine::wire`)
- Serves Claude Code hooks (`unimatrix hook SessionStart`, `Stop`, etc.) — these are
  lightweight synchronous processes that connect, send one request, and disconnect
- Socket permissions already set to 0600 at bind time
- Uses UID-based peer credential auth (Layer 2) and `UDS_CAPABILITIES` fixed capability
  set (Read, Search, SessionWrite — not Write or Admin)
- Has `SocketGuard` RAII for cleanup and `handle_stale_socket` unconditional unlink at
  startup (ADR-004)

**PidGuard (vnc-004):**
- `flock(LOCK_EX | LOCK_NB)` on `~/.unimatrix/{hash}/unimatrix.pid` enforces
  one-daemon-per-project
- `is_unimatrix_process(pid)` checks `/proc/{pid}/cmdline` before SIGTERMing (Linux)
- `handle_stale_pid_file` resolves stale locks; handles the TOCTOU race via lock-then-
  overwrite (ADR from vnc-004, entry #146 fix pattern)

**rmcp transport:**
- Currently uses `rmcp = "=0.16.0"` with `features = ["server", "transport-io", "macros"]`
- `transport-io` provides `rmcp::transport::io::stdio()`
- `transport-async-rw` (pulled in by `server` feature) provides `IntoTransport` for
  `(AsyncRead, AsyncWrite)` pairs and for any type implementing both
- `tokio::net::UnixStream` implements `AsyncRead + AsyncWrite` — can be wrapped as an
  MCP transport without adding any new rmcp features
- No rmcp features for native UDS MCP server are needed; `transport-async-rw` is the
  right primitive

**Claude Code MCP connection model:**
- `.mcp.json` uses `"command"` / `"args"` today — Claude Code spawns the binary per
  session, pipes stdin/stdout
- Claude Code does not natively support UDS-type MCP connections in `.mcp.json`
  (confirmed by ass-008 research: "Unix domain socket transport not supported by MCP
  stdio out-of-the-box; needs MCP client support or a shim")
- The standard pattern for daemon-backed MCP servers is: `.mcp.json` spawns a thin
  client process that bridges stdio ↔ UDS to the running daemon
- The thin client is the **same binary** (`unimatrix`) in "bridge" mode — either as the
  default no-subcommand path or via `unimatrix serve` (no `--daemon`)

### Background Tick Continuity Gap

The background tick (15-minute interval) currently runs only while a session is active.
Confidence refresh, graph compaction, co-access cleanup, extraction rules, and
auto-quarantine are all session-scoped today. Between sessions (the typical overnight
gap), no background work happens. This is the core problem daemon mode solves.

### Socket Multiplexing Decision — **RESOLVED: Two sockets**

The existing hook IPC socket and MCP session socket are separate:
- Hook IPC stays on `unimatrix.sock` (length-framed bincode, synchronous, unchanged)
- MCP sessions use a new `unimatrix-mcp.sock` (newline-delimited JSON-RPC)

Rationale informed by long-term direction: the roadmap moves to a container/SaaS model
serving N projects via HTTP transport (W2-2). The MCP socket is the surface that gets
promoted to an HTTP listener in W2-2. Keeping hook IPC and MCP transport separate now
means W2-2 simply replaces `unimatrix-mcp.sock` with an HTTP listener — no discriminator
logic to untangle. A shared socket would add complexity that gets discarded at W2-2.

### Key Constraint: Graceful Shutdown Changes

Currently `graceful_shutdown` is triggered by: (a) transport close — the only trigger
today. In daemon mode, MCP transport close must NOT trigger daemon shutdown. Instead:
- Session end → close that session's MCP transport, release session state
- Daemon shutdown → SIGTERM/SIGINT only (or an explicit `unimatrix stop` subcommand)

The `running.waiting()` / `QuitReason::Closed` path in main.rs must be restructured:
daemon mode loops accepting new connections; each connection's lifetime is independent
of the daemon's lifetime.

## Proposed Approach

### Component 1: MCP-over-UDS Session Acceptor (daemon server side)

In daemon mode, instead of calling `serve(stdio())` once:
- Bind a second UDS socket (e.g., `unimatrix-mcp.sock`) with 0600 permissions
- Accept loop: for each incoming connection, wrap the `UnixStream` as an rmcp transport
  using `transport-async-rw`, spawn a tokio task that calls `server.serve(stream)` for
  that session
- Each session task runs independently; when a client disconnects, only that task exits
- The `UnimatrixServer` handler must be clone-able (or constructed per-session from
  shared `Arc` references — the current structure already uses `Arc` for everything
  state-ful)
- Daemon shutdown triggered only by SIGTERM/SIGINT

### Component 2: Thin Client / stdio Bridge — **Default binary behavior: RESOLVED Option A**

The default `unimatrix` invocation (no subcommand) becomes bridge mode:
1. Check `unimatrix-mcp.sock` for a live daemon (connect attempt)
2. If alive: bridge stdin → daemon socket, daemon socket → stdout (bidirectional pipe)
3. If not alive: spawn `unimatrix serve --daemon` as a background process, wait up to
   5 seconds for the socket to appear, then bridge
4. The bridge itself is lightweight: no store, no vector index, no tokio runtime beyond
   what's needed for the async copy

`.mcp.json` stays unchanged (`"command": "unimatrix", "args": []`). Explicit stdio
server mode becomes `unimatrix serve --stdio` for development/testing use.

### Component 3: CLI Changes

```
# Start daemon in background (detaches, logs to ~/.unimatrix/{hash}/unimatrix.log)
unimatrix serve --daemon

# Start daemon in foreground (for debugging/testing)
unimatrix serve --stdio

# Connect to daemon as stdio bridge — the new default .mcp.json invocation
unimatrix  (no subcommand — bridges to daemon, auto-starts if needed)

# Stop daemon
unimatrix stop
```

The `.mcp.json` remains `"command": "unimatrix", "args": []`. The binary's default
behavior changes from "start server" to "bridge to daemon or start one".

### Component 4: Security Hardening

- MCP UDS socket: 0600 at bind time (already done for hook socket, same pattern)
- Auto-start stale check: `is_unimatrix_process(pid)` before spawning a new daemon
- Bridge process: no capability escalation — the daemon's per-session tool handler
  enforces the same auth as today

## Acceptance Criteria

- AC-01: `unimatrix serve --daemon` starts the daemon as a background process, binds
  the MCP UDS socket, and exits the launcher process (daemonizes).
- AC-02: The MCP UDS socket file is created with `0600` permissions before accepting
  connections.
- AC-03: The default `unimatrix` invocation (no subcommand) connects to a running
  daemon's MCP socket and bridges stdin/stdout to it.
- AC-04: When the bridge client disconnects (stdin closes), the daemon process continues
  running — the background tick loop, vector index, and all in-memory state are intact.
- AC-05: Auto-start: when `unimatrix` (bridge mode) finds no daemon socket, it spawns
  `unimatrix serve --daemon`, waits up to 5 seconds for the socket to appear, then
  bridges.
- AC-06: Auto-start stale check: the bridge calls `is_unimatrix_process(pid)` on the
  PID file before concluding a daemon is already running; never spawns a second daemon
  when one is healthy.
- AC-07: `unimatrix serve --daemon` fails fast (non-zero exit) if a healthy daemon is
  already running for this project (PID file + flock check).
- AC-08: SIGTERM/SIGINT on the daemon triggers graceful shutdown: vector dump, adapt
  save, DB compaction — same as the current stdio shutdown sequence.
- AC-09: The existing stdio server path (`unimatrix` invoked by Claude Code today)
  continues to work as a fallback mode; no regression in stdio behavior.
- AC-10: The existing hook IPC socket and protocol are unaffected — `unimatrix hook
  SessionStart` continues to work against a running daemon.
- AC-11: Multiple simultaneous MCP sessions can be active against a single daemon
  without data corruption (all state shared via `Arc`; concurrent session tasks).
- AC-12: `unimatrix stop` sends SIGTERM to the daemon (reads PID file) and exits 0 when
  the daemon has exited, non-zero if no daemon was running.

## Constraints

1. **rmcp version pinned at `=0.16.0`** — no upgrade path available without breaking
   changes; UDS MCP transport must use `transport-async-rw` which is already activated
   by the `server` feature.
2. **`#![forbid(unsafe_code)]`** on all crates — `SO_PEERCRED` for caller auth is not
   available without unsafe; peer credentials are already checked via UID comparison
   (Layer 2 auth in the hook listener); same approach must be used for MCP sessions.
3. **`UnimatrixServer` must become Clone or Arc-wrapped** — currently constructed once
   and moved into `serve()`; daemon mode requires constructing a handler per session or
   sharing a single handler behind an Arc. The existing internal state is already `Arc`-
   wrapped, so this is a structural refactor, not a semantic one.
4. **No tokio runtime for hook subcommand** — `unimatrix hook` is synchronous (sub-50ms
   budget, ADR-002); the bridge client path must be async (it copies streams), but must
   not affect the hook path. These are different CLI entry points.
5. **SQLite single-writer** — the daemon already serializes writes via `Mutex<Connection>`
   in the store; no change needed for multi-session concurrency.
6. **Daemonization on Linux** — `fork(2)` + `setsid(2)` pattern for true detachment;
   the `nix` crate (already in Cargo.toml) provides this without unsafe in the calling
   code.
7. **Socket path length limit** — Unix domain socket paths are limited to 104–108 bytes
   depending on OS. The path `~/.unimatrix/{16-char-hash}/unimatrix-mcp.sock` must fit.
   With a typical home directory like `/home/user`, total path is ~55 bytes — within
   limits, but must be documented.
8. **`graceful_shutdown` coupling** — the current shutdown sequence (abort tick handle,
   drop socket guard, drain store refs, compact DB) is tightly coupled to the stdio
   session lifecycle; decoupling it from "transport closed" is a required refactor.

## Resolved Design Decisions

All open questions resolved prior to Phase 1b:

- **OQ-01 → Two sockets.** Hook IPC stays on `unimatrix.sock`; MCP sessions use
  `unimatrix-mcp.sock`. Informed by W2-2 migration path: the MCP socket is what gets
  promoted to HTTP transport; keeping it separate avoids untangling a discriminator later.
- **OQ-02 → Option A (bridge as default).** Default `unimatrix` invocation becomes
  bridge mode. Explicit stdio requires `unimatrix serve --stdio`. `.mcp.json` unchanged.
- **OQ-03 → Log file.** Daemon logs to `~/.unimatrix/{hash}/unimatrix.log`.
- **OQ-04 → 5-second timeout.** Sufficient; embedding model load is lazy.
- **OQ-05 → Feature-cycle-keyed accumulator.** `pending_entries_analysis` becomes a
  `HashMap<feature_cycle, Vec<EntryRecord>>`. Sessions contribute to the bucket for the
  active `feature_cycle`. `context_retrospective` drains the bucket for the specified
  topic — spanning all sessions that worked on that feature. This preserves retrospective
  semantics (scoped to a feature, not a session) and is richer than the current model.
- **OQ-06 → Include `unimatrix stop`.** In scope for this feature.

## Tracking

https://github.com/dug-21/unimatrix/issues/295
