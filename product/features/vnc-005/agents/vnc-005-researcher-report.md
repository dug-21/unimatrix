# vnc-005 Researcher Agent Report

## Summary

SCOPE.md written to `/workspaces/unimatrix/product/features/vnc-005/SCOPE.md`.

The problem space is well-defined but has one surprising finding: significant
infrastructure already exists that is relevant to this feature, and one critical
constraint changes the implementation approach.

---

## Key Findings

### 1. UDS infrastructure already exists — but for the wrong protocol

The server already binds `~/.unimatrix/{hash}/unimatrix.sock` at startup
(`uds/listener.rs`). It is 0600, peer-credential-authenticated, and has a full
accept loop. However it speaks a custom binary hook protocol (`HookRequest`/`HookResponse`
via bincode). This socket cannot be reused for MCP sessions without a protocol
discriminator. Recommendation: bind a second socket (`unimatrix-mcp.sock`) for MCP.

### 2. Claude Code does NOT support UDS-native MCP in .mcp.json

The product vision says "Claude Code connects via UDS instead of spawning a new stdio
process per session" — this requires a thin stdio-to-UDS bridge, not a native UDS config
type. The ass-008 security research explicitly documented this: "Unix domain socket
transport not supported by MCP stdio out-of-the-box; needs MCP client support or a shim."
The bridge pattern is: `.mcp.json` keeps `command`/`args`, but the binary's default
behavior shifts from "start full server" to "connect to daemon or auto-start one, bridge
stdio to MCP socket".

### 3. rmcp transport-async-rw is sufficient — no new dependencies needed

`tokio::net::UnixStream` implements `AsyncRead + AsyncWrite` and is compatible with
rmcp's `IntoTransport<Role, Error, TransportAdapterAsyncCombinedRW>`. The `server`
feature already activates `transport-async-rw`. No new rmcp features or crates needed
for the daemon's MCP-over-UDS listener.

### 4. UnimatrixServer structural constraint

`UnimatrixServer` is currently constructed once and moved into `serve(stdio())`. In
daemon mode it must be shareable across concurrent sessions. All internal state is
already `Arc`-wrapped; what's needed is either (a) making the server Clone or (b)
constructing a new handler instance per session from shared Arc references. Option (b)
is lower risk and avoids Clone constraints on rmcp handler traits.

### 5. graceful_shutdown is tightly coupled to "transport closed"

The current shutdown sequence runs when `running.waiting()` returns. In daemon mode,
"transport closed" means "one session ended" — not "daemon should shut down". The
shutdown trigger must be decoupled: session end → session cleanup only; daemon shutdown
→ SIGTERM/SIGINT only.

### 6. Background tick continuity is the primary value driver

The background tick (15-minute intervals) already does: confidence refresh, graph
compaction, co-access cleanup, auto-quarantine, extraction rules. Currently this stops
when the session ends. Daemon mode is what makes these operations actually happen between
sessions — every Wave 1+ feature depends on this.

---

## Scope Boundaries — Rationale

**Included:**
- MCP-over-UDS session acceptor (daemon server side)
- Thin stdio bridge (client / `.mcp.json` command path)
- Auto-start with stale PID detection
- `unimatrix serve --daemon` and `unimatrix stop` subcommands
- Session-scoped shutdown (vs daemon-scoped shutdown decoupling)

**Excluded:**
- HTTP transport — Wave 2 per product vision, additive on top of working daemon
- Windows — UDS on Windows is non-standard; dev workspace is Linux/macOS
- Hook IPC changes — existing hook socket and protocol untouched
- SO_PEERCRED caller auth for MCP — separate feature, orthogonal

---

## Open Questions Raised

- OQ-01 (blocker): One socket or two? Recommendation is two; needs human confirmation.
- OQ-02: Default binary behavior change — backward-compatible or opt-in?
- OQ-03: Daemon log destination (file, syslog, or discard)
- OQ-04: Auto-start timeout adequacy (5s proposed)
- OQ-05: Shared `pending_entries_analysis` semantics across concurrent sessions
- OQ-06: Whether `unimatrix stop` is in-scope for this feature

---

## Risks

- **Structural refactor risk (Medium):** Decoupling `graceful_shutdown` from transport
  close and making `UnimatrixServer` session-shareable touches the core server lifecycle.
  Well-understood change but requires careful test coverage.
- **Default behavior change risk (Medium):** If the default no-subcommand path changes
  meaning, existing users with `"command": "unimatrix"` in `.mcp.json` see different
  behavior. Must be handled carefully (backward compat or clear migration).
- **Daemonization on Linux (Low):** `fork`+`setsid` via the `nix` crate is standard but
  has edge cases (open file descriptors, signal masks). Worth a targeted test.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for daemon mode, UDS transport, MCP server startup,
  PidGuard patterns — returned relevant ADRs (#77 stdio transport, #245 socket lifecycle,
  #667 lock-then-mutate PID guard) and patterns (#300 UDS capability set, #1560
  background tick state cache, #1366 tick loop error recovery). No prior daemon mode
  entries existed.
- Stored: entry #1897 "Daemon MCP via UDS: thin stdio bridge required for Claude Code
  compatibility" via `/uni-store-pattern`
- Stored: entry #1898 "UDS socket separation: hook IPC and MCP sessions should use
  distinct sockets" via `/uni-store-pattern`
