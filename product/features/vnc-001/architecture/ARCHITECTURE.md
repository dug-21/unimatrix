# Architecture: vnc-001 MCP Server Core

## System Overview

vnc-001 creates `unimatrix-server`, a binary crate that exposes Unimatrix's knowledge engine to AI agents via the Model Context Protocol (MCP) over stdio transport. It sits above `unimatrix-core` and is the first consumer of the trait abstractions established in nxs-004.

The server has two distinct responsibility layers: **lifecycle management** (startup, project isolation, shutdown, persistence) and **request handling** (MCP protocol, tool dispatch, security infrastructure). The lifecycle layer holds concrete types (`Arc<Store>`, `Arc<VectorIndex>`) for operations that are not on the core traits (compact, dump, load). The request handling layer uses trait objects via async wrappers for all tool operations.

```
Claude Code / MCP Client
        |
        | stdio (JSON-RPC 2.0)
        v
unimatrix-server (binary)
  |-- ServerHandler (rmcp)        -- MCP protocol handling
  |-- ToolRouter                  -- tool dispatch (stubs in vnc-001, real in vnc-002)
  |-- RequestContext              -- agent identity, audit logging
  |-- SecurityMiddleware [vnc-002 plugs in here]
  |
  |-- AsyncEntryStore             -- spawn_blocking -> StoreAdapter -> Store
  |-- AsyncVectorStore            -- spawn_blocking -> VectorAdapter -> VectorIndex
  |-- AsyncEmbedService           -- spawn_blocking -> EmbedAdapter -> OnnxProvider
  |
  |-- ProjectManager              -- data directory, auto-init
  |-- ShutdownCoordinator         -- compact + dump orchestration
  |
  v
unimatrix-core (traits + async wrappers)
  |         |          |
  v         v          v
store    vector      embed
```

## Component Breakdown

### C1: Binary Entry Point (`crates/unimatrix-server/src/main.rs`)

The `#[tokio::main]` entry point. Responsible for the startup sequence and signal handling.

**Responsibilities:**
- Parse command-line arguments (minimal: `--project-dir` override, `--verbose` for tracing)
- Call ProjectManager to detect/create project data directory
- Initialize all subsystems (store, vector, embed, registry, audit)
- Construct UnimatrixServer with all dependencies
- Serve over stdio transport via rmcp
- Register SIGTERM/SIGINT handlers that trigger graceful shutdown

**Constraints:**
- Must not block on embedding model download (lazy-load per Q3 resolution)
- Must complete MCP initialization handshake before any tool calls arrive

### C2: Server Core (`crates/unimatrix-server/src/server.rs`)

The `UnimatrixServer` struct that implements rmcp's `ServerHandler` trait. Holds all shared state.

**Responsibilities:**
- Implement `ServerHandler::get_info()` returning ServerInfo with name, version, capabilities, and instructions field
- Implement tool listing via `#[tool_handler]` macro
- Hold shared references to all subsystems
- Own the `RequestContext` factory for creating per-request context

**Structure:**
```rust
#[derive(Clone)]
pub struct UnimatrixServer {
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,  // lazy-loading wrapper
    registry: Arc<AgentRegistry>,
    audit: Arc<AuditLog>,
    server_info: ServerInfo,
}
```

The `Clone` derive is required by rmcp's service model. All fields are `Arc`-wrapped so cloning is cheap.

### C3: Tool Stubs (`crates/unimatrix-server/src/tools.rs`)

The `#[tool_router]` impl block with four tool definitions. vnc-001 provides stubs; vnc-002 replaces them with real implementations.

**Responsibilities:**
- Define `context_search`, `context_lookup`, `context_store`, `context_get` tool signatures
- Define JSON Schema for each tool's parameters via schemars
- Return structured "not yet implemented" responses from stubs
- Include tool annotations (readOnlyHint, destructiveHint, etc.)

**Tool parameter types** (defined with `#[derive(Deserialize, JsonSchema)]`):
- `SearchParams { query: String, topic: Option<String>, category: Option<String>, tags: Option<Vec<String>>, k: Option<i64>, agent_id: Option<String> }`
- `LookupParams { topic: Option<String>, category: Option<String>, tags: Option<Vec<String>>, id: Option<i64>, status: Option<String>, limit: Option<i64>, agent_id: Option<String> }`
- `StoreParams { content: String, topic: String, category: String, tags: Option<Vec<String>>, title: Option<String>, source: Option<String>, agent_id: Option<String> }`
- `GetParams { id: i64, agent_id: Option<String> }`

**Enforcement point for vnc-002:** Each tool handler method receives the full params struct. vnc-002 adds validation calls at the top of each handler before any storage operation. The pattern:

```rust
#[tool(description = "...")]
async fn context_store(&self, #[tool(aggr)] params: StoreParams) -> Result<CallToolResult, ErrorData> {
    // [ENFORCEMENT POINT: vnc-002 input validation]
    // validate_store_params(&params)?;

    // [ENFORCEMENT POINT: vnc-002 capability check]
    // let identity = self.resolve_agent(&params.agent_id).await?;
    // self.registry.require_capability(&identity, Capability::Write)?;

    // [ENFORCEMENT POINT: vnc-002 content scanning]
    // scan_content(&params.content)?;

    // ... actual implementation ...
}
```

### C4: Project Manager (`crates/unimatrix-server/src/project.rs`)

Detects the project root, computes the project hash, and manages the data directory.

**Responsibilities:**
- Detect project root: walk up from cwd looking for `.git/` directory; fallback to cwd
- Compute project hash: `SHA-256(canonical_path)[..16]` (first 16 hex chars of hex digest)
- Create data directory at `~/.unimatrix/{project_hash}/` if absent
- Create `vector/` subdirectory for hnsw_rs dump files
- Return `ProjectPaths` struct with all resolved paths

**Structure:**
```rust
pub struct ProjectPaths {
    pub project_root: PathBuf,
    pub project_hash: String,
    pub data_dir: PathBuf,       // ~/.unimatrix/{hash}/
    pub db_path: PathBuf,        // ~/.unimatrix/{hash}/unimatrix.redb
    pub vector_dir: PathBuf,     // ~/.unimatrix/{hash}/vector/
}
```

**Algorithm for project root detection:**
1. Start from `std::env::current_dir()`
2. Check if `.git/` exists in current directory
3. If not, move to parent and repeat
4. If filesystem root reached, use original cwd
5. Canonicalize the detected path (resolve symlinks)
6. Compute `SHA-256(canonical_path_string)`, take first 16 hex chars

### C5: Agent Registry (`crates/unimatrix-server/src/registry.rs`)

Manages agent identity, trust levels, and capabilities using AGENT_REGISTRY redb table.

**Responsibilities:**
- Define `AgentRecord`, `TrustLevel`, and `Capability` types
- Bootstrap default agents on first run ("human" = Privileged, "system" = System)
- Auto-enroll unknown agents as Restricted on first encounter
- Update `last_seen_at` on each request
- Provide lookup methods for identity resolution
- Expose capability query interface for vnc-002's enforcement checks

**Types:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub agent_id: String,
    pub trust_level: TrustLevel,
    pub capabilities: Vec<Capability>,
    pub allowed_topics: Option<Vec<String>>,
    pub allowed_categories: Option<Vec<String>>,
    pub enrolled_at: u64,
    pub last_seen_at: u64,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustLevel {
    System,      // Unimatrix internals
    Privileged,  // Human user via MCP client
    Internal,    // Orchestrator agents (scrum-master, etc.)
    Restricted,  // Unknown/worker agents (read-only default)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    Read,
    Write,
    Search,
    Admin,
}
```

**Table:** `AGENT_REGISTRY: TableDefinition<&str, &[u8]>` -- key is agent_id string, value is bincode-serialized AgentRecord.

**Default capabilities by trust level:**

| Trust Level | Capabilities |
|------------|-------------|
| System | Read, Write, Search, Admin |
| Privileged | Read, Write, Search, Admin |
| Internal | Read, Write, Search |
| Restricted | Read, Search |

**Enforcement point for vnc-002:** The registry exposes `has_capability(&self, agent_id: &str, cap: Capability) -> Result<bool>` and `require_capability(&self, agent_id: &str, cap: Capability) -> Result<()>`. vnc-002 calls these at the top of each tool handler. The registry does not enforce -- it provides the query interface. Enforcement is the tool handler's responsibility.

### C6: Audit Log (`crates/unimatrix-server/src/audit.rs`)

Append-only request logging using AUDIT_LOG redb table.

**Responsibilities:**
- Define `AuditEvent` struct
- Assign monotonic event IDs via COUNTERS table (key: "next_audit_id")
- Append events -- no read, update, or delete exposed in vnc-001
- Provide `log_event()` method called after each tool request completes

**Types:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub event_id: u64,
    pub timestamp: u64,       // unix seconds
    pub session_id: String,
    pub agent_id: String,
    pub operation: String,    // "context_search", "context_store", etc.
    pub target_ids: Vec<u64>, // entry IDs affected (empty for search)
    pub outcome: Outcome,
    pub detail: String,       // human-readable detail
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Outcome {
    Success,
    Denied,
    Error,
    NotImplemented,
}
```

**Table:** `AUDIT_LOG: TableDefinition<u64, &[u8]>` -- key is monotonic event_id, value is bincode-serialized AuditEvent.

**Enforcement point for vnc-002:** vnc-002 extends the audit detail to include denied reasons (capability violation, input validation failure, content scan hit). The `log_event()` interface already supports arbitrary `detail` and `outcome` values.

### C7: Identity Resolution (`crates/unimatrix-server/src/identity.rs`)

Extracts agent identity from tool call parameters and threads it through the request pipeline.

**Responsibilities:**
- Extract `agent_id` from tool parameter structs (every tool has an optional `agent_id` field)
- Default to `"anonymous"` when `agent_id` is absent
- Look up or auto-enroll agent in registry
- Produce `ResolvedIdentity` struct for downstream use
- Update `last_seen_at` in registry

**Types:**
```rust
pub struct ResolvedIdentity {
    pub agent_id: String,
    pub trust_level: TrustLevel,
    pub capabilities: Vec<Capability>,
}
```

**Design note:** The `agent_id` is a tool parameter (not MCP metadata) because stdio transport provides no per-request metadata channel. This is self-reported and inherently unverified. Future transports (HTTPS with OAuth 2.1) will provide verified identity via bearer tokens, but the internal pipeline (`ResolvedIdentity` -> capability check -> audit log) remains identical. See ADR-003.

### C8: Embed Service Handle (`crates/unimatrix-server/src/embed_handle.rs`)

Lazy-loading wrapper around the embedding service that implements the `EmbedService` trait.

**Responsibilities:**
- Wrap `OnnxProvider` initialization in a background task
- Expose `EmbedService` trait methods that check readiness before delegating
- Return structured "embedding model initializing" error for calls before model loads
- Transition from "loading" to "ready" state atomically

**Structure:**
```rust
pub struct EmbedServiceHandle {
    state: RwLock<EmbedState>,
}

enum EmbedState {
    Loading,
    Ready(Arc<EmbedAdapter>),
    Failed(String),
}
```

**Initialization flow:**
1. Server creates `EmbedServiceHandle` in `Loading` state
2. Spawns a tokio task that calls `OnnxProvider::new(config)` (may download model)
3. On success, transitions state to `Ready(adapter)`
4. On failure, transitions to `Failed(error_message)` and logs error
5. Tool handlers calling `embed_entry()` before `Ready` get a structured error

### C9: Shutdown Coordinator (`crates/unimatrix-server/src/shutdown.rs`)

Orchestrates graceful shutdown following the resolved shutdown sequence.

**Responsibilities:**
- Listen for SIGTERM/SIGINT via `tokio::signal`
- Listen for MCP session close (rmcp server returns from `waiting()`)
- Execute shutdown sequence:
  1. Stop accepting new requests (rmcp cancel)
  2. Wait for in-flight requests to complete (bounded timeout: 5 seconds)
  3. Call `VectorIndex::dump(&vector_dir)` through `Arc<VectorIndex>` (dump takes &self)
  4. Drop all `Arc` clones (async wrappers, adapters, server struct)
  5. `Arc::try_unwrap(store)` -> if Ok, call `compact()`; if Err, log warning
  6. Exit with code 0

**Key insight:** The server must hold the original `Arc<Store>` and `Arc<VectorIndex>` separately from the trait-wrapped versions. The async wrappers and adapters clone the Arcs. During shutdown, dropping the server (and all its clones) reduces refcount. The shutdown coordinator holds the "last" Arc references for lifecycle operations.

**Structure:**
```rust
pub struct LifecycleHandles {
    pub store: Arc<Store>,
    pub vector_index: Arc<VectorIndex>,
    pub vector_dir: PathBuf,
}
```

### C10: Server Error Types (`crates/unimatrix-server/src/error.rs`)

Server-specific error types and mapping to rmcp's `ErrorData`.

**Responsibilities:**
- Define `ServerError` enum covering all server-specific failure modes
- Map `CoreError` variants to appropriate MCP error codes
- Map `ServerError` to rmcp `ErrorData` with actionable messages
- Provide error code constants

**Error mapping strategy:**

| Error Category | MCP Error Code | Example |
|---------------|---------------|---------|
| Entry not found | -32001 | "Entry 42 not found. Verify the ID from a previous search result." |
| Invalid parameters | -32602 | "Parameter 'topic' must be non-empty." |
| Capability denied | -32003 | "Agent 'anonymous' lacks Write capability. Contact project admin." |
| Embedding not ready | -32004 | "Embedding model is initializing. Try again in a few seconds, or use context_lookup which does not require embeddings." |
| Internal error | -32603 | "Internal storage error. The operation was not completed." |
| Not implemented | -32005 | "context_search is not yet implemented. Available in vnc-002." |

**Enforcement point for vnc-002:** vnc-002 adds error codes for input validation failures (-32010 through -32019) and content scanning rejections (-32020 through -32029). The `ServerError` enum is extensible via new variants.

## Component Interactions

### Data Flow: Server Startup

```
main()
  |
  v
ProjectManager::detect_or_create()
  |-> find .git/, compute hash, create ~/.unimatrix/{hash}/
  |-> return ProjectPaths { data_dir, db_path, vector_dir }
  |
  v
Store::open(db_path)
  |-> creates 10 tables (8 existing + AGENT_REGISTRY + AUDIT_LOG)
  |-> runs schema migration if needed
  |-> return Arc<Store>
  |
  v
VectorIndex::load(store, config, vector_dir) or VectorIndex::new(store, config)
  |-> if vector/unimatrix-vector.meta exists: load from dump
  |-> else: create empty index
  |-> return Arc<VectorIndex>
  |
  v
EmbedServiceHandle::new()
  |-> spawn background task: OnnxProvider::new(config)
  |-> return Arc<EmbedServiceHandle> (state: Loading)
  |
  v
AgentRegistry::new(store)
  |-> bootstrap "human" and "system" if not present
  |-> return Arc<AgentRegistry>
  |
  v
AuditLog::new(store)
  |-> return Arc<AuditLog>
  |
  v
Build adapters + async wrappers:
  StoreAdapter::new(store.clone()) -> AsyncEntryStore::new(Arc::new(adapter))
  VectorAdapter::new(vector_index.clone()) -> AsyncVectorStore::new(Arc::new(adapter))
  |
  v
UnimatrixServer { entry_store, vector_store, embed_service, registry, audit }
  |
  v
server.serve(stdio()).await
  |-> MCP initialize handshake
  |-> ServerInfo { name: "unimatrix", version, instructions, capabilities }
  |-> tool list: [context_search, context_lookup, context_store, context_get]
  |
  v
server.waiting().await  // blocks until session close or signal
```

### Data Flow: Tool Request (vnc-001 stub)

```
MCP Client -> JSON-RPC: tools/call { name: "context_search", arguments: {...} }
  |
  v
rmcp dispatches to UnimatrixServer::context_search(params)
  |
  v
1. Extract agent_id from params (default: "anonymous")
2. Resolve identity: registry.resolve_or_enroll(agent_id)
3. [ENFORCEMENT POINT: capability check -- vnc-002]
4. [ENFORCEMENT POINT: input validation -- vnc-002]
5. Return stub response: CallToolResult with "not yet implemented" message
6. Log audit event: { operation: "context_search", outcome: NotImplemented }
  |
  v
JSON-RPC response -> MCP Client
```

### Data Flow: Tool Request (vnc-002 real, showing enforcement points)

```
MCP Client -> JSON-RPC: tools/call { name: "context_store", arguments: {...} }
  |
  v
rmcp dispatches to UnimatrixServer::context_store(params)
  |
  v
1. Extract agent_id from params
2. Resolve identity: registry.resolve_or_enroll(agent_id)
3. [ENFORCEMENT: capability check]
   registry.require_capability(agent_id, Capability::Write)?
4. [ENFORCEMENT: input validation]
   validate_store_params(&params)?  -- max lengths, patterns, no control chars
5. [ENFORCEMENT: category allowlist]
   validate_category(&params.category)?
6. [ENFORCEMENT: content scanning]
   scan_content(&params.content)?  -- injection patterns, PII
7. Build NewEntry from validated params
8. entry_store.insert(entry).await
9. embed_service.embed_entry(title, content).await
10. vector_store.insert(entry_id, embedding).await
11. Log audit event: { operation: "context_store", target_ids: [entry_id], outcome: Success }
12. Return CallToolResult with confirmation
```

### Data Flow: Graceful Shutdown

```
SIGTERM or MCP session close
  |
  v
ShutdownCoordinator receives signal
  |
  v
1. server.cancel()  -- rmcp stops accepting new requests
2. tokio::time::timeout(5s, drain_in_flight)
3. vector_index.dump(&vector_dir)  -- works through Arc (&self)
4. drop(server)  -- drops UnimatrixServer and all its Arc clones
5. drop(async_wrappers)  -- drops AsyncEntryStore, AsyncVectorStore, etc.
6. drop(adapters)  -- drops StoreAdapter, VectorAdapter, etc.
7. match Arc::try_unwrap(store) {
       Ok(mut store) => store.compact(),
       Err(_) => tracing::warn!("skipping compact: outstanding references"),
   }
8. std::process::exit(0)
```

## Technology Decisions

| Decision | Choice | Rationale | ADR |
|----------|--------|-----------|-----|
| MCP SDK | rmcp 0.16.0, pinned exactly | Official Rust SDK, 1.14M downloads/month, proc macro tool definition | ADR-001 |
| Transport | stdio only | Claude Code integration; HTTP deferred | ADR-001 |
| Crate type | Binary in `crates/unimatrix-server/` | Workspace member, consistent with existing crate structure | ADR-002 |
| Agent identity | Self-reported `agent_id` tool parameter | stdio has no per-request metadata; transport-agnostic internal pipeline | ADR-003 |
| Project isolation | SHA-256 hash of canonical path, first 16 hex chars | Deterministic, collision-resistant, filesystem-safe | ADR-004 |
| Shutdown strategy | Arc::try_unwrap after dropping all clones | Preserves data integrity via compact+dump; graceful degradation if try_unwrap fails | ADR-005 |
| Embedding initialization | Lazy-load in background task | Non-blocking MCP init; reads work immediately | ADR-006 |
| Security enforcement | Documented enforcement points in tool handlers | Security checks are trivially pluggable in vnc-002 without refactoring | ADR-007 |

## Integration Points

### Consumed Dependencies

| Component | Crate | Interface Used |
|-----------|-------|---------------|
| Entry storage | unimatrix-core (re-exports unimatrix-store) | `StoreAdapter`, `AsyncEntryStore`, `EntryStore` trait, `Store::open()`, `Store::compact()` |
| Vector search | unimatrix-core (re-exports unimatrix-vector) | `VectorAdapter`, `AsyncVectorStore`, `VectorStore` trait, `VectorIndex::new()`, `VectorIndex::load()`, `VectorIndex::dump()` |
| Embedding | unimatrix-core (re-exports unimatrix-embed) | `EmbedAdapter`, `AsyncEmbedService`, `EmbedService` trait, `OnnxProvider::new()` |
| MCP protocol | rmcp 0.16.0 | `ServerHandler` trait, `#[tool_router]`, `#[tool_handler]`, `CallToolResult`, `ErrorData`, `ServerInfo`, `stdio()` |

### New Tables in unimatrix-store

| Table | Definition | Purpose |
|-------|-----------|---------|
| AGENT_REGISTRY | `TableDefinition<&str, &[u8]>` | Agent identity and trust records |
| AUDIT_LOG | `TableDefinition<u64, &[u8]>` | Append-only request audit trail |

These tables are created by `Store::open()` alongside the existing 8 tables (10 total). The table definitions live in `unimatrix-store/src/schema.rs`. The business logic (AgentRecord struct, AuditEvent struct, CRUD operations) lives in `unimatrix-server`.

### Exposed Interfaces (for vnc-002)

| Interface | Type | Purpose |
|-----------|------|---------|
| `UnimatrixServer` | struct, Clone | Server state holder; vnc-002 adds tool implementations |
| `AgentRegistry::has_capability()` | `fn(&self, &str, Capability) -> Result<bool>` | Capability query for enforcement |
| `AgentRegistry::require_capability()` | `fn(&self, &str, Capability) -> Result<()>` | Capability enforcement (returns error if denied) |
| `AuditLog::log_event()` | `fn(&self, AuditEvent) -> Result<()>` | Event recording |
| `ResolvedIdentity` | struct | Agent identity resolved from tool params |
| `ServerError` | enum | Extensible error type with MCP error mapping |
| `EmbedServiceHandle` | struct | Lazy-loading embed service wrapper |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `Store::open(path)` | `fn open(path: impl AsRef<Path>) -> Result<Store>` | unimatrix-store/src/db.rs |
| `Store::compact()` | `fn compact(&mut self) -> Result<()>` | unimatrix-store/src/db.rs |
| `VectorIndex::new(store, config)` | `fn new(store: Arc<Store>, config: VectorConfig) -> Result<VectorIndex>` | unimatrix-vector/src/index.rs |
| `VectorIndex::load(store, config, dir)` | `fn load(store: Arc<Store>, config: VectorConfig, dir: &Path) -> Result<VectorIndex>` | unimatrix-vector/src/persistence.rs |
| `VectorIndex::dump(dir)` | `fn dump(&self, dir: &Path) -> Result<()>` | unimatrix-vector/src/persistence.rs |
| `OnnxProvider::new(config)` | `fn new(config: EmbedConfig) -> Result<OnnxProvider>` | unimatrix-embed/src/onnx.rs |
| `StoreAdapter::new(store)` | `fn new(store: Arc<Store>) -> StoreAdapter` | unimatrix-core/src/adapters.rs |
| `VectorAdapter::new(index)` | `fn new(index: Arc<VectorIndex>) -> VectorAdapter` | unimatrix-core/src/adapters.rs |
| `EmbedAdapter::new(provider)` | `fn new(provider: Arc<dyn EmbeddingProvider>) -> EmbedAdapter` | unimatrix-core/src/adapters.rs |
| `AsyncEntryStore::new(inner)` | `fn new(inner: Arc<T>) -> AsyncEntryStore<T>` | unimatrix-core/src/async_wrappers.rs |
| `AsyncVectorStore::new(inner)` | `fn new(inner: Arc<T>) -> AsyncVectorStore<T>` | unimatrix-core/src/async_wrappers.rs |
| `AsyncEmbedService::new(inner)` | `fn new(inner: Arc<T>) -> AsyncEmbedService<T>` | unimatrix-core/src/async_wrappers.rs |
| `AGENT_REGISTRY` (new) | `TableDefinition<&str, &[u8]>` | unimatrix-store/src/schema.rs |
| `AUDIT_LOG` (new) | `TableDefinition<u64, &[u8]>` | unimatrix-store/src/schema.rs |
| `COUNTERS["next_audit_id"]` (new) | counter key in existing COUNTERS table | unimatrix-store/src/schema.rs |
| `ServerHandler::get_info()` | `fn get_info(&self) -> ServerInfo` | rmcp ServerHandler trait |
| `#[tool_router]` | proc macro on impl block | rmcp macros |
| `#[tool_handler]` | proc macro on ServerHandler impl | rmcp macros |
| `stdio()` | `fn stdio() -> impl Transport` | rmcp transport-io |
| `ServiceExt::serve(transport)` | `async fn serve(self, transport: T) -> RunningService` | rmcp |

## vnc-002 Enforcement Points Summary

This section explicitly documents where vnc-002 will plug in security checks. The architecture ensures each enforcement point is a single function call at the top of a tool handler -- no refactoring required.

| Enforcement Point | Location | What vnc-002 Adds | Interface |
|-------------------|----------|-------------------|-----------|
| **Input validation** | Top of each tool handler | Parameter length limits, pattern matching, control char rejection | `validate_{tool}_params(&params) -> Result<(), ServerError>` |
| **Category allowlist** | context_store handler | Reject unknown categories | `validate_category(&cat) -> Result<(), ServerError>` |
| **Content scanning** | context_store handler | ~50 injection patterns + PII detection | `scan_content(&content) -> Result<(), ServerError>` |
| **Capability check** | Top of each tool handler, after identity resolution | Read/Write/Search per tool | `registry.require_capability(&agent_id, cap)?` |
| **Output framing** | Response assembly in each read tool | Wrap entries with `_meta: "KNOWLEDGE_ENTRY_DATA"` | `frame_output(entries) -> CallToolResult` |
| **Audit detail** | End of each tool handler | Extended detail for denials and scanning results | `audit.log_event(event)` (already exists) |

## Implementation Order

```
C4 (project manager)       -- standalone, no other component deps
C10 (server error types)   -- standalone, needed by everything
  |
  v
C5 (agent registry)        -- depends on Store being available
C6 (audit log)             -- depends on Store being available
  |
  v
C7 (identity resolution)   -- depends on C5 (registry)
C8 (embed service handle)  -- standalone async wrapper
  |
  v
C2 (server core)           -- depends on C5, C6, C7, C8
C3 (tool stubs)            -- depends on C2 (server struct)
  |
  v
C9 (shutdown coordinator)  -- depends on C2 being defined
C1 (main entry point)      -- wires everything together
```
