# nan-002: Knowledge Import -- Architecture

## System Overview

Knowledge Import is the consuming side of the nan-001 export contract. It restores a Unimatrix knowledge base from a JSONL dump file via a new `unimatrix-server import` CLI subcommand. The subcommand runs synchronously (no tokio runtime), opens the database via `Store::open()`, inserts data via direct SQL, re-embeds all entries using the ONNX model, and builds a fresh HNSW vector index.

Import sits alongside `export` and `hook` as a sync CLI subcommand in the unimatrix-server binary. It depends on three workspace crates: unimatrix-store (database), unimatrix-embed (ONNX embeddings), and unimatrix-vector (HNSW index).

## Component Breakdown

### 1. CLI Registration (`main.rs`)

New `Import` variant in the `Command` enum with three parameters:
- `--input <PATH>` (required): Input JSONL file path
- `--skip-hash-validation` (optional): Skip hash chain and content hash checks
- `--force` (optional): Drop all existing data before import

Match arm dispatches to `import::run_import()` on the sync path.

### 2. Import Module (`import.rs`)

New module at `crates/unimatrix-server/src/import.rs`, registered as `pub mod import;` in `lib.rs`. Contains the full import pipeline. Single public entry point: `run_import()`.

**Responsibilities:**
- Parse and validate the JSONL header (format_version, schema_version)
- Pre-flight checks (empty database or --force, ONNX model availability)
- JSONL line-by-line ingestion with per-table deserialization
- Direct SQL INSERT for all 8 tables
- Hash chain and content hash validation
- Re-embedding via OnnxProvider
- HNSW vector index construction and persistence
- Audit log provenance entry
- Progress reporting to stderr

### 3. Shared Format Types (`format.rs`)

New module at `crates/unimatrix-server/src/format.rs`, registered as `pub mod format;` in `lib.rs`. Contains typed deserialization structs for the JSONL format_version 1 contract. Both export and import reference these types, eliminating implicit format coupling (SR-08).

**Structs:**
- `ExportHeader` -- header line fields
- `ExportRow` -- tagged enum over `_table` discriminator
- Per-table row structs: `CounterRow`, `EntryRow`, `EntryTagRow`, `CoAccessRow`, `FeatureEntryRow`, `OutcomeIndexRow`, `AgentRegistryRow`, `AuditLogRow`

These structs use `serde::Deserialize` with explicit field names matching the JSON keys from export. Export continues to use `serde_json::Value` for serialization (ADR-002 from nan-001), but the shared types provide compile-time documentation of the format contract and are the single source of truth for import deserialization.

### 4. Embedding & Vector Reconstruction

Not a new component -- import uses existing APIs:
- `OnnxProvider::new(EmbedConfig::default())` to initialize the ONNX model
- `embed_entries(provider, batch, ": ")` for batch embedding
- `VectorIndex::new(store, config)` + `VectorIndex::insert(id, embedding)` to build the index
- `VectorIndex::dump(vector_dir)` to persist the HNSW index to disk

## Component Interactions

```
main.rs
  |
  |-- Command::Import { input, skip_hash_validation, force }
  |     |
  |     +-- import::run_import(project_dir, input, skip_hash_validation, force)
  |           |
  |           |-- [1] project::ensure_data_directory()  --> ProjectPaths
  |           |-- [2] Store::open(db_path)              --> Arc<Store>
  |           |-- [3] Pre-flight checks:
  |           |       - Read schema_version from counters table
  |           |       - Check entry count (empty or --force)
  |           |       - If --force: drop_all_data(conn)
  |           |-- [4] Parse header line --> ExportHeader
  |           |       - Validate format_version == 1
  |           |       - Validate schema_version <= db schema_version
  |           |-- [5] BEGIN IMMEDIATE transaction
  |           |-- [6] Ingest JSONL lines:
  |           |       - Deserialize each line via format::ExportRow
  |           |       - Route to per-table INSERT function
  |           |       - Track counts for progress reporting
  |           |-- [7] Hash validation (unless --skip-hash-validation):
  |           |       - Content hash: recompute via compute_content_hash()
  |           |       - Chain integrity: verify previous_hash links
  |           |-- [8] COMMIT transaction
  |           |-- [9] Re-embed all entries:
  |           |       - OnnxProvider::new(EmbedConfig::default())
  |           |       - Read all entries (id, title, content) from DB
  |           |       - Batch embed (64 entries per batch)
  |           |       - VectorIndex::new() + insert per entry
  |           |       - VectorIndex::dump(vector_dir)
  |           |-- [10] Record import in audit log
  |           |-- [11] Print summary to stderr
  |           +-- Return Ok(()) or error
```

### Data Flow

1. **Input**: JSONL file on disk (format_version 1, schema_version 11)
2. **Parse**: Line-by-line BufReader, serde_json deserialization into `format::ExportRow`
3. **Store**: Direct SQL INSERT via `store.lock_conn()` within a single IMMEDIATE transaction
4. **Embed**: Read entries back from DB, batch embed via OnnxProvider, insert into VectorIndex
5. **Persist**: `VectorIndex::dump()` writes HNSW index files to `vector/` directory
6. **Audit**: Append provenance entry to audit_log after restoring exported audit data

### Error Boundaries

| Error Source | Handling | User Impact |
|---|---|---|
| File I/O (missing input file) | Fail fast before transaction | Exit 1, stderr message |
| JSON parse error | Fail with line number | Exit 1, transaction rollback |
| Header validation (format/schema) | Fail fast before transaction | Exit 1, actionable stderr message |
| Non-empty DB without --force | Fail fast before transaction | Exit 1, suggests --force |
| Hash validation failure | Fail after ingestion, before commit | Exit 1, lists broken entries |
| SQL constraint violation (FK, PK) | Transaction rollback | Exit 1, stderr with SQL error |
| ONNX model unavailable | Fail after DB commit, before vector build | Exit 1, suggests model download |
| Embedding failure | Fail during vector build | Exit 1, stderr with entry context |

Note: ONNX model failure occurs after the DB transaction is committed. This is acceptable because: (a) the database is fully restored and usable for non-search operations, (b) re-running import with --force will retry embedding, (c) the alternative (embedding inside the transaction) would make the transaction excessively long and hold the write lock during CPU-bound work.

## Technology Decisions

| Decision | ADR | Rationale |
|---|---|---|
| Shared format types between export and import | ADR-001 | Eliminates implicit format contract (SR-08) |
| Direct SQL INSERT, not Store API | ADR-002 | Must preserve original IDs, timestamps, confidence (prior art: Unimatrix #336, #344) |
| --force with stderr confirmation warning | ADR-003 | Balances safety (SR-04) with CLI automation needs |
| Re-embed after DB commit, not inside transaction | ADR-004 | Bounds transaction duration; partial restore is still useful |

## Integration Points

### Dependencies (consumed)

| Crate | API Used | Purpose |
|---|---|---|
| unimatrix-store | `Store::open()`, `store.lock_conn()`, `compute_content_hash()` | Database creation, raw SQL access, hash computation |
| unimatrix-embed | `OnnxProvider::new()`, `embed_entries()` | ONNX model initialization, batch embedding |
| unimatrix-vector | `VectorIndex::new()`, `VectorIndex::insert()`, `VectorIndex::dump()` | HNSW index construction and persistence |
| unimatrix-engine | `project::ensure_data_directory()` | Project path resolution |
| serde_json | Deserialization | JSONL parsing |

### Contract with nan-001 Export

Import consumes format_version 1 as produced by `export.rs`. The contract is:
- Header line with `_header: true`, `format_version: 1`, `schema_version: N`, `exported_at: T`, `entry_count: N`
- Data lines with `_table` discriminator: `counters`, `entries`, `entry_tags`, `co_access`, `feature_entries`, `outcome_index`, `agent_registry`, `audit_log`
- Tables emitted in dependency order (entries before entry_tags, entries before co_access)
- 26 entry columns matching the entries DDL
- SQL NULL as JSON null, REAL as JSON number, JSON-in-TEXT as raw strings

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| `import::run_import` | `pub fn run_import(project_dir: Option<&Path>, input: &Path, skip_hash_validation: bool, force: bool) -> Result<(), Box<dyn std::error::Error>>` | `crates/unimatrix-server/src/import.rs` (new) |
| `Command::Import` | `Import { #[arg(short, long)] input: PathBuf, #[arg(long)] skip_hash_validation: bool, #[arg(long)] force: bool }` | `crates/unimatrix-server/src/main.rs` (modified) |
| `format::ExportHeader` | `pub struct ExportHeader { pub schema_version: i64, pub exported_at: i64, pub entry_count: i64, pub format_version: i64 }` | `crates/unimatrix-server/src/format.rs` (new) |
| `format::ExportRow` | `pub enum ExportRow { Counter(CounterRow), Entry(EntryRow), EntryTag(EntryTagRow), CoAccess(CoAccessRow), FeatureEntry(FeatureEntryRow), OutcomeIndex(OutcomeIndexRow), AgentRegistry(AgentRegistryRow), AuditLog(AuditLogRow) }` | `crates/unimatrix-server/src/format.rs` (new) |
| `format::EntryRow` | Struct with 26 fields matching export columns: `id: i64, title: String, content: String, topic: String, category: String, source: String, status: i64, confidence: f64, created_at: i64, updated_at: i64, last_accessed_at: i64, access_count: i64, supersedes: Option<i64>, superseded_by: Option<i64>, correction_count: i64, embedding_dim: i64, created_by: String, modified_by: String, content_hash: String, previous_hash: String, version: i64, feature_cycle: String, trust_source: String, helpful_count: i64, unhelpful_count: i64, pre_quarantine_status: Option<i64>` | `crates/unimatrix-server/src/format.rs` (new) |
| `format::CounterRow` | `pub struct CounterRow { pub name: String, pub value: i64 }` | `crates/unimatrix-server/src/format.rs` (new) |
| `format::EntryTagRow` | `pub struct EntryTagRow { pub entry_id: i64, pub tag: String }` | `crates/unimatrix-server/src/format.rs` (new) |
| `format::CoAccessRow` | `pub struct CoAccessRow { pub entry_id_a: i64, pub entry_id_b: i64, pub count: i64, pub last_updated: i64 }` | `crates/unimatrix-server/src/format.rs` (new) |
| `format::FeatureEntryRow` | `pub struct FeatureEntryRow { pub feature_cycle: String, pub entry_id: i64 }` | `crates/unimatrix-server/src/format.rs` (new) |
| `format::OutcomeIndexRow` | `pub struct OutcomeIndexRow { pub feature_cycle: String, pub entry_id: i64 }` | `crates/unimatrix-server/src/format.rs` (new) |
| `format::AgentRegistryRow` | `pub struct AgentRegistryRow { pub agent_id: String, pub trust_level: i64, pub capabilities: String, pub allowed_topics: Option<String>, pub allowed_categories: Option<String>, pub enrolled_at: i64, pub last_seen_at: i64, pub active: i64 }` | `crates/unimatrix-server/src/format.rs` (new) |
| `format::AuditLogRow` | `pub struct AuditLogRow { pub event_id: i64, pub timestamp: i64, pub session_id: String, pub agent_id: String, pub operation: String, pub target_ids: String, pub outcome: i64, pub detail: String }` | `crates/unimatrix-server/src/format.rs` (new) |
| `Store::open` | `pub fn open(path: &Path) -> Result<Arc<Store>>` | `crates/unimatrix-store/src/db.rs` (existing) |
| `Store::lock_conn` | `pub fn lock_conn(&self) -> MutexGuard<'_, Connection>` | `crates/unimatrix-store/src/db.rs` (existing) |
| `compute_content_hash` | `pub fn compute_content_hash(title: &str, content: &str) -> String` | `crates/unimatrix-store/src/hash.rs` (existing) |
| `OnnxProvider::new` | `pub fn new(config: EmbedConfig) -> Result<Self>` | `crates/unimatrix-embed/src/onnx.rs` (existing) |
| `embed_entries` | `pub fn embed_entries(provider: &dyn EmbeddingProvider, entries: &[(String, String)], separator: &str) -> Result<Vec<Vec<f32>>>` | `crates/unimatrix-embed/src/text.rs` (existing) |
| `VectorIndex::new` | `pub fn new(store: Arc<Store>, config: VectorConfig) -> Result<Self>` | `crates/unimatrix-vector/src/index.rs` (existing) |
| `VectorIndex::insert` | `pub fn insert(&self, entry_id: u64, embedding: &[f32]) -> Result<()>` | `crates/unimatrix-vector/src/index.rs` (existing) |
| `VectorIndex::dump` | `pub fn dump(&self, dir: &Path) -> Result<()>` | `crates/unimatrix-vector/src/persistence.rs` (existing) |
| `project::ensure_data_directory` | `pub fn ensure_data_directory(project_dir: Option<&Path>, socket_dir: Option<&Path>) -> Result<ProjectPaths>` | `crates/unimatrix-engine/src/project.rs` (existing) |

## Open Questions

1. **Schema version constant accessibility**: `CURRENT_SCHEMA_VERSION` is `pub(crate)` in unimatrix-store. Import needs to compare the header's schema_version against the current version. The pragmatic solution is to read schema_version from the counters table after `Store::open()` (same approach as export). This avoids changing visibility of an internal constant. Implementation agents should use this approach.

2. **Feature_entries and outcome_index column names**: The export emits these tables but the exact SQL DDL column names need verification from `schema.rs`. Implementation agents should verify the DDL for `feature_entries` and `outcome_index` tables before writing INSERT statements.
