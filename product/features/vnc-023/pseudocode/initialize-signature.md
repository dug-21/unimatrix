# Component: initialize-signature

## Purpose

Adapt the `ServerHandler::initialize` override in `server.rs` (lines 1038-1096) if the trait signature changed between rmcp 0.16.0 and 1.7.0. Per ADR-001, this is compile-driven -- no preemptive rewrite.

## Current Code (lines 1038-1096)

```rust
fn initialize(
    &self,
    request: rmcp::model::InitializeRequestParams,
    context: rmcp::service::RequestContext<rmcp::RoleServer>,
) -> impl std::future::Future<Output = Result<rmcp::model::InitializeResult, rmcp::ErrorData>>
+ Send
+ '_ {
    // ... client_type_map population logic (lines 1045-1092) ...
    std::future::ready(Ok(self.get_info()))
}
```

## Scenarios

### Scenario 1: Trait signature unchanged

No changes needed. The current code compiles as-is. The `impl Future` return type with `std::future::ready()` body is valid.

### Scenario 2: Trait changed to `async fn`

Change the function signature and return expression only. All internal logic is identical.

```
// Before:
fn initialize(
    &self,
    request: rmcp::model::InitializeRequestParams,
    context: rmcp::service::RequestContext<rmcp::RoleServer>,
) -> impl std::future::Future<Output = Result<rmcp::model::InitializeResult, rmcp::ErrorData>>
+ Send
+ '_ {
    // ... all internal logic unchanged ...
    std::future::ready(Ok(self.get_info()))
}

// After:
async fn initialize(
    &self,
    request: rmcp::model::InitializeRequestParams,
    context: rmcp::service::RequestContext<rmcp::RoleServer>,
) -> Result<rmcp::model::InitializeResult, rmcp::ErrorData> {
    // ... all internal logic unchanged ...
    Ok(self.get_info())
}
```

Changes (if needed):
1. `fn initialize(...) -> impl Future<...> + Send + '_` becomes `async fn initialize(...) -> Result<InitializeResult, ErrorData>`
2. `std::future::ready(Ok(self.get_info()))` at line 1095 becomes `Ok(self.get_info())`
3. Lines 1045-1092 (client_type_map logic) remain byte-for-byte identical

### Scenario 3: Parameter types renamed

If `InitializeRequestParams` or `RequestContext<RoleServer>` were renamed in rmcp 1.7, update the type references. The compiler error message will show the expected types. This is a mechanical find-and-replace.

## Constraint

**No logic changes permitted** (C-07 from the brief). The internal body (client_name extraction, truncation, session_key extraction from Parts, client_type_map insertion) must remain identical. Only the function signature wrapper and the final return expression may change.

## Error Handling

The function returns `Result<InitializeResult, ErrorData>`. The current implementation always returns `Ok(...)`. No error paths exist in the body. This does not change.

## Key Test Scenarios

1. **Compile gate** (R-02, AC-12): `cargo build -p unimatrix-server` succeeds
2. **Initialize handshake** (R-02): MCP client completes initialize, receives correct ServerInfo
3. **client_type_map populated** (R-02): After initialize, `client_type_map` contains the client's name keyed on session ID
4. **Session key extraction** (R-02): HTTP transport initialize extracts Mcp-Session-Id from Parts headers
5. **Extension propagation** (R-01): RequestContext.extensions still contains http::request::Parts after rmcp 1.7 processing -- validated by the initialize function's own session_key extraction code
