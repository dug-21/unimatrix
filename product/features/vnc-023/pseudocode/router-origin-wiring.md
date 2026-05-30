# Component: router-origin-wiring

## Purpose

Wire `allowed_origins` through `ProjectRouter::new()` and `McpAdapter::new()` to `StreamableHttpServerConfig` in `crates/unimatrix-server/src/http/router.rs`. This connects the operator's config to rmcp's Origin header validation (ADR-002).

## Current Code

### McpAdapter::new (lines 384-397)

```rust
impl McpAdapter {
    fn new(server: UnimatrixServer, max_body_bytes: usize) -> Self {
        let session_manager = Arc::new(LocalSessionManager::default());
        let config = StreamableHttpServerConfig::default();
        let streamable =
            StreamableHttpService::new(move || Ok(server.clone()), session_manager, config);
        McpAdapter { streamable, max_body_bytes }
    }
}
```

### ProjectRouter::new (lines 313-320)

```rust
impl<ReqBody> ProjectRouter<ReqBody> {
    pub fn new(server: UnimatrixServer, max_body_bytes: usize) -> Self {
        let mcp_adapter = McpAdapter::new(server, max_body_bytes);
        ProjectRouter { default_server: mcp_adapter, _phantom: std::marker::PhantomData }
    }
}
```

## New/Modified Functions

### McpAdapter::new -- add allowed_origins parameter

```
fn new(server: UnimatrixServer, max_body_bytes: usize, allowed_origins: Vec<String>) -> Self {
    let session_manager = Arc::new(LocalSessionManager::default())

    // Build config with allowed_origins.
    // StreamableHttpServerConfig may be #[non_exhaustive] in rmcp 1.7.
    // Compile-driven: try field assignment, fall back to builder if available.

    // Strategy A: If StreamableHttpServerConfig has pub fields and Default works
    let mut config = StreamableHttpServerConfig::default()
    config.allowed_origins = allowed_origins

    // Strategy B: If builder pattern is available
    let config = StreamableHttpServerConfig::default()
        .with_allowed_origins(allowed_origins)

    // Note: Do NOT modify config.allowed_hosts -- rmcp defaults it to localhost,
    // which is the CVE-2026-42559 fix. Overriding it would reintroduce the vulnerability.

    let streamable = StreamableHttpService::new(
        move || Ok(server.clone()),
        session_manager,
        config,
    )

    McpAdapter { streamable, max_body_bytes }
}
```

### ProjectRouter::new -- add allowed_origins parameter, pass through

```
pub fn new(server: UnimatrixServer, max_body_bytes: usize, allowed_origins: Vec<String>) -> Self {
    let mcp_adapter = McpAdapter::new(server, max_body_bytes, allowed_origins)
    ProjectRouter {
        default_server: mcp_adapter,
        _phantom: std::marker::PhantomData,
    }
}
```

## Data Flow

```
ProjectRouter::new(server, max_body_bytes, allowed_origins)
    |
    v
McpAdapter::new(server, max_body_bytes, allowed_origins)
    |
    v
StreamableHttpServerConfig { allowed_origins: allowed_origins, ..default }
    |
    v
StreamableHttpService::new(factory, session_mgr, config)
```

The `allowed_origins` value passes through without transformation. It is not cloned (ownership transferred), not validated (rmcp validates at request time), and not logged (could contain sensitive origin patterns).

## Error Handling

- Construction is infallible. No error paths introduced.
- `StreamableHttpServerConfig::default()` is infallible.
- `StreamableHttpService::new()` is infallible.
- `LocalSessionManager::default()` gains a 5-minute keep_alive -- this is acceptable per scope (NFR-01).

## Critical Constraint

**Do NOT set or clear `config.allowed_hosts`**. The rmcp default (`["localhost"]` or equivalent) is the CVE-2026-42559 fix. Any code that sets `allowed_hosts = vec![]` reintroduces the DNS rebinding vulnerability.

## Key Test Scenarios

1. **Wiring integrity** (R-04): McpAdapter constructed with `allowed_origins = vec!["https://example.com"]` propagates that value to the StreamableHttpServerConfig used by StreamableHttpService
2. **Empty default** (R-04): McpAdapter constructed with `allowed_origins = vec![]` uses default behavior (no origin restriction)
3. **allowed_hosts untouched** (R-05): StreamableHttpServerConfig retains its default allowed_hosts after McpAdapter construction -- verify McpAdapter does not clear or override it
4. **CVE defense** (R-05): After construction, the StreamableHttpServerConfig has non-empty allowed_hosts (localhost)
5. **LocalSessionManager compiles** (FR-05, R-06): `LocalSessionManager::default()` call site unchanged and compiles
