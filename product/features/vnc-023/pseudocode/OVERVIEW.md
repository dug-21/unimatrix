# vnc-023 Pseudocode Overview

## Components

| Component | File Modified | Purpose |
|-----------|--------------|---------|
| cargo-version-bump | `Cargo.toml` | Pin rmcp to `=1.7.0` |
| server-struct-migration | `server.rs` (production) | Replace Implementation/ServerInfo struct literals with constructors |
| server-test-migration | `server.rs` (test module) | Replace ClientInfo/Implementation struct literals with constructors |
| config-allowed-origins | `infra/config.rs` | Add `allowed_origins` field to HttpConfig |
| router-origin-wiring | `http/router.rs` | Wire `allowed_origins` through ProjectRouter/McpAdapter to StreamableHttpServerConfig |
| main-call-site | `main.rs` | Pass `config.http.allowed_origins` to ProjectRouter::new() |
| initialize-signature | `server.rs` (initialize fn) | Adapt ServerHandler::initialize if trait signature changed |

## Data Flow

```
config.toml
  [http]
  allowed_origins = ["https://example.com"]
      |
      v
HttpConfig.allowed_origins: Vec<String>    (config.rs — deserialized via serde)
      |
      v
main.rs: config.http.allowed_origins.clone()
      |
      v
ProjectRouter::new(server, max_body_bytes, allowed_origins)   (router.rs)
      |
      v
McpAdapter::new(server, max_body_bytes, allowed_origins)       (router.rs)
      |
      v
StreamableHttpServerConfig { allowed_origins, ..default }      (rmcp type)
      |
      v
StreamableHttpService::new(factory, session_mgr, config)       (rmcp)
```

## Shared Types

No new types are introduced. One existing type is modified:

- `HttpConfig` gains `allowed_origins: Vec<String>` (default: empty vec)

All rmcp types (`Implementation`, `ServerInfo`, `ClientInfo`, `StreamableHttpServerConfig`) are external -- their construction pattern changes from struct literals to constructors/builders.

## Sequencing Constraints

1. **cargo-version-bump** first -- all other components depend on rmcp 1.7 being resolvable
2. **server-struct-migration** + **initialize-signature** next -- fix production compilation
3. **server-test-migration** -- fix test compilation
4. **config-allowed-origins** before **router-origin-wiring** -- the field must exist before it can be read
5. **router-origin-wiring** before **main-call-site** -- the parameter must exist before main can pass it
6. Verify: `cargo build`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`
