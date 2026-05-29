# lifecycle-integration (C8) -- `src/infra/shutdown.rs` + `src/main.rs` + `src/services/mod.rs`

## Purpose

Wire the HTTP listener into the existing startup and shutdown lifecycle. Extends `LifecycleHandles` with HTTP-specific fields. Adds `CallerId::HttpBearer` variant. Integrates HTTP startup into `tokio_main_daemon` and `tokio_main_foreground`.

## Modifications to Existing Files

### 1. `src/services/mod.rs` -- CallerId Extension

Add new variant to the `CallerId` enum (Unimatrix pattern #319):

```
pub(crate) enum CallerId:
    Agent(String),
    UdsSession(String),
    HttpBearer(String),   // NEW -- HTTP bearer-token caller

// The compiler enforces exhaustive match. Every existing match on CallerId
// must add an HttpBearer arm. Key match sites:
//   - Rate limiter: HttpBearer is NOT exempt (unlike UdsSession) (R-17)
//   - Audit: HttpBearer maps to credential_type "static_token"
//   - Display/Debug: include transport in output
```

**Critical**: When adding the `HttpBearer` variant, the Rust compiler will flag every incomplete match. Each site must be handled:

```
// In rate limiter (gateway.rs or wherever rate limiting matches on CallerId):
match caller_id:
    CallerId::Agent(id) => // rate limited (existing)
    CallerId::UdsSession(_) => // exempt (existing)
    CallerId::HttpBearer(id) => // rate limited (NEW -- same as Agent, NOT exempt)
```

### 2. `src/infra/shutdown.rs` -- LifecycleHandles Extension

Add HTTP acceptor fields to `LifecycleHandles`:

```
pub struct LifecycleHandles:
    // ... all existing fields unchanged ...

    /// HTTP accept loop task handle (vnc-021).
    /// Aborted during graceful_shutdown between MCP acceptor (Step 0) and hook IPC (Step 0b).
    /// `None` when HTTP is disabled or in stdio mode.
    pub http_acceptor_handle: Option<JoinHandle<()>>,

    /// HTTP listener bound address (vnc-021).
    /// Stored for logging/debugging. `None` when HTTP is disabled.
    pub http_listener_addr: Option<SocketAddr>,
```

### 3. `src/infra/shutdown.rs` -- graceful_shutdown Extension

Insert HTTP shutdown step between MCP acceptor (Step 0) and hook IPC (Step 0b):

```
pub async fn graceful_shutdown(mut handles: LifecycleHandles) -> Result<(), ServerError>:
    tokio::time::sleep(Duration::from_millis(100)).await

    // Step 0: Stop MCP acceptor task (existing, unchanged)
    if let Some(handle) = handles.mcp_acceptor_handle.take():
        handle.abort()
        match tokio::time::timeout(Duration::from_secs(35), handle).await:
            Ok(_) => tracing::info!("MCP acceptor task finished"),
            Err(_) => tracing::warn!("MCP acceptor task did not finish within 35s"),

    // Step 0-http: Stop HTTP acceptor task (NEW -- vnc-021)
    // Placed after MCP acceptor to match the architecture's specified ordering.
    // The HTTP accept loop stops accepting new connections when its
    // CancellationToken is cancelled (which happens when the daemon token
    // is cancelled, before graceful_shutdown is called). This abort + join
    // ensures the accept loop task itself is cleaned up.
    if let Some(handle) = handles.http_acceptor_handle.take():
        handle.abort()
        match tokio::time::timeout(Duration::from_secs(35), handle).await:
            Ok(_) => tracing::info!("HTTP acceptor task finished"),
            Err(_) => tracing::warn!("HTTP acceptor task did not finish within 35s"),

    // Step 0a: Drop MCP socket guard (existing, unchanged)
    drop(handles.mcp_socket_guard.take())

    // Step 0b: Stop hook IPC UDS listener (existing, unchanged)
    // ... rest of shutdown unchanged ...
```

### 4. `src/main.rs` -- Startup Wiring

In both `tokio_main_daemon` and `tokio_main_foreground`, add HTTP listener startup after UDS listener setup:

```
// In tokio_main_daemon / tokio_main_foreground:

// ... existing: load config, build server, start UDS listeners ...

// --- HTTP LISTENER STARTUP (vnc-021) ---
let (http_acceptor_handle, http_listener_addr) = if config.http.enabled:
    // 1. Load or generate token
    let token_bytes = load_or_generate_token(&data_dir)?
    let token_array: [u8; 32] = token_bytes.try_into()
        .map_err(|_| ServerError::Config("token must be exactly 32 bytes"))?

    // 2. Build TLS acceptor (may be None for proxy-terminated)
    let tls_acceptor = build_tls_acceptor(&config.tls)?

    // 3. Build the tower service stack (inside-out):
    //    StreamableHttpService<UnimatrixServer>
    //    -> ProjectRouter
    //    -> PathRouter
    //    -> StaticTokenAuth
    let project_router = ProjectRouter::new(server.clone(), &config.http)
    let path_router = PathRouter::new(project_router)
    let auth_layer = StaticTokenAuthLayer::new(token_array)
    let service = auth_layer.layer(path_router)

    // 4. Start HTTP listener
    let (handle, addr) = start_http_listener(
        &config.http,
        tls_acceptor,
        service,
        daemon_token.child_token(),
    ).await?

    tracing::info!("HTTP transport active on {addr}")
    (Some(handle), Some(addr))

else:
    // HTTP disabled -- emit informational log (human review note #2)
    tracing::info!("HTTP transport available — set [http] enabled = true in config.toml")
    (None, None)

// ... existing: build LifecycleHandles ...

let handles = LifecycleHandles {
    // ... existing fields ...
    http_acceptor_handle,
    http_listener_addr,
}
```

### 5. `src/lib.rs` -- Module Declaration

```
pub mod http;  // NEW -- HTTP transport modules
```

### 6. `src/http/mod.rs` -- Module File

```
//! HTTP transport: HTTPS listener with static bearer token authentication.
//!
//! Provides network-accessible MCP connections for Claude Code, Codex CLI,
//! and Gemini CLI. See vnc-021.

pub(crate) mod auth;
pub(crate) mod health;
pub(crate) mod listener;
pub(crate) mod router;
pub(crate) mod tls;
pub(crate) mod token;
```

### 7. `Cargo.toml` -- Dependencies

```toml
# Add to existing rmcp line:
rmcp = { version = "=0.16.0", features = [
    "server", "client", "transport-io", "macros",
    "transport-streamable-http-server",        # NEW
    "transport-streamable-http-server-session", # NEW
] }

# New direct dependencies (all already transitive -- Unimatrix #4661):
tokio-rustls = "0.26"
rustls-pemfile = "2"
subtle = "2"
tower = { version = "0.5", features = ["util"] }
hyper = { version = "1", features = ["http1", "server"] }
hyper-util = { version = "0.1", features = ["tokio", "http1"] }
```

**WARNING**: Do NOT enable rmcp's `auth` or `reqwest` features -- those cause a reqwest 0.12/0.13 version conflict (Unimatrix #4661).

## credential_type Wiring

The `build_context_with_external_identity` function in `server.rs` currently builds an `AuditSource::Mcp` with the identity's agent_id and trust_level. The `credential_type` field must be set to `CREDENTIAL_TYPE_STATIC_TOKEN` when `external_identity` is `Some(...)`.

The implementation agent must trace the audit emission path to find where `credential_type` is written to the audit_log table. Two possible locations:
1. `AuditContext` struct has a `credential_type` field -- set it in `build_context_with_external_identity`
2. `AuditEvent` emission derives credential_type from the source -- add the derivation

The key invariant: `credential_type = "static_token"` for HTTP, `credential_type = "none"` for UDS/stdio.

## HTTP Disabled in Stdio Mode (R-16)

The HTTP listener activation check is `config.http.enabled`. Stdio mode does not use daemon/foreground startup paths (it has its own `tokio_main_stdio`), so HTTP is structurally excluded from stdio mode. No additional guard needed.

## Error Handling

| Error Case | Behavior | Notes |
|-----------|----------|-------|
| Token generation fails | Server refuses to start | Propagated from `load_or_generate_token` |
| TLS config invalid | Server refuses to start | Propagated from `build_tls_acceptor` |
| HTTP bind fails | Server refuses to start | Propagated from `start_http_listener` |
| HTTP disabled | Log info message, continue | UDS/stdio unaffected |
| Shutdown with HTTP active | HTTP acceptor aborted between MCP and hook IPC | Matches architecture ordering |

## Key Test Scenarios

1. **CallerId::HttpBearer not rate-exempt**: Add `HttpBearer` to CallerId. Verify rate limiter match arm does not exempt it (R-17).
2. **LifecycleHandles HTTP fields**: Construct with `http_acceptor_handle: None`. Verify compiles and shutdown succeeds (same as stdio mode).
3. **Shutdown ordering**: Verify HTTP acceptor is aborted after MCP acceptor and before hook IPC.
4. **HTTP disabled log**: Start with `http.enabled = false`. Verify info log "HTTP transport available".
5. **Full startup**: Start with HTTP enabled, valid config. Verify HTTP listener bound address returned.
6. **No HTTP in stdio**: Verify `tokio_main_stdio` does not reference HTTP listener.
