# Test Plan: main-call-site (C6)

## Component

`crates/unimatrix-server/src/main.rs` (line ~843) -- pass `config.http.allowed_origins` to `ProjectRouter::new()`.

## Risks Covered

- **R-04 (High)**: allowed_origins config wiring disconnected (first hop from config to router)

## Unit Test Expectations

No unit tests for this component -- `main.rs` is a binary entry point. Testing is via compile gate and integration.

## Verification Tests

### V-01: ProjectRouter::new call includes allowed_origins (R-04, AC-09)
```
assert: grep of main.rs shows ProjectRouter::new(server.clone(), config.http.max_request_body_bytes, config.http.allowed_origins...)
        Third parameter is config.http.allowed_origins (or .clone() thereof)
```

### V-02: Compiles with new signature (AC-01)
```
assert: cargo build --workspace exits 0
        main.rs call site matches updated ProjectRouter::new(server, max_body_bytes, allowed_origins) signature
```

## Compile Gate

### C-01: main.rs compiles with 3-arg ProjectRouter::new
- **Assert**: `cargo build -p unimatrix-server` exits 0
- **Assert**: No other changes to main.rs beyond the ProjectRouter::new call site

## Integration Test Expectations

- The full binary starts and accepts MCP connections. Smoke tests exercise the compiled binary -- if main.rs fails to wire config correctly, startup fails.

## Edge Cases from Risk Strategy

- **config.http.allowed_origins ownership**: If `allowed_origins` is consumed (moved) rather than cloned, and `config` is used later, compilation fails. Use `.clone()` if needed.
- **Config not loaded from file**: If the binary is started without a config file, `HttpConfig::default()` applies. `allowed_origins` defaults to empty vec. The binary should start without errors.
