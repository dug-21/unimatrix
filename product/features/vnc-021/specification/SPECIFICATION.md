# vnc-021: HTTPS Transport + Static Bearer Token Auth — Specification

## Objective

Add HTTPS transport with static bearer token authentication to the Unimatrix MCP server, enabling network-accessible MCP connections from Claude Code, Codex CLI, and Gemini CLI. This unblocks the "personal cloud" deployment model where a developer runs a containerized Unimatrix instance on a VPS and connects from multiple machines. The feature introduces path-dispatching tower middleware, a ProjectRouter structural seam for W2-6, and a `/health` endpoint for container monitoring — all alongside the existing UDS/stdio transports with zero regression.

GitHub Issue: #658

---

## Functional Requirements

### Transport

- **FR-01**: The server starts an HTTPS listener on the configured content port (default 8443) when `[http] enabled = true` in config.toml. The listener runs in the same tokio runtime as UDS/stdio transports.
- **FR-02**: The HTTPS listener uses `hyper` for TCP accept and `tokio-rustls` for TLS termination. No axum dependency. rmcp 0.16 `StreamableHttpService<UnimatrixServer>` handles MCP protocol framing.
- **FR-03**: TLS termination is active when `[tls] cert_path` and `key_path` are configured and valid PEM files. `rustls` loads certificates and private key at startup.
- **FR-04**: When `tls.enabled = false`, the listener binds plain HTTP (no TLS) for proxy-terminated deployments.
- **FR-05**: The server refuses to start with a clear error message when `tls.enabled = true` but `cert_path` or `key_path` is missing, unreadable, or contains invalid PEM data.
- **FR-06**: The HTTP listener activates in `serve --foreground` (container) and `serve --daemon` modes alongside UDS. It does NOT activate in stdio mode.
- **FR-07**: Graceful shutdown of the HTTP listener uses the same `CancellationToken` pattern as UDS. `LifecycleHandles` gains HTTP acceptor fields. In-flight HTTP sessions drain before process exit.

### Authentication

- **FR-08**: On first run, when no token file exists at `{data_volume}/token`, the server generates a 32-byte (256-bit) cryptographically random token via `OsRng`, hex-encodes it (64 characters), writes it to the token file with filesystem permissions `0600`, and prints it to stdout once with the label `[UNIMATRIX TOKEN]`.
- **FR-09**: On subsequent starts, the server loads the token file silently. No stdout output for the token on restart.
- **FR-10**: HTTP requests without an `Authorization: Bearer <token>` header receive HTTP 401 Unauthorized. The response body is JSON: `{"error": "missing or invalid authorization"}`.
- **FR-11**: HTTP requests with an incorrect bearer token receive HTTP 401 Unauthorized. Token comparison uses `subtle::ConstantTimeEq` to prevent timing side-channel attacks. The response format and latency are identical to the missing-header case.
- **FR-12**: HTTP requests with a valid bearer token pass through to MCP tool dispatch. Tool execution produces identical results to UDS/stdio paths — same service layer, same capability checks, same audit logging.
- **FR-13**: The `StaticTokenAuth` middleware is implemented as a tower `Layer`/`Service`. It reads the `Authorization` header, performs constant-time comparison, and on success inserts a `ResolvedIdentity` into the request extensions for downstream consumption.
- **FR-14**: The `StaticTokenAuth` middleware implements the `BearerValidator` trait, which is defined as a trait to allow enterprise extension (JWT/OAuth validators). The trait has a single method: `async fn validate(&self, token: &str) -> Result<ResolvedIdentity, AuthError>`.

### Audit Integration

- **FR-15**: For HTTP-authenticated requests, `credential_type` in `audit_log` is set to `"static_token"`. UDS/stdio paths continue to write `"none"`.
- **FR-16**: `agent_attribution` in `audit_log` is populated from `clientInfo.name` (the MCP initialize handshake `client_info` field) for HTTP sessions, using the existing `client_type_map` mechanism from vnc-014.
- **FR-17**: HTTP-authenticated identity flows through the existing `build_context_with_external_identity` seam (server.rs) via the `external_identity: Option<&ResolvedIdentity>` parameter. No parallel identity resolution path is created.

### Path-Dispatching

- **FR-18**: The HTTP listener uses a path-dispatching tower service that routes by request path before reaching `StreamableHttpService`. This is not a direct forward — it is an explicit routing layer.
- **FR-19**: `GET /health` returns HTTP 200 with JSON body `{"version": "<semver>", "schema_version": <int>}`. No authentication required. The version is the crate version; the schema version is the current database migration version.
- **FR-20**: `POST /observe` returns HTTP 501 Not Implemented with JSON body `{"error": "Remote telemetry not yet implemented. See W2-7."}`. Bearer token authentication is required (same middleware as MCP path). This is a static stub — zero handler logic beyond the fixed response.
- **FR-21**: All other paths (`/*`) route to `StreamableHttpService<UnimatrixServer>` for MCP protocol handling.

### ProjectRouter

- **FR-22**: A `ProjectRouter` struct exists and wraps the `UnimatrixServer`. Even in single-project mode, all HTTP requests flow through `ProjectRouter`. The struct holds a map of project slug to project context and a `default_project` field for single-project fallback.
- **FR-23**: In vnc-021, `ProjectRouter` operates exclusively in single-project default mode. No `[[projects]]` config is needed. No slug prefix in URLs. W2-6 activates multi-project routing without restructuring.
- **FR-24**: `/observe` is registered in the `ProjectRouter` routing tree so W2-6 multi-project path-prefix routing includes it from day one.

### Configuration

- **FR-25**: New `[http]` config section: `enabled` (bool, default `false`), `content_port` (u16, default `8443`), `bind_address` (string, default `"0.0.0.0"`). Port `0` is supported for OS-assigned port (testing convenience).
- **FR-26**: New `[tls]` config section: `enabled` (bool, see default rule below), `cert_path` (PathBuf, optional), `key_path` (PathBuf, optional). Default for `tls.enabled`: `true` when both `cert_path` and `key_path` are present in config, `false` otherwise.
- **FR-27**: Config sections follow existing two-level hierarchy (global + per-project, replace semantics).

### Client Documentation

- **FR-28**: Client setup documentation covers Claude Code, Codex CLI, and Gemini CLI. Each client section documents: MCP connection URL, bearer token configuration, and curl-based shell hook configuration for remote telemetry.
- **FR-29**: All three clients use curl-based shell hooks for remote telemetry. No local `unimatrix` binary is required on the client machine. No native HTTP hook handler is implemented.
- **FR-30**: Claude Code documentation specifies the `claude mcp add -H` workaround for anthropics/claude-code#28293 (headers in `.mcp.json` not forwarded on tool call POSTs).

---

## Non-Functional Requirements

- **NFR-01**: Maximum HTTP request body size: 1 MB. Requests exceeding this limit are rejected with HTTP 413 before reaching MCP dispatch.
- **NFR-02**: HTTP connection timeout: 30 seconds of inactivity.
- **NFR-03**: Maximum concurrent HTTP sessions: configurable via `[http] max_connections` (u32, default 32). Enforced at the connection accept level before TLS handshake completes, preventing slow-TLS resource exhaustion (SR-08).
- **NFR-04**: Existing UDS/stdio transport paths have zero behavioral change. All existing tests pass without modification.
- **NFR-05**: The HTTP transport shares the tokio runtime with UDS, background ticks, NLI inference, and the write queue. Connection limits (NFR-03) prevent HTTP load from starving background tasks.
- **NFR-06**: `#![forbid(unsafe_code)]` remains enforced. All new and transitive dependencies use safe Rust or are already audited.
- **NFR-07**: Maximum 500 lines per file. HTTP transport modules decomposed into focused files under `src/http/`.
- **NFR-08**: Token file is generated at runtime in the data volume. It is never baked into Docker image layers.
- **NFR-09**: All new direct dependencies (`tokio-rustls`, `rustls-pemfile`, `subtle`, `tower`, `hyper`, `hyper-util`) are already present as transitive dependencies in the lockfile. Version pins match existing transitive versions to prevent conflicts (SR-04).
- **NFR-10**: rmcp `=0.16.0` pin is preserved. HTTP features (`transport-streamable-http-server`, `transport-streamable-http-server-session`) must work with this exact version.

---

## Acceptance Criteria

### Transport + Auth

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | Server starts an HTTPS listener on the configured content port (default 8443) when `[http] enabled = true` | Integration test: start server with HTTP enabled, verify TCP connection accepted on configured port |
| AC-02 | Bearer token file generated at `{data_volume}/token` on first run (32 bytes, hex-encoded, mode 0600); printed to stdout once with `[UNIMATRIX TOKEN]` label | Integration test: start with no token file, verify file created with correct length (64 hex chars), permissions (0600), and stdout contains `[UNIMATRIX TOKEN]` |
| AC-03 | Bearer token loaded silently from existing token file on subsequent starts | Integration test: create token file, start server, verify no token output on stdout; verify server accepts the pre-existing token |
| AC-04 | HTTP requests without `Authorization: Bearer <token>` header receive HTTP 401 | Integration test: send request without auth header, assert HTTP 401 response with JSON error body |
| AC-05 | HTTP requests with incorrect bearer token receive HTTP 401; comparison uses `subtle::ConstantTimeEq` | Integration test: send request with wrong token, assert HTTP 401. Code review: verify `subtle::ConstantTimeEq` usage in token comparison path |
| AC-06 | HTTP requests with valid bearer token reach MCP tool dispatch; tools execute identically to UDS/stdio path | Integration test: send authenticated MCP tool call (e.g., `context_status`) via HTTP, verify successful JSON-RPC response with valid result |
| AC-07 | `credential_type` in audit_log is `"static_token"` for HTTP-authenticated requests | Integration test: execute tool via HTTP, query audit_log, assert `credential_type = "static_token"` |
| AC-08 | `agent_attribution` in audit_log is populated from `clientInfo.name` (MCP initialize handshake) for HTTP sessions | Integration test: connect via HTTP with MCP initialize including `clientInfo.name`, execute tool, verify `agent_attribution` matches in audit_log |
| AC-09 | TLS termination via rustls when `[tls] cert_path` and `key_path` are configured | Integration test: start with self-signed cert+key, connect with TLS client, verify successful handshake and tool execution |
| AC-10 | `tls.enabled = false` binds plain HTTP (no TLS) for proxy-terminated deployments | Integration test: start with `tls.enabled = false`, connect with plain HTTP client, verify tool execution succeeds |
| AC-11 | Server refuses to start when `tls.enabled = true` but cert_path or key_path is missing or invalid | Unit test: attempt startup with `tls.enabled = true` and missing/invalid cert, assert error with descriptive message |

### Path-Dispatching + Structural Prep

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-12 | HTTP listener uses a path-dispatching tower service; routes: `/health`, `/observe` (stub), `/*` (MCP) | Integration test: send requests to each path, verify correct routing behavior (200, 501, MCP response respectively) |
| AC-13 | `GET /health` returns JSON `{"version": "<semver>", "schema_version": <int>}` without authentication | Integration test: GET /health without auth header, assert HTTP 200 with correct JSON schema; verify version matches crate version |
| AC-14 | `POST /observe` returns HTTP 501 with JSON body `{"error": "Remote telemetry not yet implemented. See W2-7."}`. Bearer token auth required | Integration test: POST /observe without auth returns 401; POST /observe with auth returns 501 with exact JSON body |
| AC-15 | `/observe` is registered in ProjectRouter routing tree for W2-6 multi-project path-prefix compatibility | Code review: verify `/observe` appears in ProjectRouter route registration, not as a separate pre-router intercept |

### Operational

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-16 | Existing UDS/stdio transport paths are unaffected (zero regression) | Full test suite passes with no modifications to existing tests |
| AC-17 | `serve --foreground` mode (container) activates HTTP listener alongside UDS | Integration test: start in foreground mode with HTTP enabled, verify both UDS and HTTP connections accepted |
| AC-18 | `serve --daemon` mode activates HTTP listener alongside UDS | Integration test: start in daemon mode with HTTP enabled, verify both UDS and HTTP connections accepted |
| AC-19 | ProjectRouter struct exists with single-project default; requests route through it | Code review + integration test: verify all HTTP requests pass through ProjectRouter; verify single-project mode requires no `[[projects]]` config |
| AC-20 | Config sections `[http]` and `[tls]` parsed from config.toml; `[http] enabled` defaults to `false`; `tls.enabled` defaults to `true` when both `cert_path` and `key_path` are present, `false` otherwise | Unit tests: parse config with various `[http]`/`[tls]` combinations, verify defaults |
| AC-21 | Maximum request body size enforced (1 MB); connection timeout enforced (30s) | Integration test: send >1MB body, assert HTTP 413; hold idle connection, verify disconnect after 30s |
| AC-22 | Maximum concurrent HTTP sessions enforced (configurable, default 32) | Integration test: open 32 connections, verify 33rd is rejected or queued; verify configurable via `[http] max_connections` |

### Client Setup Documentation

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-23 | Client setup docs for Claude Code, Codex CLI, and Gemini CLI covering MCP connection and hook configuration | Documentation review: all three clients have connection URL, bearer token, and hook setup sections |
| AC-24 | All three clients use curl-based shell hooks — no local `unimatrix` binary required, no native HTTP hook handler | Documentation review: hook examples use `curl` commands, no references to local binary or native handler |
| AC-25 | Claude Code docs specify `claude mcp add -H` workaround for #28293 | Documentation review: Claude Code section includes `-H` flag usage and references the upstream bug |

---

## Domain Models

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| **Content Port** | The TCP port (default 8443) serving MCP protocol, health, and observe endpoints. The only active port in vnc-021 (admin port 8444 is reserved but not activated). |
| **Bearer Token** | A 32-byte (256-bit) cryptographically random value, hex-encoded to 64 characters, used as the sole HTTP authentication credential. Stored in `{data_volume}/token`. One token per deployment. |
| **StaticTokenAuth** | Tower middleware that validates `Authorization: Bearer <token>` headers using constant-time comparison. Implements the `BearerValidator` trait. |
| **BearerValidator** | Trait defining the bearer token validation contract. `StaticTokenAuth` is the OSS implementation; enterprise provides JWT/OAuth implementations. |
| **ResolvedIdentity** | Struct (already exists in `mcp/identity.rs`) representing a validated agent identity with `agent_id`, `trust_level`, and `capabilities`. HTTP auth middleware produces this; `build_context_with_external_identity` consumes it. |
| **ProjectRouter** | Routing struct that maps requests to project contexts. In vnc-021, operates in single-project default mode. Structural seam for W2-6 multi-project activation. |
| **Path-Dispatching Service** | Tower service that routes HTTP requests by path (`/health`, `/observe`, `/*`) before they reach rmcp's `StreamableHttpService`. |
| **Token File** | The file at `{data_volume}/token` containing the hex-encoded bearer token. Generated at runtime, never baked into container images. Permissions: `0600`. |
| **Data Volume** | The `unimatrix-data` named volume (container) or `~/.unimatrix/{project-hash}/` (local). Contains databases, config, logs, and the token file. |
| **StreamableHttpService** | rmcp 0.16 type (`StreamableHttpService<S, M>`) that implements `tower_service::Service<Request<RequestBody>>`. Handles MCP JSON-RPC framing over HTTP/SSE. |

### Key Entities and Relationships

```
UnimatrixConfig
  ├── HttpConfig          (new: enabled, content_port, bind_address, max_connections)
  └── TlsConfig           (new: enabled, cert_path, key_path)

HTTP Listener Stack (tower composition):
  TcpListener
    → TlsAcceptor (optional, via tokio-rustls)
      → ConnectionLimiter (semaphore, max_connections)
        → StaticTokenAuth (implements BearerValidator)
          → PathDispatcher
            ├── /health   → HealthHandler (no auth bypass — StaticTokenAuth skips /health)
            ├── /observe  → ObserveStub (501)
            └── /*        → ProjectRouter
                            └── StreamableHttpService<UnimatrixServer>

ProjectRouter
  ├── stores: HashMap<String, Arc<ProjectContext>>
  ├── default_project: Option<String>
  └── routes: ["/observe" registered for W2-6 path-prefix compatibility]

StaticTokenAuth
  ├── implements: BearerValidator trait
  ├── token: [u8; 32] (in-memory, loaded from token file)
  ├── exempt_paths: ["/health"]
  └── on_success: inserts ResolvedIdentity into request extensions

Audit Trail (existing, new values):
  audit_log.credential_type = "static_token" (HTTP) | "none" (UDS/stdio)
  audit_log.agent_attribution = clientInfo.name (from MCP initialize)
```

---

## User Workflows

### 1. Server Operator — First-Time HTTPS Setup

1. Operator adds `[http] enabled = true` to `config.toml`
2. Operator provides TLS certificate and key paths in `[tls]` section (or sets `tls.enabled = false` for reverse proxy)
3. Operator starts server (`unimatrix serve --foreground` or `--daemon`)
4. Server generates token file at `{data_volume}/token` (first run only)
5. Server prints token to stdout: `[UNIMATRIX TOKEN] <64-hex-chars>`
6. Operator copies token for client configuration
7. Server begins accepting HTTPS connections on content port

### 2. Server Operator — Proxy-Terminated Deployment

1. Operator sets `[tls] enabled = false` in config.toml
2. Reverse proxy (nginx, Caddy, cloud LB) terminates TLS and forwards plain HTTP to Unimatrix
3. Server binds plain HTTP on configured port
4. Bearer token auth still applies — the proxy does not handle auth

### 3. Client Developer — Claude Code Connection

1. Developer runs: `claude mcp add unimatrix -H "Authorization: Bearer <token>" -- https://<host>:8443`
2. The `-H` flag is required due to anthropics/claude-code#28293
3. Claude Code connects via HTTPS, sends MCP initialize handshake
4. All tool calls include the bearer token header
5. Developer configures curl-based shell hooks for remote telemetry (hook POSTs to `/observe` — returns 501 until W2-7)

### 4. Client Developer — Codex CLI / Gemini CLI Connection

1. Developer configures MCP server in client's config file with URL and bearer token header
2. Client connects via HTTPS, sends MCP initialize handshake
3. Developer configures curl-based shell hooks for remote telemetry

### 5. Container Health Monitoring

1. Docker HEALTHCHECK or external monitor sends `GET /health` (no auth required)
2. Server responds with `{"version": "x.y.z", "schema_version": 27}`
3. Monitor verifies server is up and schema version is compatible

### 6. Token Rotation (Operational Procedure)

1. Operator stops the server
2. Operator deletes `{data_volume}/token`
3. Operator restarts the server
4. Server generates a new token and prints it to stdout
5. Operator updates all client configurations with the new token

---

## Constraints

1. **rmcp 0.16.0 pinned** — `=0.16.0` in Cargo.toml. HTTP features (`transport-streamable-http-server`, `transport-streamable-http-server-session`) must work with this exact version. No upgrade without workspace-wide validation.
2. **No axum** — rmcp 0.16 HTTP transport is tower-native (rmcp PR #642 removed axum). Use hyper + tower directly.
3. **`#![forbid(unsafe_code)]`** — enforced in `lib.rs`. All dependencies must be safe Rust or already audited.
4. **Max 500 lines per file** — HTTP transport modules decomposed into focused files under `src/http/`.
5. **Test infrastructure is cumulative** — extend existing `test_support.rs` and `make_server()` fixture. Do not create isolated test scaffolding.
6. **Audit log schema is immutable** — vnc-014 schema v25 migration is shipped. Write to existing columns (`credential_type`, `capability_used`, `agent_attribution`, `metadata`). No new migration for audit.
7. **Single binary** — HTTP transport is compiled into the same `unimatrix` binary. No separate service.
8. **`build_context_with_external_identity` seam** — must use the existing identity injection point (server.rs). Do not create a parallel identity resolution path. (SR-09 mitigation: this path is exercised for the first time in production; integration tests must cover the full chain.)
9. **Token file never in Docker image layers** — generated at runtime in the data volume.
10. **Claude Code bug #28293** — headers in `.mcp.json` not forwarded on tool call POSTs. Client docs must specify `claude mcp add -H` path as the primary configuration method.
11. **Platform constraint** — curl-based shell hooks assume POSIX shell and `curl` availability. Windows/WSL clients are implicitly excluded from hook setup documentation (SR-07).
12. **Adapter boundary for rmcp isolation** — rmcp's `StreamableHttpService` API surface is new/lightly-adopted (SR-01, SR-02). The HTTP listener design must isolate rmcp-specific behavior behind a thin adapter so session bugs can be worked around without restructuring auth, routing, or the listener.
13. **Request extension propagation** — validate that `ResolvedIdentity` inserted into request extensions by `StaticTokenAuth` survives rmcp's internal request handling and is accessible in `build_context_with_external_identity` (SR-02). This must be verified early in implementation.

---

## Dependencies

### New Direct Dependencies (unimatrix-server/Cargo.toml)

| Crate | Purpose | Lockfile Status |
|-------|---------|----------------|
| `tokio-rustls` | Async TLS acceptor for TCP connections | Transitive (via reqwest -> hyper-rustls) |
| `rustls-pemfile` | PEM certificate/key file loading | Transitive (via reqwest -> hyper-rustls) |
| `subtle` | Constant-time comparison (`ConstantTimeEq`) for token validation | Transitive (via rustls) |
| `tower` | Middleware composition (`Layer`, `Service`) | Transitive (via reqwest) |
| `hyper` (1.x) | TCP listener and HTTP handling | Transitive (via reqwest) |
| `hyper-util` | Hyper service utilities | Transitive (via reqwest) |

### New rmcp Feature Flags

| Feature | Purpose |
|---------|---------|
| `transport-streamable-http-server` | `StreamableHttpService` for HTTP MCP transport |
| `transport-streamable-http-server-session` | HTTP session management for MCP |

### Existing Components Used

| Component | Location | Usage |
|-----------|----------|-------|
| `UnimatrixServer` | `server.rs` | Cloned into HTTP sessions (ADR-003 pattern) |
| `build_context_with_external_identity` | `server.rs:388` | Identity injection seam for bearer-auth callers |
| `ResolvedIdentity` | `mcp/identity.rs` | Produced by auth middleware, consumed by context builder |
| `ToolContext` | `mcp/context.rs` | Pre-validated handler context (Unimatrix pattern) |
| `AuditContext` / `AuditEvent` | `services/` | Audit logging with `credential_type` field |
| `client_type_map` | `server.rs` | Maps rmcp session key to `clientInfo.name` for `agent_attribution` |
| `UnimatrixConfig` | `infra/config.rs` | Extended with `HttpConfig` and `TlsConfig` sections |
| `LifecycleHandles` | `main.rs` | Extended with HTTP acceptor fields for graceful shutdown |
| `CancellationToken` | `main.rs` | Shared shutdown signal for HTTP listener (same as UDS) |

---

## NOT In Scope

- **Admin port activation** — port 8444 is registered/reserved but not activated. Enterprise private repo.
- **OAuth 2.1 / JWT validation** — enterprise private repo. `BearerValidator` trait enables additive extension.
- **Multi-project routing activation** — W2-6 scope. vnc-021 ships ProjectRouter with single-project default only.
- **Per-client behavioral testing** — W2-4 scope. vnc-021 covers transport, not client-specific behavior.
- **Token rotation mechanism** — documented as operational procedure (stop, delete, restart). No API or automated rotation.
- **Hot-reload of TLS certificates** — startup load from PEM is sufficient for personal cloud.
- **Metrics / Prometheus endpoint** — explicitly de-scoped.
- **`/observe` handler implementation** — ships as 501 stub only. Actual handler is W2-7 (remote telemetry transport).
- **Native HTTP hook handler** — all hooks use curl-based shell scripts. No Rust HTTP hook handler code.
- **Windows/WSL hook documentation** — POSIX-only for curl-based hooks (SR-07).
- **Rate limiting for HTTP transport** — UDS session rate limit exemption does not extend to HTTP callers (product vision non-negotiable #6), but per-endpoint rate limiting is not in vnc-021 scope.
- **HTTP in stdio mode** — stdio is dev/test only. HTTP listener activates only in daemon/foreground modes.
