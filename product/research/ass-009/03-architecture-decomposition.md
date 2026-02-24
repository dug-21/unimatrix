# ASS-009/03: Architecture Decomposition Analysis

## 1. Current Coupling Analysis

### 1.1 Dependency Graph

```
unimatrix-store        (leaf — depends on: redb, serde, bincode, sha2)
     |
unimatrix-vector       (depends on: unimatrix-store, hnsw_rs, anndists)
     |
unimatrix-embed        (leaf — depends on: ort, tokenizers, hf-hub, dirs)
     |
unimatrix-core         (depends on: unimatrix-store, unimatrix-vector, unimatrix-embed)
     |                  (optional: tokio via "async" feature)
     |
unimatrix-server       (depends on: unimatrix-core, unimatrix-store, unimatrix-vector,
                         unimatrix-embed, rmcp, tokio, schemars, serde_json,
                         regex, sha2, dirs, tracing, clap, bincode)
```

Key observation: `unimatrix-server` depends on all four foundation crates **directly**, not just through `unimatrix-core`. This is intentional — `server.rs` performs combined write transactions that reach into `unimatrix-store`'s low-level table definitions (`ENTRIES`, `TOPIC_INDEX`, etc.) and `unimatrix-vector`'s `allocate_data_id` / `insert_hnsw_only` methods.

### 1.2 Tight Coupling Points

**unimatrix-vector -> unimatrix-store (structural)**
- `VectorIndex::new()` takes `Arc<Store>` as a required parameter
- `VectorIndex::insert()` writes to `VECTOR_MAP` via `store.put_vector_mapping()`
- The vector index cannot exist without a store. This coupling is intentional: VECTOR_MAP in redb provides crash-safe persistence for the entry-to-hnsw-data-id mapping, while hnsw_rs itself is in-memory with explicit `dump()`/`load()` persistence.

**unimatrix-server -> unimatrix-store (operational bypass)**
- `server.rs` directly opens redb tables (`ENTRIES`, `TOPIC_INDEX`, `CATEGORY_INDEX`, `TAG_INDEX`, `TIME_INDEX`, `STATUS_INDEX`, `VECTOR_MAP`, `COUNTERS`) in combined write transactions
- This bypasses the `EntryStore` trait entirely for writes — the trait is only used for reads via `AsyncEntryStore`
- Reason: atomicity. `insert_with_audit()`, `correct_with_audit()`, and `deprecate_with_audit()` combine entry creation, index updates, vector map writes, and audit logging in a single redb transaction. The trait layer cannot express multi-table atomic operations.

**unimatrix-server -> unimatrix-vector (operational bypass)**
- Uses `vector_index.allocate_data_id()` and `vector_index.insert_hnsw_only()` instead of the `VectorStore` trait
- Again for atomicity: VECTOR_MAP is written inside the combined redb transaction, then HNSW insertion happens after commit.

**unimatrix-core re-exports everything**
- `unimatrix-core/src/lib.rs` re-exports all public types from store, vector, and embed
- This is convenient but means `unimatrix-core` is not a standalone abstraction layer — it's a facade that leaks all concrete types.

### 1.3 Loose Coupling Points

**unimatrix-embed is fully independent**
- Zero dependencies on any other Unimatrix crate
- Depends only on ONNX runtime, tokenizers, and HuggingFace Hub
- Could be published as a standalone crate today with minimal changes
- Connection to the rest of the system is purely through `EmbedService` trait in core

**unimatrix-store is a self-contained storage engine**
- Depends only on redb, serde, bincode, sha2
- No knowledge of vectors, embeddings, or MCP
- The only Unimatrix-specific thing is the `EntryRecord` schema and the 10-table design
- Two tables (`AGENT_REGISTRY`, `AUDIT_LOG`) are server concerns that leaked into the store's table definitions

**Traits in unimatrix-core are properly object-safe**
- `EntryStore`, `VectorStore`, `EmbedService` are all `Send + Sync` with `dyn` compatibility verified by compile-time tests
- Adapter pattern (`StoreAdapter`, `VectorAdapter`, `EmbedAdapter`) cleanly bridges concrete types to traits
- Async wrappers are feature-gated behind the `async` feature flag

### 1.4 EntryRecord: Domain-Agnostic vs Dev-Workflow-Specific

The `EntryRecord` struct in `crates/unimatrix-store/src/schema.rs` has 23 fields:

| Field | Domain-Agnostic? | Notes |
|-------|-------------------|-------|
| `id: u64` | Yes | Universal primary key |
| `title: String` | Yes | Any knowledge entry needs a title |
| `content: String` | Yes | Free-form content body |
| `topic: String` | Yes | Generic categorization axis — "auth", "billing", "compliance" work in any domain |
| `category: String` | Yes | Generic categorization axis — server enforces allowlist but that's config, not schema |
| `tags: Vec<String>` | Yes | Free-form labels |
| `source: String` | Yes | Provenance tracking |
| `status: Status` | Yes | Active/Deprecated/Proposed lifecycle states are universal |
| `confidence: f32` | Yes | Quality signal useful in any domain |
| `created_at: u64` | Yes | Universal timestamp |
| `updated_at: u64` | Yes | Universal timestamp |
| `last_accessed_at: u64` | Yes | Usage tracking, universal |
| `access_count: u32` | Yes | Usage tracking, universal |
| `supersedes: Option<u64>` | Yes | Correction chain, universal knowledge management |
| `superseded_by: Option<u64>` | Yes | Correction chain, universal |
| `correction_count: u32` | Yes | Quality signal, universal |
| `embedding_dim: u16` | Yes | Dimensionality marker for embedding compatibility |
| `created_by: String` | Yes | Attribution, universal |
| `modified_by: String` | Yes | Attribution, universal |
| `content_hash: String` | Yes | Integrity verification, universal |
| `previous_hash: String` | Yes | Hash chain for tamper detection, universal |
| `version: u32` | Yes | Versioning, universal |
| `feature_cycle: String` | Soft coupling | The name suggests dev workflows, but the field is just a string. A legal system could use "case-2024-001", a medical system could use "study-phase-3". The field name could be more generic (e.g., `context_label` or `lifecycle_tag`), but the actual type imposes no dev-specific constraint. |
| `trust_source: String` | Yes | "agent"/"human"/"system" applies to any multi-actor system |

**Assessment**: The EntryRecord schema is remarkably domain-agnostic. Only `feature_cycle` has a name that betrays dev-workflow origins, but its type (`String`) imposes no constraint. The schema could store legal precedents, medical guidelines, compliance rules, or personal knowledge without modification.

### 1.5 MCP Transport Coupling

The MCP coupling is well-contained in `unimatrix-server`:

- **Transport layer**: Only `main.rs` references `rmcp::transport::io::stdio()`. The server struct itself is transport-agnostic — it implements `rmcp::ServerHandler` which can serve over any transport.
- **Tool definitions**: The `#[tool]` macro from rmcp generates JSON Schema from `schemars::JsonSchema` derives on parameter structs. Tool parameter shapes are MCP-specific (`SearchParams`, `StoreParams`, etc.) but the business logic inside each tool handler calls generic trait methods.
- **Response formatting**: `response.rs` formats results as MCP `CallToolResult` with `Content::text()`. This is rmcp-specific.
- **Error mapping**: `error.rs` maps `ServerError` to rmcp's `ErrorData` with MCP-specific error codes.

The coupling is correctly scoped — all MCP specifics live in `unimatrix-server`. No MCP types appear in store, vector, embed, or core.

---

## 2. Packaging Options

### Option A: Monolith (Status Quo)

**What exists**: A single `unimatrix-server` binary that bundles redb storage, HNSW vector index, ONNX embedding pipeline, and MCP stdio server.

**Pros**:
- Zero-config deployment: one binary, `~/.unimatrix/{project_hash}/` auto-created on first write
- No service coordination: store + vector + embed lifecycle managed internally
- Shutdown correctness: `compact()` + `dump()` are orchestrated by the server's shutdown handler
- Minimum attack surface: single process, no IPC

**Cons**:
- ONNX runtime dependency makes the binary large (~50-100MB with model weights)
- Local ONNX inference requires CPU resources that may be unavailable in constrained environments
- No way to use the storage engine without MCP overhead
- Couples embedding model choice to server deployment

**Markets well-served**: Single-developer or small-team Unimatrix adoption where Claude Code is the primary client. The dev-container setup in the current project.

**Markets poorly served**: Enterprise deployment where embedding is done by a central service, CI/CD integration where only storage queries are needed, projects that want knowledge storage without AI inference.

**Effort**: 0 (status quo).

### Option B: Layered Library

**Concept**: `unimatrix-core` as a reusable kernel that defines traits + domain types. Domain-specific servers import core and implement their own tools.

**Current state of `unimatrix-core`**:
- Defines 3 traits: `EntryStore`, `VectorStore`, `EmbedService`
- Provides adapter pattern for bridging concrete types to traits
- Feature-gated async wrappers (`AsyncEntryStore`, `AsyncVectorStore`, `AsyncEmbedService`)
- Re-exports all public types from store, vector, and embed
- `CoreError` unifies errors from all three foundation crates

**What would need to change**:

1. **Stop re-exporting concrete types**. Currently `unimatrix-core` re-exports `Store`, `VectorIndex`, `OnnxProvider`, `DatabaseConfig`, `VectorConfig`, `EmbedConfig`, etc. These are implementation details. The trait layer should only expose the traits and the domain types needed to call them (`EntryRecord`, `NewEntry`, `QueryFilter`, `Status`, `TimeRange`, `SearchResult`). A downstream "legal knowledge engine" should not need to know about redb or hnsw_rs.

2. **Extract domain types into core**. `EntryRecord`, `NewEntry`, `QueryFilter`, `Status`, `TimeRange` are currently defined in `unimatrix-store`. For core to be a standalone abstraction, these types should either live in core or in a tiny `unimatrix-types` crate that both core and store depend on. Currently, anyone importing core gets a transitive dependency on redb (through store).

3. **The server bypass problem**. The combined write transactions in `server.rs` bypass `EntryStore` entirely. If we want core to be the universal kernel, we need either:
   - A transactional extension to `EntryStore` (e.g., `fn begin_atomic_write(&self) -> AtomicWriter` that can combine entry + index + audit operations)
   - Accept that the trait layer is for reads and simple writes, while complex atomic operations go through a higher-level `KnowledgeEngine` struct

4. **Category allowlist and content scanning are server-level policies**. These don't belong in core — they're enforcement decisions. A legal knowledge engine might have different categories and different scanning rules.

**Could someone build a "legal knowledge engine" on unimatrix-core?**

Today: partially. They could use `EntryStore` trait for storage, `VectorStore` for similarity search, `EmbedService` for embeddings. But they'd be forced to pull in `redb` and `hnsw_rs` as transitive dependencies even if they wanted to implement their own backends. The trait abstraction is correct, but the dependency chain leaks implementation details.

After refactoring (extract types, stop re-exporting concrete types): yes. They'd depend only on `unimatrix-core` for traits + types, then either use the provided implementations (store, vector, embed) or write their own.

**Effort**: Medium. 2-3 days of refactoring to extract types, clean re-exports, and potentially add a transactional write abstraction.

### Option C: Embeddable Engine (Rust Library, No MCP)

**Concept**: Use `unimatrix-store` + `unimatrix-vector` + `unimatrix-embed` as a Rust library embedded in other applications.

**Current API surface** (what a Rust consumer would use):

```rust
// Storage
let store = Store::open("path/to/db.redb")?;
let id = store.insert(NewEntry { title, content, topic, category, tags, source, status,
                                  created_by, feature_cycle, trust_source })?;
let record = store.get(id)?;
let results = store.query(QueryFilter { topic, category, tags, status, time_range })?;
store.update(record)?;
store.update_status(id, Status::Deprecated)?;
store.delete(id)?;

// Vector
let vector_index = VectorIndex::new(Arc::clone(&store), VectorConfig::default())?;
vector_index.insert(entry_id, &embedding)?;
let results = vector_index.search(&query_embedding, top_k, ef_search)?;
let results = vector_index.search_filtered(&query, top_k, ef, &allowed_ids)?;

// Embedding
let provider = OnnxProvider::new(EmbedConfig::default())?;
let embedding = provider.embed("text to embed")?;
let embeddings = provider.embed_batch(&["text1", "text2"])?;
```

**Strengths**:
- The API is already clean and usable
- `Store` is `Send + Sync`, `VectorIndex` is `Send + Sync`, `OnnxProvider` is `Send + Sync`
- Feature flags (`test-support`) keep test infrastructure out of production builds
- `#![forbid(unsafe_code)]` across all crates

**Gaps for SDK use case**:
- No convenience "all-in-one" struct that combines store + vector + embed. The server's `UnimatrixServer` fills this role but is MCP-coupled. A `KnowledgeEngine` struct that wraps all three and provides `store_entry(title, content, topic, category) -> id` + `search(query_text, top_k) -> Vec<(EntryRecord, f32)>` would be the natural SDK entry point.
- No crate exposes a clean "library" public API without pulling in MCP types. You'd import `unimatrix-store`, `unimatrix-vector`, and `unimatrix-embed` separately and wire them yourself.
- The atomic write coordination that `server.rs` handles (entry + indexes + vector map + audit in one transaction) has no library equivalent. A library user doing `store.insert()` then `vector_index.insert()` has a crash window between the two.

**Effort**: Medium. Create a `unimatrix-engine` crate (or add a `library` feature to core) that provides a `KnowledgeEngine` facade with:
- Combined insert (entry + vector + embedding in one coordinated operation)
- Combined correct (deprecate original + create correction atomically)
- `search(text, top_k) -> Vec<(EntryRecord, f32)>` that does embed + search + fetch in one call
- No MCP, no rmcp, no tokio requirement (synchronous API)

### Option D: Protocol-First (MCP Configuration)

**Concept**: Same `unimatrix-server` binary, different "personalities" via configuration.

**What already supports this**:

1. **Category allowlist is runtime-extensible**. `CategoryAllowlist` starts with 8 dev-workflow categories (`outcome`, `lesson-learned`, `decision`, `convention`, `pattern`, `procedure`, `duties`, `reference`) but exposes `add_category(String)` at runtime. The categories themselves are just strings — nothing in the storage engine cares whether the category is "convention" or "case-law" or "diagnosis".

2. **Topic and tags are free-form strings**. No validation is imposed on topic values. A legal system would use topics like "contract-law", "IP", "employment"; a medical system would use "cardiology", "oncology", "radiology".

3. **Trust levels and capabilities are generic**. System/Privileged/Internal/Restricted with Read/Write/Search/Admin capabilities — this maps to any multi-actor system.

4. **Content scanning is security, not domain**. Injection detection and PII scanning are universally applicable. The patterns are general (email, SSN, API keys, prompt injection) and would need augmentation rather than replacement for specialized domains.

5. **Server instructions are a single const string**. `SERVER_INSTRUCTIONS` in `server.rs` is hardcoded to dev-workflow language. This would need to be configurable.

**What would need to change**:

1. **Externalize the category allowlist**. Move initial categories to a config file (`~/.unimatrix/config.toml` or per-project). Currently hardcoded in `categories.rs`:
   ```rust
   const INITIAL_CATEGORIES: [&str; 8] = [
       "outcome", "lesson-learned", "decision", "convention",
       "pattern", "procedure", "duties", "reference",
   ];
   ```

2. **Externalize server instructions**. Move `SERVER_INSTRUCTIONS` from a const to a config parameter. A legal server might say: "Before drafting legal analysis, search for relevant precedents and jurisdictional conventions."

3. **Externalize default agent trust mappings**. `bootstrap_defaults()` creates "system" (System) and "human" (Privileged). Different deployments might want different default agents.

4. **Optionally externalize content scanning rules**. Domain-specific PII patterns (e.g., HIPAA identifiers for medical, case numbers for legal) could be additive to the built-in patterns.

5. **Rename `feature_cycle` field** or document it as a generic lifecycle label. Not strictly necessary (it's just a String) but improves clarity.

**Could the same binary serve legal, medical, or personal knowledge domains?**

Yes, with the configuration changes above. The binary's fundamental behavior (store structured knowledge, search by similarity, track corrections, audit operations, manage agent trust) is domain-neutral. The domain specificity is entirely in:
- Which categories are valid
- What the server instructions say
- What content scanning rules are applied
- What agents are bootstrapped

All of these are currently hardcoded but could be externalized with 1-2 days of work.

**Effort**: Low-Medium. The architecture is already protocol-first by design. Configuration externalization is the main work.

### Option E: Micro-Crates (crates.io)

**Which crates have standalone value?**

| Crate | Standalone Value | Publishability |
|-------|-----------------|----------------|
| `unimatrix-embed` | High. A clean ONNX embedding crate wrapping `ort` + `tokenizers` with 7 pre-configured models, mean pooling, L2 normalization, and HF Hub download. The market for "easy local embeddings in Rust" is underserved. | Ready today. No Unimatrix dependencies. Rename to something generic (e.g., `onnx-embeddings`). |
| `unimatrix-store` | Medium. A redb-backed knowledge store with secondary indexes. Useful for anyone building a local knowledge base. But the `EntryRecord` schema is opinionated (22 specific fields). | Publishable but niche. Users must adopt the EntryRecord shape or it's useless to them. |
| `unimatrix-vector` | Low standalone. It's a thin wrapper over `hnsw_rs` with a bidirectional ID map and redb-backed persistence. The value is the integration with `unimatrix-store`, not the wrapper itself. | Not worthwhile as a standalone crate. Use `hnsw_rs` directly. |
| `unimatrix-core` | Low standalone. It defines traits + adapters that are only useful if you're building on the Unimatrix stack. | Not publishable independently. The traits reference `EntryRecord`, `NewEntry`, etc. which are Unimatrix-specific types. |
| `unimatrix-server` | None standalone. It's the MCP server application. | Not a library crate. |

**Dependency hell risks**:

- `unimatrix-store` pins `redb = "3.1"`. Major version changes to redb would require coordinated releases.
- `unimatrix-embed` pins `ort = "=2.0.0-rc.9"` (exact version) due to glibc compatibility issues. This is fragile — any downstream crate also depending on `ort` must use the exact same version.
- `unimatrix-vector` requires a local patch for `anndists` (`patches/anndists/`). This cannot be published to crates.io as a dependency — the patch would need to be upstreamed or the crate would need to vendor the fix.
- The workspace uses `edition = "2024"` and `rust-version = "1.89"`, which narrows the user base.

**Effort**: High for marginal value (except `unimatrix-embed`). Publishing `unimatrix-embed` as a standalone crate is low effort and high value. Publishing the rest has significant dependency management overhead with limited external demand.

---

## 3. EntryRecord Flexibility

### 3.1 Domain Agnosticism of Category/Topic/Tags

The three classification axes are implemented as free-form strings:

```rust
pub topic: String,       // indexed via TOPIC_INDEX: (&str, u64) -> ()
pub category: String,    // indexed via CATEGORY_INDEX: (&str, u64) -> ()
pub tags: Vec<String>,   // indexed via TAG_INDEX: MultimapTableDefinition<&str, u64>
```

Enforcement is at the server level, not the storage level:
- `CategoryAllowlist` validates categories against a runtime-extensible set
- Topics have no validation at all
- Tags have no validation at all

This means the storage engine itself handles ANY domain without schema changes. The constraint is entirely in the server's policy layer, which is already designed to be extensible.

Example domain mappings:

| Domain | topic | category | tags |
|--------|-------|----------|------|
| Software dev | "auth", "logging", "api" | "convention", "decision", "pattern" | ["rust", "jwt", "error-handling"] |
| Legal | "contract-law", "IP", "employment" | "precedent", "statute", "brief", "ruling" | ["9th-circuit", "GDPR", "patent"] |
| Medical | "cardiology", "oncology", "neurology" | "protocol", "guideline", "case-study", "drug-interaction" | ["stage-3", "FDA-approved", "pediatric"] |
| Personal | "cooking", "fitness", "finance" | "recipe", "routine", "budget", "goal" | ["vegetarian", "30-min", "monthly"] |

No schema changes needed for any of these.

### 3.2 Security Fields: Universal or Dev-Specific?

The 7 security fields added in nxs-004:

| Field | Universal? | Rationale |
|-------|-----------|-----------|
| `created_by: String` | Yes | Attribution is universally valuable. "Who wrote this?" matters in legal, medical, dev, and personal contexts. |
| `modified_by: String` | Yes | Same as above for modifications. |
| `content_hash: String` | Yes | SHA-256 integrity verification. Essential for any system where tamper detection matters — legal discovery, medical records, audit trails. |
| `previous_hash: String` | Yes | Hash chain for version integrity. If content_hash is useful, previous_hash is the mechanism that makes it a chain rather than a point-in-time check. |
| `version: u32` | Yes | Version counting is universal. |
| `feature_cycle: String` | Soft coupling to dev | The field name implies dev workflows, but it's just a String. A legal system could store "case-2024-1234", a medical system could store "trial-phase-2". The semantics are "what lifecycle context produced this entry." |
| `trust_source: String` | Yes | "agent"/"human"/"system" source classification applies to any multi-actor knowledge system. |

**Assessment**: The security fields are universally useful. The hash chain (`content_hash` + `previous_hash`) is particularly valuable for regulated domains (legal, medical, financial) where audit trails and tamper evidence are requirements, not nice-to-haves. The only naming issue is `feature_cycle` which could be more generic.

### 3.3 Server-Level Types That Leak Into Store

Two tables in `unimatrix-store/src/schema.rs` are server concerns:

```rust
pub const AGENT_REGISTRY: TableDefinition<&str, &[u8]> = TableDefinition::new("agent_registry");
pub const AUDIT_LOG: TableDefinition<u64, &[u8]> = TableDefinition::new("audit_log");
```

These are defined in `unimatrix-store` and created by `Store::open()`, but they are only used by `unimatrix-server`'s `registry.rs` and `audit.rs`. They don't participate in any `unimatrix-store` operations. This is a minor coupling leak: the storage engine creates tables it never uses.

For packaging options B, C, or E, these should move to the server crate. The store could expose `begin_read()` / `begin_write()` (which it already does) and let the server create its own tables.

---

## 4. Recommendations

### 4.1 Maximum Reach with Minimum Refactoring: Option D (Protocol-First) + Partial Option C

The architecture is already 90% ready for Option D. The remaining 10% is configuration externalization:

1. **Move initial categories to config** (~0.5 days). Replace `const INITIAL_CATEGORIES` with a `ServerConfig` struct loaded from `~/.unimatrix/config.toml` or env vars.

2. **Make server instructions configurable** (~0.5 days). Replace the `SERVER_INSTRUCTIONS` const with a config field.

3. **Make default agents configurable** (~0.5 days). Let the config specify which agents are bootstrapped and at what trust level.

This unlocks the "same binary, different domain" use case with 1-2 days of work.

For Option C (embeddable engine), create a `KnowledgeEngine` facade (~2-3 days):

```rust
pub struct KnowledgeEngine {
    store: Arc<Store>,
    vector: Arc<VectorIndex>,
    embed: Arc<dyn EmbeddingProvider>,
}

impl KnowledgeEngine {
    pub fn open(path: &Path, config: EngineConfig) -> Result<Self>;
    pub fn store(&self, title: &str, content: &str, topic: &str, category: &str) -> Result<u64>;
    pub fn search(&self, query: &str, top_k: usize) -> Result<Vec<(EntryRecord, f32)>>;
    pub fn lookup(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>>;
    pub fn get(&self, id: u64) -> Result<EntryRecord>;
    pub fn correct(&self, original_id: u64, content: &str) -> Result<u64>;
    pub fn deprecate(&self, id: u64) -> Result<()>;
    pub fn compact_and_dump(&mut self) -> Result<()>;
}
```

This would live in `unimatrix-core` (or a new `unimatrix-engine` crate) and provide the "embed in my Rust application" use case without MCP.

### 4.2 Changes That Unlock the Most Optionality

In priority order:

1. **Extract domain types to a `unimatrix-types` crate or a `types` module in core** (High impact, Medium effort). Move `EntryRecord`, `NewEntry`, `QueryFilter`, `Status`, `TimeRange` out of `unimatrix-store` into a dependency-free types crate. This breaks the transitive redb dependency for anyone who only needs the types. Both `unimatrix-store` and `unimatrix-core` would depend on `unimatrix-types`.

2. **Move AGENT_REGISTRY and AUDIT_LOG table definitions to unimatrix-server** (Low effort, Medium impact). These are server concerns. Clean separation makes `unimatrix-store` purely a knowledge storage engine.

3. **Externalize server policy configuration** (Low effort, High impact for Option D). Categories, instructions, default agents, content scanning rules.

4. **Create KnowledgeEngine facade** (Medium effort, High impact for Option C). Provides the "library without MCP" entry point.

5. **Publish `unimatrix-embed` as standalone crate** (Low effort, Medium impact for Option E). The only crate with genuine standalone value on crates.io. Resolve the `ort` pinning issue first.

### 4.3 What Should NOT Change

These are correctly domain-agnostic and should be preserved:

1. **The `EntryRecord` schema**. All 23 fields are universally useful. The `#[serde(default)]` annotations and append-only field ordering contract enable forward migration. Do not add domain-specific fields.

2. **The three trait abstractions** (`EntryStore`, `VectorStore`, `EmbedService`). They are object-safe, Send + Sync, and correctly abstracted. The adapter pattern is clean.

3. **The generic `QueryFilter` model** (`{ topic, category, tags, status, time_range }`). This is the "three hard design constraints" decision from ASS-007 and it works for any domain.

4. **The correction chain model** (`supersedes` / `superseded_by`). Knowledge correction is universal.

5. **The security model** (trust levels, capabilities, audit log, content hash chain). This applies to any multi-actor knowledge system. The four trust levels (System > Privileged > Internal > Restricted) and four capabilities (Read, Write, Search, Admin) are generic enough for any domain.

6. **The combined write transaction pattern** in `server.rs`. Atomic operations spanning entry + indexes + vector map + audit are essential for data integrity. Do not split these for "cleaner architecture."

7. **redb as the storage backend**. Embedded, zero-config, single-file, ACID transactions, COW pages. These properties are what make Unimatrix deployable without infrastructure.

8. **`#![forbid(unsafe_code)]` across all crates**. This is a trust signal for any domain, especially regulated ones.

---

## 5. Summary Matrix

| Option | Feasibility | Effort | Reach Impact | Recommended? |
|--------|------------|--------|-------------|-------------|
| A: Monolith | Already done | 0 | Low (dev-only) | Baseline |
| B: Layered Library | High (after type extraction) | 2-3 weeks | Medium | Defer until external demand |
| C: Embeddable Engine | High | 3-5 days | High | Yes - KnowledgeEngine facade |
| D: Protocol-First | Very high | 1-2 days | High | **Yes - highest ROI** |
| E: Micro-Crates | Low for most, High for embed | 1-2 weeks total | Low (except embed) | Only unimatrix-embed |

**Recommended sequencing**: D first (config externalization, 1-2 days), then C (KnowledgeEngine facade, 3-5 days), then E for embed only (1-2 days). Defer B until there is concrete demand for alternative backend implementations.
