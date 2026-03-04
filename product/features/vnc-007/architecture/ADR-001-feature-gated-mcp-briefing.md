## ADR-001: Feature-Gated MCP Briefing Tool

### Context

The `context_briefing` MCP tool sees minimal production use (disabled in specialist agents, not called by coordinators, replaced by hook injection). However, it provides programmatic briefing access with no alternative endpoint. The human wants to keep it functional but behind a feature flag for future removal.

The question is how to gate it: Cargo feature on the method, on a wrapper, or on the entire tool registration. rmcp's `#[tool]` procedural macro generates tool registration from method signatures. It is unclear whether `#[cfg(feature = "...")]` composes correctly with `#[tool(name = "...")]` at the method level.

### Decision

Gate the `context_briefing` method with `#[cfg(feature = "mcp-briefing")]`. Add a `[features]` section to `crates/unimatrix-server/Cargo.toml` with `default = ["mcp-briefing"]`.

```toml
[features]
default = ["mcp-briefing"]
mcp-briefing = []
```

```rust
#[cfg(feature = "mcp-briefing")]
#[tool(name = "context_briefing", description = "...")]
async fn context_briefing(&self, ...) -> Result<CallToolResult, rmcp::ErrorData> {
    // thin wrapper -> BriefingService::assemble()
}
```

If rmcp does not support `#[cfg]` on individual `#[tool]` methods (i.e., the tool still gets registered even when the method is compiled out), use a fallback approach: keep the method unconditional but have its body check a runtime flag or return an "unavailable" error when the feature is off. The Cargo feature approach is preferred because it is zero-cost at runtime.

BriefingService itself is NOT gated. Only the MCP endpoint is conditional.

The `format_briefing` function in response.rs and the `Briefing` struct are also gated with `#[cfg(feature = "mcp-briefing")]` since they are only used by the MCP tool handler. BriefingResult (the service-layer type) is always available.

### Consequences

- **Easier**: Future removal of the MCP briefing tool requires only removing the feature from defaults, then later removing the dead code.
- **Easier**: Testing the no-briefing configuration is a single `cargo test --no-default-features` invocation.
- **Harder**: Two compilation configurations must be tested (with and without the feature). CI should include both.
- **Risk**: If rmcp's macro expansion does not respect `#[cfg]`, the fallback adds a small runtime check. SR-01 from the risk assessment tracks this.
