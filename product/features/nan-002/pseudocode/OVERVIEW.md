# nan-002: Knowledge Import -- Pseudocode Overview

## Components

| Component | File | Purpose |
|-----------|------|---------|
| cli-registration | `main.rs` (modify) | Add `Command::Import` variant, dispatch to `import::run_import()` |
| format-types | `format.rs` (create) | Shared typed deserialization structs for JSONL format_version 1 |
| import-pipeline | `import.rs` (create) | Full import pipeline: parse, validate, ingest, hash-check, commit |
| embedding-reconstruction | `import.rs` (embedded) | Re-embed entries + build VectorIndex after DB commit |

Also modify: `lib.rs` -- register `pub mod format;` and `pub mod import;`.

## Data Flow

```
JSONL file on disk
  |
  v
[cli-registration] -- parses --input, --skip-hash-validation, --force args
  |
  v
[import-pipeline::run_import()]
  |
  +-- ensure_data_directory() --> ProjectPaths (db_path, vector_dir, pid_path)
  +-- Store::open(db_path) --> Arc<Store>
  +-- Pre-flight: check empty DB or --force; PID file warning
  +-- Parse header line --> format::ExportHeader (validate format_version, schema_version)
  +-- If --force on non-empty: drop_all_data()
  +-- BEGIN IMMEDIATE transaction
  +-- Line-by-line JSONL: deserialize via format::ExportRow, route to per-table INSERT
  +-- Hash validation (content hash + chain integrity)
  +-- COMMIT transaction
  |
  v
[embedding-reconstruction]
  +-- OnnxProvider::new(EmbedConfig::default())
  +-- Read all entries (id, title, content) from committed DB
  +-- Batch embed (64/batch) via embed_entries()
  +-- VectorIndex::new() + insert() for each entry
  +-- VectorIndex::dump(vector_dir)
  |
  v
[import-pipeline] (cont.)
  +-- Record provenance audit entry
  +-- Print summary to stderr
```

## Shared Types (format.rs)

All types live in `crates/unimatrix-server/src/format.rs`. Import deserializes from these; export continues using `serde_json::Value` (nan-001 ADR-002).

| Type | Fields | Notes |
|------|--------|-------|
| `ExportHeader` | `_header: bool`, `schema_version: i64`, `exported_at: i64`, `entry_count: i64`, `format_version: i64` | Parsed from line 1 |
| `ExportRow` | Tagged enum `#[serde(tag = "_table")]` over 8 variants | Parsed from lines 2+ |
| `CounterRow` | `name: String`, `value: i64` | |
| `EntryRow` | 26 fields (see format-types.md) | `source`, `correction_count`, `embedding_dim` included per DDL |
| `EntryTagRow` | `entry_id: i64`, `tag: String` | |
| `CoAccessRow` | `entry_id_a: i64`, `entry_id_b: i64`, `count: i64`, `last_updated: i64` | |
| `FeatureEntryRow` | `feature_id: String`, `entry_id: i64` | JSON key is `feature_id` (DDL column name) |
| `OutcomeIndexRow` | `feature_cycle: String`, `entry_id: i64` | JSON key is `feature_cycle` (DDL column name) |
| `AgentRegistryRow` | 8 fields | `capabilities`, `allowed_topics`, `allowed_categories` are JSON-in-TEXT |
| `AuditLogRow` | 8 fields | `target_ids` is JSON-in-TEXT |

## Sequencing Constraints

1. **format-types** must be built first -- import-pipeline depends on it for deserialization.
2. **cli-registration** and **import-pipeline** can be built in parallel once format-types exists.
3. **embedding-reconstruction** is logically part of import-pipeline (same file) but is a distinct phase that runs after DB commit (ADR-004).
4. **lib.rs** modification (registering modules) is trivial and done alongside any component.

## Critical Corrections from Implementation Brief

1. `FeatureEntryRow.feature_id` (not `feature_cycle`) -- the DDL column and export JSON key are both `feature_id`.
2. `EntryRow` includes `source`, `correction_count`, `embedding_dim` -- Specification FR-06 erroneously listed agent_registry columns instead.
3. Counter restoration uses `INSERT OR REPLACE INTO counters` to handle auto-initialized counters from `Store::open()`.
