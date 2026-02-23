# Pseudocode: error.rs (C10 — Server Error Types)

## Purpose

Server-specific error types and mapping to rmcp's `ErrorData`. Standalone, needed by every other server component.

## Types

```
enum ServerError {
    Core(CoreError),
    Registry(String),
    Audit(String),
    ProjectInit(String),
    EmbedNotReady,
    EmbedFailed(String),
    CapabilityDenied { agent_id: String, capability: Capability },
    NotImplemented(String),
    Shutdown(String),
}
```

## Constants

```
const ERROR_ENTRY_NOT_FOUND: i32 = -32001;
const ERROR_INVALID_PARAMS: i32 = -32602;     // standard JSON-RPC
const ERROR_CAPABILITY_DENIED: i32 = -32003;
const ERROR_EMBED_NOT_READY: i32 = -32004;
const ERROR_NOT_IMPLEMENTED: i32 = -32005;
const ERROR_INTERNAL: i32 = -32603;            // standard JSON-RPC
```

## Trait Implementations

### Display for ServerError

```
Core(e)              -> "internal error: {e}"
Registry(msg)        -> "registry error: {msg}"
Audit(msg)           -> "audit error: {msg}"
ProjectInit(msg)     -> "project initialization failed: {msg}"
EmbedNotReady        -> "embedding model is initializing"
EmbedFailed(msg)     -> "embedding model failed: {msg}"
CapabilityDenied { agent_id, capability }
                     -> "agent '{agent_id}' lacks {capability:?} capability"
NotImplemented(tool) -> "tool '{tool}' is not yet implemented"
Shutdown(msg)        -> "shutdown error: {msg}"
```

### std::error::Error for ServerError

```
source():
    Core(e) -> Some(e)
    _ -> None
```

### From<CoreError> for ServerError

```
ServerError::Core(e)
```

### From<ServerError> for rmcp::model::ErrorData

Maps each variant to an MCP error response with actionable messages.

```
Core(CoreError::Store(StoreError::EntryNotFound(id))) ->
    ErrorData { code: ERROR_ENTRY_NOT_FOUND, message: "Entry {id} not found. Verify the ID from a previous search result." }

Core(_) ->
    ErrorData { code: ERROR_INTERNAL, message: "Internal storage error. The operation was not completed." }

CapabilityDenied { agent_id, capability } ->
    ErrorData { code: ERROR_CAPABILITY_DENIED, message: "Agent '{agent_id}' lacks {capability:?} capability. Contact project admin." }

EmbedNotReady ->
    ErrorData { code: ERROR_EMBED_NOT_READY, message: "Embedding model is initializing. Try again in a few seconds, or use context_lookup which does not require embeddings." }

EmbedFailed(msg) ->
    ErrorData { code: ERROR_EMBED_NOT_READY, message: "Embedding model failed to load: {msg}. Restart the server to retry." }

NotImplemented(tool) ->
    ErrorData { code: ERROR_NOT_IMPLEMENTED, message: "Tool '{tool}' is registered but not yet implemented. Full implementation ships in vnc-002." }

Registry(msg) ->
    ErrorData { code: ERROR_INTERNAL, message: "Agent registry error: {msg}" }

Audit(msg) ->
    ErrorData { code: ERROR_INTERNAL, message: "Audit log error: {msg}" }

ProjectInit(msg) ->
    ErrorData { code: ERROR_INTERNAL, message: "Project initialization failed: {msg}" }

Shutdown(msg) ->
    ErrorData { code: ERROR_INTERNAL, message: "Shutdown error: {msg}" }
```

Note: rmcp's `ErrorData` has fields: `code: i32`, `message: String`, `data: Option<Value>`. We set `data` to `None` for all variants.

## Error Handling

- `From<CoreError>` enables `?` propagation from core trait calls
- `From<ServerError> for ErrorData` enables `?` propagation in tool handler return types
- The `StoreError::EntryNotFound` variant is pattern-matched specifically for the -32001 code; all other `CoreError` variants map to -32603

## Key Test Scenarios

1. Each ServerError variant produces the correct MCP error code
2. Error messages are actionable (contain guidance, not raw Rust types)
3. EntryNotFound maps to -32001, not -32603
4. CapabilityDenied message includes the agent_id and capability name
5. EmbedNotReady message suggests context_lookup as alternative
6. Display impl does not leak internal details
