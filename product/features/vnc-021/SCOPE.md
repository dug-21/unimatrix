# vnc-021: HTTPS Transport + Static Bearer Token Auth

## Problem Statement

The Unimatrix MCP server currently communicates exclusively via stdio and Unix Domain Sockets (UDS). Both transports require the client to run on the same machine as the server. With W2-1 (container packaging) complete, the containerized daemon exists but is unreachable by any network client -- Claude Code, Codex CLI, and Gemini CLI cannot connect to a remote Unimatrix instance. This blocks the "personal cloud" deployment model where a developer runs Unimatrix on a VPS or cloud instance and connects from multiple machines.

This is Wave 2 critical path item W2-2 (GitHub Issue #658). Without HTTPS transport, W2-1's container is limited to same-host stdio/UDS access, and W2-6 (multi-project activation via ProjectRouter) has no transport to activate on.

## Goals

1. Add rmcp 0.16 streamable HTTP transport alongside existing UDS/stdio, so any MCP-compatible client can connect over the network
2. Authenticate all HTTP requests via a static 256-bit bearer token with constant-time validation
3. Provide TLS termination via rustls, with a configurable bypass for proxy-terminated deployments
4. Expose a health endpoint returning server version and schema version for client compatibility checking
5. Introduce structural seams for W2-6 (ProjectRouter, single-project default) and W2-7 (path-dispatching tower service, /observe stub) to prevent retrofits
6. Produce client setup documentation for Claude Code, Codex CLI, and Gemini CLI

## Non-Goals

- **Admin port activation** (8444 is reserved/registered but not active -- enterprise private repo)
- **OAuth 2.1 / JWT validation** (enterprise private repo; `BearerValidator` trait enables this as an additive layer)
- **Multi-project routing activation** (W2-6 scope; vnc-021 ships the ProjectRouter struct with single-project default only)
- **Per-client behavioral testing** (W2-4 scope; vnc-021 covers transport, not client-specific behavior)
- **Token rotation mechanism** (documented operational procedure: stop, delete token file, restart)
- **Hot-reload of TLS certificates** (startup load from PEM is sufficient for personal cloud)
- **Axum** (rmcp 0.16 HTTP transport is tower-native; no axum dependency needed)

## Background Research

### Existing Transport Architecture

The server has three transport surfaces today:

1. **Stdio** (`tokio_main_stdio` in `main.rs`): `server.serve(rmcp::transport::io::stdio())`. Single session, exits on stdin EOF.
2. **UDS MCP acceptor** (`uds/mcp_listener.rs`): Binds `unimatrix-mcp.sock` (0600), accepts up to 32 concurrent sessions, each gets `server.clone()`. CancellationToken controls lifecycle.
3. **UDS Hook IPC** (`uds/listener.rs`): Separate socket for hook events (4-byte length-prefix framing, not MCP).

The `UnimatrixServer` struct (server.rs) implements `rmcp::ServerHandler` via `#[rmcp::tool_handler]`. It is `Clone` (all fields are `Arc`-wrapped) per ADR-003 (Unimatrix #1913). The MCP UDS acceptor clones the server into each session task -- this same pattern applies to HTTP sessions.

The `build_context_with_external_identity` method (server.rs:388-468) already has a seam for bearer-auth identity injection: `external_identity: Option<&ResolvedIdentity>`. When `Some`, it bypasses `resolve_agent` and uses the provided identity directly. This was designed for W2-3 activation.

### W2-3 StaticTokenAuth Status (Audit Log Schema)

The issue claims "W2-3 StaticTokenAuth (COMPLETE)". Investigation reveals the schema work is shipped but the middleware implementation is not:

- **Shipped**: `audit_log` 4-column migration (`credential_type`, `capability_used`, `agent_attribution`, `metadata`) -- schema v25 (vnc-014). Append-only DDL triggers. `AuditEvent` struct includes all four fields with sentinels.
- **Shipped**: `build_context_with_external_identity` seam in server.rs accepting `Option<&ResolvedIdentity>`.
- **Not shipped**: `BearerValidator` trait definition, `StaticTokenAuth` tower middleware struct, token file generation/loading, `subtle` direct dependency. No files matching `BearerValidator` or `StaticTokenAuth` exist anywhere in the codebase.

The `credential_type` field defaults to `"none"` (stdio/UDS path). W2-2 will write `"static_token"` when requests arrive via HTTP bearer auth.

### rmcp 0.16 HTTP Transport

ASS-041 confirmed: `transport-streamable-http-server` feature in rmcp 0.16 provides `StreamableHttpService<S, M>` implementing `tower_service::Service<Request<RequestBody>>`. This composes directly with tower middleware.

Current Cargo.toml enables: `["server", "client", "transport-io", "macros"]`. Adding `"transport-streamable-http-server-session"` and `"transport-streamable-http-server"` activates HTTP transport. The feature does NOT pull in reqwest (that is client-side `auth` feature only), avoiding the reqwest 0.12/0.13 conflict.

The rmcp lockfile entry shows rmcp 0.16.0 currently compiles with minimal deps. The HTTP features will add `http`, `http-body`, `http-body-util`, `sse-stream`, `tower-service` -- all of which are already in the lockfile transitively.

### Dependency Landscape

Already in the lockfile (transitive), no new crate downloads needed:
- `tower 0.5.3` (via reqwest)
- `tower-http 0.6.8` (via reqwest)
- `tower-service` (via rmcp/hyper)
- `rustls 0.23.36` (via reqwest -> hyper-rustls)
- `subtle` (via rustls -- already a transitive dep)
- `hyper 1.x` (via reqwest)
- `http 1.x` (already a direct dependency of unimatrix-server)

New direct dependencies for unimatrix-server/Cargo.toml:
- `tokio-rustls` -- async TLS acceptor (pulls no new transitive deps)
- `rustls-pemfile` -- PEM certificate loading
- `subtle` -- constant-time comparison (already transitive via rustls; adding as direct makes the security dependency explicit)
- `tower` -- middleware composition (already transitive)
- `hyper` + `hyper-util` -- TCP listener (already transitive)

### Health Endpoint

Current `health.rs` is a sync CLI subcommand that probes UDS socket liveness. Returns exit code 0/1. ASS-062 Q5 recommends the HTTP health endpoint include `version` and `schema_version` fields for the same-major-version client compatibility contract.

The HTTP health endpoint is separate from the CLI health check -- it is a plain HTTP GET (no MCP framing, no auth required) served by the same TCP listener.

### ProjectRouter Seam (ASS-060)

ASS-060 specifies path-prefix routing: `/v1/{project-slug}/tools/...`. For vnc-021, the ProjectRouter ships with single-project default -- no `[[projects]]` config needed, no slug prefix in URLs. The struct exists so W2-6 can activate multi-project without restructuring the HTTP listener.

```rust
struct ProjectRouter {
    stores: HashMap<String, Arc<ProjectContext>>,  // slug -> (Store, VectorIndex, ...)
    default_project: Option<String>,               // single-project fallback
}
```

### Config Structure

No HTTP/TLS config sections exist in `infra/config.rs` today. New sections needed:
- `[http]` -- `enabled`, `content_port`, `bind_address`
- `[tls]` -- `enabled`, `cert_path`, `key_path`

## Proposed Approach

### Architecture

Add a third transport surface (HTTP) alongside existing UDS/stdio. The HTTP listener runs in the same tokio runtime as the daemon, sharing the same `UnimatrixServer` via `Clone`. Auth middleware intercepts requests before they reach rmcp's `StreamableHttpService`.

```
TcpListener:8443 (TLS via tokio-rustls, or plain HTTP when tls.enabled=false)
  -> tower layer: StaticTokenAuth
      read Authorization: Bearer <hex>
      constant-time compare with in-memory token
      success -> insert ResolvedIdentity into request extensions
      failure -> HTTP 401 (except /health which bypasses auth)
  -> Path-dispatching tower service:
      GET  /health  -> JSON response (no auth)
      POST /observe -> 501 stub (auth required)
      /*            -> StreamableHttpService<UnimatrixServer>
                        tool handler reads Extension<Parts> for identity
                        credential_type = "static_token" in audit events
```

### Key Design Choices

1. **Token file at `{data_volume}/token`**: Generated on first run via `OsRng`, written with mode 0600, printed to stdout once. Loaded silently on subsequent starts. 32 bytes = 64 hex chars. Mirrors Jupyter's token pattern.

2. **TLS is default-on when cert/key paths provided**: No `--insecure` flag. When `tls.enabled = false`, the server binds plain HTTP (for proxy-terminated deployments). When `tls.enabled = true` (default when cert/key present), requires valid cert/key paths or refuses to start.

3. **Single TCP listener on content port (8443)**: Admin port (8444) is registered in config but not activated. This avoids complexity for the personal cloud tier.

4. **ProjectRouter wraps UnimatrixServer**: Even in single-project mode, requests flow through ProjectRouter. This ensures the routing seam is exercised from day one.

5. **Health endpoint is unauthenticated**: `/health` returns `{"version": "x.y.z", "schema_version": 27}` without requiring a bearer token. This enables external monitoring and Docker HEALTHCHECK via HTTP.

## Acceptance Criteria

### Transport + Auth
- AC-01: Server starts an HTTPS listener on the configured content port (default 8443) when `[http] enabled = true`
- AC-02: Bearer token file generated at `{data_volume}/token` on first run (32 bytes, hex-encoded, mode 0600); printed to stdout once with `[UNIMATRIX TOKEN]` label
- AC-03: Bearer token loaded silently from existing token file on subsequent starts
- AC-04: HTTP requests without `Authorization: Bearer <token>` header receive HTTP 401
- AC-05: HTTP requests with incorrect bearer token receive HTTP 401; comparison uses `subtle::ConstantTimeEq`
- AC-06: HTTP requests with valid bearer token reach MCP tool dispatch; tools execute identically to UDS/stdio path
- AC-07: `credential_type` in audit_log is `"static_token"` for HTTP-authenticated requests
- AC-08: `agent_attribution` in audit_log is populated from `clientInfo.name` (MCP initialize handshake) for HTTP sessions
- AC-09: TLS termination via rustls when `[tls] cert_path` and `key_path` are configured
- AC-10: `tls.enabled = false` binds plain HTTP (no TLS) for proxy-terminated deployments
- AC-11: Server refuses to start when `tls.enabled = true` but cert_path or key_path is missing or invalid

### Path-Dispatching + Structural Prep (ASS-064)
- AC-12: HTTP listener uses a path-dispatching tower service, not direct forwarding to `StreamableHttpService`. Routes: `/health`, `/observe` (stub), `/*` (MCP)
- AC-13: `GET /health` returns JSON `{"version": "<semver>", "schema_version": <int>}` without authentication
- AC-14: `POST /observe` returns HTTP 501 Not Implemented with JSON body `{"error": "Remote telemetry not yet implemented. See W2-7."}`. Bearer token auth required (same as MCP path).
- AC-15: `/observe` is registered in ProjectRouter routing tree so W2-6 multi-project path-prefix routing includes it from day one

### Operational
- AC-16: Existing UDS/stdio transport paths are unaffected (zero regression)
- AC-17: `serve --foreground` mode (container) activates HTTP listener alongside UDS
- AC-18: `serve --daemon` mode activates HTTP listener alongside UDS
- AC-19: ProjectRouter struct exists with single-project default; requests route through it
- AC-20: Config sections `[http]` and `[tls]` parsed from config.toml; `[http] enabled` defaults to `false`; `tls.enabled` defaults to `true` when both `cert_path` and `key_path` are present in config, `false` otherwise
- AC-21: Maximum request body size enforced (1 MB); connection timeout enforced (30s)
- AC-22: Maximum concurrent HTTP sessions enforced (configurable, default 32)

### Client Setup Documentation
- AC-23: Client setup docs for Claude Code, Codex CLI, and Gemini CLI covering MCP connection (URL + bearer token) and hook configuration for remote telemetry
- AC-24: All three clients (Claude Code, Codex CLI, Gemini CLI) use curl-based shell hooks for remote telemetry -- no local `unimatrix` binary required, no native HTTP hook handler
- AC-25: Claude Code docs specify `claude mcp add -H` workaround for #28293

## Constraints

1. **rmcp 0.16.0 pinned** (`=0.16.0` in Cargo.toml) -- cannot upgrade without workspace-wide validation. HTTP features must work with this exact version.
2. **No axum** -- rmcp 0.16 HTTP transport is tower-native (PR #642 removed axum dependency from rmcp). Use hyper + tower directly.
3. **`#![forbid(unsafe_code)]`** -- enforced in `lib.rs`. All dependencies must be safe Rust or already audited.
4. **Max 500 lines per file** -- existing workspace rule. HTTP transport modules must be decomposed into focused files.
5. **Test infrastructure is cumulative** -- extend existing `test_support.rs` and `make_server()` fixture; do not create isolated test scaffolding.
6. **Audit log schema is immutable** -- the vnc-014 schema v25 migration is shipped. W2-2 writes to existing columns; no new migration for audit.
7. **Single binary** -- HTTP transport is compiled into the same `unimatrix` binary, not a separate service.
8. **`build_context_with_external_identity` seam** -- must use the existing identity injection point (server.rs:388); do not create a parallel identity resolution path.
9. **Token file never in Docker image layers** -- generated at runtime in the data volume, never baked into the image.
10. **Claude Code bug anthropics/claude-code#28293** -- headers in `.mcp.json` not forwarded on tool call POSTs. Client docs must specify `claude mcp add -H` path.

## Open Questions

1. **HTTP listener in stdio mode**: No. Stdio is dev/test. Mixing exit semantics invites bugs for zero user benefit. HTTP listener only in daemon/foreground modes.

2. **Graceful shutdown for HTTP sessions**: Yes, `LifecycleHandles` gets HTTP acceptor fields. Same `CancellationToken` pattern as UDS. Implementation detail, not a scope question.

3. **Content port configurability**: Support `0` (OS-assigned) for testing. Trivial addition with high testing value.

4. **ProjectRouter home**: `src/http/router.rs`. Transport-specific routing logic. Transport-agnostic state (stores) injected via Arc.

## Resolved by ASS-064

ASS-064 (Remote Telemetry + MCP Transport Unification) resolved the remote telemetry architecture question. Key findings:

1. **No local binary required.** All three clients (Claude Code, Codex CLI, Gemini CLI) use curl-based shell hooks to POST observation events to `/observe` with bearer token auth. The hook process runs locally, POSTs event JSON to the remote server, receives injection content in the response body, writes it to stdout. Same mechanism as UDS, different wire.

2. **Path-dispatching tower service.** vnc-021 routes by path (not direct-to-StreamableHttpService). `/observe` ships as a 501 stub. The actual handler ships as W2-7 (remote telemetry transport) without any vnc-021 rework.

3. **Single TCP listener, single port, single auth layer.** MCP on `/*`, telemetry on `/observe`, health on `/health`. Path-based routing. Enterprise extends with per-team path prefixes (`/v1/{team-slug}/observe`).

4. **Sync hook latency budget.** 4 of 13 hook events are synchronous (UserPromptSubmit, PreCompact, SubagentStart, Ping). Remote HTTP adds 80-200ms per sync event. Configurable timeout (default 500ms for remote) required in W2-7.

## Tracking

GitHub Issue: #658
