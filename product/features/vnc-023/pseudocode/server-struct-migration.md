# Component: server-struct-migration

## Purpose

Replace `Implementation` and `ServerInfo` struct literal construction in the production `UnimatrixServer::new()` method (server.rs, lines 274-287) with constructor/builder calls compatible with rmcp 1.7's `#[non_exhaustive]` attributes. Add `Implementation` description enrichment (FR-08, Opp 20).

## Current Code (lines 274-287)

```rust
let server_info = ServerInfo {
    server_info: Implementation {
        name: SERVER_NAME.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        ..Default::default()
    },
    capabilities: ServerCapabilities::builder().enable_tools().build(),
    instructions: Some(
        instructions.unwrap_or_else(|| SERVER_INSTRUCTIONS_DEFAULT.to_string()),
    ),
    ..Default::default()
};
```

## New/Modified Functions

### UnimatrixServer::new() -- server_info construction block

```
// Implementation: use constructor + builder for description enrichment
let implementation = Implementation::new(SERVER_NAME, env!("CARGO_PKG_VERSION"))
    .with_description("Self-learning knowledge engine for agentic workflows")

// ServerInfo: use constructor/builder or ..Default::default() rest syntax
// Two strategies depending on what rmcp 1.7 provides:

// Strategy A: If ..Default::default() works with #[non_exhaustive] + Default derive
//   (unlikely for external crate -- #[non_exhaustive] blocks struct literal construction
//    from outside the defining crate, even with ..Default::default())
let server_info = ServerInfo {
    server_info: implementation,
    capabilities: ServerCapabilities::builder().enable_tools().build(),
    instructions: Some(instructions.unwrap_or_else(|| SERVER_INSTRUCTIONS_DEFAULT.to_string())),
    ..Default::default()
}

// Strategy B: If rmcp 1.7 provides a ServerInfo constructor/builder
//   (compile-driven -- let the compiler guide which pattern works)
let server_info = ServerInfo::new(implementation)
    .with_capabilities(ServerCapabilities::builder().enable_tools().build())
    .with_instructions(instructions.unwrap_or_else(|| SERVER_INSTRUCTIONS_DEFAULT.to_string()))

// Strategy C: Use Default + field mutation if fields are pub
let mut server_info = ServerInfo::default()
server_info.server_info = implementation
server_info.capabilities = ServerCapabilities::builder().enable_tools().build()
server_info.instructions = Some(instructions.unwrap_or_else(|| SERVER_INSTRUCTIONS_DEFAULT.to_string()))
```

**Decision**: Compile-driven. Try Strategy A first. If it fails (expected for `#[non_exhaustive]`), use whatever constructor/builder rmcp 1.7 provides. The implementer must check `rmcp::model::ServerInfo` docs or source for the available API.

### get_info() (server.rs)

No change. `get_info()` returns `self.server_info.clone()`. The `server_info` field type remains `ServerInfo` -- only its construction changes.

## Data Flow

- Inputs: `SERVER_NAME` (const), `CARGO_PKG_VERSION` (env), `instructions: Option<String>` (config)
- Output: `ServerInfo` stored in `self.server_info`
- Consumed by: `get_info()` which is called by rmcp during `initialize` handshake

## Error Handling

Construction is infallible. No error paths. If a constructor returns `Result`, unwrap with `.expect("ServerInfo construction")` -- this is server startup, not runtime.

## Key Test Scenarios

1. **Field correctness** (R-03): `get_info()` returns ServerInfo with:
   - `server_info.name` == "unimatrix"
   - `server_info.version` == CARGO_PKG_VERSION
   - `server_info.description` == Some("Self-learning knowledge engine for agentic workflows")
   - `capabilities` includes tools capability
   - `instructions` is Some(non-empty string)
2. **Description enrichment** (FR-08, R-12): initialize response contains `serverInfo.implementation.description`
3. **Capabilities preserved** (R-03): ServerCapabilities includes tools (not empty)
