# Test Plan: router-origin-wiring (C4)

## Component

`crates/unimatrix-server/src/http/router.rs` -- wire `allowed_origins` through `ProjectRouter::new()` and `McpAdapter::new()` to `StreamableHttpServerConfig`.

## Risks Covered

- **R-04 (High)**: allowed_origins config wiring disconnected (hops 2-4)
- **R-05 (High)**: CVE resolution -- McpAdapter must not override allowed_hosts default
- **R-13 (High)**: allowed_origins vs allowed_hosts interaction

## Unit Test Expectations

### T-01: McpAdapter::new accepts allowed_origins parameter (R-04, AC-09)
```
arrange: server = test UnimatrixServer, origins = vec!["https://example.com"]
act:     McpAdapter::new(server, 1_048_576, origins)
assert:  construction succeeds (no panic, compiles)
```

### T-02: McpAdapter::new with empty origins (backward compat)
```
arrange: server = test UnimatrixServer, origins = vec![]
act:     McpAdapter::new(server, 1_048_576, origins)
assert:  construction succeeds
```

### T-03: ProjectRouter::new passes allowed_origins through (R-04, AC-09)
```
arrange: server = test UnimatrixServer, origins = vec!["https://example.com"]
act:     ProjectRouter::new(server, 1_048_576, origins)
assert:  construction succeeds
```

### T-04: StreamableHttpServerConfig receives allowed_origins (R-04)
```
arrange: origins = vec!["https://claude.ai".to_string()]
act:     construct StreamableHttpServerConfig, set allowed_origins = origins
assert:  config.allowed_origins == vec!["https://claude.ai"]
```
Note: If StreamableHttpServerConfig fields are not directly inspectable (e.g., `#[non_exhaustive]`), this test may need to be a compile-only verification that the field assignment compiles.

### T-05: allowed_hosts not overridden by McpAdapter (R-05, R-13, AC-05)
```
arrange: (none)
act:     let config = StreamableHttpServerConfig::default()
assert:  config.allowed_hosts is non-empty (contains localhost or equivalent default)
         McpAdapter does NOT set config.allowed_hosts to empty
```
Note: If `allowed_hosts` is not publicly readable, verify via code review that `McpAdapter::new()` does not assign to `config.allowed_hosts`.

## Compile Gate

### C-01: Signature change compiles (AC-09)
- **Assert**: `McpAdapter::new(server, max_body_bytes, allowed_origins)` compiles
- **Assert**: `ProjectRouter::new(server, max_body_bytes, allowed_origins)` compiles

## Integration Test Expectations

- Security suite tests that exercise capability enforcement validate the full request chain. If `McpAdapter` is misconfigured, these fail.
- No new integration test needed -- Origin enforcement is rmcp's responsibility. Our tests verify wiring only.

## Edge Cases from Risk Strategy

- **allowed_origins + allowed_hosts both configured (R-13)**: Both are independent checks per ADR-002. Code review must confirm McpAdapter does not modify `allowed_hosts` when setting `allowed_origins`. T-05 addresses this.
- **Empty allowed_origins + populated allowed_hosts**: rmcp should enforce hosts but skip origin check. This is rmcp default behavior -- no test needed on our side.
- **LocalSessionManager::default() keep_alive (R-06, AC-05)**: McpAdapter uses `LocalSessionManager::default()`. Verify it compiles and existing session tests pass. No override of keep_alive.
