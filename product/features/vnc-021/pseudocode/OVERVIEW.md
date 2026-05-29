# vnc-021 Pseudocode Overview

## Components and Wave Ordering

| Wave | Component | File | Depends On |
|------|-----------|------|------------|
| 0 | Spike: rmcp extension propagation | (test in router.rs) | nothing |
| 1 | token-manager | `src/http/token.rs` | nothing |
| 1 | config-extensions | `src/infra/config.rs` | nothing |
| 2 | static-token-auth | `src/http/auth.rs` | token-manager |
| 2 | tls-config | `src/http/tls.rs` | config-extensions |
| 2 | health-handler | `src/http/health.rs` | nothing |
| 3 | path-router | `src/http/router.rs` | health-handler, static-token-auth, spike result |
| 4 | http-listener | `src/http/listener.rs` | tls-config, path-router, config-extensions |
| 5 | lifecycle-integration | `src/infra/shutdown.rs`, `src/main.rs` | all above |

Wave 0 MUST complete before Wave 2. If the spike proves extensions are dropped, ADR-003 adapter fallback activates in the path-router component.

## Data Flow

```
[Config TOML] --> config-extensions --> HttpConfig, TlsConfig
                                           |            |
[Token file]  --> token-manager -------> token_bytes    |
                                           |            |
                                    StaticTokenAuth   TlsAcceptor
                                           |            |
                           PathRouter <----+            |
                             |   |   |                  |
                   /health   |   |   /* (MCP)           |
                   (C6)      |   |   StreamableHttp     |
                          /observe                      |
                          (501)                         |
                                                        |
                           http-listener <--------------+
                             |
                           lifecycle-integration
                             |
                           main.rs startup
```

## Shared Types

These types are defined in existing crates and referenced across components:

| Type | Location | Used By |
|------|----------|---------|
| `ResolvedIdentity` | `mcp/identity.rs` | static-token-auth, path-router |
| `CallerId` | `services/mod.rs` | lifecycle-integration (new variant `HttpBearer`) |
| `UnimatrixConfig` | `infra/config.rs` | config-extensions (add `HttpConfig`, `TlsConfig`) |
| `LifecycleHandles` | `infra/shutdown.rs` | lifecycle-integration (add HTTP fields) |
| `ServerError` | `error.rs` | all components for error propagation |
| `UnimatrixServer` | `server.rs` | path-router (cloned into StreamableHttpService) |

### New Types Introduced

| Type | Defined In | Purpose |
|------|-----------|---------|
| `HttpConfig` | `infra/config.rs` | `[http]` TOML section |
| `TlsConfig` | `infra/config.rs` | `[tls]` TOML section |
| `BearerValidator` (trait) | `http/auth.rs` | Abstraction for token validation (FR-14) |
| `StaticTokenAuth<S>` | `http/auth.rs` | Tower Layer/Service for bearer auth |
| `StaticTokenAuthLayer` | `http/auth.rs` | Tower Layer factory |
| `AuthError` | `http/auth.rs` | Auth-specific error type |
| `PathRouter` | `http/router.rs` | Path-dispatching tower Service |
| `ProjectRouter` | `http/router.rs` | Project-aware MCP dispatch (W2-6 seam) |
| `McpAdapter` | `http/router.rs` | Thin rmcp isolation boundary (ADR-003) |

### Constants

| Constant | Value | Location |
|----------|-------|----------|
| `CREDENTIAL_TYPE_STATIC_TOKEN` | `"static_token"` | `http/auth.rs` |
| `HEALTH_PATH` | `"/health"` | `http/auth.rs` |
| `OBSERVE_PATH` | `"/observe"` | `http/router.rs` |
| `TOKEN_FILE_NAME` | `"token"` | `http/token.rs` |
| `TOKEN_HEX_LEN` | `64` | `http/token.rs` |
| `TOKEN_BYTE_LEN` | `32` | `http/token.rs` |

## Module Declaration

`src/http/mod.rs` re-exports:
```
pub(crate) mod auth;
pub(crate) mod health;
pub(crate) mod listener;
pub(crate) mod router;
pub(crate) mod tls;
pub(crate) mod token;
```

`src/lib.rs` adds: `pub mod http;`
