# Pseudocode Overview: vnc-001 MCP Server Core

## Components

| Component | File | Purpose |
|-----------|------|---------|
| project | project.rs | Project root detection, hash, data directory |
| error | error.rs | ServerError enum, MCP error mapping |
| registry | registry.rs | AgentRegistry, AgentRecord, trust/capabilities |
| audit | audit.rs | AuditLog, AuditEvent, monotonic IDs |
| identity | identity.rs | Agent identity extraction + resolution |
| embed-handle | embed_handle.rs | Lazy-loading EmbedServiceHandle state machine |
| server | server.rs | UnimatrixServer, ServerHandler impl |
| tools | tools.rs | Tool stubs, param types, tool_router |
| shutdown | shutdown.rs | LifecycleHandles, graceful shutdown sequence |
| main | main.rs + lib.rs | Entry point, wiring, module declarations |

## Data Flow

```
main.rs
  -> project::ensure_data_directory(override_dir)  => ProjectPaths
  -> Store::open(paths.db_path)                    => Arc<Store>
  -> VectorIndex::load or ::new                    => Arc<VectorIndex>
  -> EmbedServiceHandle::new() + start_loading()   => Arc<EmbedServiceHandle>
  -> AgentRegistry::new(store) + bootstrap          => Arc<AgentRegistry>
  -> AuditLog::new(store)                          => Arc<AuditLog>
  -> Build adapters: StoreAdapter, VectorAdapter    => AsyncEntryStore, AsyncVectorStore
  -> UnimatrixServer::new(all subsystems)           => server
  -> server.serve(stdio()).await                    => RunningService
  -> shutdown::graceful_shutdown(handles, running)  => exit
```

## Shared Types

All shared types are defined in their owning modules:

- `ServerError` (error.rs) -- used by all components
- `AgentRecord`, `TrustLevel`, `Capability` (registry.rs) -- used by identity, server, tools
- `AuditEvent`, `Outcome` (audit.rs) -- used by identity, tools
- `ResolvedIdentity` (identity.rs) -- used by server, tools
- `ProjectPaths` (project.rs) -- used by main
- `EmbedServiceHandle`, `EmbedState` (embed_handle.rs) -- used by server
- `LifecycleHandles` (shutdown.rs) -- used by main
- Tool param structs (tools.rs) -- used only in tools

## Implementation Order

```
1. Store table extension (unimatrix-store schema.rs + db.rs)
2. project.rs     -- standalone
3. error.rs       -- standalone, needed by everything
4. registry.rs    -- depends on Store, error
5. audit.rs       -- depends on Store, error
6. identity.rs    -- depends on registry, error
7. embed_handle.rs -- standalone async wrapper
8. server.rs      -- depends on 4,5,6,7
9. tools.rs       -- depends on server
10. shutdown.rs   -- depends on server lifecycle types
11. main.rs + lib.rs -- wires everything
```

## Cross-Component Contracts

- Registry and audit both take `Arc<Store>` and operate on their own tables (AGENT_REGISTRY, AUDIT_LOG)
- Identity resolution calls `registry.resolve_or_enroll()` and returns `ResolvedIdentity`
- Tool handlers call `identity::extract_agent_id()` then `identity::resolve_identity()` then `audit.log_event()`
- Shutdown coordinator receives `LifecycleHandles` and the running service future; manages ordered teardown
- `EmbedServiceHandle` wraps the async init; exposes `embed_entry()` that checks state before delegating
