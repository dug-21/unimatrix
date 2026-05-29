# FINDINGS: Remote Telemetry + MCP Transport Unification

**Spike**: ass-064
**Date**: 2026-05-29
**Approach**: Codebase investigation + web research
**Confidence**: Directional

---

## RQ-1: Hook IPC Contract Audit

13 distinct event types flow over the hook IPC socket. Decomposed into 3 tiers by intelligence pipeline criticality.

### Complete Event Catalog

| Event | Direction | Frequency | Wire Type | Fire-and-Forget? | Pipeline Consumer | Tier |
|---|---|---|---|---|---|---|
| **SessionStart** | client->server | per-session | SessionRegister | Yes | Session registry, feature attribution | Critical |
| **Stop** | client->server | per-session | SessionClose | Yes | Session registry, outcome recording | Critical |
| **TaskCompleted** | client->server | per-session | SessionClose | Yes | Session registry (alias of Stop) | Critical |
| **PreToolUse** | client->server | per-tool-call | RecordEvent | Yes | Observation pipeline, cycle event interception | Critical |
| **PostToolUse** | client->server | per-tool-call | RecordEvent | Yes | Rework tracking, observation pipeline, co-access pairs | Critical |
| **PostToolUseFailure** | client->server | per-tool-call | RecordEvent | Yes | Observation pipeline (failure tracking) | Important |
| **UserPromptSubmit** | client->server, server->client | per-user-turn | ContextSearch->Entries | No (sync) | Proactive injection, observation pipeline | Critical |
| **PreCompact** | client->server, server->client | per-compaction | CompactPayload->BriefingContent | No (sync) | Transcript restoration, knowledge re-injection | Critical |
| **SubagentStart** | client->server, server->client | per-subagent | ContextSearch->Entries | No (sync) | Subagent context injection (hookSpecificOutput) | Important |
| **SubagentStop** | client->server | per-subagent | RecordEvent | Yes | Session tracking | Nice-to-have |
| **Ping** | client->server, server->client | periodic/health | Ping->Pong | No (sync) | Health check | Nice-to-have |
| **cycle_start** | client->server | per-feature-start | RecordEvent (via PreToolUse interception) | Yes | Feature cycle management | Important |
| **cycle_stop** | client->server | per-feature-end | RecordEvent (via PreToolUse interception) | Yes | Feature cycle management | Important |

### Tier Summary

- **Critical (6)**: SessionStart, Stop/TaskCompleted, PreToolUse, PostToolUse, UserPromptSubmit, PreCompact. Without these, the intelligence pipeline is materially degraded.
- **Important (4)**: PostToolUseFailure, SubagentStart, cycle_start, cycle_stop. Reduced signal quality but core pipeline still functions.
- **Nice-to-have (3)**: SubagentStop, Ping, unrecognized events. Incremental; can be deferred.

### Latency Analysis

- **Fire-and-forget (9 events)**: Client does not block on response. Tolerate 100ms+ network latency trivially.
- **Synchronous (4 events)**: UserPromptSubmit, PreCompact, SubagentStart, Ping. Client blocks on response. Current budget: 50ms (40ms HOOK_TIMEOUT + 10ms startup). Over HTTP, network RTT alone may consume 20-50ms. Requires relaxed timeout for remote (recommend 500ms) or async delivery with local polling.

---

## RQ-2: Transport Architecture Options

### Recommendation: Hybrid (a) + (b)

Use a **`/observe` HTTP endpoint** on the same TCP listener for hook-originated events, plus a **`context_observe` MCP tool** for in-session telemetry. Reject options (c), (d), and (e).

### Option Comparison Matrix

| Criterion | (a) Dual-endpoint HTTP | (b) MCP tool tunneling | (c) Bidirectional rmcp | (d) WebSocket sidecar | (e) MCP notification extensions |
|---|---|---|---|---|---|
| **Impl complexity** | Medium -- separate HTTP POST handler at `/observe`, same listener, same auth | Low -- one new MCP tool, no new transport surface | High -- rmcp 0.16 SSE is server->client only; no client->server streaming | High -- separate WS transport, rmcp WS module commented out | Medium -- `CustomNotification` exists for server->client; client->server impossible from hooks |
| **Client integration** | Moderate -- hook scripts POST via curl or Claude Code HTTP hook handler | Light for in-session; impossible for hook-originated (hooks run outside MCP session) | Not viable | Heavy -- requires WS client in hook handlers | Not viable for hooks |
| **Auth model fit** | Good -- same bearer token, different path | Excellent -- same auth as all MCP tools | N/A | Separate auth handshake | Same as MCP |
| **Failure modes** | Independent: telemetry can fail without affecting MCP | Coupled: MCP down = telemetry down | N/A | Independent failure domains | Coupled |
| **Enterprise portability** | Good -- per-path routing, telemetry can flow to analytics pipeline | Excellent -- single auth, single tenant resolution | Poor | Moderate -- two connections per tenant | Good |
| **rmcp 0.16 compat** | Yes -- custom tower layer | Yes -- new tool definition | No -- requires transport mods | No -- `ws.rs` commented out | Partial -- client support unproven |

### Why Hybrid

1. **`POST /observe` endpoint** (option a): For hook-originated events. Same bearer token auth. Same `dispatch_request` logic. Hook scripts POST event JSON and receive response (injection content for sync events, 204 for fire-and-forget).
2. **`context_observe` MCP tool** (option b): For in-session telemetry where the agentic loop can call it directly. Eliminates hook process overhead for events originating within the session.
3. Single TCP listener, single port, single TLS, single auth layer. Path-based routing: MCP on `/`, observation on `/observe`.

### Rejected Options

- **(c) Bidirectional rmcp**: rmcp 0.16 streamable HTTP is asymmetric (POST client->server, GET/SSE server->client). Not viable without modifying rmcp.
- **(d) WebSocket sidecar**: rmcp 0.16 `ws.rs` module is commented out. Building custom WS alongside rmcp adds complexity for marginal benefit.
- **(e) MCP notification extensions**: Hook processes run outside the MCP session -- they cannot send notifications through the client's connection. Claude Code issue #2722 reports incomplete notification handling.

---

## RQ-3: Proactive Injection Over HTTP

### Key Insight

The hook process is a local binary even in remote deployments -- it just talks to a remote server instead of a local socket. stdout injection works identically. What changes is how the hook process gets the content: from UDS to HTTP.

### Viable Mechanisms

**Mechanism 1: Endpoint Response Payloads (viable now, recommended)**

When a hook fires and POSTs to `/observe`, the response body carries injection content:
- UserPromptSubmit: response includes ranked entries as formatted text
- PreCompact: response includes briefing content
- SubagentStart: response includes hookSpecificOutput envelope

The hook handler receives the HTTP response body and writes it to stdout locally. Zero client changes required.

**Mechanism 2: SSE Server Notifications (protocol-ready, client-blocked)**

rmcp 0.16 supports server->client SSE:
- `handle_get` opens SSE stream for an existing session
- `ServerNotification::CustomNotification` allows arbitrary method/params
- Server can call `peer.send_notification()` at any time

Claude Code handling is incomplete (#36665 requests server push notification support). SSE injection is protocol-viable but cannot be activated until clients implement it.

**Mechanism 3: Client Polling (fallback)**

A `context_check_notifications` tool polled periodically by the agentic loop. Inferior to SSE but works with all clients today. Only useful for events not triggered by hooks.

### Recommendation

Use Mechanism 1 immediately. Design the observation pipeline to also emit SSE-compatible events, so Mechanism 2 activates when client support lands.

---

## RQ-4: Client Integration Constraints

### Client Compatibility Matrix

| Capability | Claude Code | Codex CLI | Gemini CLI |
|---|---|---|---|
| **MCP: streamable HTTP** | Yes (`type: "http"` in .mcp.json) | Yes (URL-based servers) | Yes (`--transport http`) |
| **MCP: bearer token** | Yes (`-H "Authorization: Bearer <token>"`) | Yes (bearer token config) | Yes (via headers) |
| **MCP: OAuth 2.0** | Yes (built-in) | Yes (built-in) | Partial (401 does not trigger OAuth flow) |
| **Server notifications: SSE** | Incomplete (#36665, #2722) | Unknown/undocumented | Unknown/undocumented |
| **Hook: HTTP handler** | Yes (`"type": "http"`) | No -- shell commands only | No -- shell commands only |
| **Hook: MCP tool handler** | Yes (`"type": "mcp_tool"`) | No | No |
| **Hook event names** | Canonical: PreToolUse, PostToolUse, etc. | Same as Claude Code | Gemini-specific: BeforeTool, AfterTool, SessionEnd (normalized by Unimatrix) |
| **Known bugs** | #28293: custom headers not forwarded on tool call POSTs | #2129: SSE transport support request | Streamable HTTP POST handshake failures; OAuth 401 bug |

### Integration Path Per Client

**Claude Code**: HTTP hook handler (`"type": "http"`) POSTs observation events directly to `/observe` with bearer token. Response body written to stdout for injection. Workaround for #28293: use CLI flag path, not `.mcp.json` headers.

**Codex CLI**: Shell command hooks in `config.toml`. Hook script reads stdin (event JSON), POSTs to `/observe` via `unimatrix hook-remote`, writes response to stdout.

**Gemini CLI**: Shell command hooks in `.gemini/settings.json`. Same pattern. Event name translation already handled by `normalize_event_name` in hook.rs.

### Recommendation

Compile `hook-remote` as a subcommand of the `unimatrix` binary. It reads hook event JSON from stdin, POSTs to the configured Unimatrix URL with bearer token, and writes the response to stdout. Config: two env vars (`UNIMATRIX_URL`, `UNIMATRIX_TOKEN`). Standardizes the pattern across all three clients.

---

## RQ-5: Single vs. Dual Connection

### Recommendation: Single TCP Listener with Path-Based Routing

- One port (8443), one TLS termination, one bearer token validation layer
- MCP path: `POST /` -> `StreamableHttpService` -> rmcp session -> tool dispatch
- Observe path: `POST /observe` -> tower handler -> `dispatch_request` -> `HookResponse`
- Firewall: one port. Proxy: one upstream. Load balancer: one health check.
- Enterprise: single OAuth validation. Tenant resolution from JWT applies uniformly. Per-team routing splits at path level (`/v1/{team-slug}/observe`).

### Rejected Alternatives

- **Purely single connection (MCP-only)**: Requires every hook handler to establish an MCP session. Hook processes are ephemeral -- they cannot share the client's MCP session. Codex and Gemini hooks would need their own MCP client per invocation.
- **Dual TCP listeners (separate ports)**: Over-engineering for personal cloud. Enterprise can add dedicated telemetry port later as additive feature.

---

## RQ-6: Impact on vnc-021 Scope

### Answer: vnc-021 does not need restructuring. Telemetry transport is additive.

Three structural decisions prevent rework:

1. **Path-dispatching tower service**: Route by path rather than forwarding all requests directly to `StreamableHttpService`. Routes: `/health` -> health, `/metrics` -> metrics, `/observe` -> 501 stub, `/*` -> `StreamableHttpService`. Cost: ~50 lines.

2. **ProjectRouter registration**: Include `/observe` in the ProjectRouter's routing tree from day one. Prevents the observe endpoint from being outside the router's path space when multi-project activates.

3. **Composable auth middleware**: Auth middleware's next-layer should be the path-dispatching service, not `StreamableHttpService` directly. Composition choice, not a code change.

### Minimum Viable vnc-021

Ship vnc-021 as currently scoped, plus:
- Path-dispatching tower service (~50 lines)
- `/observe` route returning 501 Not Implemented
- `/observe` registered in ProjectRouter routing tree

Total additional scope: ~50-80 lines. Zero risk to delivery timeline. Telemetry transport ships as a follow-on PR replacing the stub with the real handler.

---

## Out-of-Scope Discoveries

1. **MCP 2026-07-28 spec revision**: Introduces stateless core and Tasks extension. Tasks could be a cleaner mechanism for observation delivery than `context_observe`. Monitor but not actionable until spec finalizes and rmcp implements.

2. **Claude Code server push notifications (#36665)**: When implemented, enables SSE-based proactive injection. Design observation pipeline to be notification-ready.

3. **Gemini CLI OAuth flow bug**: 401 does not trigger OAuth. Blocks enterprise OAuth path. Static bearer token unaffected.

4. **Codex CLI OpenTelemetry**: Codex v0.133+ supports OTLP. An OTLP-compatible endpoint could replace hook scripts for Codex. Significant scope -- not for this wave.

5. **Hook timeout budget for remote**: 50ms budget insufficient for HTTP RTT. Remote hook handlers need configurable timeout (recommend 500ms default) or fully async fire-and-forget delivery.

---

## Recommendations Summary

| RQ | Finding | Action |
|---|---|---|
| RQ-1 | 13 events; 6 critical, 4 important, 3 nice-to-have. Fire-and-forget (9) port trivially; synchronous (4) need relaxed timeouts. | Classify and prioritize for transport implementation. |
| RQ-2 | Hybrid architecture: `/observe` HTTP endpoint + `context_observe` MCP tool. | Implement both on same TCP listener. |
| RQ-3 | Endpoint response payloads for injection (zero client changes). SSE deferred until client support lands. | Design pipeline to be notification-ready. |
| RQ-4 | All three clients support streamable HTTP. Provide `unimatrix hook-remote` subcommand. | Build hook-remote CLI as the cross-client standard. |
| RQ-5 | Single TCP listener, path-based routing. | Same port, same TLS, same auth. |
| RQ-6 | Add path-dispatching tower service + `/observe` stub to vnc-021. ~50 lines. | No restructuring; telemetry is additive. |
