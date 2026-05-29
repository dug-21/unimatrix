# vnc-021: HTTPS Transport + Static Bearer Token Auth -- Architecture

## System Overview

vnc-021 adds a third transport surface (HTTPS) to the Unimatrix MCP server alongside existing UDS and stdio transports. The HTTP listener runs in the same tokio runtime as the daemon, sharing the same `UnimatrixServer` via `Clone` (ADR-003, Unimatrix #1913). Auth middleware intercepts requests before they reach rmcp's `StreamableHttpService`. This is Wave 2 critical path item W2-2 (GH #658).

The feature introduces six new modules under `src/http/` and extends three existing modules (`infra/config.rs`, `infra/shutdown.rs`, `main.rs`). No existing transport paths are modified -- HTTP is purely additive.

```
                          +-----------+
                          |  main.rs  |
                          +-----+-----+
                                |
              +-----------------+------------------+
              |                 |                   |
         stdio transport   UDS transport      HTTP transport (NEW)
         (rmcp::io)       (uds/mcp_listener)  (http/listener.rs)
              |                 |                   |
              v                 v                   v
              +-------- UnimatrixServer (Clone) ----+
              |                                      |
              +--- ServiceLayer (Arc-shared) --------+
              |                                      |
              +--- Store / VectorIndex / etc. -------+
```

## Component Breakdown

### C1: HTTP Listener (`src/http/listener.rs`)

Responsibility: TCP bind, TLS accept loop, connection limiting, spawning per-connection tasks.

- Binds `TcpListener` on configured address/port (default `0.0.0.0:8443`)
- Wraps accepted connections in `tokio_rustls::TlsAcceptor` when TLS enabled
- Enforces `max_concurrent_sessions` via `Arc<Semaphore>` pre-TLS (SR-08 mitigation)
- Spawns per-connection tasks using `hyper::server::conn::http1::Builder` (or http2 if rmcp requires it)
- Passes connections through the tower service stack
- Returns `JoinHandle` and bind address for `LifecycleHandles`

### C2: StaticTokenAuth Middleware (`src/http/auth.rs`)

Responsibility: Bearer token extraction, constant-time validation, identity injection.

- Tower `Layer` + `Service` implementation
- Extracts `Authorization: Bearer <hex>` header
- Validates via `subtle::ConstantTimeEq` against in-memory token (ADR-001)
- On success: inserts `ResolvedIdentity` into request extensions
- On failure: returns HTTP 401 with JSON error body
- Bypasses auth for `/health` path (ADR-002)
- Sets `credential_type = "static_token"` for downstream audit

### C3: Path-Dispatching Service (`src/http/router.rs`)

Responsibility: Route requests by path to the correct handler.

- Tower `Service` implementation dispatching on `Request<Body>` URI path
- Routes:
  - `GET /health` -> `health_handler` (no auth)
  - `POST /observe` -> 501 stub (auth required)
  - `/* (everything else)` -> `StreamableHttpService<UnimatrixServer>` (auth required)
- Contains `ProjectRouter` struct (single-project default for W2-6 seam)

### C4: Token Manager (`src/http/token.rs`)

Responsibility: Token file lifecycle -- generate, load, validate format.

- Generates 32-byte token via `rand::rngs::OsRng` on first run
- Writes hex-encoded (64 chars) to `{data_volume}/token` with mode 0600
- Prints token to stdout once with `[UNIMATRIX TOKEN]` label on generation
- Loads silently from existing file on subsequent starts
- Validates format: exactly 64 hex characters, no trailing newline

### C5: TLS Configuration (`src/http/tls.rs`)

Responsibility: rustls `ServerConfig` construction from PEM files.

- Loads certificate chain from `tls.cert_path` via `rustls_pemfile`
- Loads private key from `tls.key_path` via `rustls_pemfile`
- Constructs `rustls::ServerConfig` with safe defaults (no client auth)
- Returns `tokio_rustls::TlsAcceptor`
- Validates cert/key at startup -- refuses to start on invalid files when `tls.enabled = true`

### C6: Health Handler (`src/http/health.rs`)

Responsibility: Unauthenticated HTTP health endpoint.

- Returns JSON `{"version": "<semver>", "schema_version": <int>}` for `GET /health`
- No MCP framing, no auth required
- Distinct from CLI `health` subcommand (UDS probe) -- see SR-10 clarification

### C7: Config Extensions (`src/infra/config.rs` -- modified)

Responsibility: Parse `[http]` and `[tls]` config sections.

- `HttpConfig`: `enabled` (bool, default false), `content_port` (u16, default 8443), `bind_address` (String, default "0.0.0.0"), `max_concurrent_sessions` (usize, default 32), `max_request_body_bytes` (usize, default 1MB), `connection_timeout_secs` (u64, default 30)
- `TlsConfig`: `enabled` (bool, default determined by cert/key presence), `cert_path` (Option<PathBuf>), `key_path` (Option<PathBuf>)

### C8: Lifecycle Integration (`src/infra/shutdown.rs` -- modified)

Responsibility: HTTP listener graceful shutdown coordination.

- New fields on `LifecycleHandles`: `http_acceptor_handle`, `http_listener_addr`
- HTTP acceptor aborted in shutdown sequence between MCP acceptor (Step 0) and hook IPC (Step 0b)
- Connection semaphore prevents new connections during drain

## Component Interactions

### Request Flow: Authenticated MCP Tool Call

```
Client -> TcpListener (C1)
       -> TlsAcceptor (C5, when enabled)
       -> Semaphore acquire (C1, connection limit)
       -> hyper HTTP/1.1 parse
       -> StaticTokenAuth (C2)
          - extract Authorization header
          - constant-time compare token
          - insert ResolvedIdentity into extensions
       -> PathRouter (C3)
          - match /* -> StreamableHttpService
       -> rmcp StreamableHttpService
          - MCP protocol framing (initialize, tool call)
          - server.build_context_with_external_identity(external_identity: Some(&resolved))
          - tool dispatch via ToolRouter
       -> ServiceLayer (existing)
          - capability check (require_cap)
          - business logic
          - audit log with credential_type="static_token"
```

### Request Flow: Health Check

```
Client -> TcpListener (C1)
       -> TlsAcceptor (C5, when enabled)
       -> StaticTokenAuth (C2)
          - path == "/health" -> bypass auth
       -> PathRouter (C3)
          - match GET /health -> health_handler (C6)
       -> JSON response (no MCP framing)
```

### Startup Wiring (main.rs)

```
tokio_main_daemon():
  1. Load config (existing)
  2. Parse HttpConfig + TlsConfig from config (C7)
  3. Load or generate token (C4)
  4. Build UnimatrixServer (existing)
  5. Start UDS listeners (existing)
  6. If http.enabled:
     a. Build TlsAcceptor if tls.enabled (C5)
     b. Build StreamableHttpService wrapping server.clone() (rmcp)
     c. Build PathRouter wrapping StreamableHttpService (C3)
     d. Build StaticTokenAuth layer wrapping PathRouter (C2)
     e. Start HTTP listener (C1) -> returns JoinHandle
     f. Store handle in LifecycleHandles (C8)
  7. Start background tick (existing)
  8. Wait for daemon token (existing)
  9. Graceful shutdown including HTTP (C8)
```

### Identity Injection Path

The `build_context_with_external_identity` seam (server.rs:388) is the single integration point. The HTTP auth middleware constructs a `ResolvedIdentity` from the bearer token and inserts it into request extensions. The rmcp `StreamableHttpService` propagates extensions through to `RequestContext`. The server's `build_context_with_external_identity` reads the external identity from extensions.

```
StaticTokenAuth (C2):
  ResolvedIdentity {
    agent_id: "http-bearer",        // static for all bearer-token callers
    trust_level: TrustLevel::Standard,
    capabilities: [Read, Write, Search],
  }
  -> inserted into http::Extensions

StreamableHttpService:
  -> rmcp propagates Extensions into RequestContext.extensions

build_context_with_external_identity:
  external_identity: Some(&resolved_from_extensions)
  -> bypasses resolve_agent
  -> ToolContext.audit_ctx.source = AuditSource::Mcp { agent_id: "http-bearer", ... }
  -> credential_type = "static_token" (distinct from "none" for UDS/stdio)
```

### CallerId Extension

The `CallerId` enum (services/mod.rs) gains a new variant for HTTP transport:

```rust
pub(crate) enum CallerId {
    Agent(String),       // MCP (stdio/UDS)
    UdsSession(String),  // Hook IPC
    HttpBearer(String),  // HTTP bearer-token caller (NEW)
}
```

`HttpBearer` is NOT exempt from rate limiting (unlike `UdsSession`). The rate limiter's exhaustive match ensures this is enforced at compile time.

## Technology Decisions

| Decision | Choice | ADR | Unimatrix |
|----------|--------|-----|-----------|
| Token validation algorithm | `subtle::ConstantTimeEq` | ADR-001 | #4665 |
| Auth bypass for /health | Path-match before auth layer | ADR-002 | #4666 |
| rmcp adapter boundary | Thin adapter isolating StreamableHttpService | ADR-003 | #4667 |
| Connection limiting strategy | Pre-TLS semaphore | ADR-004 | #4668 |
| TLS termination | rustls with configurable bypass | ADR-005 | #4669 |
| HTTP credential_type value | "static_token" string literal | ADR-006 | #4670 |

## Integration Points

### Existing Components Modified

| Component | File | Change |
|-----------|------|--------|
| Config | `infra/config.rs` | Add `HttpConfig`, `TlsConfig` sections to `UnimatrixConfig` |
| Shutdown | `infra/shutdown.rs` | Add `http_acceptor_handle` to `LifecycleHandles` |
| Main | `main.rs` | HTTP listener startup in `tokio_main_daemon` |
| Lib | `lib.rs` | Add `pub mod http;` |
| CallerId | `services/mod.rs` | Add `HttpBearer(String)` variant |
| Cargo.toml | `Cargo.toml` | New rmcp features, new direct deps |

### New Dependencies (Cargo.toml)

```toml
# rmcp features -- add to existing rmcp line
rmcp = { version = "=0.16.0", features = [
    "server", "client", "transport-io", "macros",
    "transport-streamable-http-server",
    "transport-streamable-http-server-session",
] }

# New direct deps (all already transitive in lockfile -- Unimatrix #4661)
tokio-rustls = "0.26"
rustls-pemfile = "2"
subtle = "2"
tower = { version = "0.5", features = ["util"] }
hyper = { version = "1", features = ["http1", "server"] }
hyper-util = { version = "0.1", features = ["tokio", "http1"] }
```

### Existing Components NOT Modified

- `server.rs` -- no changes; `build_context_with_external_identity` already accepts `external_identity: Option<&ResolvedIdentity>`
- `mcp/tools.rs` -- no changes; tool handlers are transport-agnostic
- `services/*` -- no changes; service layer is transport-agnostic (by design, vnc-006)
- `uds/*` -- no changes; UDS transport is unaffected

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `UnimatrixServer::build_context_with_external_identity` | `async fn(&self, &Option<String>, &Option<String>, &Option<String>, &RequestContext<RoleServer>, Option<&ResolvedIdentity>) -> Result<ToolContext, ErrorData>` | `server.rs:388` |
| `ResolvedIdentity` | `struct { agent_id: String, trust_level: TrustLevel, capabilities: Vec<Capability> }` | `mcp/identity.rs:9` |
| `CallerId` | `enum { Agent(String), UdsSession(String), HttpBearer(String) }` | `services/mod.rs:72` |
| `LifecycleHandles` | `struct { ..., http_acceptor_handle: Option<JoinHandle<()>> }` | `infra/shutdown.rs:42` |
| `UnimatrixConfig` | `struct { ..., http: HttpConfig, tls: TlsConfig }` | `infra/config.rs` |
| `HttpConfig` | `struct { enabled: bool, content_port: u16, bind_address: String, max_concurrent_sessions: usize, max_request_body_bytes: usize, connection_timeout_secs: u64 }` | `infra/config.rs` (new) |
| `TlsConfig` | `struct { enabled: bool, cert_path: Option<PathBuf>, key_path: Option<PathBuf> }` | `infra/config.rs` (new) |
| `start_http_listener` | `async fn(config: &HttpConfig, tls: Option<TlsAcceptor>, service: S, shutdown: CancellationToken) -> Result<(JoinHandle<()>, SocketAddr), ServerError>` | `http/listener.rs` (new) |
| `StaticTokenAuth<S>` | `tower::Layer` + `tower::Service` wrapping inner `S` | `http/auth.rs` (new) |
| `PathRouter` | `tower::Service<Request<Body>>` dispatching by URI path | `http/router.rs` (new) |
| `ProjectRouter` | `struct { default_server: UnimatrixServer }` (single-project default) | `http/router.rs` (new) |
| `load_or_generate_token` | `fn(data_dir: &Path) -> Result<Vec<u8>, ServerError>` | `http/token.rs` (new) |
| `build_tls_acceptor` | `fn(config: &TlsConfig) -> Result<TlsAcceptor, ServerError>` | `http/tls.rs` (new) |

## Module Decomposition

All new HTTP modules live under `src/http/`:

```
src/http/
  mod.rs          -- module declarations, re-exports
  listener.rs     -- TCP bind, TLS accept, connection limiting (~120 lines)
  auth.rs         -- StaticTokenAuth layer + service (~150 lines)
  router.rs       -- PathRouter + ProjectRouter + path dispatch (~130 lines)
  token.rs        -- token file I/O + generation (~80 lines)
  tls.rs          -- rustls ServerConfig from PEM (~70 lines)
  health.rs       -- /health JSON handler (~40 lines)
```

All files stay well under the 500-line workspace limit.

## Open Questions

1. **rmcp extension propagation (SR-02)**: The architecture assumes `StreamableHttpService` propagates `http::Extensions` through to `RequestContext.extensions` for identity injection. If rmcp drops extensions internally, the adapter boundary (ADR-003) allows a fallback: store the identity in a task-local or pass it via a side channel. This must be validated during implementation with a spike test before full build-out.

2. **hyper HTTP version**: rmcp's `StreamableHttpService` may require HTTP/1.1 for SSE streaming. The listener should start with HTTP/1.1 only and add HTTP/2 support if rmcp supports it. This is an implementation-time discovery.
