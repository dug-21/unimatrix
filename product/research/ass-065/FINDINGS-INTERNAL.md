# FINDINGS: rmcp 0.16 to 1.4 Migration — Internal API Surface and Transport Impact

**Spike**: ass-065
**Track**: Internal
**Date**: 2026-05-29
**Approach**: investigation
**Confidence**: validated

---

## Findings

### Q1: What rmcp APIs, types, traits, and features does `unimatrix-server` use across all source files?

**Answer**: The codebase uses 6 Cargo features and imports rmcp types across 18 distinct source files (plus test modules). The integration surface consists of 3 traits/impls, 2 proc-macro attributes, 10 model types, 3 transport types, 3 service types, and 1 top-level function.

**Evidence**: File-by-file audit of every `use rmcp` and `rmcp::` reference in `crates/unimatrix-server/src/`.

#### Cargo Features Used

```toml
rmcp = { version = "=0.16.0", features = [
    "server",
    "client",
    "transport-io",
    "macros",
    "transport-streamable-http-server",
    "transport-streamable-http-server-session",
] }
```

Note: `transport-async-rw` is also used transitively (enabled by `server` feature) for UDS transport via `(OwnedReadHalf, OwnedWriteHalf)` tuple.

#### Complete Type/Trait/Function Inventory

**Proc-Macro Attributes (from `macros` feature)**

| Macro | File | Usage | API Surface |
|-------|------|-------|-------------|
| `#[rmcp::tool_handler]` | `server.rs:1022` | Applied to `impl rmcp::ServerHandler for UnimatrixServer` | Public (trait impl) |
| `#[rmcp::tool_router(vis = "pub(crate)")]` | `tools.rs:447` | Applied to `impl UnimatrixServer` block containing 14 tool methods | Internal |
| `#[tool(name = "...", description = "...")]` | `tools.rs` (14 occurrences) | Applied to each MCP tool handler method | Internal |

**Traits Implemented**

| Trait | File | Usage |
|-------|------|-------|
| `rmcp::ServerHandler` | `server.rs:1023` | Core trait — `get_info()` returns `ServerInfo`, `initialize()` handles MCP handshake |

**Model Types (`rmcp::model::*`)**

| Type | Files | Usage Pattern |
|------|-------|---------------|
| `ErrorCode` | `error.rs` | Construct custom error codes via `ErrorCode(-32001)` etc. |
| `ErrorData` | `error.rs`, `server.rs`, `tools.rs`, `graph_read*.rs`, `services/mod.rs`, `response/retrospective.rs` | Primary error return type; constructed via `ErrorData::new(code, message, None)`, `ErrorData::from(ServerError)`, `ErrorData::invalid_params(msg, None)` |
| `CallToolResult` | `tools.rs`, `graph_read.rs`, `response/*.rs`, `test_support.rs`, `response/status.rs` | Return type for all tool handlers; constructed via `CallToolResult::success(vec![...])`, `CallToolResult::error(vec![...])` |
| `Content` | `response/*.rs`, `tools.rs` | MCP response content items; constructed via `Content::text(string)` |
| `RawContent` | `tools.rs:1178`, `test_support.rs:246` | Pattern-matched as `RawContent::Text(t)` to access `t.text` for mutation/extraction |
| `Implementation` | `server.rs:8` | Struct with `name`, `version` fields; used in `ServerInfo` construction |
| `ServerCapabilities` | `server.rs:8` | Used via builder: `ServerCapabilities::builder().enable_tools().build()` |
| `ServerInfo` | `server.rs:8` | Return type of `get_info()`; struct with `server_info`, `capabilities`, `instructions` fields |
| `InitializeRequestParams` | `server.rs:1040` | Parameter type for `initialize()` handler; contains `client_info.name` |
| `InitializeResult` | `server.rs:1042` | Return type of `initialize()`; same as `ServerInfo` |
| `ClientInfo` | `server.rs:3257` (test only) | MCP client handshake parameters |
| `ClientCapabilities` | `server.rs:3249` (test only) | Client capabilities for handshake |
| `ProtocolVersion` | `server.rs:3249` (test only) | `ProtocolVersion::LATEST` |

**Service Types (`rmcp::service::*`)**

| Type | Files | Usage Pattern |
|------|-------|---------------|
| `RequestContext<RoleServer>` | `tools.rs` (14 tool handlers), `server.rs:393` | Second parameter of every tool handler; accessed via `.extensions.get::<http::request::Parts>()` to extract `mcp-session-id` header |
| `ServiceExt` (trait) | `main.rs:10`, `uds/mcp_listener.rs:22`, `server.rs:3250` (test) | Provides `.serve(transport)` method on `UnimatrixServer`; returns `RunningService` |

**Transport Types (`rmcp::transport::*`)**

| Type | Files | Usage Pattern |
|------|-------|---------------|
| `rmcp::transport::io::stdio()` | `main.rs:1229` | Creates stdio transport for `server.serve()` |
| `StreamableHttpService<UnimatrixServer, LocalSessionManager>` | `router.rs:251` | Held inside `McpAdapter`; implements `tower::Service<Request<Full<Bytes>>>` |
| `StreamableHttpServerConfig` | `router.rs:23` | Used as `StreamableHttpServerConfig::default()` in `McpAdapter::new()` |
| `LocalSessionManager` | `router.rs:22` | Session manager for HTTP transport; created via `Arc::new(LocalSessionManager::default())` |

**Handler/Router Types (`rmcp::handler::*`)**

| Type | Files | Usage Pattern |
|------|-------|---------------|
| `rmcp::handler::server::router::tool::ToolRouter<Self>` | `server.rs:7, 226` | Generated by `#[tool_router]` macro; stored as `tool_router` field on `UnimatrixServer` |
| `rmcp::handler::server::wrapper::Parameters` | `tools.rs:10` | Wrapper type for deserialized tool parameters; used as `Parameters(params): Parameters<ParamStruct>` |

**Top-Level Items**

| Item | Files | Usage Pattern |
|------|-------|---------------|
| `rmcp::ServiceExt` (re-export) | `main.rs:10` | Trait providing `.serve()` on server types |
| `rmcp::RoleServer` | `server.rs`, `tools.rs` | Type parameter for `RequestContext<RoleServer>` |
| `rmcp::ErrorData` (re-export) | Many files | Same as `rmcp::model::ErrorData`; used as `rmcp::ErrorData::from(...)` |
| `rmcp::serve_client` | `server.rs:3273` (test only) | Creates MCP client for in-memory handshake tests |

**RunningService API (consumed but not imported by name)**

| Method | Files | Usage |
|--------|-------|-------|
| `.cancellation_token()` | `main.rs:1245`, `uds/mcp_listener.rs:269` | Get token to cancel the MCP transport loop |
| `.waiting().await` | `main.rs:1257`, `uds/mcp_listener.rs:282` | Wait for the transport loop to exit; returns `QuitReason` |

#### File-by-File Import Summary

| File | rmcp Imports |
|------|-------------|
| `server.rs` | `ToolRouter`, `Implementation`, `ServerCapabilities`, `ServerInfo`, `ServerHandler`, `#[tool_handler]`, `RequestContext<RoleServer>`, `InitializeRequestParams`, `InitializeResult`, `ErrorData` |
| `main.rs` | `ServiceExt`, `transport::io::stdio()` |
| `error.rs` | `ErrorCode`, `ErrorData` |
| `http/router.rs` | `LocalSessionManager`, `StreamableHttpServerConfig`, `StreamableHttpService` |
| `mcp/tools.rs` | `Parameters`, `CallToolResult`, `#[tool_router]`, `#[tool]`, `RequestContext<RoleServer>`, `ErrorData`, `Content`, `RawContent` |
| `mcp/graph_read.rs` | `CallToolResult`, `ErrorData`, `Content` |
| `mcp/graph_read_filter.rs` | `ErrorData` |
| `mcp/graph_read_inverse.rs` | `ErrorData` |
| `mcp/graph_read_neighbors.rs` | `ErrorData` |
| `mcp/graph_read_path.rs` | `ErrorData` |
| `mcp/graph_read_subgraph.rs` | `ErrorData` |
| `mcp/graph_read_supersession.rs` | `ErrorData` |
| `mcp/response/mod.rs` | `CallToolResult`, `Content` |
| `mcp/response/briefing.rs` | `CallToolResult`, `Content` |
| `mcp/response/entries.rs` | `CallToolResult`, `Content` |
| `mcp/response/mutations.rs` | `CallToolResult`, `Content` |
| `mcp/response/retrospective.rs` | `CallToolResult`, `Content`, `ErrorData` |
| `mcp/response/status.rs` | `CallToolResult`, `Content` |
| `services/mod.rs` | `ErrorData` (via `From<ServiceError>` impl) |
| `test_support.rs` | `RawContent` |
| `uds/mcp_listener.rs` | `ServiceExt` |

#### Error Conversion Surface

The codebase implements `From<ServerError> for ErrorData` in `error.rs` and `From<ServiceError> for rmcp::ErrorData` in `services/mod.rs`. These are the two bridge points where internal error types convert to rmcp's wire error format. The `ErrorData` constructor uses:
- `ErrorData::new(code, message, data)` — 30+ call sites
- `ErrorData::from(ServerError)` — 40+ call sites via `.map_err(rmcp::ErrorData::from)?`
- `ErrorData::invalid_params(msg, None)` — 6 call sites

**Recommendation**: Build the migration plan around these five categories: (1) proc-macro attributes (`tool_handler`, `tool_router`, `tool`), (2) `ServerHandler` trait impl, (3) transport layer types (`StreamableHttpService`, `LocalSessionManager`, `stdio`), (4) model types (`ErrorData`, `CallToolResult`, `Content`), (5) service lifecycle (`ServiceExt::serve`, `RunningService`, `cancellation_token`, `waiting`). Categories 1 and 2 are the highest-risk because they involve code generation. Categories 3 and 4 are highest-volume. Category 5 has the most behavioral coupling.

---

### Q4: What is the impact on vnc-021's transport layer (HTTPS listener, bearer token auth, session management)?

**Answer**: vnc-021's transport layer has a **tightly coupled dependency on 4 rmcp transport APIs** but is **well-isolated by an intentional adapter boundary** (ADR-003). The auth layer (`http/auth.rs`) has **zero rmcp dependency**. The listener (`http/listener.rs`) has **zero rmcp dependency**. All rmcp coupling is concentrated in `http/router.rs` via the `McpAdapter` struct. Session management relies on `LocalSessionManager` from the `transport-streamable-http-server-session` feature.

**Evidence**: Detailed analysis of all files under `src/http/` plus `uds/mcp_listener.rs` and relevant server lifecycle code.

#### Transport Layer Architecture (vnc-021)

```
TcpListener (http/listener.rs)          -- NO rmcp dependency
    |
TlsAcceptor (http/tls.rs)              -- NO rmcp dependency
    |
StaticTokenAuth (http/auth.rs)          -- NO rmcp dependency
    |
PathRouter (http/router.rs)             -- NO rmcp dependency (dispatches by path)
    |
    +-- GET /health -> health_response  -- NO rmcp dependency
    +-- POST /observe -> 501 stub       -- NO rmcp dependency
    +-- /* else -> McpAdapter           -- ALL rmcp dependency lives here
                    |
                    StreamableHttpService<UnimatrixServer, LocalSessionManager>
```

#### Specific rmcp APIs Used in Transport Layer

**`http/router.rs` — McpAdapter (rmcp isolation boundary)**

| API | Usage | Impact Assessment |
|-----|-------|-------------------|
| `StreamableHttpService<UnimatrixServer, LocalSessionManager>` | Stored as field `streamable` on `McpAdapter` | Core rmcp-to-HTTP adapter. Type parameters may change if `ServerHandler` trait changes. |
| `LocalSessionManager` | Created via `Arc::new(LocalSessionManager::default())` | Session management for HTTP transport. If session API changes, this is affected. |
| `StreamableHttpServerConfig` | Created via `StreamableHttpServerConfig::default()` | Configuration for the HTTP transport. If config fields change, only default construction is affected. |
| `StreamableHttpService::new(factory, session_manager, config)` | Constructor call in `McpAdapter::new()` | The factory closure `move \|\| Ok(server.clone())` pattern may change. |
| `StreamableHttpService::call(request)` | Called in `McpAdapter::handle()` with `Request<Full<Bytes>>` | This is the tower `Service::call` — the request body type (`Full<Bytes>`) must match what rmcp expects. |

**`http/auth.rs` — StaticTokenAuth**

Zero rmcp dependency. Auth is a pure tower middleware layer. It injects `ResolvedIdentity` into `http::Extensions`, which rmcp propagates through to `RequestContext.extensions` (validated by R-01 spike, documented in ADR-003). The auth layer interacts with rmcp only indirectly: the `ResolvedIdentity` it inserts must survive rmcp's internal request processing.

**`http/listener.rs` — HTTP Listener**

Zero rmcp dependency. Pure hyper/tower TCP accept loop.

**`uds/mcp_listener.rs` — UDS Transport**

| API | Usage | Impact Assessment |
|-----|-------|-------------------|
| `ServiceExt` (trait) | `use rmcp::ServiceExt` — provides `.serve(transport)` | Entry point for MCP over UDS. |
| `.serve((read_half, write_half))` | Passes `(OwnedReadHalf, OwnedWriteHalf)` tuple as transport | Relies on `IntoTransport<RoleServer>` blanket impl from `transport-async-rw` feature. |
| `RunningService.cancellation_token()` | Gets CancellationToken for daemon shutdown propagation | Behavioral API. |
| `RunningService.waiting().await` | Waits for session end; returns `QuitReason` | Session lifecycle API. |

**`main.rs` — Stdio Transport**

| API | Usage | Impact Assessment |
|-----|-------|-------------------|
| `ServiceExt` (trait) | `use rmcp::ServiceExt` — provides `.serve(transport)` | Same trait as UDS path. |
| `rmcp::transport::io::stdio()` | Creates stdio transport | Transport factory function. |
| `RunningService.cancellation_token()` | Gets CancellationToken for signal handling | Same pattern as UDS. |
| `RunningService.waiting().await` | Waits for transport exit | Same pattern as UDS. |

#### Bearer Token Auth Integration with rmcp Session Model

The bearer token auth does NOT directly integrate with rmcp's session model. They operate on different layers:

1. **Bearer token auth** (`http/auth.rs`): HTTP request level. Validates `Authorization: Bearer <hex>` header. Injects `ResolvedIdentity` into `http::Extensions`. Runs BEFORE rmcp.
2. **rmcp session management** (`LocalSessionManager`): MCP protocol level. Manages `Mcp-Session-Id` UUIDs. Runs INSIDE rmcp's `StreamableHttpService`.
3. **Session ID extraction** (`server.rs:440-445`): `build_context_with_external_identity` reads `mcp-session-id` header from `RequestContext.extensions.get::<http::request::Parts>()`.
4. **Extension propagation** is the critical bridge: Bearer token auth inserts `ResolvedIdentity` → rmcp propagates it via `Parts` → tool handlers extract both identity and session ID from the same `Parts` struct.

#### Transport Impact Summary

1. **LOW RISK — Auth** (`http/auth.rs`): Zero rmcp dependency. Will not break.
2. **LOW RISK — Listener** (`http/listener.rs`): Zero rmcp dependency. Will not break.
3. **MEDIUM RISK — McpAdapter** (`http/router.rs`): Concentrated rmcp dependency. ~40-line adapter isolated by ADR-003.
4. **MEDIUM RISK — Session management**: `LocalSessionManager` rename/API changes affect `McpAdapter::new()`.
5. **MEDIUM RISK — Extension propagation**: Most fragile coupling. If rmcp 1.x changes how `http::request::Parts` are injected into `RequestContext.extensions`, session ID extraction and identity extraction break.
6. **MEDIUM RISK — UDS transport**: `(OwnedReadHalf, OwnedWriteHalf)` tuple-as-transport pattern relies on blanket `IntoTransport` impl.
7. **LOW RISK — Stdio transport**: Simple factory, one-line fix if renamed.
8. **MEDIUM RISK — Service lifecycle**: `ServiceExt::serve()`, `RunningService`, `.cancellation_token()`, `.waiting()` — 4-method lifecycle API used in both stdio and UDS paths.

**Recommendation**: Focus transport migration on: (1) `McpAdapter` in `router.rs` (~40 lines, fully isolated), (2) `run_session` in `uds/mcp_listener.rs` (~40 lines), (3) `tokio_main_stdio` in `main.rs` (~20 lines). Auth, TLS, listener, and health handler require zero changes. Extension propagation must be re-validated after migration.

---

## Unanswered Questions

None. Both assigned questions are fully answered.

---

## Out-of-Scope Discoveries

1. **`transport-async-rw` feature dependency is implicit**: UDS transport relies on a blanket `IntoTransport` impl from `transport-async-rw`, transitively enabled by `server`. Not explicitly in Cargo.toml. If rmcp 1.x changes which features enable this, UDS transport silently breaks at compile time.

2. **Test infrastructure uses `rmcp::serve_client` and client-side types**: `run_initialize_handshake` test helper uses `ClientInfo`, `ClientCapabilities`, `ProtocolVersion::LATEST`, `rmcp::serve_client`, and `ServiceExt` from the `client` feature. Lower priority than production code.

3. **`ErrorData::invalid_params` convenience method**: Used at 6 call sites. May be unique to rmcp's ErrorData. Worth checking if it exists in rmcp 1.x.
