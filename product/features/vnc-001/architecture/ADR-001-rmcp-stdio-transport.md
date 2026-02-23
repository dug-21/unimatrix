## ADR-001: rmcp 0.16 with stdio Transport

### Context

vnc-001 needs to expose Unimatrix as an MCP server. The Rust MCP SDK landscape was evaluated in ASS-002. Options: rmcp (official, 1.14M downloads/month, 139 contributors), rust-mcp-sdk (11.5K downloads, 5 contributors), mcpkit (zero adoption), or DIY (~750 LoC but spec tracking burden).

For transport, options are stdio (Claude Code's native MCP transport), HTTP/SSE (remote servers), or both. Claude Code uses stdio for local MCP servers. HTTP adds TLS, CORS, auth complexity with no current use case.

### Decision

Use `rmcp = { version = "=0.16.0", features = ["server", "transport-io", "macros"] }` with stdio transport only.

Pin the exact version (`=0.16.0`) because rmcp is pre-1.0 with frequent breaking changes. Upgrade deliberately, not automatically.

Use the `#[tool_router]` / `#[tool_handler]` proc macros for tool definition. Use schemars for JSON Schema generation on tool parameter types.

Stdio transport via `rmcp::transport::stdio::stdio()` returning `(stdin(), stdout())`.

### Consequences

- **Easier:** Full MCP protocol compliance without implementing JSON-RPC 2.0, schema generation, or tool dispatch manually. Tool definitions are declarative via proc macros.
- **Easier:** stdio transport is zero-config -- no TLS certificates, no port management, no CORS.
- **Harder:** Pre-1.0 SDK means potential breaking changes on upgrade. Pinning mitigates but requires manual upgrades.
- **Harder:** 35% doc coverage on rmcp means relying on examples and source code for edge cases.
- **Not affected:** HTTP transport can be added later behind a feature flag without changing the tool handler implementations. The `ServerHandler` trait is transport-agnostic.
