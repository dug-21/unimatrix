# Implementation Brief: vnc-001 MCP Server Core

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/vnc-001/SCOPE.md |
| Architecture | product/features/vnc-001/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-001/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-001/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-001/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| project | pseudocode/project.md | test-plan/project.md |
| error | pseudocode/error.md | test-plan/error.md |
| registry | pseudocode/registry.md | test-plan/registry.md |
| audit | pseudocode/audit.md | test-plan/audit.md |
| identity | pseudocode/identity.md | test-plan/identity.md |
| embed-handle | pseudocode/embed-handle.md | test-plan/embed-handle.md |
| server | pseudocode/server.md | test-plan/server.md |
| tools | pseudocode/tools.md | test-plan/tools.md |
| shutdown | pseudocode/shutdown.md | test-plan/shutdown.md |
| main | pseudocode/main.md | test-plan/main.md |

## Goal

Create the `unimatrix-server` binary crate that runs as an MCP server over stdio transport, exposing Unimatrix's knowledge engine to AI agents. Build the security infrastructure (agent registry, audit log, agent identity resolution), project data isolation, graceful shutdown coordination, and tool registration pattern with stubs that vnc-002 replaces with real implementations.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| MCP SDK | rmcp 0.16.0 pinned exactly, features: server, transport-io, macros | ASS-002 evaluation | architecture/ADR-001-rmcp-stdio-transport.md |
| Transport | stdio only (no HTTP) | SCOPE.md constraint | architecture/ADR-001-rmcp-stdio-transport.md |
| Crate structure | Binary crate `crates/unimatrix-server/` with lib.rs for testing | Workspace convention | architecture/ADR-002-binary-crate-structure.md |
| Agent identity | Self-reported `agent_id` tool param, transport-agnostic internal pipeline | ADR-003, ASS-004 | architecture/ADR-003-agent-identity-via-tool-params.md |
| Project isolation | SHA-256(canonical_path)[..16], data at ~/.unimatrix/{hash}/ | SCOPE.md Q1 resolution | architecture/ADR-004-project-isolation-via-path-hash.md |
| Shutdown strategy | dump() through Arc, Arc::try_unwrap for compact, graceful degradation | SCOPE.md Q2 resolution | architecture/ADR-005-shutdown-via-arc-try-unwrap.md |
| Embedding init | Lazy-load in background task, EmbedServiceHandle state machine | SCOPE.md Q3 resolution | architecture/ADR-006-lazy-embed-initialization.md |
| Security enforcement | Explicit enforcement points in tool handlers, documented for vnc-002 | Human directive | architecture/ADR-007-enforcement-point-architecture.md |
| New table creation | Extend Store::open() in unimatrix-store for AGENT_REGISTRY + AUDIT_LOG | SCOPE.md Q1 resolution | -- |
| Audit log key | u64 monotonic counter via COUNTERS["next_audit_id"] | SCOPE.md Q4 resolution | -- |

## Files to Create/Modify

### New Files (unimatrix-server crate)

| Path | Purpose |
|------|---------|
| `crates/unimatrix-server/Cargo.toml` | Binary crate manifest with rmcp, tokio, unimatrix-core deps |
| `crates/unimatrix-server/src/main.rs` | Entry point: arg parsing, init sequence, serve, signal handling |
| `crates/unimatrix-server/src/lib.rs` | Module declarations, pub exports for integration testing |
| `crates/unimatrix-server/src/server.rs` | UnimatrixServer struct, ServerHandler impl, get_info() |
| `crates/unimatrix-server/src/tools.rs` | #[tool_router] impl with 4 tool stubs, param types |
| `crates/unimatrix-server/src/project.rs` | ProjectPaths, project root detection, data dir management |
| `crates/unimatrix-server/src/registry.rs` | AgentRegistry, AgentRecord, TrustLevel, Capability |
| `crates/unimatrix-server/src/audit.rs` | AuditLog, AuditEvent, Outcome, event recording |
| `crates/unimatrix-server/src/identity.rs` | ResolvedIdentity, agent extraction from params, registry lookup |
| `crates/unimatrix-server/src/embed_handle.rs` | EmbedServiceHandle, EmbedState, lazy loading wrapper |
| `crates/unimatrix-server/src/shutdown.rs` | LifecycleHandles, shutdown sequence, signal handling |
| `crates/unimatrix-server/src/error.rs` | ServerError enum, MCP error mapping, error codes |

### Modified Files

| Path | Change |
|------|--------|
| `crates/unimatrix-store/src/schema.rs` | Add AGENT_REGISTRY and AUDIT_LOG table definitions |
| `crates/unimatrix-store/src/db.rs` | Add table creation for AGENT_REGISTRY and AUDIT_LOG in Store::open() |
| `Cargo.toml` (workspace root) | Already includes `crates/*` -- no change needed if crate is in `crates/` |

## Data Structures

### UnimatrixServer (server.rs)

```rust
#[derive(Clone)]
pub struct UnimatrixServer {
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    registry: Arc<AgentRegistry>,
    audit: Arc<AuditLog>,
    server_info: ServerInfo,
}
```

### AgentRecord (registry.rs)

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
pub enum TrustLevel { System, Privileged, Internal, Restricted }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability { Read, Write, Search, Admin }
```

### AuditEvent (audit.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub event_id: u64,
    pub timestamp: u64,
    pub session_id: String,
    pub agent_id: String,
    pub operation: String,
    pub target_ids: Vec<u64>,
    pub outcome: Outcome,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Outcome { Success, Denied, Error, NotImplemented }
```

### ResolvedIdentity (identity.rs)

```rust
pub struct ResolvedIdentity {
    pub agent_id: String,
    pub trust_level: TrustLevel,
    pub capabilities: Vec<Capability>,
}
```

### ProjectPaths (project.rs)

```rust
pub struct ProjectPaths {
    pub project_root: PathBuf,
    pub project_hash: String,
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub vector_dir: PathBuf,
}
```

### EmbedServiceHandle (embed_handle.rs)

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

### LifecycleHandles (shutdown.rs)

```rust
pub struct LifecycleHandles {
    pub store: Arc<Store>,
    pub vector_index: Arc<VectorIndex>,
    pub vector_dir: PathBuf,
    pub registry: Arc<AgentRegistry>,
    pub audit: Arc<AuditLog>,
}
```

### Tool Parameter Types (tools.rs)

```rust
#[derive(Deserialize, JsonSchema)]
pub struct SearchParams {
    pub query: String,
    pub topic: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub k: Option<i64>,
    pub agent_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct LookupParams {
    pub topic: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub id: Option<i64>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub agent_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct StoreParams {
    pub content: String,
    pub topic: String,
    pub category: String,
    pub tags: Option<Vec<String>>,
    pub title: Option<String>,
    pub source: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetParams {
    pub id: i64,
    pub agent_id: Option<String>,
}
```

### Table Definitions (unimatrix-store/src/schema.rs -- additions)

```rust
pub const AGENT_REGISTRY: TableDefinition<&str, &[u8]> =
    TableDefinition::new("agent_registry");

pub const AUDIT_LOG: TableDefinition<u64, &[u8]> =
    TableDefinition::new("audit_log");
```

## Function Signatures

### server.rs

```rust
impl ServerHandler for UnimatrixServer {
    fn get_info(&self) -> ServerInfo;
}

impl UnimatrixServer {
    pub fn new(/* all subsystem Arcs */) -> Self;
    pub async fn resolve_agent(&self, agent_id: &Option<String>) -> Result<ResolvedIdentity, ServerError>;
}
```

### project.rs

```rust
pub fn detect_project_root(override_dir: Option<&Path>) -> io::Result<PathBuf>;
pub fn compute_project_hash(project_root: &Path) -> String;
pub fn ensure_data_directory(project_root: &Path) -> io::Result<ProjectPaths>;
```

### registry.rs

```rust
impl AgentRegistry {
    pub fn new(store: Arc<Store>) -> Result<Self, ServerError>;
    pub fn bootstrap_defaults(&self) -> Result<(), ServerError>;
    pub fn resolve_or_enroll(&self, agent_id: &str) -> Result<AgentRecord, ServerError>;
    pub fn has_capability(&self, agent_id: &str, cap: Capability) -> Result<bool, ServerError>;
    pub fn require_capability(&self, agent_id: &str, cap: Capability) -> Result<(), ServerError>;
    pub fn update_last_seen(&self, agent_id: &str) -> Result<(), ServerError>;
}
```

### audit.rs

```rust
impl AuditLog {
    pub fn new(store: Arc<Store>) -> Self;
    pub fn log_event(&self, event: AuditEvent) -> Result<(), ServerError>;
}
```

### identity.rs

```rust
pub fn extract_agent_id(agent_id: &Option<String>) -> String;
pub async fn resolve_identity(registry: &AgentRegistry, agent_id: &str) -> Result<ResolvedIdentity, ServerError>;
```

### embed_handle.rs

```rust
impl EmbedServiceHandle {
    pub fn new() -> Arc<Self>;
    pub fn start_loading(self: &Arc<Self>, config: EmbedConfig);
    pub fn embed_entry(&self, title: &str, content: &str) -> Result<Vec<f32>, ServerError>;
    pub fn is_ready(&self) -> bool;
}
```

### shutdown.rs

```rust
pub async fn graceful_shutdown(
    handles: LifecycleHandles,
    server: impl Future<Output = ()>,
) -> Result<(), ServerError>;
```

### error.rs

```rust
pub enum ServerError {
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

impl From<ServerError> for ErrorData { ... }
```

## Constraints

- rmcp pinned to `=0.16.0`
- Tokio async runtime (required by rmcp)
- Rust edition 2024, MSRV 1.89
- `#![forbid(unsafe_code)]`
- No hardcoded agent roles
- Store::compact() requires `&mut self` -- Arc lifecycle managed via try_unwrap
- Single project per server process
- anndists local patch in workspace Cargo.toml

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| unimatrix-core | path, features = ["async"] | Traits, adapters, async wrappers |
| rmcp | =0.16.0, features = ["server", "transport-io", "macros"] | MCP SDK |
| tokio | 1, features = ["full"] | Async runtime, signals |
| schemars | 1 | JSON Schema for tool params |
| serde | 1 | Serialization |
| serde_json | 1 | JSON |
| sha2 | 0.10 | Project path hashing |
| dirs | 6 | Home directory |
| tracing | 0.1 | Structured logging |
| tracing-subscriber | 0.3 | Log formatting |
| clap | 4 | CLI args |
| chrono | 0.4 | Timestamps |
| bincode | 2, features = ["serde"] | AgentRecord/AuditEvent serialization |

## Implementation Order

```
1. Store table extension (unimatrix-store) -- AGENT_REGISTRY + AUDIT_LOG definitions
2. project.rs -- standalone, no server deps
3. error.rs -- standalone, needed by everything
4. registry.rs -- depends on Store
5. audit.rs -- depends on Store
6. identity.rs -- depends on registry
7. embed_handle.rs -- standalone async wrapper
8. server.rs -- depends on 4, 5, 6, 7
9. tools.rs -- depends on server
10. shutdown.rs -- depends on server
11. main.rs -- wires everything
```

## NOT in Scope

- Tool implementations (vnc-002)
- Input validation, content scanning, capability enforcement (vnc-002)
- Output framing (vnc-002)
- Near-duplicate detection (vnc-002)
- HTTP transport (future)
- CLI commands (nan-001)
- Multi-project support (dsn-001/dsn-002)
- Configuration file (dsn-004)
- Confidence computation (crt-002)

## Alignment Status

**5 PASS, 1 WARN, 0 VARIANCE, 0 FAIL**

The single WARN is for the `clap` dependency addition for CLI argument parsing, which was not explicitly mentioned in SCOPE.md but is minimal and standard practice. No variances require human approval.
