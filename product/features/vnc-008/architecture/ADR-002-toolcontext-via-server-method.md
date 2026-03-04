## ADR-002: ToolContext Constructed via UnimatrixServer Method

### Context

Each of the 12 MCP tool handlers repeats a 15-25 line ceremony: identity resolution, format parsing, AuditContext construction, and error mapping (79 occurrences of `.map_err(rmcp::ErrorData::from)` in tools.rs). This ceremony needs extraction into a reusable `ToolContext` struct.

The rmcp `#[tool]` macro constrains handler signatures to `(&self, Parameters<T>) -> Result<CallToolResult, ErrorData>`. ToolContext cannot be injected as a function parameter or via middleware. SR-02 flagged this constraint.

Options considered:
1. **Standalone constructor**: `ToolContext::new(server, agent_id, format)` — requires passing `&self` (UnimatrixServer) as a parameter, awkward ownership
2. **Method on UnimatrixServer**: `self.build_context(agent_id, format)` — natural given `&self` is available in every handler
3. **Derive macro / proc macro**: Over-engineered for the problem

### Decision

ToolContext is constructed via `self.build_context(&params.agent_id, &params.format)` method on UnimatrixServer. Capability checking is a separate `self.require_cap(&ctx.agent_id, capability)` call.

Capability is separated because:
- Different tools require different capabilities (Read, Write, Search, Admin)
- Some tools check multiple capabilities
- Keeping it explicit makes the security model auditable

The `build_context()` method lives in `server.rs` (where UnimatrixServer is defined), not in `mcp/context.rs` (which only defines the ToolContext struct). This avoids circular imports between `mcp/` and root.

### Consequences

- Each handler reduces from 15-25 lines of ceremony to 2-3 lines: `build_context()` + `require_cap()`
- `.map_err(rmcp::ErrorData::from)` count drops by ~60% (identity, format, audit construction all absorbed)
- Capability check remains explicit per handler — no risk of accidentally granting wrong capability
- ToolContext struct is defined in `mcp/context.rs` but construction lives in `server.rs` — this split is necessary to avoid `mcp/` importing `server.rs` and `server.rs` importing `mcp/`
