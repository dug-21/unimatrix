# Architecture: nxs-004 Core Traits & Domain Adapters

## System Overview

nxs-004 introduces a trait abstraction layer between Unimatrix's three foundation crates (unimatrix-store, unimatrix-vector, unimatrix-embed) and their downstream consumers (starting with vnc-001 MCP server). It also adds 7 security fields to EntryRecord and implements the first schema migration.

The feature creates a new crate, `unimatrix-core`, which becomes the single dependency for all consumers. It re-exports domain types from the lower crates and defines the canonical trait contracts that the MCP server and future features program against.

```
                         vnc-001 (MCP Server)
                              |
                       unimatrix-core
                    /        |         \
           EntryStore   VectorStore   EmbedService    <-- traits
               |             |             |
         StoreAdapter  VectorAdapter  EmbedAdapter    <-- domain adapters
               |             |             |
        unimatrix-store  unimatrix-vector  unimatrix-embed  <-- concrete crates
```

## Component Breakdown

### C1: Core Traits (`unimatrix-core/src/traits.rs`)

Three object-safe, `Send + Sync` traits that abstract the storage, vector search, and embedding operations.

**Responsibilities:**
- Define method signatures for entry CRUD and query operations (`EntryStore`)
- Define method signatures for vector insert and search (`VectorStore`)
- Define method signatures for entry-level embedding (`EmbedService`)
- All methods return `Result<T, CoreError>` using a unified error type

**Constraints:**
- Object-safe: usable as `dyn EntryStore`, `dyn VectorStore`, `dyn EmbedService`
- `Send + Sync` bounds: compatible with `Arc` sharing across threads
- Synchronous: no async in trait methods (async wrappers are separate)

### C2: Core Error Type (`unimatrix-core/src/error.rs`)

A unified error enum that all core trait methods return. Converts from crate-specific error types.

**Responsibilities:**
- Define `CoreError` enum covering storage, vector, and embedding failure modes
- Implement `From<StoreError>`, `From<VectorError>`, `From<EmbedError>` conversions
- Implement `std::error::Error` and `Display`

### C3: Type Re-exports (`unimatrix-core/src/lib.rs`)

Re-export domain types so consumers only need `unimatrix-core`.

**Re-exported types:**
- From `unimatrix-store`: `EntryRecord`, `NewEntry`, `QueryFilter`, `Status`, `TimeRange`, `DatabaseConfig`, `Store`
- From `unimatrix-vector`: `SearchResult`, `VectorConfig`, `VectorIndex`
- From `unimatrix-embed`: `EmbeddingProvider`, `EmbedConfig`, `OnnxProvider`

### C4: Domain Adapters (`unimatrix-core/src/adapters.rs`)

Thin adapter structs that implement core traits by delegating to concrete types.

**Adapters:**
- `StoreAdapter(Arc<Store>)` -- implements `EntryStore`
- `VectorAdapter(Arc<VectorIndex>)` -- implements `VectorStore`
- `EmbedAdapter(Arc<dyn EmbeddingProvider>)` -- implements `EmbedService`

**Responsibilities:**
- Map concrete method calls to trait method signatures
- Convert crate-specific errors to `CoreError`
- Hold `Arc` references for safe sharing

### C5: Async Wrappers (`unimatrix-core/src/async_wrappers.rs`)

Feature-gated (`async` feature) async wrappers that use `tokio::task::spawn_blocking`.

**Wrappers:**
- `AsyncEntryStore<T: EntryStore + Send + Sync + 'static>` -- wraps any `EntryStore` in async
- `AsyncVectorStore<T: VectorStore + Send + Sync + 'static>` -- wraps any `VectorStore` in async
- `AsyncEmbedService<T: EmbedService + Send + Sync + 'static>` -- wraps any `EmbedService` in async

**Responsibilities:**
- Take `Arc<T>` in constructor
- Expose async versions of all trait methods
- Delegate to `tokio::task::spawn_blocking(move || inner.method(args))`
- Convert `JoinError` to `CoreError`

### C6: Security Schema Fields (`unimatrix-store/src/schema.rs`)

Seven new fields appended to `EntryRecord`, after the existing `embedding_dim` field.

**New fields (in order):**
1. `created_by: String` -- `#[serde(default)]`
2. `modified_by: String` -- `#[serde(default)]`
3. `content_hash: String` -- `#[serde(default)]`
4. `previous_hash: String` -- `#[serde(default)]`
5. `version: u32` -- `#[serde(default)]`
6. `feature_cycle: String` -- `#[serde(default)]`
7. `trust_source: String` -- `#[serde(default)]`

**NewEntry extensions:**
- Add `created_by: String`, `feature_cycle: String`, `trust_source: String` as caller-provided fields

### C7: Content Hash Computation (`unimatrix-store/src/hash.rs`)

SHA-256 computation module for content integrity.

**Responsibilities:**
- `compute_content_hash(title: &str, content: &str) -> String` -- returns lowercase hex SHA-256 of `"{title}: {content}"`
- Uses the `sha2` crate (pure Rust, no unsafe)
- Hash format matches `prepare_text(title, content, ": ")` from unimatrix-embed

### C8: Insert/Update Security Logic (`unimatrix-store/src/write.rs`)

Modifications to existing insert and update paths.

**Insert changes:**
- Compute `content_hash` via C7
- Set `version = 1`
- Set `modified_by = created_by` (from NewEntry)
- Set `previous_hash = ""`
- Accept `created_by`, `feature_cycle`, `trust_source` from NewEntry

**Update changes:**
- Read old `content_hash` -> store as `previous_hash`
- Compute new `content_hash` from updated title/content
- Increment `version` by 1
- `modified_by` comes from the caller's EntryRecord

### C9: Schema Migration (`unimatrix-store/src/migration.rs`)

Scan-and-rewrite migration triggered on database open.

**Responsibilities:**
- Define `CURRENT_SCHEMA_VERSION: u64 = 1`
- On `Store::open()`, read `schema_version` from COUNTERS (default 0)
- If behind: within a single write transaction, iterate all ENTRIES, deserialize, populate new fields, re-serialize, write back
- Migration field defaults: `created_by=""`, `modified_by=""`, `content_hash=<computed>`, `previous_hash=""`, `version=1`, `feature_cycle=""`, `trust_source="system"`
- Bump `schema_version` to `CURRENT_SCHEMA_VERSION`
- If no entries exist, just bump the version counter (skip scan)

### C10: Crate Setup (`unimatrix-core/Cargo.toml`)

New crate with appropriate dependencies.

**Dependencies:**
- `unimatrix-store` (path dependency)
- `unimatrix-vector` (path dependency)
- `unimatrix-embed` (path dependency)
- `thiserror` for error derives

**Optional dependencies (feature-gated):**
- `tokio = { version = "1", features = ["rt"], optional = true }`

**Features:**
- `async = ["tokio"]` -- enables async wrapper module
- `test-support = ["unimatrix-store/test-support", "unimatrix-vector/test-support", "unimatrix-embed/test-support"]`

## Component Interactions

### Data Flow: Insert Path (with security fields)

```
Caller (vnc-001)
  |
  v
NewEntry { title, content, topic, category, tags, source, status,
           created_by, feature_cycle, trust_source }
  |
  v
EntryStore::insert(new_entry) -> Result<u64>
  |
  v
StoreAdapter -> Store::insert()
  |
  v
1. Generate entry ID (COUNTERS)
2. Compute content_hash = sha256("{title}: {content}")
3. Build EntryRecord:
   - version = 1
   - modified_by = created_by
   - previous_hash = ""
   - content_hash = computed
   - (all other fields from NewEntry + engine defaults)
4. Serialize + write to ENTRIES
5. Update all 5 index tables
6. Commit transaction
  |
  v
Return entry_id: u64
```

### Data Flow: Update Path (with version tracking)

```
Caller
  |
  v
EntryRecord (with updated fields, modified_by set by caller)
  |
  v
EntryStore::update(record) -> Result<()>
  |
  v
StoreAdapter -> Store::update()
  |
  v
1. Read old record from ENTRIES
2. old_hash = old_record.content_hash
3. Compute new_hash = sha256("{new_title}: {new_content}")
4. Set record.previous_hash = old_hash
5. Set record.content_hash = new_hash
6. Set record.version = old_record.version + 1
7. Diff indexes, update as needed
8. Serialize + write to ENTRIES
9. Commit transaction
  |
  v
Return Ok(())
```

### Data Flow: Migration Path

```
Store::open(path)
  |
  v
1. Open database, create tables
2. Read schema_version from COUNTERS (default 0)
3. If schema_version >= CURRENT_SCHEMA_VERSION -> done
4. Begin write transaction
5. For each entry in ENTRIES:
   a. Deserialize (new fields default to zero/empty)
   b. Compute content_hash from title + content
   c. Set version=1, trust_source="system"
   d. Re-serialize and write back
6. Set schema_version = CURRENT_SCHEMA_VERSION
7. Commit transaction
```

### Data Flow: Async Wrapper

```
Caller (async context)
  |
  v
async_store.get(id).await
  |
  v
AsyncEntryStore::get()
  |
  v
tokio::task::spawn_blocking(move || {
    inner.get(id)
}).await?
  |
  v
Result<EntryRecord, CoreError>
```

## Technology Decisions

| Decision | Choice | Rationale | ADR |
|----------|--------|-----------|-----|
| Core traits location | New `unimatrix-core` crate | Single dependency for consumers; avoids circular deps | ADR-001 |
| Error unification | `CoreError` enum with `From` conversions | Each crate keeps its own error type; core provides unified view | ADR-002 |
| Async approach | Feature-gated `spawn_blocking` wrappers | Keeps core traits sync; async is opt-in via `async` feature | ADR-003 |
| Content hash algorithm | SHA-256 via `sha2` crate | Industry standard, pure Rust, matches security research recommendations | ADR-004 |
| Migration strategy | Scan-and-rewrite with schema_version counter | Established pattern from PRODUCT-VISION.md; atomic single-transaction | ADR-005 |
| Trait object safety | Object-safe traits with `Send + Sync` | Enables `dyn Trait` usage in MCP server; `Arc<dyn EntryStore>` | ADR-006 |

## Integration Points

### Existing Dependencies (Consumed)

| Component | Crate | Interface |
|-----------|-------|-----------|
| Entry storage | `unimatrix-store` | `Store` struct, `EntryRecord`, `NewEntry`, `QueryFilter`, `Status`, `TimeRange` |
| Vector search | `unimatrix-vector` | `VectorIndex` struct, `SearchResult`, `VectorConfig` |
| Embedding generation | `unimatrix-embed` | `EmbeddingProvider` trait, `OnnxProvider`, `embed_entry`, `embed_entries` |

### New Interfaces (Exposed)

| Component | Interface | Consumer |
|-----------|-----------|----------|
| `EntryStore` trait | Entry CRUD + query | vnc-001 MCP server |
| `VectorStore` trait | Vector insert + search | vnc-001 MCP server |
| `EmbedService` trait | Entry embedding | vnc-001 MCP server |
| `AsyncEntryStore` | Async entry operations | vnc-001 MCP server |
| `AsyncVectorStore` | Async vector operations | vnc-001 MCP server |
| `AsyncEmbedService` | Async embedding | vnc-001 MCP server |

### Future Consumers

| Feature | What it Consumes | Notes |
|---------|-----------------|-------|
| vnc-001 | All traits + async wrappers | Primary consumer, drives the async feature requirement |
| vnc-002 | `EntryStore::insert/update` | Content scanning wraps these calls |
| crt-001 | `EntryStore::update` | Usage tracking updates access_count |
| crt-003 | `VectorStore::search` + `EmbedService` | Contradiction detection needs both |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `EntryStore::insert` | `fn insert(&self, entry: NewEntry) -> Result<u64, CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::update` | `fn update(&self, entry: EntryRecord) -> Result<(), CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::update_status` | `fn update_status(&self, id: u64, status: Status) -> Result<(), CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::delete` | `fn delete(&self, id: u64) -> Result<(), CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::get` | `fn get(&self, id: u64) -> Result<EntryRecord, CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::exists` | `fn exists(&self, id: u64) -> Result<bool, CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::query` | `fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>, CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::query_by_topic` | `fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>, CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::query_by_category` | `fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>, CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::query_by_tags` | `fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>, CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::query_by_time_range` | `fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>, CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::query_by_status` | `fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>, CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::put_vector_mapping` | `fn put_vector_mapping(&self, entry_id: u64, hnsw_data_id: u64) -> Result<(), CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::get_vector_mapping` | `fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>, CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::iter_vector_mappings` | `fn iter_vector_mappings(&self) -> Result<Vec<(u64, u64)>, CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::read_counter` | `fn read_counter(&self, name: &str) -> Result<u64, CoreError>` | unimatrix-core/src/traits.rs |
| `EntryStore::compact` | `fn compact(&mut self) -> Result<(), CoreError>` | unimatrix-core/src/traits.rs |
| `VectorStore::insert` | `fn insert(&self, entry_id: u64, embedding: &[f32]) -> Result<(), CoreError>` | unimatrix-core/src/traits.rs |
| `VectorStore::search` | `fn search(&self, query: &[f32], top_k: usize, ef_search: usize) -> Result<Vec<SearchResult>, CoreError>` | unimatrix-core/src/traits.rs |
| `VectorStore::search_filtered` | `fn search_filtered(&self, query: &[f32], top_k: usize, ef_search: usize, allowed: &[u64]) -> Result<Vec<SearchResult>, CoreError>` | unimatrix-core/src/traits.rs |
| `VectorStore::point_count` | `fn point_count(&self) -> usize` | unimatrix-core/src/traits.rs |
| `VectorStore::contains` | `fn contains(&self, entry_id: u64) -> bool` | unimatrix-core/src/traits.rs |
| `VectorStore::stale_count` | `fn stale_count(&self) -> usize` | unimatrix-core/src/traits.rs |
| `EmbedService::embed_entry` | `fn embed_entry(&self, title: &str, content: &str) -> Result<Vec<f32>, CoreError>` | unimatrix-core/src/traits.rs |
| `EmbedService::embed_entries` | `fn embed_entries(&self, entries: &[(String, String)]) -> Result<Vec<Vec<f32>>, CoreError>` | unimatrix-core/src/traits.rs |
| `EmbedService::dimension` | `fn dimension(&self) -> usize` | unimatrix-core/src/traits.rs |
| `compute_content_hash` | `fn compute_content_hash(title: &str, content: &str) -> String` | unimatrix-store/src/hash.rs |
| `migrate_schema` | `fn migrate_schema(db: &redb::Database) -> Result<(), StoreError>` | unimatrix-store/src/migration.rs |
| `CURRENT_SCHEMA_VERSION` | `const CURRENT_SCHEMA_VERSION: u64 = 1` | unimatrix-store/src/migration.rs |

## Implementation Order

The components have clear dependency ordering:

```
C10 (crate-setup) -- must exist first
  |
  v
C6 (security schema fields) + C7 (content hash) -- schema changes in unimatrix-store
  |
  v
C9 (migration) -- depends on new schema being defined
  |
  v
C8 (insert/update security logic) -- depends on C6 + C7
  |
  v
C2 (core error) -- standalone, but needed before traits
  |
  v
C1 (core traits) -- depends on C2 for error types
  |
  v
C3 (type re-exports) -- depends on C1 being defined
  |
  v
C4 (domain adapters) -- depends on C1 (traits) + concrete crate types
  |
  v
C5 (async wrappers) -- depends on C1 (traits) + tokio
```
