# Rust MCP SDK Evaluation

**Date**: 2026-02-20
**Decision**: Use `rmcp` (official SDK)

---

## Landscape

| SDK | Version | Downloads/mo | Stars | Contributors | Verdict |
|-----|---------|-------------|-------|-------------|---------|
| **`rmcp`** (official) | 0.16.0 | 1.14M | ~3,000 | 139 | **Use this** |
| `rust-mcp-sdk` | 0.8.3 | 11.5K | 152 | ~5 | Skip — immature, heavy deps |
| `mcpkit` | 0.5.0 | ~0 | 2 | 1 | Skip — zero adoption |
| DIY | N/A | N/A | N/A | N/A | Skip — ~750 LoC but spec tracking burden |

---

## rmcp Details

**Repository**: [modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk)

### Supported Features

- Tools (via `#[tool]` proc macro + `schemars` JSON Schema generation)
- Resources
- Prompts
- Tasks (experimental)
- Elicitation
- Logging / progress notifications
- Completions

### Transports

- `stdio()` — primary for Claude Code integration
- `StreamableHttpService` — for future remote deployment
- `TokioChildProcess` — for spawning child servers

### Dependencies (with our feature flags)

```toml
rmcp = { version = "0.16", features = ["server", "transport-io", "macros"] }
```

Core deps pulled in: `tokio`, `serde`, `serde_json`, `schemars`, `futures`, `async-trait`, `tracing`, `chrono`, `thiserror`

**Not pulled in** (behind feature flags): `reqwest`, `oauth2`, `hyper`, `axum`

### API Pattern

```rust
// 1. Define your server struct
#[derive(Clone)]
struct UnimatrixServer { /* state */ }

// 2. Define tool argument types (auto-generates JSON Schema)
#[derive(Deserialize, JsonSchema)]
struct SearchArgs {
    query: String,
    k: Option<i64>,
}

// 3. Implement tools via proc macro
#[tool_router]
impl UnimatrixServer {
    #[tool(description = "Search project memory")]
    async fn memory_search(&self, #[tool(aggr)] args: SearchArgs) -> Result<CallToolResult, McpError> {
        // implementation
        Ok(CallToolResult::text("results"))
    }
}

// 4. Implement ServerHandler trait
#[tool_handler]
impl ServerHandler for UnimatrixServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo { name: "unimatrix".into(), version: "0.1.0".into() }
    }
}

// 5. Serve over stdio
let service = UnimatrixServer.serve(stdio()).await?;
service.waiting().await?;
```

### Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Pre-1.0 with frequent breaking changes | Medium | Pin exact version: `"=0.16.0"` |
| 35% doc coverage | Low | Examples directory has 10+ server examples |
| schemars 1.0 dependency (breaking from 0.8) | Low | We're starting fresh, no legacy schemars usage |
| Protocol spec still evolving | Low | SDK abstracts protocol changes |

---

## Why Not DIY

Building the MCP protocol directly requires ~750-1000 lines of code:

| Component | LoC |
|-----------|-----|
| JSON-RPC 2.0 types | ~80 |
| MCP message types (serde structs) | ~200 |
| Stdio transport loop | ~80 |
| Router/dispatcher | ~100 |
| Handler implementations | ~240 |
| Error handling/logging | ~50 |
| **Total** | **~750-1000** |

This is feasible but:
1. Must manually track MCP spec changes (the spec is actively evolving)
2. No proc macros for tool definitions — manual JSON Schema construction
3. Protocol subtleties (capability negotiation, cursor pagination, progress tokens) must be implemented correctly
4. Every new MCP feature requires manual implementation

The `rmcp` SDK eliminates all of this. The proc macros alone save significant boilerplate.

---

## Recommendation

Use `rmcp` 0.16 with features `["server", "transport-io", "macros"]`. Pin the exact version during development. The SDK aligns with our Tokio stack, handles protocol complexity, and provides proc macros for clean tool definitions.

Add HTTP transport feature (`transport-streamable-http`) later when remote deployment becomes relevant (post v0.3).
