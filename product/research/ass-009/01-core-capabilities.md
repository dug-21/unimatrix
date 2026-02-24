# ASS-009: Core Capabilities Analysis

## Purpose

Extract and assess the domain-agnostic capabilities of Unimatrix's implemented codebase. Determine what constitutes a "portable kernel" that could serve domains beyond multi-agent software development orchestration.

---

## Capability Taxonomy

### 1. Structured Knowledge Storage

**What it does.** Persists typed records (title, content, topic, category, tags, source, status, confidence, timestamps) in an embedded transactional database (redb). Records are serialized via bincode v2 serde path. All writes are atomic across a primary table and 5 secondary index tables. Schema evolution uses append-only fields with scan-and-rewrite migration on open.

**Implementation.** `unimatrix-store`: `Store`, `EntryRecord`, `NewEntry`, `ENTRIES` table, `COUNTERS` table. 10 tables total. `#![forbid(unsafe_code)]`. `Send + Sync`. Single-file database.

**Domain-specific elements.** The category allowlist in the server layer has 8 initial values (`outcome`, `lesson-learned`, `decision`, `convention`, `pattern`, `procedure`, `duties`, `reference`) -- these are software-process oriented but runtime-extensible via `CategoryAllowlist::add_category()`. The `feature_cycle` field on `EntryRecord` presumes feature-based workflows. The `source` field has no fixed semantics.

**What makes it general.** The record schema is a generic knowledge unit: {title, content, topic, category, tags, metadata}. This is the same shape as a note-taking app, a wiki entry, a compliance record, or a research annotation. Topic, category, and tags are user-defined strings with no hardcoded values at the storage layer. The query model (`QueryFilter`: topic, category, tags, status, time_range) is pure set intersection -- no domain logic.

**Portability: HIGH.** The storage layer is fully domain-agnostic. The only coupling is the `feature_cycle` field name on `EntryRecord`, which is effectively a free-form string that any domain could repurpose (e.g., "project phase", "campaign", "sprint", "experiment").

---

### 2. Multi-Axis Deterministic Retrieval

**What it does.** Queries records via 5 secondary indexes using compound tuple keys: topic prefix scan (`TOPIC_INDEX`), category prefix scan (`CATEGORY_INDEX`), tag set intersection (`TAG_INDEX` multimap), temporal range scan (`TIME_INDEX`), status filtering (`STATUS_INDEX`). Combined queries compute intersection of result sets from each index.

**Implementation.** `Store::query()`, `Store::query_by_topic()`, `Store::query_by_category()`, `Store::query_by_tags()`, `Store::query_by_time_range()`, `Store::query_by_status()`. Exposed via MCP as `context_lookup`.

**Domain-specific elements.** None. The index dimensions (topic, category, tags, time, status) are generic metadata axes applicable to any structured content.

**What makes it general.** Any system that stores categorized, tagged, time-stamped content needs exactly these retrieval patterns. The set-intersection approach (`QueryFilter`) is domain-neutral; the caller chooses which axes matter.

**Portability: HIGH.** Zero domain coupling. The axes themselves (topic/category/tags/time/status) are universal knowledge organization dimensions.

---

### 3. Semantic Vector Search

**What it does.** Generates 384-dimensional embeddings from text (title + content concatenation) using local ONNX sentence-transformer models. Inserts embeddings into an HNSW graph (hnsw_rs, DistDot distance). Searches return top-k results ranked by cosine similarity. Supports filtered search with pre-computed entry ID allow-lists (for combining semantic and deterministic retrieval).

**Implementation.** Three crates cooperate:
- `unimatrix-embed`: ONNX Runtime inference, 7 pre-configured models (all 384-d), tokenizer, mean pooling + L2 normalization. `EmbeddingProvider` trait (object-safe, Send + Sync). Local model cache at `~/.cache/unimatrix/models/`.
- `unimatrix-vector`: `VectorIndex` wrapping `Hnsw<f32, DistDot>` with `RwLock`. Bidirectional `IdMap` (entry_id <-> hnsw_data_id). `VECTOR_MAP` bridge table in redb. Persistent dump/load. `SearchResult { entry_id, similarity }`.
- Server: `context_search` tool combines embedding + vector search + entry retrieval. Near-duplicate detection at 0.92 cosine threshold on `context_store`.

**Domain-specific elements.** The text preparation strategy (title + ": " + content) is geared toward short knowledge entries. The 384-dimension default is optimized for the all-MiniLM-L6-v2 model family, which excels at English sentence-level semantics. The near-duplicate threshold (0.92) was tuned for knowledge deduplication, not for other similarity tasks.

**What makes it general.** Semantic search over text content is one of the most broadly applicable capabilities in modern software. The same pipeline serves: customer support (finding similar tickets), legal discovery (finding relevant precedents), research (finding related papers), personal knowledge management (finding notes by meaning), compliance (finding related policies). The `EmbeddingProvider` trait abstracts the model; the `VectorIndex` abstracts the search. Both are pluggable.

**Portability: HIGH.** The embedding pipeline and vector index are fully general. The 384-d constraint and model catalog are configurable. The only opinion baked in is "local inference" (no API fallback) -- which is actually a portability advantage (no external dependencies at runtime).

---

### 4. Content Integrity Chain

**What it does.** Every insert computes SHA-256 of "title: content". Every update sets `previous_hash = old.content_hash`, recomputes `content_hash`, and increments `version`. Creates an append-only hash chain per entry. `update_status()` explicitly does NOT bump version or hash (metadata-only change).

**Implementation.** `unimatrix-store`: `hash::compute_content_hash()`, fields on `EntryRecord`: `content_hash`, `previous_hash`, `version`. Computed by the engine, not the caller.

**Domain-specific elements.** None. Content hashing and version chains are domain-agnostic integrity primitives.

**What makes it general.** This is effectively a lightweight audit trail built into every record -- useful for any system where content tampering detection matters. Regulatory compliance (HIPAA, SOX), legal document management, pharmaceutical research records, financial audit trails. The per-entry hash chain pattern is simpler than a Merkle tree but provides per-record tamper evidence.

**Portability: HIGH.** Pure data integrity primitive. No domain coupling whatsoever.

---

### 5. Knowledge Lifecycle Management

**What it does.** Entries have three status states: Active, Deprecated, Proposed. Status transitions are atomic (STATUS_INDEX migration + counter adjustment in one transaction). Entries can be corrected via `context_correct` (deprecate original, create linked replacement with `supersedes`/`superseded_by` chain). Entries can be deprecated via `context_deprecate` with idempotency.

**Implementation.** `Store::update_status()`. Server: `context_correct`, `context_deprecate`. `EntryRecord`: `supersedes: Option<u64>`, `superseded_by: Option<u64>`, `correction_count: u32`, `status: Status`.

**Domain-specific elements.** The three-state lifecycle (Active/Deprecated/Proposed) is somewhat opinionated -- some domains would want more states (Draft, Reviewed, Published, Archived). However, the existing states map cleanly to a minimal governance model.

**What makes it general.** Any knowledge management system needs content lifecycle: create, correct, retire. The correction chain (supersedes/superseded_by) is how Wikipedia handles revisions, how regulatory systems handle policy updates, how medical knowledge bases handle guideline changes. The "proposed" status enables review workflows.

**Portability: MEDIUM.** The three-state model is a reasonable default but may require extension for domains with richer approval workflows. The correction chain pattern is universally applicable. The limitation is that adding new status values requires code changes (`Status` enum is Rust code, not data).

---

### 6. Agent Identity and Trust Hierarchy

**What it does.** Maintains an agent registry (`AGENT_REGISTRY` table) with 4 trust levels: System > Privileged > Internal > Restricted. Each agent has a capability set (Read, Write, Search, Admin). Unknown agents auto-enroll as Restricted (Read + Search only). Identity resolved from optional `agent_id` parameter; absent = "anonymous" (Restricted). Capability checks enforced per-tool.

**Implementation.** `unimatrix-server`: `registry.rs` (`AgentRegistry`, `AgentRecord`, `TrustLevel`, `Capability`), `identity.rs` (`extract_agent_id`, `resolve_identity`). Bootstrap creates "system" and "human" agents.

**Domain-specific elements.** The trust hierarchy (System/Privileged/Internal/Restricted) maps well to multi-agent software systems but is not tightly coupled to that domain. The specific names (e.g., "Internal" for orchestrator agents) carry dev-team semantics. Topic/category restrictions (`allowed_topics`, `allowed_categories`) are generic access control dimensions.

**What makes it general.** Role-based access control with hierarchical trust is a standard security pattern. The "auto-enroll as restricted" pattern is good for any system where new actors should start with minimal privileges. The capability model (Read/Write/Search/Admin) maps to standard CRUD+admin patterns. Applicable to: multi-user note systems, team knowledge bases, compliance document stores, any system where different actors get different access.

**Portability: MEDIUM.** The 4-level trust hierarchy is reasonable but may be too coarse for enterprise deployments (which often need project-scoped roles, custom capability sets, delegated administration). The capability model is clean but fixed to 4 capabilities. The auto-enrollment pattern is genuinely useful across domains.

---

### 7. Append-Only Audit Trail

**What it does.** Every MCP request generates an `AuditEvent` (event_id, timestamp, session_id, agent_id, operation, target_ids, outcome, detail) written to the `AUDIT_LOG` table. Event IDs are monotonic across sessions (counter persisted in `COUNTERS` table). Supports writing audit events within an existing transaction (for atomic entry+audit writes).

**Implementation.** `unimatrix-server`: `audit.rs` (`AuditLog`, `AuditEvent`, `Outcome`). `log_event()` for standalone writes, `write_in_txn()` for transactional writes.

**Domain-specific elements.** The `operation` field stores MCP tool names (e.g., "context_search", "context_store") but is a free-form string. The `Outcome` enum (Success, Denied, Error, NotImplemented) is generic.

**What makes it general.** Append-only audit logging with monotonic IDs, timestamps, actor attribution, and operation tracking is a compliance requirement in healthcare (HIPAA), finance (SOX), government (FedRAMP), and any system that needs forensic reconstruction. The transactional audit write (`write_in_txn`) ensures audit events are atomically committed with the operations they record -- a strong integrity guarantee.

**Portability: HIGH.** Audit logging is a universal requirement. The schema is generic. The cross-session ID continuity is a quality-of-implementation detail that many audit systems get wrong.

---

### 8. Content Security Scanning

**What it does.** Scans text content against ~31 compiled regex patterns in two categories: injection patterns (instruction override, role impersonation, system prompt extraction, delimiter injection, encoding evasion) and PII patterns (email, phone, SSN, API keys/tokens). Patterns compiled once via `OnceLock` singleton. Hard-reject on match -- no fuzzy scoring.

**Implementation.** `unimatrix-server`: `scanning.rs` (`ContentScanner`, `ScanResult`, `PatternCategory`). `scan()` checks both categories. `scan_title()` checks injection only.

**Domain-specific elements.** The injection patterns are specifically designed for LLM prompt injection defense -- these are highly specific to AI/agent systems. The PII patterns (email, phone, SSN, API key) are general data protection patterns.

**What makes it general.** PII scanning is universally applicable to any system that stores user-generated content. The injection scanning is specific to AI-adjacent systems but is increasingly relevant as LLM integrations spread across industries. The pattern architecture (compiled singleton, category enum, hard-reject) is reusable even if the specific patterns change.

**Portability: MEDIUM.** PII patterns: high portability. Injection patterns: applicable to any AI-integrated system (growing market) but irrelevant for pure human-operated systems. The scanner architecture itself is fully portable; the pattern set is the domain-specific part and is easy to swap.

---

### 9. Format-Selectable Response Rendering

**What it does.** Every tool response can be rendered in three formats: summary (compact human-readable, default), markdown (structured with `[KNOWLEDGE DATA]` output framing markers), or json (machine-readable). Single Content block per response. Format selected via `format` parameter on each tool call.

**Implementation.** `unimatrix-server`: `response.rs` (`ResponseFormat`, `format_search_results`, `format_lookup_results`, `format_single_entry`, `format_store_success`, `format_correct_success`, `format_deprecate_success`, `format_status_report`, `format_briefing`).

**Domain-specific elements.** The `[KNOWLEDGE DATA]` framing markers in markdown format are specifically designed for LLM output parsing -- they help AI agents distinguish retrieved knowledge from instructions. The content of the rendered output references knowledge-specific fields (topic, category, confidence, etc.).

**What makes it general.** Multi-format output is a standard API design pattern. Summary/Markdown/JSON covers the main consumer types (humans, rich clients, machines). The pattern of format-selectable responses is portable even if the specific field labels change.

**Portability: MEDIUM.** The rendering architecture is generic. The output framing is AI-specific. The field labels in rendered output reflect the knowledge-entry schema which is domain-neutral.

---

### 10. Compiled Orientation Briefing

**What it does.** `context_briefing` takes a role and task description, then: (1) looks up entries tagged with that role in the "duties" category, (2) looks up entries in the "convention" category, (3) semantically searches for entries relevant to the task description, (4) assembles everything into a token-budgeted briefing. Intended to orient a new actor (agent or human) on what they need to know before starting a task.

**Implementation.** `unimatrix-server`: `context_briefing` tool in `tools.rs`. Combines deterministic lookup (role duties, conventions) with semantic search (task relevance). `max_tokens` parameter controls output budget.

**Domain-specific elements.** The "role + task" framing is oriented toward agent orchestration. The specific categories queried (duties, convention) are software-process categories. The token budget concept is AI-context-window aware.

**What makes it general.** Role-based contextual onboarding is useful in any organizational knowledge system: "Show a new nurse what they need to know about medication protocols", "Brief a new analyst on this client's compliance requirements", "Orient a contractor on this building's safety procedures." The pattern of compiling a targeted briefing from multiple knowledge sources is universally valuable.

**Portability: MEDIUM.** The underlying retrieval mechanism is generic. The role/task framing maps to many domains. The token-budget concept is AI-specific but degrades gracefully (just becomes a "max length" constraint). Needs category remapping for non-dev domains.

---

### 11. Schema Migration Infrastructure

**What it does.** On `Store::open()`, checks a `schema_version` counter. If the stored version is behind the code version, performs a scan-and-rewrite of all entries to add new fields. Migration is idempotent and runs at database open time.

**Implementation.** `unimatrix-store`: `migration.rs`. Currently handles v0-to-v1 migration (adding security fields). Designed for incremental field additions.

**Domain-specific elements.** None.

**What makes it general.** Automatic schema migration on open is a desirable property for any embedded database application. Users never run migration commands; the system handles it transparently.

**Portability: HIGH.** Universal infrastructure concern.

---

### 12. MCP Protocol Server

**What it does.** Exposes all capabilities via the Model Context Protocol (MCP) over stdio transport. Supports tool discovery, parameterized tool calls, format-selectable responses. Server behavioral instructions drive AI agent behavior.

**Implementation.** `unimatrix-server`: `server.rs` (rmcp 0.16 SDK), `tools.rs` (8 tools), `main.rs` (stdio transport, lifecycle management, graceful shutdown).

**Domain-specific elements.** MCP is currently an AI-agent integration protocol. The stdio transport is specific to local process communication (Claude Code integration). The server instructions are software-dev-agent oriented.

**What makes it general.** MCP is becoming a standard for tool integration with AI systems. Any knowledge system that wants AI agent access will benefit from MCP exposure. The tool architecture (parameterized inputs, structured outputs, capability checks) would work identically over HTTP/SSE or other transports.

**Portability: MEDIUM.** MCP adoption is growing but still early. The server is well-architected for transport evolution (internal plumbing is transport-agnostic). The protocol itself is not domain-specific -- it is an integration protocol. The domain-specific part is the server instructions text.

---

## Portability Summary

| # | Capability | Portability | Bottleneck |
|---|-----------|-------------|------------|
| 1 | Structured Knowledge Storage | **High** | `feature_cycle` field name |
| 2 | Multi-Axis Deterministic Retrieval | **High** | None |
| 3 | Semantic Vector Search | **High** | Text-focused preparation |
| 4 | Content Integrity Chain | **High** | None |
| 5 | Knowledge Lifecycle Management | **Medium** | Fixed 3-state enum |
| 6 | Agent Identity & Trust | **Medium** | 4-level hierarchy, 4 capabilities |
| 7 | Append-Only Audit Trail | **High** | None |
| 8 | Content Security Scanning | **Medium** | Injection patterns are AI-specific |
| 9 | Format-Selectable Responses | **Medium** | Output framing is AI-specific |
| 10 | Compiled Orientation Briefing | **Medium** | Role/task framing, token budget |
| 11 | Schema Migration | **High** | None |
| 12 | MCP Protocol Server | **Medium** | MCP adoption is early-stage |

---

## The Portable Kernel

Strip the Unimatrix framing. What ships as a general-purpose embedded knowledge engine:

### Layer 1: Storage Foundation (fully portable)

- Embedded transactional knowledge store (redb, single-file, no external dependencies)
- Typed records with user-defined topic/category/tags/status metadata
- 5-axis secondary indexing with set-intersection query
- Content integrity chain (SHA-256 hash chain per record, version tracking)
- Append-only audit trail with monotonic IDs and transactional consistency
- Automatic schema migration on open

These capabilities have zero domain coupling. They constitute a "knowledge-store-as-a-library" that could be embedded in any Rust application.

### Layer 2: Intelligence (mostly portable)

- Local embedding generation (ONNX inference, 7 pre-configured models, no API dependency)
- HNSW-based approximate nearest neighbor search with filtered search support
- Combined semantic + deterministic retrieval (embed query, search vectors, intersect with metadata filters)
- Near-duplicate detection on write (cosine similarity threshold)

The intelligence layer assumes text content but makes no assumptions about what the text is about. Swapping the embedding model or adjusting the dimension is a configuration change, not a code change.

### Layer 3: Access Control (portable with domain remapping)

- Agent/actor registry with hierarchical trust (4 levels)
- Capability-based access control (Read, Write, Search, Admin)
- Auto-enrollment for unknown actors
- Per-request audit logging with actor attribution
- Content scanning (PII + injection detection)

The access control layer is designed for multi-actor systems. The trust hierarchy and capability model are generic but may need extension (more levels, custom capabilities) for complex enterprise deployments.

### Layer 4: Protocol Interface (AI-specific but growing)

- MCP server with 8 tools
- Behavioral driving via server instructions
- Compiled orientation briefings
- Output framing for AI context separation

This layer is the most domain-coupled to AI agent systems. However, MCP adoption is expanding, and the protocol itself is domain-neutral. The specific tool implementations (search, lookup, store, get, correct, deprecate, status, briefing) represent general knowledge management operations that happen to be exposed via MCP.

---

## Assessment: What Is Genuinely Reusable

**The storage and retrieval core (Layers 1+2) is a general-purpose embedded knowledge engine.** It competes with (and in some ways exceeds) systems like ChromaDB, LanceDB, or Qdrant for local-first knowledge management, while adding structured metadata indexing, content integrity, and audit logging that those vector-only databases lack.

**The access control layer (Layer 3) is reusable for any multi-actor knowledge system** -- the trust model is simple enough to be broadly applicable.

**The protocol layer (Layer 4) is correctly positioned** for the growing AI-agent-integration market but needs the underlying capabilities to also be accessible as a Rust library (which they are, via `unimatrix-core` traits).

**What would need to change for non-dev domains:**

1. `feature_cycle` field: rename or document as "context label" (no code change needed -- it is a free-form string)
2. Initial category allowlist: swap the 8 default categories (one constant change)
3. Server instructions text: swap the behavioral driving text (one constant change)
4. Content scanning patterns: add domain-specific patterns, keep or drop injection patterns
5. `Status` enum: potentially add states for domains with richer workflows (code change, but migration infrastructure supports it)

None of these are architectural blockers. They are configuration-level changes.

**The hard constraint on portability is the Rust-only API surface.** Non-Rust applications must integrate via MCP (which means running the server as a sidecar process). A C FFI or WebAssembly boundary would dramatically expand the addressable market.
