# Specification: nxs-004 Core Traits & Domain Adapters

## Objective

Define trait abstractions over Unimatrix's three foundation crates, add 7 security fields to EntryRecord, implement scan-and-rewrite schema migration, and provide domain adapters with optional async wrappers. This completes Milestone 1 (Foundation) by establishing the trait contracts that the MCP server (vnc-001) will program against.

## Functional Requirements

### FR-01: Core Trait Definitions

The `unimatrix-core` crate defines three traits:

**FR-01a: `EntryStore` trait** with methods:
- `insert(entry: NewEntry) -> Result<u64, CoreError>`
- `update(entry: EntryRecord) -> Result<(), CoreError>`
- `update_status(id: u64, status: Status) -> Result<(), CoreError>`
- `delete(id: u64) -> Result<(), CoreError>`
- `get(id: u64) -> Result<EntryRecord, CoreError>`
- `exists(id: u64) -> Result<bool, CoreError>`
- `query(filter: QueryFilter) -> Result<Vec<EntryRecord>, CoreError>`
- `query_by_topic(topic: &str) -> Result<Vec<EntryRecord>, CoreError>`
- `query_by_category(category: &str) -> Result<Vec<EntryRecord>, CoreError>`
- `query_by_tags(tags: &[String]) -> Result<Vec<EntryRecord>, CoreError>`
- `query_by_time_range(range: TimeRange) -> Result<Vec<EntryRecord>, CoreError>`
- `query_by_status(status: Status) -> Result<Vec<EntryRecord>, CoreError>`
- `put_vector_mapping(entry_id: u64, hnsw_data_id: u64) -> Result<(), CoreError>`
- `get_vector_mapping(entry_id: u64) -> Result<Option<u64>, CoreError>`
- `iter_vector_mappings() -> Result<Vec<(u64, u64)>, CoreError>`
- `read_counter(name: &str) -> Result<u64, CoreError>`

**FR-01b: `VectorStore` trait** with methods:
- `insert(entry_id: u64, embedding: &[f32]) -> Result<(), CoreError>`
- `search(query: &[f32], top_k: usize, ef_search: usize) -> Result<Vec<SearchResult>, CoreError>`
- `search_filtered(query: &[f32], top_k: usize, ef_search: usize, allowed_entry_ids: &[u64]) -> Result<Vec<SearchResult>, CoreError>`
- `point_count() -> usize`
- `contains(entry_id: u64) -> bool`
- `stale_count() -> usize`

**FR-01c: `EmbedService` trait** with methods:
- `embed_entry(title: &str, content: &str) -> Result<Vec<f32>, CoreError>`
- `embed_entries(entries: &[(String, String)]) -> Result<Vec<Vec<f32>>, CoreError>`
- `dimension() -> usize`

### FR-02: Security Schema Fields

Seven new fields appended to `EntryRecord` after the existing `embedding_dim` field:

| Field | Type | Source | Default | serde(default) |
|-------|------|--------|---------|----------------|
| `created_by` | `String` | Caller (via NewEntry) | `""` | Yes |
| `modified_by` | `String` | Caller (via EntryRecord on update) / Engine (= created_by on insert) | `""` | Yes |
| `content_hash` | `String` | Engine-computed SHA-256 | `""` | Yes |
| `previous_hash` | `String` | Engine-computed (old content_hash on update) | `""` | Yes |
| `version` | `u32` | Engine-managed (starts at 1, increments on update) | `0` | Yes |
| `feature_cycle` | `String` | Caller (via NewEntry) | `""` | Yes |
| `trust_source` | `String` | Caller (via NewEntry) | `""` | Yes |

`NewEntry` extended with three caller-provided fields:
- `created_by: String`
- `feature_cycle: String`
- `trust_source: String`

### FR-03: Content Hash Computation

- Hash function: SHA-256 via `sha2` crate
- Hash input: `prepare_text(title, content, ": ")` -- same format as embedding pipeline
  - Both non-empty: `"{title}: {content}"`
  - Title empty: content only
  - Content empty: title only
  - Both empty: empty string
- Hash output: lowercase hex string (64 characters)
- Computed automatically on `insert()` and `update()`

### FR-04: Version Tracking

- On `insert()`: `version` = 1
- On `update()`: `version` = old `version` + 1
- `version` is engine-managed -- callers cannot set or override it
- On `update()`: `previous_hash` = old `content_hash`

### FR-05: Schema Migration

- `CURRENT_SCHEMA_VERSION: u64 = 1`
- On `Store::open()`, after table creation:
  1. Read `schema_version` from COUNTERS (default: 0)
  2. If `schema_version < CURRENT_SCHEMA_VERSION`:
     - In a single write transaction: iterate ENTRIES, deserialize each entry, populate new fields, re-serialize, write back
     - Migration field values: `content_hash` = computed SHA-256, `version` = 1, `trust_source` = "system", `created_by` = "", `modified_by` = "", `previous_hash` = "", `feature_cycle` = ""
     - Set `schema_version` = `CURRENT_SCHEMA_VERSION`
     - Commit transaction
  3. If no entries exist: set `schema_version` = `CURRENT_SCHEMA_VERSION` and commit

### FR-06: Domain Adapters

- `StoreAdapter` wraps `Arc<Store>`, implements `EntryStore`
- `VectorAdapter` wraps `Arc<VectorIndex>`, implements `VectorStore`
- `EmbedAdapter` wraps `Arc<dyn EmbeddingProvider>`, implements `EmbedService`
- Adapters delegate all method calls to the wrapped concrete type
- Adapters convert crate-specific errors to `CoreError`

### FR-07: Async Wrappers (feature-gated)

- Feature flag: `async = ["dep:tokio"]`
- `AsyncEntryStore<T: EntryStore + Send + Sync + 'static>` wraps `Arc<T>`
- `AsyncVectorStore<T: VectorStore + Send + Sync + 'static>` wraps `Arc<T>`
- `AsyncEmbedService<T: EmbedService + Send + Sync + 'static>` wraps `Arc<T>`
- Each async method clones the `Arc`, calls `tokio::task::spawn_blocking`, and awaits
- `JoinError` from tokio is converted to `CoreError::JoinError`

### FR-08: Core Error Type

```rust
pub enum CoreError {
    Store(StoreError),
    Vector(VectorError),
    Embed(EmbedError),
    JoinError(String),
}
```

- Implements `std::fmt::Display`, `std::error::Error`
- Implements `From<StoreError>`, `From<VectorError>`, `From<EmbedError>`

### FR-09: Type Re-exports

`unimatrix-core` re-exports from lower crates:

From `unimatrix-store`:
- `EntryRecord`, `NewEntry`, `QueryFilter`, `Status`, `TimeRange`, `DatabaseConfig`, `Store`, `StoreError`

From `unimatrix-vector`:
- `SearchResult`, `VectorConfig`, `VectorIndex`, `VectorError`

From `unimatrix-embed`:
- `EmbeddingProvider`, `EmbedConfig`, `OnnxProvider`, `EmbedError`

## Non-Functional Requirements

### NFR-01: Backward Compatibility

All 246 existing tests across unimatrix-store (85), unimatrix-vector (85), and unimatrix-embed (76) must continue to pass. Schema changes in unimatrix-store are backward compatible via migration.

### NFR-02: Object Safety

All three core traits must be object-safe: usable as `dyn EntryStore`, `dyn VectorStore`, `dyn EmbedService`. This enables dynamic dispatch in the MCP server.

### NFR-03: Thread Safety

All three core traits require `Send + Sync` bounds. All adapters and async wrappers are `Send + Sync`. Compatible with `Arc<dyn Trait>` sharing across tokio tasks.

### NFR-04: Migration Performance

Schema migration must complete in under 1 second for databases with up to 10,000 entries. At Unimatrix scale (<1000 entries at M1 completion), this should be under 100ms.

### NFR-05: No Unsafe Code

`#![forbid(unsafe_code)]` maintained on all crates including the new `unimatrix-core`.

### NFR-06: Hash Determinism

`compute_content_hash(title, content)` must be deterministic: identical inputs always produce identical output. No randomness, no timestamps in the hash computation.

### NFR-07: Migration Atomicity

Schema migration uses a single write transaction. On failure (crash, I/O error), the database remains at the old schema version. Next open attempt retries the migration.

## Acceptance Criteria (from SCOPE.md)

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `unimatrix-core` crate exists with `EntryStore`, `VectorStore`, `EmbedService` traits | Compilation test; trait definitions exist in source |
| AC-02 | `EntryStore` has 16 methods matching Store's public API | Compilation test; adapter implements all methods |
| AC-03 | `VectorStore` has 6 methods matching VectorIndex's public API | Compilation test; adapter implements all methods |
| AC-04 | `EmbedService` has 3 methods for entry-level embedding | Compilation test; adapter implements all methods |
| AC-05 | `EntryRecord` has 7 new security fields with `#[serde(default)]` | Unit test: roundtrip serialization with all fields |
| AC-06 | `NewEntry` accepts `created_by`, `feature_cycle`, `trust_source` | Compilation test; insert with all new fields |
| AC-07 | `insert()` computes content_hash, sets version=1, modified_by=created_by | Unit test: insert and verify all computed fields |
| AC-08 | `update()` sets previous_hash, computes new content_hash, increments version | Unit test: update and verify hash chain + version |
| AC-09 | Migration rewrites entries on schema version mismatch | Integration test: create DB with old schema, reopen, verify fields |
| AC-10 | Migration is atomic (single write transaction) | Architecture review; redb transaction semantics |
| AC-11 | After migration, schema_version equals CURRENT_SCHEMA_VERSION | Integration test: read counter after migration |
| AC-12 | Domain adapters implement core traits | Compilation test; type-check assertions |
| AC-13 | Async wrappers use spawn_blocking for all trait methods | Code review; feature-gated compilation test |
| AC-14 | All unimatrix-store tests pass | `cargo test -p unimatrix-store` |
| AC-15 | All unimatrix-vector tests pass | `cargo test -p unimatrix-vector` |
| AC-16 | All unimatrix-embed tests pass | `cargo test -p unimatrix-embed` |
| AC-17 | Roundtrip serialization with all 7 new fields | Unit test in schema.rs |
| AC-18 | content_hash is SHA-256 hex of `"{title}: {content}"` | Unit test with known SHA-256 values |
| AC-19 | version starts at 1, increments on update | Unit test: insert -> check 1, update -> check 2, update -> check 3 |
| AC-20 | Traits are object-safe | Compilation test: `fn _check(x: &dyn EntryStore) {}` |
| AC-21 | Traits require Send + Sync | Compilation test: `fn _check<T: Send + Sync>() {}` applied to trait objects |
| AC-22 | `#![forbid(unsafe_code)]` on all crates | Compilation check |

## Domain Models

### EntryRecord (Extended)

The canonical knowledge entry. All fields (original 17 + new 7 = 24):

```
EntryRecord {
    // Core identity
    id: u64,                    // Engine-assigned, monotonic, starts at 1
    title: String,              // Human-readable title
    content: String,            // Knowledge content body

    // Classification
    topic: String,              // Primary topic (indexed)
    category: String,           // Category (indexed)
    tags: Vec<String>,          // Tag set (intersection-queryable)
    source: String,             // Origin identifier

    // Lifecycle
    status: Status,             // Active | Deprecated | Proposed
    confidence: f32,            // Computed confidence score (default 0.0)
    created_at: u64,            // Unix timestamp (engine-set)
    updated_at: u64,            // Unix timestamp (engine-set)

    // Usage tracking (populated by crt-001)
    last_accessed_at: u64,      // Unix timestamp of last read
    access_count: u32,          // Total read count

    // Correction chain
    supersedes: Option<u64>,    // Entry this one replaces
    superseded_by: Option<u64>, // Entry that replaces this one
    correction_count: u32,      // Number of corrections made

    // Embedding
    embedding_dim: u16,         // Dimension of stored embedding (0 = no embedding)

    // Security (NEW in nxs-004)
    created_by: String,         // Agent ID that created this entry
    modified_by: String,        // Agent ID that last modified
    content_hash: String,       // SHA-256 hex of "{title}: {content}"
    previous_hash: String,      // content_hash before last update
    version: u32,               // Starts at 1, increments on update
    feature_cycle: String,      // Feature ID (e.g., "nxs-003")
    trust_source: String,       // "agent" | "human" | "system"
}
```

### NewEntry (Extended)

Input for creating new entries. Engine-assigned fields excluded.

```
NewEntry {
    title: String,
    content: String,
    topic: String,
    category: String,
    tags: Vec<String>,
    source: String,
    status: Status,
    // NEW in nxs-004:
    created_by: String,
    feature_cycle: String,
    trust_source: String,
}
```

### CoreError

Unified error type for trait methods.

```
CoreError {
    Store(StoreError),       // Storage engine errors
    Vector(VectorError),     // Vector index errors
    Embed(EmbedError),       // Embedding errors
    JoinError(String),       // Async wrapper: tokio task failure
}
```

### Schema Version Model

```
COUNTERS table:
  "schema_version" -> u64   (0 = pre-nxs-004, 1 = nxs-004+)
  "next_entry_id" -> u64    (existing)
  "total_active" -> u64     (existing)
  "total_deprecated" -> u64 (existing)
  "total_proposed" -> u64   (existing)
```

## User Workflows

### Workflow 1: MCP Server Initialization (vnc-001 consuming nxs-004)

```
1. Open Store (triggers migration if needed)
2. Create VectorIndex with Arc<Store>
3. Create OnnxProvider (embedding model)
4. Create adapters:
   - StoreAdapter(Arc::new(store))
   - VectorAdapter(Arc::new(vector_index))
   - EmbedAdapter(Arc::new(provider))
5. Create async wrappers:
   - AsyncEntryStore::new(Arc::new(store_adapter))
   - AsyncVectorStore::new(Arc::new(vector_adapter))
   - AsyncEmbedService::new(Arc::new(embed_adapter))
6. Use async wrappers in MCP tool handlers
```

### Workflow 2: Entry Insert with Security Fields

```
1. Caller constructs NewEntry with created_by, feature_cycle, trust_source
2. Calls entry_store.insert(new_entry)
3. Engine:
   a. Generates entry ID
   b. Computes content_hash = sha256("{title}: {content}")
   c. Sets version = 1
   d. Sets modified_by = created_by
   e. Sets previous_hash = ""
   f. Writes to ENTRIES + all index tables
4. Returns entry_id
```

### Workflow 3: Entry Update with Hash Chain

```
1. Caller reads entry via entry_store.get(id)
2. Modifies fields (title, content, etc.)
3. Sets modified_by on the record
4. Calls entry_store.update(modified_record)
5. Engine:
   a. Reads old record
   b. Sets previous_hash = old.content_hash
   c. Computes new content_hash
   d. Sets version = old.version + 1
   e. Diffs indexes, updates as needed
   f. Writes updated record
```

### Workflow 4: Schema Migration (Transparent)

```
1. Application calls Store::open(path)
2. Tables are created/opened
3. Migration checks schema_version
4. If version 0 (pre-nxs-004):
   a. Reads all entries
   b. For each: compute content_hash, set defaults
   c. Re-serializes and writes back
   d. Sets schema_version = 1
5. Store is ready for use (all entries have new fields)
```

## Constraints

- Rust edition 2024, MSRV 1.89
- bincode v2 serde-compatible path (encode_to_vec / decode_from_slice)
- Fields append-only on EntryRecord (positional encoding contract)
- `#![forbid(unsafe_code)]` on all crates
- sha2 crate for SHA-256 (pure Rust)
- tokio dependency only under `async` feature flag
- No changes to redb table layout (8 tables unchanged)

## Dependencies

| Dependency | Version | Purpose | New? |
|-----------|---------|---------|------|
| `unimatrix-store` | path | Entry storage, schema, migration | Existing (modified) |
| `unimatrix-vector` | path | Vector search | Existing (unmodified) |
| `unimatrix-embed` | path | Embedding generation | Existing (unmodified) |
| `sha2` | latest | SHA-256 content hashing | NEW |
| `thiserror` | 2 | CoreError derives | NEW for core |
| `tokio` | 1 (optional) | Async wrappers (spawn_blocking) | NEW, feature-gated |

## NOT in Scope

- MCP server implementation (vnc-001)
- Agent Registry or Audit Log tables (vnc-001)
- Input validation or content scanning (vnc-002)
- Confidence computation (crt-002)
- New redb tables
- Vector index or embedding crate internal changes
- Runtime category allowlist or trust level enforcement
- CLI commands
