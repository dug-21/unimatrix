# vnc-001: MCP Server Core

## Problem Statement

Unimatrix has a complete foundation layer (storage, vector search, embeddings, core traits) but no way for AI agents to access it. The four foundation crates (unimatrix-store, unimatrix-vector, unimatrix-embed, unimatrix-core) expose Rust library APIs only. Without an MCP server, no agent can search, store, or retrieve knowledge -- the core value proposition remains inaccessible.

This is Milestone 2's foundational feature. It creates the MCP server binary, the stdio transport layer, the security infrastructure (agent registry, audit log), the project data directory strategy, and the graceful shutdown coordination. The v0.1 tool implementations (context_search, context_lookup, context_store, context_get) are vnc-002's responsibility -- this feature ships the server skeleton that vnc-002 builds on.

## Goals

1. Create a `unimatrix-server` binary crate that runs as an MCP server over stdio transport using rmcp 0.16, implementing the ServerHandler trait with `get_info()` returning server name, version, capabilities, and an `instructions` field for behavioral driving.
2. Wire the three foundation traits (EntryStore, VectorStore, EmbedService) into the server via unimatrix-core's async wrappers and domain adapters, establishing the runtime dependency graph: server -> core -> store/vector/embed.
3. Implement project isolation via hashed data directories at `~/.unimatrix/{project_hash}/`, where `project_hash` is a deterministic SHA-256 of the project root path. Auto-detect project root from the current working directory (walk up to find `.git/` or fallback to cwd). Auto-initialize on first run (create directory, open database, create vector index, load embedding model).
4. Implement an AGENT_REGISTRY redb table for agent identity and capability tracking. Bootstrap with default `"human"` (Privileged) and `"system"` (System) agents. Unknown agents auto-enroll as Restricted (read-only). Expose trust levels: System, Privileged, Internal, Restricted.
5. Implement an AUDIT_LOG redb table (append-only) that records every MCP request with agent_id, operation, target_ids, outcome, and timestamp. No delete or update operations on this table.
6. Implement the agent identification flow: extract `agent_id` from tool call parameters, look up in AGENT_REGISTRY, thread agent identity through the request handling pipeline so downstream tool handlers (vnc-002) can enforce capability checks and populate `created_by`/`modified_by`.
7. Implement graceful shutdown coordination: on SIGTERM/SIGINT or MCP session close, call `Store::compact()` and `VectorIndex::dump()` to persist all data, then exit cleanly.
8. Define the tool registration infrastructure (rmcp `#[tool_router]` / `#[tool_handler]` pattern) with placeholder tool stubs that return "not yet implemented" -- providing the wiring that vnc-002 replaces with real implementations.
9. Implement structured error responses that give agents actionable guidance (not raw error strings), using rmcp's `ErrorData` type with error codes and descriptive messages.

## Non-Goals

- **No tool implementations.** context_search, context_lookup, context_store, and context_get are vnc-002. This feature provides the server skeleton, transport, security infrastructure, and tool registration pattern only. Tool stubs exist solely to validate the wiring.
- **No input validation or content scanning.** Parameter validation, injection pattern detection, PII scanning, category allowlists, and content size limits are vnc-002 responsibilities that build on vnc-001's security infrastructure.
- **No output framing.** Wrapping returned entries with data markers to distinguish knowledge content from instructions is vnc-002.
- **No capability enforcement on tool calls.** vnc-001 builds the AGENT_REGISTRY and the lookup pipeline. vnc-002 adds per-tool capability checks (Read for search/lookup/get, Write for store).
- **No HTTP/SSE transport.** stdio only. HTTP transport is a future concern when the deployment context demands it.
- **No CLI commands.** `unimatrix init`, `unimatrix status`, etc. are nan-001. This feature runs as a stdio server invoked by the MCP client.
- **No multi-project support.** Each server instance serves one project. Multi-project coordination is dsn-001/dsn-002.
- **No configuration file.** Server behavior is determined by sensible defaults and compile-time constants. `~/.unimatrix/config.toml` is dsn-004.
- **No near-duplicate detection.** Requires vector search integration (vnc-002).
- **No confidence computation.** The confidence field exists on EntryRecord but the formula is crt-002.

## Background Research

### Prior Spike Research

Six completed spikes and one security analysis inform this feature:

**ASS-002 (MCP Protocol Deep Dive)** at `product/research/ass-002/`:
- Evaluated rmcp 0.16 as the official Rust MCP SDK (1.14M downloads/month, 139 contributors, official repo)
- Documented the `#[tool_router]` + `#[tool_handler]` pattern for tool definition
- Confirmed stdio transport via `(stdin(), stdout())` tuple
- Identified `ServerInfo.instructions` field as the primary behavioral driving mechanism (70-85% compliance)
- Dependencies: rmcp pulls in tokio, serde, serde_json, schemars, futures, tracing, thiserror
- Risk: pre-1.0 SDK with frequent breaking changes -- pin exact version

**ASS-004 (Context Injection Patterns)** at `product/research/ass-004/`:
- Three hard design constraints: (1) no hardcoded agent roles -- roles are DATA, (2) deterministic vs semantic retrieval driven by `query` param presence, (3) generic query model `{ topic, category, query }`
- These constraints shape the tool parameter design (vnc-002) but vnc-001 must not preclude them in the server architecture

**ASS-006 (Config Surface)** at `product/research/ass-006/`:
- Three-tier integration model: (1) zero-config via `claude mcp add`, (2) CLAUDE.md reinforcement, (3) full multi-agent integration
- Server `instructions` field wording established for Tier 1
- User-scoped MCP servers (`--scope user`) required for custom subagent compatibility

**ASS-007 (Interface Specification)** at `product/research/ass-007/`:
- Defined v0.1 tool signatures: context_search, context_lookup, context_store, context_get
- Defined v0.2 tool signatures: context_correct, context_deprecate, context_status, context_briefing
- Specified response format: compact markdown in `content`, JSON in `structuredContent`
- Established tool annotations (readOnlyHint, destructiveHint, idempotentHint)

**MCP Security Analysis** at `product/research/mcp-security/`:
- Unimatrix faces amplified knowledge poisoning risks (OWASP ASI06) because entries propagate across feature cycles
- AGENT_REGISTRY and AUDIT_LOG must exist before the first MCP-written entry (vnc-002)
- Agent identification flow via `agent_id` tool parameter for stdio transport, designed transport-agnostic for future `_meta` field and OAuth 2.1
- Trust hierarchy: System > Privileged > Internal > Restricted
- Unknown agents auto-enroll as Restricted

### Existing Codebase Patterns

**Foundation crates** (all complete, tested, merged to main):
- `unimatrix-store`: redb-backed storage, 8 tables, `Store` wrapper (Send + Sync), sync API
- `unimatrix-vector`: hnsw_rs integration, `VectorIndex` wrapper, requires `Arc<Store>` for VECTOR_MAP coordination
- `unimatrix-embed`: ONNX embedding, `OnnxProvider` implementing `EmbeddingProvider` trait, model cache at `~/.cache/unimatrix/models/`
- `unimatrix-core`: 3 traits (EntryStore, VectorStore, EmbedService), 3 adapters (StoreAdapter, VectorAdapter, EmbedAdapter), feature-gated async wrappers, unified CoreError

**Key integration points**:
- `Store::open(path)` creates or opens a database file. Requires mutable access for `compact()`.
- `VectorIndex::new(config, store)` requires `Arc<Store>` for VECTOR_MAP coordination. `dump()` persists to disk.
- `OnnxProvider::new(config)` loads model on first use from `~/.cache/unimatrix/models/`
- Async wrappers: `AsyncEntryStore::new(Arc::new(adapter))`, `AsyncVectorStore::new(Arc::new(adapter))`, `AsyncEmbedService::new(Arc::new(adapter))`
- `Store` needs `&mut self` for `compact()` -- requires special handling in shutdown (drop other Arc references or use interior mutability pattern)

**Workspace structure**: Cargo workspace at repo root, crates in `crates/`, edition 2024, MSRV 1.89, `#![forbid(unsafe_code)]`

### Technical Constraints Discovered

- **rmcp 0.16 requires tokio async runtime.** The server binary runs a tokio runtime. Foundation crates are synchronous; the async wrappers bridge via `spawn_blocking`.
- **Store::compact() requires `&mut self`.** This conflicts with `Arc<Store>` shared across the server. The shutdown sequence must ensure all tool handlers have completed and Arc references are dropped before calling compact. Alternatively, use `Mutex<Store>` with a shutdown lock, or make compact the last operation after the server stops accepting requests.
- **VectorIndex::dump() basename quirk.** From nxs-002: the dump path's actual basename must be captured from `file_dump` output. The server must manage the vector index file paths within the project data directory.
- **ONNX model download on first use.** `OnnxProvider::new()` may trigger a model download from HuggingFace Hub. This can take significant time on first run. The server should initialize the embedding provider before starting to accept MCP requests.
- **anndists local patch.** The workspace requires `[patch.crates-io] anndists = { path = "patches/anndists" }` for edition 2024 compatibility.
- **AGENT_REGISTRY and AUDIT_LOG are new redb tables.** They live in the same database file as the existing 8 tables. `Store::open()` creates all tables on open -- vnc-001 must either extend Store to create these tables or create them separately in the server initialization.
- **Project root detection.** Walk up from cwd looking for `.git/` directory. Hash the canonical (resolved) path with SHA-256 for the data directory name. This must be deterministic across sessions.

## Proposed Approach

### Crate Structure

Create a `unimatrix-server` binary crate in `crates/unimatrix-server/`. This is a binary (not library), producing the `unimatrix-server` executable that Claude Code invokes via `claude mcp add`.

Dependencies:
- `unimatrix-core` (with `async` feature enabled) -- provides traits, adapters, async wrappers, and re-exported domain types
- `rmcp = { version = "=0.16.0", features = ["server", "transport-io", "macros"] }` -- MCP SDK
- `tokio = { version = "1", features = ["full"] }` -- async runtime
- `schemars` -- JSON Schema generation for tool parameters
- `serde`, `serde_json` -- serialization
- `sha2` -- SHA-256 for project path hashing and content hashing
- `dirs` -- home directory detection for `~/.unimatrix/`
- `tracing`, `tracing-subscriber` -- structured logging

### Server Architecture

```
unimatrix-server (binary)
  main.rs           -- entry point: arg parsing, project detection, init, serve
  server.rs         -- UnimatrixServer struct, ServerHandler impl, tool stubs
  project.rs        -- project root detection, data directory management
  registry.rs       -- AGENT_REGISTRY table operations, trust levels, agent enrollment
  audit.rs          -- AUDIT_LOG table operations, event recording
  identity.rs       -- agent identification extraction and threading
  shutdown.rs       -- graceful shutdown coordination (compact + dump)
  error.rs          -- server-specific error types, MCP error mapping
```

### Data Directory Layout

```
~/.unimatrix/
  {project_hash}/          -- per-project data directory
    unimatrix.redb         -- redb database (entries + indexes + agent registry + audit log)
    vector/                -- hnsw_rs dump files
      index.hnsw.data      -- vector data
      index.hnsw.graph     -- graph structure
```

### Initialization Sequence

1. Detect project root (walk up to `.git/`, fallback to cwd)
2. Compute `project_hash = SHA-256(canonical_path)[..16]` (first 16 hex chars)
3. Create `~/.unimatrix/{project_hash}/` if it does not exist
4. Open Store at `~/.unimatrix/{project_hash}/unimatrix.redb`
5. Create AGENT_REGISTRY and AUDIT_LOG tables (extending Store's table set or via separate init)
6. Bootstrap default agents (`"human"` as Privileged, `"system"` as System)
7. Open or create VectorIndex at `~/.unimatrix/{project_hash}/vector/`
8. Initialize OnnxProvider (triggers model download on first run)
9. Wrap in adapters and async wrappers
10. Start MCP server on stdio transport
11. Register signal handlers for graceful shutdown

### Security Infrastructure

**Agent Registry**: AGENT_REGISTRY table in the same redb database. Key: agent_id string. Value: bincode-serialized AgentRecord with trust_level, capabilities, enrollment metadata.

**Trust Levels**: System (server internals), Privileged (human user), Internal (orchestrator agents), Restricted (unknown/worker agents).

**Audit Log**: AUDIT_LOG table, append-only. Key: monotonic nanosecond timestamp (u128 if redb supports, else compound key). Value: bincode-serialized AuditEvent. No delete/update exposed.

**Agent ID Flow**: Every tool call extracts `agent_id` from params -> looks up in registry -> threads identity through handler -> populates `created_by`/`modified_by` on mutations -> writes audit event.

## Acceptance Criteria

- AC-01: A `unimatrix-server` binary crate exists at `crates/unimatrix-server/` that compiles with `cargo build` and produces an executable binary.
- AC-02: The binary runs as an MCP server over stdio transport using rmcp 0.16, completing the MCP initialization handshake and returning `ServerInfo` with server name "unimatrix", version string, and a non-empty `instructions` field.
- AC-03: The server's `instructions` field contains behavioral guidance text that directs agents to search before working and store knowledge after making decisions (per ASS-006 research).
- AC-04: The server auto-detects the project root by walking up from the current working directory to find a `.git/` directory, falling back to the cwd if none is found.
- AC-05: The server computes a deterministic project hash (SHA-256 of the canonical project root path, truncated to 16 hex characters) and uses it to create and manage a data directory at `~/.unimatrix/{project_hash}/`.
- AC-06: On first run for a project, the server auto-initializes: creates the data directory, opens the redb database, creates the vector index directory, and loads the embedding model.
- AC-07: The AGENT_REGISTRY redb table exists in the database, storing AgentRecord structs keyed by agent_id string, with fields for trust_level, capabilities, allowed_topics, allowed_categories, enrolled_at, last_seen_at, and active status.
- AC-08: On first run, the server bootstraps two default agents: `"human"` with Privileged trust and `"system"` with System trust.
- AC-09: When a tool call arrives with an unknown `agent_id`, the agent is auto-enrolled in AGENT_REGISTRY with Restricted trust (read-only capabilities).
- AC-10: The AUDIT_LOG redb table exists in the database, storing AuditEvent structs with timestamp key, recording agent_id, operation, target_ids, and outcome for every request. No delete or update operations are exposed on this table.
- AC-11: Agent identity is extracted from tool call parameters and threaded through the request handling pipeline, available to downstream tool handlers for capability checks and attribution.
- AC-12: On SIGTERM, SIGINT, or MCP session close, the server executes graceful shutdown: completes in-flight requests, calls `Store::compact()` and `VectorIndex::dump()`, then exits with code 0.
- AC-13: The server registers four tool stubs (context_search, context_lookup, context_store, context_get) via rmcp's tool registration system. Each stub returns a structured "not yet implemented" response. Tool definitions include descriptions and JSON Schema for parameters.
- AC-14: Server errors are mapped to rmcp `ErrorData` responses with error codes and descriptive messages that give agents actionable guidance.
- AC-15: The server binary links against unimatrix-core (with async feature), using AsyncEntryStore, AsyncVectorStore, and AsyncEmbedService wrappers for all storage operations.
- AC-16: The project data directory structure follows `~/.unimatrix/{project_hash}/unimatrix.redb` for the database and `~/.unimatrix/{project_hash}/vector/` for hnsw_rs dump files.
- AC-17: The AGENT_REGISTRY and AUDIT_LOG tables are created alongside the existing 8 tables during database initialization, extending the Store's table creation to 10 tables total.
- AC-18: The server handles the complete MCP lifecycle: initialize -> tool calls -> shutdown, including proper response to ping requests.
- AC-19: All server code follows workspace conventions: `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89.

## Constraints

- **rmcp 0.16.0** pinned exactly (`=0.16.0`) to avoid breaking changes from a pre-1.0 SDK.
- **tokio runtime** required by rmcp and the async wrappers. The binary targets `#[tokio::main]`.
- **Rust edition 2024, MSRV 1.89** per workspace conventions.
- **`#![forbid(unsafe_code)]`** per workspace conventions.
- **No hardcoded agent roles** in server code (ASS-004 Constraint 1). The AGENT_REGISTRY stores trust levels and capabilities, not role-specific behavior.
- **Store::compact() requires `&mut self`** -- shutdown sequence must handle Arc<Store> lifecycle carefully.
- **anndists local patch** must be maintained in workspace Cargo.toml.
- **Model download latency** -- OnnxProvider initialization may take 30+ seconds on first run (model download from HuggingFace Hub). Server must complete MCP initialization before model is ready and handle early tool calls gracefully.
- **Single project per server instance** -- each `unimatrix-server` process serves one project, determined at startup from cwd.

## Resolved Open Questions

1. **AGENT_REGISTRY and AUDIT_LOG table creation**: RESOLVED -- Extend `Store::open()` in unimatrix-store to create these tables. The store manages table creation; the server owns the logic. This keeps the abstraction clean.
2. **Store::compact() and Arc**: RESOLVED -- Use `Arc::try_unwrap()` after draining. Shutdown sequence: stop accepting -> drain in-flight -> `VectorIndex::dump()` (works through Arc since dump takes &self) -> drop all Arc clones -> `Arc::try_unwrap(Store)` -> `compact()` -> exit. If try_unwrap fails (leaked reference), log warning and skip compact -- redb is crash-safe. Note: VectorIndex::dump() takes &self, not &mut self. Neither dump() nor compact() is on the core traits -- server holds concrete type references for lifecycle management alongside trait objects for tool handlers.
3. **Model download during MCP init**: RESOLVED -- Start MCP immediately, lazy-load the embedding model. Reads (context_lookup, context_get) don't need embeddings. Return "embedding model initializing" for context_search on first call. Don't block MCP init for model download.
4. **AUDIT_LOG key type**: RESOLVED -- Use `u64` monotonic counter (same pattern as entry IDs). Timestamp goes in the AuditEvent value, not the key. Simple, ordered, no collision issues.

## Tracking

https://github.com/dug-21/unimatrix/issues/9
