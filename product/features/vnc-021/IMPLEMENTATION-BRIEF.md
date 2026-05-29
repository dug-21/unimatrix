# vnc-021: HTTPS Transport + Static Bearer Token Auth — Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/vnc-021/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-021/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-021/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-021/specification/SPECIFICATION.md |
| Risk & Test Strategy | product/features/vnc-021/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-021/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| http-listener | pseudocode/http-listener.md | test-plan/http-listener.md |
| static-token-auth | pseudocode/static-token-auth.md | test-plan/static-token-auth.md |
| path-router | pseudocode/path-router.md | test-plan/path-router.md |
| token-manager | pseudocode/token-manager.md | test-plan/token-manager.md |
| tls-config | pseudocode/tls-config.md | test-plan/tls-config.md |
| health-handler | pseudocode/health-handler.md | test-plan/health-handler.md |
| config-extensions | pseudocode/config-extensions.md | test-plan/config-extensions.md |
| lifecycle-integration | pseudocode/lifecycle-integration.md | test-plan/lifecycle-integration.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Add HTTPS transport with static bearer token authentication to the Unimatrix MCP server, enabling network-accessible MCP connections from Claude Code, Codex CLI, and Gemini CLI. This unblocks the "personal cloud" deployment model (Wave 2 critical path W2-2, GH #658) where a containerized Unimatrix instance on a VPS is reachable by any MCP-compatible client over the network. The feature introduces path-dispatching tower middleware, a ProjectRouter structural seam for W2-6, and a `/health` endpoint for container monitoring — all alongside existing UDS/stdio transports with zero regression.

## Human Review Notes

These notes from the human review MUST be incorporated during implementation:

1. **R-01 spike test first**: rmcp extension propagation (R-01) is the single highest-risk integration point. A spike test validating that `http::Extensions` survive `StreamableHttpService` processing MUST happen FIRST before building the full auth chain. If rmcp drops extensions, ADR-003 adapter fallback activates.
2. **Startup log for disabled HTTP**: When HTTP is available but not enabled (`[http] enabled = false`), emit a startup log line: `"HTTP transport available — set [http] enabled = true in config.toml"`. This reduces support friction.
3. **Observability deliberately de-scoped**: Prometheus metrics and structured logging are NOT part of vnc-021. The path-dispatching router makes a future `/metrics` endpoint trivial to add.
4. **BearerValidator trait accepted**: FR-14's `BearerValidator` trait is architecturally necessary as the W2-3 bridge to enterprise JWT/OAuth validators.
5. **All clients use curl-based shell hooks**: No native HTTP hook handler. All hook examples use `curl` commands.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Token validation algorithm | `subtle::ConstantTimeEq` for constant-time comparison; early-return only on missing header or non-Bearer prefix | ADR-001, Unimatrix #4665 | architecture/ADR-001-constant-time-token-validation.md |
| Health endpoint auth bypass | Path-match in auth middleware; exact match on `/health` + GET method; compile-time constant bypass list | ADR-002, Unimatrix #4666 | architecture/ADR-002-health-endpoint-auth-bypass.md |
| rmcp adapter boundary | Thin adapter in `router.rs` isolating `StreamableHttpService`; copies extensions if rmcp drops them; enforces body size pre-rmcp | ADR-003, Unimatrix #4667 | architecture/ADR-003-rmcp-adapter-boundary.md |
| Connection limiting strategy | Pre-TLS `Arc<Semaphore>` acquired immediately after `TcpListener::accept()`; rejected connections get TCP RST | ADR-004, Unimatrix #4668 | architecture/ADR-004-pre-tls-connection-limiting.md |
| TLS termination | `rustls` 0.23 via `tokio-rustls` 0.26; configurable bypass via `[tls] enabled`; auto-detect from cert/key presence | ADR-005, Unimatrix #4669 | architecture/ADR-005-tls-termination-with-bypass.md |
| HTTP credential_type value | `"static_token"` string literal; constant `CREDENTIAL_TYPE_STATIC_TOKEN` in `http/auth.rs` | ADR-006, Unimatrix #4670 | architecture/ADR-006-credential-type-static-token.md |

## Files to Create

| Path | Purpose |
|------|---------|
| `src/http/mod.rs` | Module declarations and re-exports for HTTP transport |
| `src/http/listener.rs` | TCP bind, TLS accept loop, connection limiting, per-connection task spawning (~120 lines) |
| `src/http/auth.rs` | `StaticTokenAuth` tower Layer/Service, `BearerValidator` trait, constant-time validation (~150 lines) |
| `src/http/router.rs` | `PathRouter` tower Service, `ProjectRouter` struct, path dispatch, rmcp adapter (~130 lines) |
| `src/http/token.rs` | Token file generate/load/validate lifecycle (~80 lines) |
| `src/http/tls.rs` | `rustls` ServerConfig construction from PEM files (~70 lines) |
| `src/http/health.rs` | `/health` JSON handler returning version + schema_version (~40 lines) |
| `docs/client-setup.md` | Client setup documentation for Claude Code, Codex CLI, Gemini CLI |

## Files to Modify

| Path | Change |
|------|--------|
| `src/lib.rs` | Add `pub mod http;` |
| `src/infra/config.rs` | Add `HttpConfig` and `TlsConfig` structs to `UnimatrixConfig` |
| `src/infra/shutdown.rs` | Add `http_acceptor_handle` and `http_listener_addr` to `LifecycleHandles` |
| `src/main.rs` | HTTP listener startup wiring in `tokio_main_daemon` and `tokio_main_foreground` |
| `src/services/mod.rs` | Add `CallerId::HttpBearer(String)` variant |
| `Cargo.toml` | Add rmcp HTTP features + new direct dependencies |

## Data Structures

### New Structs

```rust
// src/infra/config.rs
pub struct HttpConfig {
    pub enabled: bool,                    // default: false
    pub content_port: u16,                // default: 8443
    pub bind_address: String,             // default: "0.0.0.0"
    pub max_concurrent_sessions: usize,   // default: 32
    pub max_request_body_bytes: usize,    // default: 1_048_576 (1 MB)
    pub connection_timeout_secs: u64,     // default: 30
}

pub struct TlsConfig {
    pub enabled: bool,           // default: true when both cert_path and key_path present
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
}

// src/http/router.rs
struct ProjectRouter {
    stores: HashMap<String, Arc<ProjectContext>>,
    default_project: Option<String>,
}

// src/http/auth.rs
pub(crate) const CREDENTIAL_TYPE_STATIC_TOKEN: &str = "static_token";
```

### Modified Enums

```rust
// src/services/mod.rs — add variant
pub(crate) enum CallerId {
    Agent(String),
    UdsSession(String),
    HttpBearer(String),   // NEW — NOT exempt from rate limiting
}
```

### Modified Structs

```rust
// src/infra/shutdown.rs — add fields
pub struct LifecycleHandles {
    // ... existing fields ...
    pub http_acceptor_handle: Option<JoinHandle<()>>,
    pub http_listener_addr: Option<SocketAddr>,
}
```

## Function Signatures

### New Public/Crate Functions

```rust
// src/http/listener.rs
pub(crate) async fn start_http_listener(
    config: &HttpConfig,
    tls: Option<TlsAcceptor>,
    service: S,
    shutdown: CancellationToken,
) -> Result<(JoinHandle<()>, SocketAddr), ServerError>;

// src/http/auth.rs
pub(crate) trait BearerValidator: Send + Sync + 'static {
    async fn validate(&self, token: &str) -> Result<ResolvedIdentity, AuthError>;
}

// src/http/token.rs
pub(crate) fn load_or_generate_token(data_dir: &Path) -> Result<Vec<u8>, ServerError>;

// src/http/tls.rs
pub(crate) fn build_tls_acceptor(config: &TlsConfig) -> Result<TlsAcceptor, ServerError>;
```

### Existing Functions Used (Not Modified)

```rust
// src/server.rs:388
pub(crate) async fn build_context_with_external_identity(
    &self,
    tool_name: &Option<String>,
    uri: &Option<String>,
    description: &Option<String>,
    ctx: &RequestContext<RoleServer>,
    external_identity: Option<&ResolvedIdentity>,
) -> Result<ToolContext, ErrorData>;
```

## Constraints

1. **rmcp =0.16.0 pinned** — HTTP features must work with this exact version; no upgrade without workspace-wide validation
2. **No axum** — rmcp 0.16 HTTP transport is tower-native; use hyper + tower directly
3. **`#![forbid(unsafe_code)]`** — all dependencies must be safe Rust or already audited
4. **Max 500 lines per file** — HTTP modules decomposed under `src/http/`
5. **Test infrastructure is cumulative** — extend existing `test_support.rs` and `make_server()` fixture
6. **Audit log schema is immutable** — write to existing vnc-014 schema v25 columns; no new migration
7. **Single binary** — HTTP transport compiled into the same `unimatrix` binary
8. **`build_context_with_external_identity` seam** — use existing identity injection point; no parallel path
9. **Token file never in Docker image layers** — generated at runtime in data volume
10. **Claude Code bug #28293** — client docs must use `claude mcp add -H` as primary path
11. **POSIX-only** — curl-based shell hooks assume POSIX shell; Windows/WSL excluded from hook docs
12. **Adapter boundary** — rmcp `StreamableHttpService` isolated behind thin adapter (ADR-003) for workaround seam
13. **Extension propagation validation** — must spike-test that `ResolvedIdentity` in extensions survives rmcp processing BEFORE full build-out

## Dependencies

### New Direct Dependencies (Cargo.toml)

| Crate | Version | Purpose |
|-------|---------|---------|
| `tokio-rustls` | `0.26` | Async TLS acceptor |
| `rustls-pemfile` | `2` | PEM certificate/key loading |
| `subtle` | `2` | Constant-time comparison (`ConstantTimeEq`) |
| `tower` | `0.5` (features: `["util"]`) | Middleware composition |
| `hyper` | `1` (features: `["http1", "server"]`) | TCP listener, HTTP handling |
| `hyper-util` | `0.1` (features: `["tokio", "http1"]`) | Hyper service utilities |

All are already transitive dependencies in the lockfile. Version pins must match existing transitives.

### rmcp Feature Flags (add to existing rmcp line)

- `transport-streamable-http-server`
- `transport-streamable-http-server-session`

### Existing Components Used

| Component | Location | Usage |
|-----------|----------|-------|
| `UnimatrixServer` | `server.rs` | Cloned into HTTP sessions |
| `build_context_with_external_identity` | `server.rs:388` | Identity injection for bearer-auth callers |
| `ResolvedIdentity` | `mcp/identity.rs` | Produced by auth middleware |
| `ToolContext` | `mcp/context.rs` | Pre-validated handler context |
| `AuditContext` / `AuditEvent` | `services/` | Audit logging with `credential_type` field |
| `client_type_map` | `server.rs` | Maps session key to `clientInfo.name` |
| `UnimatrixConfig` | `infra/config.rs` | Extended with `HttpConfig`, `TlsConfig` |
| `LifecycleHandles` | `infra/shutdown.rs` | Extended with HTTP acceptor fields |
| `CancellationToken` | `main.rs` | Shared shutdown signal |

## NOT in Scope

- Admin port activation (8444 reserved, not active)
- OAuth 2.1 / JWT validation (enterprise private repo; `BearerValidator` trait enables additive extension)
- Multi-project routing activation (W2-6; ProjectRouter ships in single-project default mode only)
- Per-client behavioral testing (W2-4)
- Token rotation mechanism (operational procedure: stop, delete token file, restart)
- Hot-reload of TLS certificates
- Prometheus metrics / structured logging (deliberately de-scoped; path-dispatching router enables future `/metrics`)
- `/observe` handler implementation (501 stub only; W2-7 ships actual handler)
- Native HTTP hook handler (all hooks use curl-based shell scripts)
- Windows/WSL hook documentation
- Per-endpoint rate limiting
- HTTP in stdio mode

## Alignment Status

All vision alignment checks PASS. No variances requiring human approval.

Three variances were resolved during design:

1. **Observability de-scoped** — WAVE2-ROADMAP.md W2-2 title updated to "HTTPS Transport + Static Token Auth". Observability may ship as a separate feature. RESOLVED.
2. **BearerValidator trait accepted** — FR-14 (~10 lines) is architecturally necessary as the W2-3 bridge to enterprise JWT/OAuth validators. RESOLVED.
3. **ASS-060 added to research index** — ProjectRouter path-prefix design source was untraced in roadmap. Documentation gap fixed. RESOLVED.

## Critical Implementation Ordering

Per human review and risk analysis, implementation MUST follow this order:

1. **Spike: rmcp extension propagation** (R-01) — validate `http::Extensions` survive `StreamableHttpService` processing. If they do not, activate ADR-003 adapter fallback before proceeding.
2. **Token manager** (C4) — generate/load token file, validate format
3. **Config extensions** (C7) — `HttpConfig`, `TlsConfig` parsing with defaults
4. **StaticTokenAuth middleware** (C2) — `BearerValidator` trait, tower Layer/Service, constant-time validation
5. **TLS configuration** (C5) — `rustls` ServerConfig from PEM, `TlsAcceptor`
6. **Health handler** (C6) — unauthenticated `/health` endpoint
7. **Path-dispatching router** (C3) — `PathRouter`, `ProjectRouter`, `/observe` stub, rmcp adapter
8. **HTTP listener** (C1) — TCP bind, TLS accept, connection limiting, per-connection tasks
9. **Lifecycle integration** (C8) — `LifecycleHandles` extension, graceful shutdown wiring
10. **Main wiring** — startup logic in `tokio_main_daemon` / `tokio_main_foreground`
11. **Client documentation** — Claude Code (`-H` workaround), Codex CLI, Gemini CLI
