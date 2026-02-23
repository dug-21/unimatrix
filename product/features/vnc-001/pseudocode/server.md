# Pseudocode: server.rs (C2 — Server Core)

## Purpose

The `UnimatrixServer` struct implements rmcp's `ServerHandler` trait. Holds all shared state. Cloneable (all fields Arc-wrapped).

## Types

```
#[derive(Clone)]
struct UnimatrixServer {
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    registry: Arc<AgentRegistry>,
    audit: Arc<AuditLog>,
    server_info: ServerInfo,
}
```

## Constants

```
const SERVER_NAME: &str = "unimatrix";
const SERVER_INSTRUCTIONS: &str = "Unimatrix is this project's knowledge engine. Before starting implementation, architecture, or design tasks, search for relevant patterns and conventions using the context tools. Apply what you find. After discovering reusable patterns or making architectural decisions, store them for future reference. Do not store workflow state or process steps.";
```

## Functions

### UnimatrixServer::new(...)

```
fn new(
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    registry: Arc<AgentRegistry>,
    audit: Arc<AuditLog>,
) -> Self {
    let server_info = ServerInfo {
        name: SERVER_NAME.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        instructions: Some(SERVER_INSTRUCTIONS.to_string()),
        // capabilities set by rmcp based on handler impl
    };

    UnimatrixServer {
        entry_store,
        vector_store,
        embed_service,
        registry,
        audit,
        server_info,
    }
}
```

### ServerHandler impl

```
#[tool_handler]
impl ServerHandler for UnimatrixServer {
    fn get_info(&self) -> ServerInfo {
        self.server_info.clone()
    }
}
```

The `#[tool_handler]` macro from rmcp wires up tool dispatch. The actual tool methods are in tools.rs via `#[tool_router]`.

### UnimatrixServer::resolve_agent(&self, agent_id: &Option<String>) -> Result<ResolvedIdentity, ServerError>

```
let extracted = identity::extract_agent_id(agent_id);
let identity = identity::resolve_identity(&self.registry, &extracted).await?;
Ok(identity)
```

This is a convenience method called by each tool handler to resolve the agent from the tool params. It combines extraction and resolution into one call.

## rmcp Integration Notes

The rmcp `ServerHandler` trait requires:
1. `get_info()` -> returns ServerInfo
2. Tool methods decorated with `#[tool]` inside a `#[tool_router]` impl block

The server is served via:
```
let service = server.serve(rmcp::transport::io::stdio()).await?;
service.waiting().await;
```

`serve()` returns a `RunningService` that handles the MCP protocol. `waiting()` blocks until the session ends.

## Error Handling

- `resolve_agent` propagates registry errors as `ServerError`
- The `ServerInfo` construction is infallible

## Key Test Scenarios

1. get_info returns ServerInfo with name="unimatrix"
2. get_info returns non-empty version string
3. get_info returns instructions containing "knowledge engine"
4. Server is Clone (required by rmcp)
5. resolve_agent with Some("test") resolves correctly
6. resolve_agent with None defaults to "anonymous"
