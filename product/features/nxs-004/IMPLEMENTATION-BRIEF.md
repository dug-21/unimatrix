# Implementation Brief: nxs-004 Core Traits & Domain Adapters

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/nxs-004/SCOPE.md |
| Architecture | product/features/nxs-004/architecture/ARCHITECTURE.md |
| Specification | product/features/nxs-004/specification/SPECIFICATION.md |
| Risk Strategy | product/features/nxs-004/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nxs-004/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| crate-setup | pseudocode/crate-setup.md | test-plan/crate-setup.md |
| security-schema | pseudocode/security-schema.md | test-plan/security-schema.md |
| content-hash | pseudocode/content-hash.md | test-plan/content-hash.md |
| migration | pseudocode/migration.md | test-plan/migration.md |
| write-security | pseudocode/write-security.md | test-plan/write-security.md |
| core-error | pseudocode/core-error.md | test-plan/core-error.md |
| core-traits | pseudocode/core-traits.md | test-plan/core-traits.md |
| adapters | pseudocode/adapters.md | test-plan/adapters.md |
| async-wrappers | pseudocode/async-wrappers.md | test-plan/async-wrappers.md |
| re-exports | pseudocode/re-exports.md | test-plan/re-exports.md |

## Goal

Define trait abstractions (`EntryStore`, `VectorStore`, `EmbedService`) over Unimatrix's three foundation crates in a new `unimatrix-core` crate, add 7 security fields to `EntryRecord` with SHA-256 content hashing and version tracking, implement the first scan-and-rewrite schema migration, and provide domain adapters with feature-gated async wrappers. This completes Milestone 1 (Foundation).

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Where to host traits | New `unimatrix-core` crate, re-exports domain types | SCOPE.md + Architecture | architecture/ADR-001-core-crate-as-trait-host.md |
| Error type unification | `CoreError` enum with `From` conversions from each crate's error | Architecture | architecture/ADR-002-unified-core-error.md |
| Async wrapper location | Feature-gated in unimatrix-core (`async = ["tokio"]`) | SCOPE.md decision #1 | architecture/ADR-003-feature-gated-async-wrappers.md |
| Content hash algorithm | SHA-256 of `"{title}: {content}"` via `sha2` crate | SCOPE.md + Architecture | architecture/ADR-004-sha256-content-hash.md |
| Migration strategy | Eager scan-and-rewrite with `schema_version` counter in COUNTERS | Architecture | architecture/ADR-005-scan-and-rewrite-migration.md |
| Trait object safety | Object-safe `Send + Sync` traits; `compact()` excluded | Architecture | architecture/ADR-006-object-safe-send-sync-traits.md |
| Domain type re-exports | unimatrix-core re-exports from store/vector/embed | SCOPE.md decision #2 | N/A |
| Migration backfill | Compute `content_hash` for existing entries during migration | SCOPE.md decision #3 | N/A |
| update_status and version | `update_status()` does NOT increment `version` (metadata-only change) | Specification FR-04 | N/A |

## Files to Create/Modify

### New Files (unimatrix-core crate)

| File | Purpose |
|------|---------|
| `crates/unimatrix-core/Cargo.toml` | Crate manifest with deps on store/vector/embed, optional tokio |
| `crates/unimatrix-core/src/lib.rs` | Module declarations, type re-exports |
| `crates/unimatrix-core/src/traits.rs` | `EntryStore`, `VectorStore`, `EmbedService` trait definitions |
| `crates/unimatrix-core/src/error.rs` | `CoreError` enum with `From` impls |
| `crates/unimatrix-core/src/adapters.rs` | `StoreAdapter`, `VectorAdapter`, `EmbedAdapter` |
| `crates/unimatrix-core/src/async_wrappers.rs` | `AsyncEntryStore`, `AsyncVectorStore`, `AsyncEmbedService` (feature-gated) |

### Modified Files (unimatrix-store crate)

| File | Change |
|------|--------|
| `crates/unimatrix-store/Cargo.toml` | Add `sha2` dependency |
| `crates/unimatrix-store/src/schema.rs` | Add 7 fields to `EntryRecord`, extend `NewEntry` |
| `crates/unimatrix-store/src/write.rs` | Add content_hash computation, version tracking, security field logic to insert() and update() |
| `crates/unimatrix-store/src/lib.rs` | Add `mod hash; mod migration;` |
| `crates/unimatrix-store/src/hash.rs` | New: `compute_content_hash()` function |
| `crates/unimatrix-store/src/migration.rs` | New: Schema migration logic |
| `crates/unimatrix-store/src/db.rs` | Call migration on `Store::open()` |
| `crates/unimatrix-store/src/test_helpers.rs` | Extend `TestEntry` builder with new fields |

### Unmodified Files

- `crates/unimatrix-vector/` -- no changes (adapters wrap, don't modify)
- `crates/unimatrix-embed/` -- no changes (adapters wrap, don't modify)

## Data Structures

### EntryRecord (24 fields, after nxs-004)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntryRecord {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub topic: String,
    pub category: String,
    pub tags: Vec<String>,
    pub source: String,
    pub status: Status,
    #[serde(default)]
    pub confidence: f32,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub last_accessed_at: u64,
    #[serde(default)]
    pub access_count: u32,
    #[serde(default)]
    pub supersedes: Option<u64>,
    #[serde(default)]
    pub superseded_by: Option<u64>,
    #[serde(default)]
    pub correction_count: u32,
    #[serde(default)]
    pub embedding_dim: u16,
    // -- nxs-004 security fields (appended after embedding_dim) --
    #[serde(default)]
    pub created_by: String,
    #[serde(default)]
    pub modified_by: String,
    #[serde(default)]
    pub content_hash: String,
    #[serde(default)]
    pub previous_hash: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub feature_cycle: String,
    #[serde(default)]
    pub trust_source: String,
}
```

### NewEntry (10 fields, after nxs-004)

```rust
#[derive(Debug, Clone)]
pub struct NewEntry {
    pub title: String,
    pub content: String,
    pub topic: String,
    pub category: String,
    pub tags: Vec<String>,
    pub source: String,
    pub status: Status,
    // -- nxs-004 caller-provided fields --
    pub created_by: String,
    pub feature_cycle: String,
    pub trust_source: String,
}
```

### CoreError

```rust
#[derive(Debug)]
pub enum CoreError {
    Store(unimatrix_store::StoreError),
    Vector(unimatrix_vector::VectorError),
    Embed(unimatrix_embed::EmbedError),
    JoinError(String),
}
```

## Function Signatures

### Core Traits (unimatrix-core/src/traits.rs)

```rust
pub trait EntryStore: Send + Sync {
    fn insert(&self, entry: NewEntry) -> Result<u64, CoreError>;
    fn update(&self, entry: EntryRecord) -> Result<(), CoreError>;
    fn update_status(&self, id: u64, status: Status) -> Result<(), CoreError>;
    fn delete(&self, id: u64) -> Result<(), CoreError>;
    fn get(&self, id: u64) -> Result<EntryRecord, CoreError>;
    fn exists(&self, id: u64) -> Result<bool, CoreError>;
    fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>, CoreError>;
    fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>, CoreError>;
    fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>, CoreError>;
    fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>, CoreError>;
    fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>, CoreError>;
    fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>, CoreError>;
    fn put_vector_mapping(&self, entry_id: u64, hnsw_data_id: u64) -> Result<(), CoreError>;
    fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>, CoreError>;
    fn iter_vector_mappings(&self) -> Result<Vec<(u64, u64)>, CoreError>;
    fn read_counter(&self, name: &str) -> Result<u64, CoreError>;
}

pub trait VectorStore: Send + Sync {
    fn insert(&self, entry_id: u64, embedding: &[f32]) -> Result<(), CoreError>;
    fn search(&self, query: &[f32], top_k: usize, ef_search: usize) -> Result<Vec<SearchResult>, CoreError>;
    fn search_filtered(&self, query: &[f32], top_k: usize, ef_search: usize, allowed_entry_ids: &[u64]) -> Result<Vec<SearchResult>, CoreError>;
    fn point_count(&self) -> usize;
    fn contains(&self, entry_id: u64) -> bool;
    fn stale_count(&self) -> usize;
}

pub trait EmbedService: Send + Sync {
    fn embed_entry(&self, title: &str, content: &str) -> Result<Vec<f32>, CoreError>;
    fn embed_entries(&self, entries: &[(String, String)]) -> Result<Vec<Vec<f32>>, CoreError>;
    fn dimension(&self) -> usize;
}
```

### Content Hash (unimatrix-store/src/hash.rs)

```rust
pub(crate) fn compute_content_hash(title: &str, content: &str) -> String
```

### Migration (unimatrix-store/src/migration.rs)

```rust
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 1;
pub(crate) fn migrate_if_needed(db: &redb::Database) -> Result<(), StoreError>
```

## Constraints

- Rust edition 2024, MSRV 1.89
- bincode v2 serde-compatible path (encode_to_vec / decode_from_slice)
- Fields append-only on EntryRecord (after `embedding_dim`)
- `#![forbid(unsafe_code)]` on all crates
- sha2 for content hash (pure Rust)
- tokio only under `async` feature flag
- All 246 existing tests must pass
- No changes to redb table layout

## Dependencies

| Crate | Version | New? | Purpose |
|-------|---------|------|---------|
| sha2 | latest | YES | SHA-256 content hashing (unimatrix-store) |
| thiserror | 2 | YES | CoreError derives (unimatrix-core) |
| tokio | 1 (optional) | YES | Async wrappers, feature-gated (unimatrix-core) |
| unimatrix-store | path | NO | Re-export + adapter |
| unimatrix-vector | path | NO | Re-export + adapter |
| unimatrix-embed | path | NO | Re-export + adapter |

## NOT in Scope

- MCP server (vnc-001)
- Agent Registry or Audit Log tables (vnc-001)
- Input validation or content scanning (vnc-002)
- Confidence computation (crt-002)
- New redb tables
- Vector index or embedding crate internal changes
- Runtime category allowlist or trust level enforcement
- CLI commands

## Alignment Status

From ALIGNMENT-REPORT.md:
- Vision Alignment: **PASS**
- Milestone Fit: **PASS**
- Scope Gaps: **PASS**
- Scope Additions: **WARN** (compact() excluded from trait -- justified by ADR-006)
- Architecture Consistency: **PASS**
- Risk Completeness: **PASS**

No variances requiring human approval.

## Implementation Order

```
1. crate-setup         -- Create unimatrix-core crate, Cargo.toml, workspace member
2. security-schema     -- Add 7 fields to EntryRecord, extend NewEntry
3. content-hash        -- Implement compute_content_hash() + sha2 dep
4. migration           -- Implement schema migration in Store::open()
5. write-security      -- Update insert() and update() with security field logic
6. core-error          -- CoreError enum with From conversions
7. core-traits         -- EntryStore, VectorStore, EmbedService trait definitions
8. re-exports          -- Type re-exports in unimatrix-core lib.rs
9. adapters            -- StoreAdapter, VectorAdapter, EmbedAdapter
10. async-wrappers     -- Feature-gated AsyncEntryStore, AsyncVectorStore, AsyncEmbedService
```

Components 2-5 modify unimatrix-store. Components 1, 6-10 create unimatrix-core. Components 2 and 3 can be done in parallel. Component 4 depends on 2+3. Component 5 depends on 3+4.
