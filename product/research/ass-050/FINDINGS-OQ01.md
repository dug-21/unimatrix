# FINDINGS: OQ-01 Resolution — rmcp 0.16 clientInfo + Session ID Availability

**Spike**: ASS-050 (supplemental OQ resolution)
**Date**: 2026-04-22
**Approach**: investigation — direct rmcp 0.16 source read
**Confidence**: empirical — all claims cite specific file and line

rmcp source: `/usr/local/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rmcp-0.16.0/`

---

## OQ-01: Is `clientInfo` accessible at tool call dispatch time via `RequestContext`?

**Answer: PARTIAL** — `clientInfo` is accessible at tool call time but NOT through `RequestContext.extensions`. It is accessible through `RequestContext.peer.peer_info()`, which returns `Option<&ClientInfo>`. The access path differs from what was assumed.

### Evidence

**`ClientInfo` is a type alias for `InitializeRequestParams`** (`src/model.rs:785`):
```rust
pub type ClientInfo = InitializeRequestParams;
```

**`InitializeRequestParams`** (`src/model.rs:740-749`) contains `client_info: Implementation`. **`Implementation`** (`src/model.rs:837-838`) contains `pub name: String`. The client name is at `client_info.name` — one level deeper than `ClientInfo.name`.

**`RoleServer` assigns `PeerInfo = ClientInfo`** (`src/service/server.rs:40`):
```rust
type PeerInfo = ClientInfo;
```

**`Peer<R>::peer_info()`** (`src/service.rs:410-412`):
```rust
pub fn peer_info(&self) -> Option<&R::PeerInfo> {
    self.info.get()
}
```

**`Peer` is populated during `serve_server_with_ct_inner()`** (`src/service/server.rs:203`):
```rust
let (peer, peer_rx) = Peer::new(id_provider, Some(peer_info.params.clone()));
```
`peer_info.params` is the `InitializeRequestParams` received during the MCP `initialize` handshake. It is stored in the `Peer` before any tool calls are dispatched.

**`RequestContext` carries `peer`** (`src/service.rs:575-583`):
```rust
pub struct RequestContext<R: ServiceRole> {
    pub ct: CancellationToken,
    pub id: RequestId,
    pub meta: Meta,
    pub extensions: Extensions,
    pub peer: Peer<R>,    // ClientInfo lives here
}
```

**`RequestContext` flows into `ToolCallContext`** (`src/handler/server/tool.rs:31-37`). The `FromContextPart` trait allows tool handlers to declare `RequestContext<RoleServer>` as a parameter and receive it automatically (`src/handler/server/common.rs:98-105`).

### Exact Access Path

```rust
async fn my_tool(
    ctx: RequestContext<RoleServer>,
    params: Parameters<MyParams>,
) -> impl IntoCallToolResult {
    let client_name: Option<&str> = ctx
        .peer
        .peer_info()                        // Option<&ClientInfo> = Option<&InitializeRequestParams>
        .map(|ci| ci.client_info.name.as_str());
    // client_name is the MCP clientInfo.name string
}
```

### What Is NOT in `extensions`

`clientInfo` is NOT stored in `RequestContext.extensions`. The extensions map at tool dispatch time contains `http::request::Parts` — the HTTP request parts injected by `StreamableHttpService::handle_post()` (`tower.rs:325-334` and `383-384`):
```rust
ClientJsonRpcMessage::Request(req) => {
    req.request.extensions_mut().insert(part);  // http::request::Parts, not ClientInfo
}
```

`serve_inner()` then moves extensions from the request into `RequestContext` (`src/service.rs:832-845`):
```rust
std::mem::swap(&mut extensions, request.extensions_mut());
let context = RequestContext { ..., extensions, peer: peer.clone(), ... };
```

### Caveat

`peer_info()` returns `None` if `initialize` was never completed — specifically when `serve_directly()` is used with `peer_info: None` (`src/service.rs:594-607`). Unimatrix's current stateful transport path uses `serve_server()`, which always completes the handshake before tool dispatch. Guard the `None` case defensively in implementation.

---

## OQ-03: Is the rmcp-level session ID accessible at tool call dispatch time?

**Answer: PARTIAL** — The rmcp session ID (UUID assigned at `initialize`) is NOT a field on `RequestContext` or `Peer`, and NOT stored as a typed extension. It IS indirectly accessible via the `http::request::Parts` extension in `RequestContext.extensions` — specifically by reading the `Mcp-Session-Id` header. This is rmcp's documented mechanism for exposing HTTP-level data to handlers.

### Evidence

**Session ID type and generation** (`src/transport/common/server_side_http.rs:14-17`):
```rust
pub type SessionId = Arc<str>;
pub fn session_id() -> SessionId {
    uuid::Uuid::new_v4().to_string().into()
}
```
The rmcp session ID is a UUID v4 string, server-assigned, not client-controlled.

**`http::request::Parts` is injected for every tool call POST** (`tower.rs:323-334`):
```rust
match &mut message {
    ClientJsonRpcMessage::Request(req) => {
        req.request.extensions_mut().insert(part);  // 'part' is http::request::Parts
    }
    ...
}
```
The `part` here is the HTTP request parts after body extraction. It carries all HTTP headers including `Mcp-Session-Id` that the client echoes back on every request within a session.

**`serve_inner()` moves extensions into `RequestContext`** (`src/service.rs:832-845`) — same path as OQ-01.

**rmcp's own documentation** (`tower.rs:62-69`) explicitly describes this as the intended mechanism:
```
The http service will consume the request body, however the rest part will be remain
and injected into Extensions, which you can get from RequestContext.

use rmcp::handler::server::tool::Extension;
use http::request::Parts;
async fn my_tool(Extension(parts): Extension<Parts>) {
    tracing::info!("http parts:{parts:?}")
}
```

### Access Patterns

```rust
use http::request::Parts;
use rmcp::handler::server::tool::Extension;

// Pattern A — via Extension extractor in a handler:
async fn my_tool(
    Extension(parts): Extension<Parts>,
) -> impl IntoCallToolResult {
    let rmcp_session_id = parts.headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());
}

// Pattern B — from RequestContext.extensions (for build_context_with_external_identity):
fn extract_rmcp_session_id(extensions: &rmcp::model::Extensions) -> Option<String> {
    extensions
        .get::<Parts>()
        .and_then(|parts| parts.headers.get("mcp-session-id"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned())
}
```

### Distinction from `session_id` tool parameter

These are categorically different:

| | rmcp session ID | `session_id` tool parameter |
|---|---|---|
| Source | Server-assigned UUID at `initialize` | Client-chosen string in JSON args |
| Spoofable? | No — assigned by server, client echoes it | Yes — entirely client-controlled |
| Stable per session? | Yes — same UUID for all calls in a session | Only if client sends consistently |
| Available in stateless mode? | No | Yes |

### Caveat: stateless mode

In stateless mode (`stateful_mode = false`), no `Mcp-Session-Id` is assigned — each POST is processed independently. The `http::request::Parts` extension is still injected (`tower.rs:463`), but the header will be absent. Implementation must handle the `None` case.

---

## Implementation Implications for vnc-014

### For `agent_attribution` population

`clientInfo.name` is accessible at tool dispatch time with no rmcp upstream changes. The path is:
```
ctx.peer.peer_info().map(|ci| ci.client_info.name.as_str())
```

For Unimatrix's `build_context()`: `RequestContext<RoleServer>` is available in `ToolCallContext.request_context` but is not currently passed into `build_context()`. The required work is adding `build_context_with_external_identity()` (Seam 2 from FINDINGS.md Section 5) that accepts `&RequestContext<RoleServer>` and extracts `clientInfo.name` from it. No rmcp changes.

### For rmcp session ID — `client_type_map` key, NOT `audit_log.session_id`

The rmcp-assigned session UUID is accessible via `RequestContext.extensions.get::<Parts>()` + `Mcp-Session-Id` header extraction. This is server-assigned and non-spoofable.

Both `clientInfo.name` and the rmcp session ID are available simultaneously at tool dispatch. Their correct uses:
- `clientInfo.name` → `agent_attribution` (non-spoofable client identity for audit)
- rmcp session UUID → `client_type_map` lookup key (vnc-014 design)

> **CORRECTED — 2026-04-22**: The original conclusion — "rmcp UUID → `audit_log.session_id`"
> — was wrong. `audit_log.session_id` must use the agent-declared session_id (from
> `ToolContext.audit_ctx.session_id`), not the rmcp UUID. The behavioral provenance chain
> goes `audit_log.session_id → sessions.session_id → sessions.feature_cycle =
> cycle_events.cycle_id`. `sessions.session_id` is keyed on the agent-declared value;
> using the rmcp UUID for `audit_log.session_id` would break the first hop.

### Required implementation work — all in Unimatrix, no rmcp changes

1. **`server.rs`** — add `build_context_with_external_identity(request_context: &RequestContext<RoleServer>, ...)` overload. Extract `clientInfo.name` via `request_context.peer.peer_info()`. Extract rmcp session UUID via `extract_rmcp_session_id(&request_context.extensions)`.
2. **All 12 tool handlers** — thread `request_context` into the new overload. `ToolCallContext.request_context` already carries it.
3. **`AuditEvent` struct** — add `agent_attribution` field (per schema migration already specified in FINDINGS.md Section 4).

---

## Additional Session/Identity Data Found in rmcp Not Previously Known

**A. `SessionId` is always UUID v4** (`server_side_http.rs:16-18`). Unimatrix can validate the UUID format when reading the `mcp-session-id` header to detect malformed values.

**B. `Peer<RoleServer>` exposes typed capability check methods** (`service/server.rs:393-404`). `supports_sampling_tools()` and similar methods read `ClientCapabilities` from `peer_info()`. These could be used to gate Unimatrix behavior on client-declared capabilities at handshake time, independent of auth. Relevant for any future capability-negotiated behavior.

**C. `serve_directly()` sets `peer_info: None`** (`service.rs:594-607`). This code path skips `initialize` entirely. If Unimatrix ever uses `serve_directly()`, `peer.peer_info()` is `None`. The current server uses `serve_server()` exclusively; defensive `None` guards are sufficient.

---

## Recommendations Summary

| Question | Answer | Required work |
|----------|--------|---------------|
| OQ-01: Is `clientInfo.name` accessible at tool call time? | YES — via `ctx.peer.peer_info().map(\|ci\| ci.client_info.name.as_str())` | Unimatrix only: `build_context_with_external_identity()` overload (Seam 2) |
| OQ-03: Is the rmcp session UUID accessible at tool call time? | YES — via `extensions.get::<Parts>()` + `Mcp-Session-Id` header | Unimatrix only: `extract_rmcp_session_id()` helper — use as `client_type_map` key, NOT as `audit_log.session_id` *(corrected 2026-04-22)* |
| vnc-014 blocked? | **NO** — both data sources confirmed, implementation path clear, no rmcp upstream changes needed | |
