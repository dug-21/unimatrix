# Component: main-call-site

## Purpose

Update the `ProjectRouter::new()` call in `main.rs` (line 843) to pass `config.http.allowed_origins` as the new third parameter, completing the config-to-rmcp wiring chain.

## Current Code (line 843)

```rust
let project_router = ProjectRouter::new(server.clone(), config.http.max_request_body_bytes);
```

## New/Modified Functions

### main.rs -- ProjectRouter construction

```
let project_router = ProjectRouter::new(
    server.clone(),
    config.http.max_request_body_bytes,
    config.http.allowed_origins.clone(),
)
```

**Note**: `.clone()` is required because `config` is borrowed later (e.g., by `start_http_listener(&config.http, ...)`). `Vec<String>` clone is cheap for the expected small number of origins (0-5 entries typically).

## Data Flow

```
config.http.allowed_origins    (HttpConfig, deserialized from config.toml)
    |
    v  .clone()
ProjectRouter::new(server, max_body_bytes, allowed_origins)
```

This is the single call site connecting config deserialization to the router chain. No other code path creates a ProjectRouter.

## Error Handling

Not applicable. Constructor call is infallible. The `.clone()` is infallible.

## Key Test Scenarios

1. **Compile gate**: `cargo build -p unimatrix-server` succeeds with the added parameter
2. **End-to-end wiring** (R-04): A config.toml with `allowed_origins = ["https://example.com"]` results in rmcp receiving that origin list (covered by router-origin-wiring tests, not main.rs-specific tests)
