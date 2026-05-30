## ADR-002: allowed_origins as Additive HttpConfig Field

### Context

rmcp 1.6.0 added `allowed_origins: Vec<String>` to `StreamableHttpServerConfig` for Origin header validation (CSRF defense-in-depth, complementing the Host header validation from 1.4.0 that fixes CVE-2026-42559).

Currently, `McpAdapter::new()` uses `StreamableHttpServerConfig::default()` with no external configuration surface. Origin validation defaults to disabled (empty vec = all origins accepted). To expose this to operators, the config chain needs extending: `config.toml` -> `HttpConfig` -> `ProjectRouter` -> `McpAdapter` -> `StreamableHttpServerConfig`.

Alternatives considered:
1. **Hardcode origins in McpAdapter**: Inflexible. Different deployments (localhost dev, reverse proxy, direct HTTPS) need different allowed origins.
2. **Expose full StreamableHttpServerConfig in config.toml**: Over-exposes rmcp internals. Most fields (`json_response`, `stateful_mode`, `init_timeout`) have correct defaults and should not be user-configurable yet.
3. **Add only `allowed_origins` to HttpConfig**: Targeted. One field, backward-compatible default, clear semantics.

### Decision

Add `allowed_origins: Vec<String>` to `HttpConfig` in `config.rs`. Default: empty vec (no origin restriction -- backward compatible with existing deployments).

Wire through the constructor chain:
- `HttpConfig.allowed_origins` (config.rs, deserialized from `[http]` section)
- `ProjectRouter::new(server, max_body_bytes, allowed_origins)` (router.rs)
- `McpAdapter::new(server, max_body_bytes, allowed_origins)` (router.rs)
- Set on `StreamableHttpServerConfig` before passing to `StreamableHttpService::new()`

Config.toml example:
```toml
[http]
enabled = true
allowed_origins = ["https://example.com", "https://app.example.com"]
```

**Interaction with `allowed_hosts`**: These are independent security layers. `allowed_hosts` validates the `Host` header (DNS rebinding defense, defaults to localhost in rmcp 1.4+). `allowed_origins` validates the `Origin` header (CSRF defense, defaults to disabled). Both are checked independently by rmcp -- a request must pass both checks to reach the MCP handler. Document this in the field comment.

### Consequences

- **Easier**: Operators can restrict which web origins may access the MCP endpoint, defense-in-depth alongside `allowed_hosts`.
- **Easier**: Empty default means zero behavioral change for existing deployments. Existing `config.toml` files parse without modification.
- **Easier**: Single-field addition follows the existing `HttpConfig` `#[serde(default)]` pattern.
- **Harder**: Adds one parameter to `ProjectRouter::new()` and `McpAdapter::new()` constructor chains. The call site in `main.rs` must pass the new parameter.
