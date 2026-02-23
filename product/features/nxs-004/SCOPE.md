# nxs-004: Core Traits & Domain Adapters

## Problem Statement

Unimatrix has three independent crates (unimatrix-store, unimatrix-vector, unimatrix-embed) that work but have no shared abstraction layer. Each crate exposes concrete types directly -- `Store` with its redb methods, `VectorIndex` with its hnsw_rs internals, `OnnxProvider` with its ONNX runtime details. This creates three problems:

1. **No trait contracts.** Downstream consumers (vnc-001 MCP server) must depend on concrete implementations. There is no way to mock, swap, or test components independently. The MCP server will need to compose all three crates behind a unified interface.

2. **No async boundary.** All three crates are synchronous (redb is sync-only, hnsw_rs is sync-only, ONNX inference blocks). The MCP server (vnc-001) needs async operation via `tokio::task::spawn_blocking` with `Arc`-wrapped components. This wrapping pattern must be defined once, not reinvented in every consumer.

3. **No security schema fields.** The current `EntryRecord` has no provenance, integrity, or versioning fields. Once vnc-002 ships `context_store`, agent-written entries start flowing. Any security field not present at that point creates a permanent gap in the audit chain. The product vision mandates 7 security fields be added in nxs-004 before the MCP layer exists.

4. **No migration capability.** The product vision describes scan-and-rewrite migration triggered by `schema_version` in the COUNTERS table. This mechanism does not exist yet. nxs-004 is the first schema evolution event and must establish the migration pattern for all future field additions.

This is the last feature in Milestone 1 (Foundation). It bridges the concrete implementations (nxs-001/002/003) to the MCP server (vnc-001) by providing trait abstractions, async adapters, security schema fields, and the migration mechanism.

## Goals

1. Define storage trait `EntryStore` in a new `unimatrix-core` crate that abstracts the entry CRUD and query operations currently on `Store`.
2. Define storage trait `VectorStore` in `unimatrix-core` that abstracts vector insert, search, and filtered search operations currently on `VectorIndex`.
3. Define trait `EmbedService` in `unimatrix-core` that abstracts embedding generation (wrapping the existing `EmbeddingProvider` trait from unimatrix-embed with entry-level semantics).
4. Add 7 security fields to `EntryRecord`: `created_by` (String), `modified_by` (String), `content_hash` (String, SHA-256), `previous_hash` (String), `version` (u32), `feature_cycle` (String), `trust_source` (String: "agent"|"human"|"system").
5. Implement scan-and-rewrite migration capability: on database open, check `schema_version` in COUNTERS against the code's current version; if behind, rewrite all entries with the new schema and bump the counter. This is the first migration event and establishes the pattern.
6. Implement domain adapter structs that implement the core traits by delegating to the concrete crate types (`StoreAdapter` implementing `EntryStore`, `VectorAdapter` implementing `VectorStore`, `EmbedAdapter` implementing `EmbedService`).
7. Provide async wrapper types that take `Arc<T>` where `T: EntryStore` (or `VectorStore`, `EmbedService`) and expose async methods using `tokio::task::spawn_blocking`.
8. Update `NewEntry` to accept the new security-relevant caller-provided fields (`created_by`, `feature_cycle`, `trust_source`) and have the engine auto-populate computed fields (`content_hash`, `version`, `modified_by`, `previous_hash`) on insert and update.
9. Ensure `content_hash` is computed as SHA-256 of `"{title}: {content}"` on every insert and update, and `previous_hash` is set to the old `content_hash` on update (empty string on initial insert).
10. Ensure `version` starts at 1 on insert and increments on every update.

## Non-Goals

- **No MCP server.** The MCP server skeleton, transport, lifecycle, and tool definitions are vnc-001/vnc-002. This feature provides the trait layer they consume.
- **No Agent Registry or Audit Log tables.** These are vnc-001 responsibilities. The `created_by`/`modified_by` fields on EntryRecord are populated by the caller (the MCP server will fill them from agent identity).
- **No input validation or content scanning.** These are vnc-002 responsibilities. The traits accept what the caller provides.
- **No confidence computation changes.** The `confidence` field remains as-is (f32, default 0.0). Computation logic is crt-002.
- **No new redb tables.** The 8 existing tables are sufficient. Security fields are columns on EntryRecord, not separate tables.
- **No changes to the vector index or embedding crate internals.** Adapters wrap existing APIs; they do not modify hnsw_rs or ONNX internals.
- **No runtime category allowlist or trust level enforcement.** These are vnc-001/vnc-002 concerns. The `trust_source` field on EntryRecord is a string populated by the caller.
- **No unconditional async runtime dependency in unimatrix-core.** The core traits themselves are runtime-agnostic (synchronous `Result<T>`). Async wrappers are feature-gated behind `[features] async = ["tokio"]` so the base crate has no tokio dependency.

## Background Research

### Existing Crate APIs (Current State)

**unimatrix-store** (85 tests):
- `Store::open(path)` / `Store::open_with_config(path, config)` -- creates/opens redb database
- `Store::insert(NewEntry) -> Result<u64>` -- atomic multi-table insert, returns entry ID
- `Store::update(EntryRecord) -> Result<()>` -- atomic multi-table update with index diffing
- `Store::update_status(id, Status) -> Result<()>` -- specialized status transition
- `Store::delete(id) -> Result<()>` -- atomic multi-table delete
- `Store::get(id) -> Result<EntryRecord>` -- point lookup
- `Store::exists(id) -> Result<bool>` -- existence check
- `Store::query(QueryFilter) -> Result<Vec<EntryRecord>>` -- combined filter with set intersection
- `Store::query_by_topic/category/tags/time_range/status` -- individual index queries
- `Store::put_vector_mapping/get_vector_mapping/iter_vector_mappings` -- VECTOR_MAP operations
- `Store::read_counter(name) -> Result<u64>` -- counter reads
- `Store::compact() -> Result<()>` -- database compaction

**unimatrix-vector** (85 tests):
- `VectorIndex::new(Arc<Store>, VectorConfig) -> Result<Self>` -- creates empty index
- `VectorIndex::insert(entry_id, &[f32]) -> Result<()>` -- insert/re-embed vector
- `VectorIndex::search(query, top_k, ef_search) -> Result<Vec<SearchResult>>` -- similarity search
- `VectorIndex::search_filtered(query, top_k, ef_search, &[u64]) -> Result<Vec<SearchResult>>` -- filtered search
- `VectorIndex::point_count/contains/stale_count/config` -- index metadata
- Persistence: `dump(path)` / `load(path, Arc<Store>)` -- explicit file-based persistence

**unimatrix-embed** (76 active tests):
- `EmbeddingProvider` trait: `embed(&str)`, `embed_batch(&[&str])`, `dimension()`, `name()`
- `OnnxProvider::new(EmbedConfig) -> Result<Self>` -- loads ONNX model
- `embed_entry(provider, title, content, separator) -> Result<Vec<f32>>` -- entry-level embedding
- `embed_entries(provider, &[(String,String)], separator) -> Result<Vec<Vec<f32>>>` -- batch embedding
- `prepare_text(title, content, separator) -> String` -- text concatenation

### Schema Evolution Contract (Documented in nxs-001)

From `schema.rs` tests: bincode v2 with serde path uses positional encoding. `#[serde(default)]` does NOT handle missing trailing fields during bincode deserialization. The actual contract is:
- All inserts/updates write the FULL current EntryRecord.
- When new fields are added, a one-time migration rewrites all existing records.
- Fields are only appended, never removed or reordered.

The COUNTERS table exists and is used for `next_entry_id` and status counters. Adding a `schema_version` counter is straightforward.

### Security Schema (From PRODUCT-VISION.md and Security Research)

Seven fields mandated for nxs-004:
- `created_by: String` -- agent ID that created the entry (caller-provided)
- `modified_by: String` -- agent ID that last modified the entry (caller-provided)
- `content_hash: String` -- SHA-256 of `"{title}: {content}"` (engine-computed)
- `previous_hash: String` -- content_hash before last update (engine-computed)
- `version: u32` -- incremented on each update, starts at 1 (engine-managed)
- `feature_cycle: String` -- feature ID that generated this entry (caller-provided)
- `trust_source: String` -- "agent"|"human"|"system" (caller-provided)

All fields get `#[serde(default)]` for consistency with existing extension fields. On migration, defaults are: `created_by = ""`, `modified_by = ""`, `content_hash = <computed from existing title+content>`, `previous_hash = ""`, `version = 1`, `feature_cycle = ""`, `trust_source = "system"`.

### Async Pattern (From ASS-003 Research)

The established pattern from redb research (ASS-003 D2):
```rust
let store = Arc::new(store);
let s = Arc::clone(&store);
let result = tokio::task::spawn_blocking(move || s.get(id)).await??;
```

nxs-004 formalizes this into typed async wrapper structs.

### Content Hash Format

SHA-256 of `"{title}: {content}"` matches the `prepare_text(title, content, ": ")` format used by the embedding pipeline (nxs-003). This ensures the content hash covers the same text that gets embedded.

## Proposed Approach

### New Crate: `unimatrix-core`

A new crate `crates/unimatrix-core/` containing:

1. **Core traits** (`traits.rs`): `EntryStore`, `VectorStore`, `EmbedService` -- pure trait definitions with no implementation dependencies.
2. **Domain types** (re-exported from unimatrix-store): `EntryRecord`, `NewEntry`, `QueryFilter`, `Status`, `TimeRange`, `SearchResult` -- shared types used by traits.
3. **Error types**: A core error enum that trait methods return, with conversions from crate-specific errors.
4. **Async wrappers** (`async_adapters.rs`): Generic async wrappers over any trait implementor, using `tokio::task::spawn_blocking`.

### Schema Changes in `unimatrix-store`

1. Add 7 fields to `EntryRecord` (all `#[serde(default)]`).
2. Extend `NewEntry` with caller-provided fields: `created_by`, `feature_cycle`, `trust_source`.
3. Update `Store::insert()` to compute `content_hash`, set `version = 1`, set `modified_by = created_by`.
4. Update `Store::update()` to compute new `content_hash`, set `previous_hash`, increment `version`, require `modified_by`.
5. Add `schema_version` counter and migration function.
6. Add `sha2` dependency for SHA-256.

### Domain Adapters in `unimatrix-core`

Thin adapter structs that implement core traits by delegating to concrete types:
- `StoreAdapter(Arc<Store>)` implementing `EntryStore`
- `VectorAdapter(Arc<VectorIndex>)` implementing `VectorStore`
- `EmbedAdapter(Arc<dyn EmbeddingProvider>)` implementing `EmbedService`

### Migration Mechanism

On `Store::open()`:
1. Read `schema_version` from COUNTERS (default: 0 if absent, meaning pre-nxs-004).
2. If `schema_version < CURRENT_SCHEMA_VERSION`:
   - Scan all entries in ENTRIES table.
   - Deserialize each with old schema (fields default to zero/empty via serde).
   - Populate missing fields (compute content_hash, set version=1, etc.).
   - Re-serialize and write back.
   - Update `schema_version` to `CURRENT_SCHEMA_VERSION`.
3. All within a single write transaction for atomicity.

## Acceptance Criteria

- AC-01: A `unimatrix-core` crate exists at `crates/unimatrix-core/` with `EntryStore`, `VectorStore`, and `EmbedService` traits.
- AC-02: `EntryStore` trait methods cover: insert, update, update_status, delete, get, exists, query, query_by_topic, query_by_category, query_by_tags, query_by_time_range, query_by_status, put_vector_mapping, get_vector_mapping, iter_vector_mappings, read_counter.
- AC-03: `VectorStore` trait methods cover: insert, search, search_filtered, point_count, contains, stale_count.
- AC-04: `EmbedService` trait methods cover: embed_entry, embed_entries, dimension.
- AC-05: `EntryRecord` has 7 new security fields: `created_by`, `modified_by`, `content_hash`, `previous_hash`, `version`, `feature_cycle`, `trust_source`. All with `#[serde(default)]`.
- AC-06: `NewEntry` accepts `created_by`, `feature_cycle`, `trust_source` as caller-provided fields.
- AC-07: `Store::insert()` auto-computes `content_hash` (SHA-256 of `"{title}: {content}"`), sets `version = 1`, sets `modified_by = created_by`, sets `previous_hash = ""`.
- AC-08: `Store::update()` sets `previous_hash` to old `content_hash`, computes new `content_hash`, increments `version`, requires `modified_by` on the input record.
- AC-09: Scan-and-rewrite migration runs on `Store::open()` when `schema_version` counter is behind CURRENT_SCHEMA_VERSION. All existing entries are rewritten with new fields populated (content_hash computed, version=1, trust_source="system", others="").
- AC-10: Migration is atomic (single write transaction). If migration fails, the database is not left in a partial state.
- AC-11: After migration, `schema_version` counter equals CURRENT_SCHEMA_VERSION.
- AC-12: Domain adapters exist: `StoreAdapter` implements `EntryStore`, `VectorAdapter` implements `VectorStore`, `EmbedAdapter` implements `EmbedService`.
- AC-13: Async wrappers exist that take `Arc<T: Trait + Send + Sync + 'static>` and expose async methods via `tokio::task::spawn_blocking`.
- AC-14: All existing unimatrix-store tests pass (backward compatible schema change).
- AC-15: All existing unimatrix-vector tests pass (no behavioral changes).
- AC-16: All existing unimatrix-embed tests pass (no behavioral changes).
- AC-17: Serialization roundtrip tests pass for EntryRecord with all 7 new fields populated and with defaults.
- AC-18: `content_hash` computation uses SHA-256 and matches the format `sha256("{title}: {content}")`.
- AC-19: `version` field starts at 1 on insert and increments to 2, 3, ... on successive updates.
- AC-20: Core traits are object-safe (usable as `dyn EntryStore`, `dyn VectorStore`, `dyn EmbedService`).
- AC-21: Core traits require `Send + Sync` bounds (compatible with `Arc` sharing).
- AC-22: `#![forbid(unsafe_code)]` is maintained on all crates.

## Constraints

- **Rust edition 2024, MSRV 1.89** -- workspace standard.
- **bincode v2 serde-compatible path** -- must use `bincode::serde::encode_to_vec` / `decode_from_slice`, NOT native Encode/Decode derives. This is a hard constraint from nxs-001.
- **Fields append-only on EntryRecord** -- new fields must be added after the last existing field (`embedding_dim: u16`). Never reorder or remove fields (bincode positional encoding contract).
- **No `unsafe` code** -- `#![forbid(unsafe_code)]` on all crates.
- **No async runtime in core traits** -- traits return synchronous `Result<T>`. Async wrappers are separate. This keeps traits usable in sync contexts.
- **tokio dependency** -- async wrappers use tokio's `spawn_blocking`. This is acceptable since vnc-001 (MCP server) will use tokio anyway. Gated behind a feature flag if needed.
- **sha2 crate** -- new dependency for SHA-256 content hash computation. Pure Rust, no unsafe.
- **Existing test count must not decrease** -- 246 tests across three crates must continue to pass.

## Resolved Decisions

1. **Async wrappers**: Feature-gated in `unimatrix-core` using `[features] async = ["tokio"]` with optional tokio dependency. NOT a separate crate. The async wrappers are thin `spawn_blocking` delegation and will not grow significantly.

2. **Re-exports**: `unimatrix-core` re-exports domain types (`EntryRecord`, `NewEntry`, `QueryFilter`, `Status`, `TimeRange`, `SearchResult`) from `unimatrix-store` and `unimatrix-vector`. Downstream consumers (vnc-001) only need `unimatrix-core` as a dependency.

3. **Migration backfill**: Yes, compute `content_hash` (SHA-256) for all existing entries during the scan-and-rewrite migration. The data is already being read for rewrite, and leaving content_hash empty would violate the integrity model.

## Open Questions

None -- all scope questions resolved.

## Tracking

https://github.com/dug-21/unimatrix/issues/7
