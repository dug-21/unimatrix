## ADR-001: client_type_map Storage as Arc<Mutex<HashMap<String, String>>> on UnimatrixServer

### Context

VNC-014 must capture `clientInfo.name` from the MCP `initialize` handshake and propagate it to
every subsequent audit record in that rmcp session. The mapping must survive for the duration of a
connection and be readable from all tool call handlers.

Two coupling surfaces exist that are deliberately kept separate:
- The rmcp transport-level session ID (`Mcp-Session-Id` UUID), managed by rmcp's session manager
- The agent-declared `session_id` tool parameter, which keys `SessionRegistry`

These are different namespaces. Bridging them is not needed: `client_type` is resolved at tool
call time by looking up the rmcp session ID directly in a dedicated map, then attached to
`ToolContext` before the handler executes.

SR-01 flags that `Arc<Mutex<...>>` is a global write-path lock. The alternatives evaluated were:

(A) `Arc<Mutex<HashMap<String, String>>>` on `UnimatrixServer` — simple, correct for current
    concurrency profile. Write path is only at `initialize` time (one write per session
    establishment). Read path is at every tool call but is O(1) and very short-held.

(B) `DashMap<String, String>` — concurrent hash map avoiding the global Mutex. Adds a dependency
    and additional complexity for a contention point that is not currently saturated. STDIO has one
    session ever; HTTP sessions are few in current deployments.

(C) Per-session state injected by rmcp — not available in rmcp 0.16.0 without forking the crate.
    The server handler is `Clone`d per connection by rmcp; per-session state is possible only if
    stored outside the server struct or via the existing shared-state fields.

(D) Keying on the agent-declared `session_id` (wiring into `SessionRegistry`) — architecturally
    wrong. Attribution is transport-attested; it must not depend on agent-supplied parameters.

Approach (A) is chosen. The Mutex is held for one HashMap insert (at `initialize` time) and one
HashMap lookup (at each tool call). Neither hold spans overlap I/O. Contention is negligible at
current HTTP session counts.

For the **stdio path**: stdio has exactly one connection per server lifetime. The empty string `""`
is used as the map key (SR-06 documents the single-session invariant). If the `""` key is
overwritten (only possible in tests via sequential stdio connect+reconnect), a WARN is logged.

### Decision

Add `client_type_map: Arc<Mutex<HashMap<String, String>>>` to `UnimatrixServer` struct.

- Key: rmcp-level session ID string — the `Mcp-Session-Id` header UUID for HTTP; `""` for stdio
- Value: `clientInfo.name` string, truncated to 256 chars at write time (AC-10)
- Populated: in the `ServerHandler::initialize` override (ADR-002)
- Consumed: in `build_context_with_external_identity()` (ADR-003)
- The field is `Arc`-wrapped so `UnimatrixServer::clone()` (required by rmcp) shares the same map
  across the cloned instance that handles subsequent tool calls

The `""` key for stdio is a documented invariant. A `tracing::warn!` is emitted if the `""`
key is overwritten (second stdio client in same process lifetime — only possible in unusual test
scenarios).

`DashMap` is deferred to the W2-2 HTTP transport work, if concurrent session counts grow to where
the Mutex becomes a measurable bottleneck. That decision point is documented explicitly in the
field comment.

### Consequences

Easier:
- Simple, dependency-free implementation
- Correct cross-session isolation for HTTP (key is the UUID per session)
- Single code path handles both HTTP and stdio

Harder:
- `UnimatrixServer::new()` gains another `Arc`-wrapped field (minor, consistent with existing pattern)
- Any future scale-out to high HTTP concurrency must revisit this to `DashMap` (documented)
- Tests constructing `UnimatrixServer` directly will have an additional initialized field
