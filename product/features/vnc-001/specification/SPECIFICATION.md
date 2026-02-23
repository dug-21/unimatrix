# Specification: vnc-001 MCP Server Core

## Objective

Create the MCP server binary that connects Unimatrix's knowledge engine to AI agents via stdio transport. This feature builds the server skeleton, security infrastructure (agent registry, audit log, agent identity), project data isolation, graceful shutdown, and tool registration pattern. It ships tool stubs that vnc-002 replaces with real implementations.

## Functional Requirements

### FR-01: MCP Server Binary

**FR-01a:** A `unimatrix-server` binary crate exists at `crates/unimatrix-server/` and compiles via `cargo build`.

**FR-01b:** The binary runs as an MCP server over stdio transport using rmcp 0.16.0, accepting JSON-RPC 2.0 messages on stdin and writing responses to stdout.

**FR-01c:** The server completes the MCP initialize handshake, returning `ServerInfo` with:
- `name`: `"unimatrix"`
- `version`: crate version string from Cargo.toml
- `instructions`: behavioral guidance text (see FR-02)
- `capabilities`: tools enabled

**FR-01d:** The server responds to MCP ping requests.

**FR-01e:** The server handles the complete MCP lifecycle: initialize -> tool calls -> shutdown.

### FR-02: Server Instructions

**FR-02a:** The `instructions` field in `ServerInfo` contains:
```
Unimatrix is this project's knowledge engine. Before starting implementation, architecture, or design tasks, search for relevant patterns and conventions using the context tools. Apply what you find. After discovering reusable patterns or making architectural decisions, store them for future reference. Do not store workflow state or process steps.
```

**FR-02b:** The instructions text is a compile-time constant, not read from configuration.

### FR-03: Project Root Detection

**FR-03a:** The server detects the project root by walking up from the current working directory looking for a `.git/` directory.

**FR-03b:** If no `.git/` directory is found between cwd and the filesystem root, the cwd itself is used as the project root.

**FR-03c:** The detected project root path is canonicalized (symlinks resolved) before use.

**FR-03d:** An optional `--project-dir` command-line argument overrides auto-detection, using the specified directory as the project root.

### FR-04: Project Data Directory

**FR-04a:** The project hash is computed as `SHA-256(canonical_project_root_path_as_utf8_string)`, taking the first 16 characters of the hex digest.

**FR-04b:** The project data directory is located at `~/.unimatrix/{project_hash}/`.

**FR-04c:** If the data directory does not exist, the server creates it along with the `vector/` subdirectory.

**FR-04d:** The home directory is resolved via the `dirs` crate (`dirs::home_dir()`).

**FR-04e:** The data directory layout is:
```
~/.unimatrix/{project_hash}/
  unimatrix.redb         -- redb database
  vector/                -- hnsw_rs dump files
```

### FR-05: Database Initialization

**FR-05a:** The server opens the redb database at `{data_dir}/unimatrix.redb` via `Store::open()`.

**FR-05b:** `Store::open()` creates 10 tables: the existing 8 (ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, COUNTERS) plus AGENT_REGISTRY and AUDIT_LOG.

**FR-05c:** The AGENT_REGISTRY table has key type `&str` (agent_id) and value type `&[u8]` (bincode-serialized AgentRecord).

**FR-05d:** The AUDIT_LOG table has key type `u64` (monotonic event_id) and value type `&[u8]` (bincode-serialized AuditEvent).

**FR-05e:** The COUNTERS table key `"next_audit_id"` is used for monotonic audit event ID generation.

### FR-06: Vector Index Initialization

**FR-06a:** On startup, the server checks for existing vector index files at `{data_dir}/vector/unimatrix-vector.meta`.

**FR-06b:** If the metadata file exists, the server loads the vector index via `VectorIndex::load(store, config, vector_dir)`.

**FR-06c:** If the metadata file does not exist (first run), the server creates an empty vector index via `VectorIndex::new(store, config)` with default `VectorConfig` (384 dimensions, 16 max connections, ef_construction=200).

### FR-07: Embedding Service Initialization

**FR-07a:** The embedding service is initialized asynchronously in a background tokio task.

**FR-07b:** On startup, the server creates an `EmbedServiceHandle` in `Loading` state and spawns a task that calls `OnnxProvider::new()` with default `EmbedConfig`.

**FR-07c:** When `OnnxProvider::new()` succeeds, the handle transitions to `Ready` state with the provider wrapped in an `EmbedAdapter`.

**FR-07d:** When `OnnxProvider::new()` fails, the handle transitions to `Failed` state with the error message logged.

**FR-07e:** Tool handlers that require embeddings (context_search) check handle readiness and return error code -32004 ("Embedding model is initializing") when not ready.

### FR-08: Agent Registry

**FR-08a:** `AgentRecord` contains: agent_id (String), trust_level (TrustLevel enum), capabilities (Vec\<Capability\>), allowed_topics (Option\<Vec\<String\>\>), allowed_categories (Option\<Vec\<String\>\>), enrolled_at (u64 unix seconds), last_seen_at (u64 unix seconds), active (bool).

**FR-08b:** `TrustLevel` is an enum with variants: System, Privileged, Internal, Restricted.

**FR-08c:** `Capability` is an enum with variants: Read, Write, Search, Admin.

**FR-08d:** On first run (no existing agents in registry), two default agents are bootstrapped:
- `"human"`: TrustLevel::Privileged, capabilities [Read, Write, Search, Admin]
- `"system"`: TrustLevel::System, capabilities [Read, Write, Search, Admin]

**FR-08e:** When a tool call arrives with an `agent_id` not present in the registry, a new agent is auto-enrolled with TrustLevel::Restricted and capabilities [Read, Search].

**FR-08f:** On every agent interaction, `last_seen_at` is updated to the current timestamp.

**FR-08g:** The registry exposes `has_capability(agent_id: &str, cap: Capability) -> Result<bool>` and `require_capability(agent_id: &str, cap: Capability) -> Result<()>` for capability queries.

**FR-08h:** `require_capability()` returns `ServerError::CapabilityDenied` when the agent lacks the required capability.

### FR-09: Audit Log

**FR-09a:** `AuditEvent` contains: event_id (u64), timestamp (u64 unix seconds), session_id (String), agent_id (String), operation (String), target_ids (Vec\<u64\>), outcome (Outcome enum), detail (String).

**FR-09b:** `Outcome` is an enum with variants: Success, Denied, Error, NotImplemented.

**FR-09c:** Event IDs are monotonically increasing, generated from `COUNTERS["next_audit_id"]`.

**FR-09d:** `log_event()` appends an event to AUDIT_LOG. No read, update, or delete operations are exposed in vnc-001.

**FR-09e:** Every tool call results in an audit event, even for stub responses (outcome: NotImplemented).

### FR-10: Agent Identity Resolution

**FR-10a:** Every tool parameter struct includes an `agent_id: Option<String>` field.

**FR-10b:** When `agent_id` is None or empty, it defaults to `"anonymous"`.

**FR-10c:** The server resolves the agent_id against the registry, auto-enrolling if unknown (per FR-08e).

**FR-10d:** Resolution produces a `ResolvedIdentity` struct with agent_id, trust_level, and capabilities.

**FR-10e:** The resolved identity is available to the tool handler for capability checks and is recorded in the audit event.

### FR-11: Tool Registration (Stubs)

**FR-11a:** Four tools are registered via rmcp's `#[tool_router]` macro: `context_search`, `context_lookup`, `context_store`, `context_get`.

**FR-11b:** Each tool has a JSON Schema for its parameters, generated via schemars from Rust structs:

| Tool | Required Params | Optional Params |
|------|----------------|-----------------|
| context_search | query: String | topic, category, tags, k, agent_id |
| context_lookup | (none -- at least one filter expected) | topic, category, tags, id, status, limit, agent_id |
| context_store | content: String, topic: String, category: String | tags, title, source, agent_id |
| context_get | id: i64 | agent_id |

**FR-11c:** Each tool's description matches the wording from ASS-007 interface specification.

**FR-11d:** Tool annotations are set:
- context_search: `readOnlyHint: true`
- context_lookup: `readOnlyHint: true`
- context_store: `readOnlyHint: false, destructiveHint: false`
- context_get: `readOnlyHint: true`

**FR-11e:** Stub implementations return a `CallToolResult` with text content: "Tool '{name}' is registered but not yet implemented. Full implementation ships in vnc-002."

**FR-11f:** Stub implementations resolve agent identity and log an audit event with outcome NotImplemented.

### FR-12: Graceful Shutdown

**FR-12a:** The server listens for SIGTERM and SIGINT signals via `tokio::signal`.

**FR-12b:** The server also shuts down when the MCP session closes (rmcp `waiting()` returns).

**FR-12c:** Shutdown sequence:
1. Cancel the MCP server (stop accepting new requests)
2. Wait up to 5 seconds for in-flight requests to complete
3. Call `VectorIndex::dump(&vector_dir)` through the Arc reference
4. Drop all Arc clones of Store (server, adapters, wrappers, registry, audit)
5. Call `Arc::try_unwrap(store)` -- if Ok, call `compact()`; if Err, log warning
6. Exit with code 0

**FR-12d:** If VectorIndex::dump() fails, log the error but continue shutdown (don't prevent compact).

**FR-12e:** If Store::compact() fails, log the error but exit with code 0 (compaction is optimization, not correctness).

### FR-13: Error Responses

**FR-13a:** Server errors map to rmcp `ErrorData` with numeric error codes and descriptive messages.

**FR-13b:** Error messages are actionable -- they tell agents what to do, not just what went wrong.

**FR-13c:** Error code ranges:
- -32001: Entry not found
- -32002: Invalid parameters (generic)
- -32003: Capability denied
- -32004: Embedding model initializing
- -32005: Tool not yet implemented
- -32603: Internal server error
- -32010 to -32019: Reserved for vnc-002 input validation errors
- -32020 to -32029: Reserved for vnc-002 content scanning errors

### FR-14: Foundation Crate Wiring

**FR-14a:** The server depends on `unimatrix-core` with the `async` feature enabled.

**FR-14b:** Store operations use `AsyncEntryStore<StoreAdapter>`.

**FR-14c:** Vector operations use `AsyncVectorStore<VectorAdapter>`.

**FR-14d:** Embed operations use the `EmbedServiceHandle` lazy wrapper (which internally uses `AsyncEmbedService<EmbedAdapter>` once ready).

**FR-14e:** All adapters are constructed from `Arc` references to concrete types.

## Non-Functional Requirements

### NFR-01: Startup Time

The server must complete MCP initialization (respond to `initialize` request) within 2 seconds of process start, excluding embedding model download time (which is deferred per FR-07).

### NFR-02: Memory Usage

Baseline memory usage (empty database, no entries, model not yet loaded) must be under 50 MB. This excludes the ONNX model which adds ~100 MB when loaded.

### NFR-03: Shutdown Time

Graceful shutdown (from signal to process exit) must complete within 10 seconds. The 5-second drain timeout plus compact/dump should fit within this budget.

### NFR-04: Code Quality

- `#![forbid(unsafe_code)]` on the server crate
- Rust edition 2024, MSRV 1.89
- All public types documented with rustdoc comments
- No `todo!()`, `unimplemented!()`, or placeholder code beyond the clearly-marked tool stubs

### NFR-05: Error Recoverability

The server must not panic on:
- Missing or corrupted vector index files (create fresh index)
- Embedding model download failure (transition to Failed state, serve non-embedding tools)
- Invalid tool parameters (return structured error, don't crash)
- Audit log write failure (log warning, continue serving)

## Acceptance Criteria with Verification Methods

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `unimatrix-server` binary crate compiles with `cargo build` | Build test |
| AC-02 | Server completes MCP initialize handshake with ServerInfo containing name, version, instructions | Integration test: connect stdio, send initialize, verify response |
| AC-03 | Instructions field contains behavioral guidance text per FR-02a | Unit test on get_info() |
| AC-04 | Project root detected by walking up to `.git/` directory | Unit test: create temp dir with `.git/`, verify detection |
| AC-05 | Project hash is deterministic SHA-256 of canonical path, first 16 hex chars | Unit test: same path produces same hash |
| AC-06 | Auto-initialization creates data directory, database, vector directory on first run | Integration test: start server in temp dir, verify files created |
| AC-07 | AGENT_REGISTRY table exists with AgentRecord schema | Unit test: open store, verify table accessible, roundtrip AgentRecord |
| AC-08 | Default agents "human" (Privileged) and "system" (System) bootstrapped on first run | Integration test: init registry, verify both agents present |
| AC-09 | Unknown agent_id auto-enrolls as Restricted with [Read, Search] | Unit test: resolve unknown agent, verify trust level and capabilities |
| AC-10 | AUDIT_LOG table exists with AuditEvent schema and monotonic IDs | Unit test: log multiple events, verify IDs are strictly increasing |
| AC-11 | Agent identity extracted from tool params and threaded through pipeline | Integration test: call tool stub with agent_id, verify audit event contains it |
| AC-12 | Graceful shutdown calls dump() and compact() in correct order | Integration test: start server, send shutdown signal, verify vector files written |
| AC-13 | Four tool stubs registered with correct names, schemas, and descriptions | Integration test: send tools/list, verify 4 tools with expected schemas |
| AC-14 | Server errors map to ErrorData with actionable messages | Unit test: map each error variant, verify code and message |
| AC-15 | Server uses unimatrix-core async wrappers for storage operations | Compile-time verification: AsyncEntryStore, AsyncVectorStore in dependencies |
| AC-16 | Data directory at `~/.unimatrix/{hash}/unimatrix.redb` and `~/.unimatrix/{hash}/vector/` | Integration test: verify path structure after init |
| AC-17 | Store::open() creates 10 tables (8 existing + AGENT_REGISTRY + AUDIT_LOG) | Unit test: open store, verify all 10 tables accessible |
| AC-18 | Server handles full MCP lifecycle: initialize -> tool calls -> shutdown | Integration test: full lifecycle with stdin/stdout pipes |
| AC-19 | All server code uses `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89 | Build test with MSRV, grep for unsafe |

## Domain Models

### Entity: UnimatrixServer

The central server struct. Holds shared references to all subsystems. Implements rmcp `ServerHandler`. Cloneable (all fields are Arc-wrapped).

### Entity: AgentRecord

An enrolled agent's identity and capabilities. Persisted in AGENT_REGISTRY. Mutable: `last_seen_at` updated on each interaction.

### Entity: AuditEvent

An immutable record of a single MCP request. Persisted in AUDIT_LOG. Never updated or deleted. Contains: who (agent_id), what (operation), when (timestamp), what was affected (target_ids), and what happened (outcome).

### Entity: ResolvedIdentity

Ephemeral, per-request struct. Produced by identity resolution from tool params + registry lookup. Consumed by capability checks and audit logging. Not persisted.

### Entity: ProjectPaths

Computed on startup. Contains all resolved filesystem paths for the project's data. Immutable after construction.

### Entity: EmbedServiceHandle

State machine wrapping the embedding provider. States: Loading -> Ready | Failed. Transitions are one-way.

### Value Object: TrustLevel

Ordered hierarchy: System > Privileged > Internal > Restricted. Determines default capabilities on enrollment.

### Value Object: Capability

Atomic permission unit: Read, Write, Search, Admin. Agents have a set of capabilities. Tool handlers check for specific capabilities.

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| **agent** | Any entity making tool calls via MCP. Identified by `agent_id` string. |
| **enrollment** | The process of creating an AgentRecord for a previously-unknown agent_id. |
| **trust level** | An agent's position in the trust hierarchy (System/Privileged/Internal/Restricted). |
| **capability** | A specific permission (Read/Write/Search/Admin) that determines what tools an agent can use. |
| **enforcement point** | A location in a tool handler where a security check will be inserted by vnc-002. |
| **project hash** | The first 16 hex chars of SHA-256(canonical_project_root_path). Identifies a project's data directory. |
| **lifecycle handles** | The concrete-typed Arc references (Store, VectorIndex) held for shutdown operations. |
| **compact** | redb operation that reclaims space from copy-on-write pages. Requires `&mut self`. |
| **dump** | hnsw_rs operation that persists the vector index to disk. Requires `&self`. |

## User Workflows

### Workflow 1: First Run (New Project)

1. User configures MCP: `claude mcp add --scope user --transport stdio unimatrix -- unimatrix-server`
2. Claude Code starts a session, spawning `unimatrix-server`
3. Server detects project root (`.git/`), computes hash
4. Server creates `~/.unimatrix/{hash}/`, opens database, creates empty vector index
5. Server bootstraps default agents in AGENT_REGISTRY
6. Server spawns background task for embedding model download
7. Server completes MCP initialize handshake
8. Agent makes a tool call -- gets stub response (vnc-001) or real response (vnc-002)
9. Session ends -- server dumps vector index, compacts database, exits

### Workflow 2: Subsequent Run (Existing Project)

1. Claude Code starts session, spawning `unimatrix-server`
2. Server detects project root, computes hash, finds existing data directory
3. Server opens existing database, loads vector index from dump files
4. Server verifies existing agents in registry (no bootstrap needed)
5. Server completes MCP initialize, agent makes tool calls
6. Session ends with graceful shutdown

### Workflow 3: Agent Identity Flow

1. Agent calls `context_store` with `agent_id: "uni-architect"`
2. Server extracts `agent_id` from params
3. Server looks up "uni-architect" in AGENT_REGISTRY
4. Not found -- auto-enrolls as Restricted with [Read, Search] capabilities
5. [vnc-002: capability check would deny Write here; vnc-001 stubs skip this]
6. Server logs audit event with agent_id="uni-architect", outcome=NotImplemented
7. Server returns stub response

## Constraints

- rmcp pinned to `=0.16.0`
- Tokio async runtime (required by rmcp)
- Rust edition 2024, MSRV 1.89
- `#![forbid(unsafe_code)]`
- No hardcoded agent roles in server code
- `Store::compact()` requires `&mut self` -- shutdown must manage Arc lifecycle
- anndists local patch maintained in workspace
- Single project per server process
- Model download latency on first run (deferred, not blocking)

## Dependencies

| Dependency | Version | Purpose |
|-----------|---------|---------|
| unimatrix-core | path (with `async` feature) | Traits, adapters, async wrappers, domain types |
| rmcp | =0.16.0 (server, transport-io, macros) | MCP protocol, tool macros, stdio transport |
| tokio | 1 (full features) | Async runtime, signal handling, spawn_blocking |
| schemars | 1 | JSON Schema generation for tool params |
| serde | 1 | Serialization |
| serde_json | 1 | JSON handling for MCP |
| sha2 | 0.10 | SHA-256 for project path hashing |
| dirs | 6 | Home directory detection |
| tracing | 0.1 | Structured logging |
| tracing-subscriber | 0.3 | Log output formatting |
| clap | 4 | Command-line argument parsing |
| chrono | 0.4 | Timestamp generation |

## NOT in Scope

- Tool implementations (context_search, context_lookup, context_store, context_get) -- vnc-002
- Input validation and parameter sanitization -- vnc-002
- Content scanning and injection detection -- vnc-002
- Category allowlist enforcement -- vnc-002
- Capability enforcement on tool calls -- vnc-002 (infrastructure built here)
- Output framing for knowledge entries -- vnc-002
- Near-duplicate detection -- vnc-002
- HTTP/SSE transport -- future
- CLI commands (unimatrix init, status, export) -- nan-001
- Multi-project support -- dsn-001/dsn-002
- Configuration file (~/.unimatrix/config.toml) -- dsn-004
- Confidence computation -- crt-002
- MCP Resources, Prompts -- future vnc features
