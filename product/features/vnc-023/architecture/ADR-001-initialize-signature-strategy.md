## ADR-001: Compile-First Strategy for ServerHandler::initialize Signature

### Context

The `ServerHandler::initialize` override in `server.rs` (lines 1038-1096) uses `std::future::ready()` to return a ready future matching rmcp 0.16.0's trait signature:

```rust
fn initialize(
    &self,
    request: InitializeRequestParams,
    context: RequestContext<RoleServer>,
) -> impl Future<Output = Result<InitializeResult, ErrorData>> + Send + '_ {
    // ... client_type_map population logic ...
    std::future::ready(Ok(self.get_info()))
}
```

SR-03 flagged that rmcp 1.7 may have changed this to `async fn initialize(...)`. The research spike (ass-065) did not catalog trait signature changes in the `ServerHandler` trait itself, only model type changes.

Two possible states in rmcp 1.7:
1. **Unchanged** (return position impl trait): Our code compiles as-is. No action needed.
2. **Changed to `async fn`**: Requires changing the function signature to `async fn initialize(...)` and the return from `std::future::ready(Ok(self.get_info()))` to `Ok(self.get_info())`. The internal logic (client_type_map population, session key extraction) is unaffected -- only the wrapper changes.

### Decision

Use a compile-first approach. Attempt `cargo build` after the version bump and let the compiler identify whether the trait signature changed. Do not preemptively rewrite the function.

If the compiler reports a signature mismatch:
- Change `fn initialize(...) -> impl Future<...> + Send + '_` to `async fn initialize(...) -> Result<InitializeResult, ErrorData>`
- Replace `std::future::ready(Ok(self.get_info()))` with `Ok(self.get_info())`
- All internal logic (lines 1045-1092) remains identical

This is a mechanical fix requiring no design judgment.

### Consequences

- **Easier**: No speculative rewriting. The compiler provides an exact error message showing the expected signature.
- **Easier**: If unchanged, zero work. If changed, the fix is a 2-line edit with no logic changes.
- **Harder**: Nothing. Both outcomes are well-understood and mechanical.
