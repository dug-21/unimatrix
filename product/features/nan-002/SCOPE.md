# Knowledge Import

## Problem Statement

Unimatrix's knowledge base can be exported to JSONL via `unimatrix-server export` (nan-001), but there is no way to restore from that dump. This blocks three critical use cases:

1. **Backup/restore** -- Users cannot recover from a corrupted or accidentally destroyed database. Export without import is half a backup system.
2. **Cross-project knowledge transfer** -- Knowledge accumulated in one project cannot be seeded into another. Teams starting new repos must cold-start their knowledge base.
3. **Multi-repo deployment** -- The Platform Hardening milestone requires restore capability for first production multi-repo deployments. A project's knowledge must survive re-provisioning.

The export format (JSONL with format_version 1, schema_version 11) is well-defined by nan-001. Import is the consuming side of that contract.

## Goals

1. Restore a Unimatrix knowledge base from a nan-001 export dump via `unimatrix-server import` CLI subcommand
2. Re-embed all imported entries using the current ONNX model, guaranteeing vector consistency with the running model version (no stale embeddings from a different model)
3. Validate hash chain integrity: for entries with a non-empty `previous_hash`, verify that another entry in the import set has a matching `content_hash`
4. Validate schema version compatibility: reject imports from export dumps with a schema_version newer than the current binary's schema version
5. Validate format_version compatibility: reject imports from export dumps with an unrecognized format_version
6. Preserve all learned signals exactly: confidence scores, helpful/unhelpful counts, access counts, co-access pairs, correction chains -- no cold start after restore
7. Preserve security/audit data: agent registry enrollments and the full audit log
8. Preserve ID continuity: counter values (next_entry_id, etc.) are restored so new entries after import do not collide with imported IDs

## Non-Goals

- **No merge/append mode** -- Import is a full restore into an empty (or cleared) database. Merging two knowledge bases with potentially overlapping IDs is a fundamentally different problem (ID remapping, conflict resolution, dual-chain validation). Out of scope.
- **No incremental import** -- The entire dump is imported atomically. Resumable/partial import is a future optimization.
- **No MCP tool** -- Import is a CLI subcommand, not exposed via MCP. It requires filesystem access, database exclusivity, and embedding model initialization that MCP tools do not support.
- **No stdin support** -- Input is a file path only. Stdin (`--input -`) deferred to a future version. Users can use temporary files for pipe workflows.
- **No remote/network import** -- Local filesystem only. The input is a JSONL file on disk.
- **No format version negotiation** -- If the format_version is unsupported, the import fails with an error. No attempt to interpret older/newer formats.
- **No schema migration of import data** -- The import expects data at the current schema version. If the export was produced by an older binary (lower schema_version), the user must re-export with a current binary first.
- **No adaptation state import** -- MicroLoRA weights and prototype adjustments (crt-006) live in the data directory as separate files, not in the JSONL dump. They are not part of the knowledge export/import contract.
- **No operational data import** -- Sessions, observations, injection_log, query_log, etc. are excluded from the export format and therefore not imported.
- **No decompression** -- Input is plain JSONL. If the user compressed the export, they decompress before import (or pipe: `zcat dump.jsonl.gz | unimatrix-server import --input -`).

## Background Research

### Export Format (nan-001 Contract)

The export produces a single JSONL file. Key structural details from the implementation in `crates/unimatrix-server/src/export.rs`:

- **Header line**: `{"_header":true,"schema_version":11,"exported_at":<ts>,"entry_count":<n>,"format_version":1}`
- **Data lines**: Each has a `_table` field discriminator. 8 tables exported: `counters`, `entries`, `entry_tags`, `co_access`, `feature_entries`, `outcome_index`, `agent_registry`, `audit_log`.
- **Emission order**: Tables are emitted in dependency order (entries before entry_tags, entries before co_access). This means streaming import can insert entries before their dependent rows.
- **Type encoding**: SQL NULL as JSON null, INTEGER as JSON number, REAL as JSON number (f64 precision), TEXT as JSON string. JSON-in-TEXT columns (capabilities, allowed_topics, allowed_categories, target_ids) are raw strings.
- **Entries have 26 columns** including `content_hash`, `previous_hash`, `confidence`, `helpful_count`, `unhelpful_count`, `access_count`, `pre_quarantine_status`.

### Database Write Patterns

From `crates/unimatrix-store/src/write.rs`, the `Store::insert()` method:
- Auto-generates IDs via `counters::next_entry_id()`
- Auto-computes `content_hash` from title+content
- Auto-sets `created_at`/`updated_at` to `now`
- Auto-sets `confidence` to 0.0, `version` to 1

Import cannot use `Store::insert()` because it must preserve the original IDs, timestamps, confidence, version, and content_hash from the export. Import needs direct SQL INSERT, similar to the v5-to-v6 migration pattern in `crates/unimatrix-store/src/migration.rs`.

### Embedding Pipeline

From `crates/unimatrix-embed/`:
- `OnnxProvider::new(config)` downloads/loads the ONNX model (all-MiniLM-L6-v2, 384 dimensions)
- `embed_entry(provider, title, content, ": ")` produces a 384-dim f32 vector
- `embed_entries(provider, entries, ": ")` does batch embedding
- The embedding service in the server uses `EmbedServiceHandle` with async/tokio for lazy loading. Import runs synchronously, so it needs to initialize `OnnxProvider` directly (like the existing pattern in tests and migration code).

### Vector Index Construction

From `crates/unimatrix-vector/src/index.rs`:
- `VectorIndex::new(store, config)` creates an empty HNSW index
- `VectorIndex::insert(entry_id, embedding)` validates dimensions, allocates a data_id, inserts into HNSW, and writes the vector_map entry
- The vector_map table and HNSW index are fully derived from entry content + embedding model. Export excludes them; import must rebuild them.
- The HNSW index can be saved to disk via the persistence module (files in `vector/` directory)

### Hash Chain Integrity

From `crates/unimatrix-store/src/hash.rs`:
- `compute_content_hash(title, content)` produces SHA-256 hex of `"{title}: {content}"` (with edge cases for empty title/content)
- Entries form correction chains: entry A corrects entry B when A's `previous_hash` equals B's `content_hash`
- Hash chain validation on import: for each entry with non-empty `previous_hash`, there must exist another entry in the dataset whose `content_hash` matches. Broken chains indicate data corruption or partial export.
- Content hash validation: recompute `compute_content_hash(title, content)` for each entry and compare against the stored `content_hash`. A mismatch means content was tampered with post-export.

### Schema Compatibility

From `crates/unimatrix-store/src/migration.rs`:
- `CURRENT_SCHEMA_VERSION = 11`
- `Store::open()` runs migration on startup, bringing any database to current schema
- Import should reject dumps where `header.schema_version > CURRENT_SCHEMA_VERSION` (export from a newer binary)
- Import into a fresh database (created by `Store::open()`) means the target is always at current schema

### CLI Pattern

From `crates/unimatrix-server/src/main.rs`:
- Two existing subcommands: `Hook { event }` and `Export { output }`
- Both run synchronously (no tokio runtime)
- Import can follow the same pattern, but embedding requires the ONNX runtime which is synchronous. No tokio needed.

### Existing Test Infrastructure

From `crates/unimatrix-store/src/test_helpers.rs` and the server's `test_support.rs`:
- `Store::open(":memory:")` or `Store::open(tmpfile)` for test databases
- The export module has integration tests that create populated databases and verify JSONL output
- Import tests should follow the same pattern: create export, run import, verify database state

## Proposed Approach

### New CLI Subcommand

Add `Import` variant to the `Command` enum in `main.rs`:

```rust
Import {
    /// Input file path.
    #[arg(short, long)]
    input: PathBuf,

    /// Skip hash chain integrity validation.
    #[arg(long)]
    skip_hash_validation: bool,

    /// Drop all existing data before import.
    #[arg(long)]
    force: bool,
}
```

### Implementation Location

New module `crates/unimatrix-server/src/import.rs`. Follows the `export.rs` pattern as a self-contained module.

### Import Pipeline

1. **Open and validate input**: Read first line, parse header, validate `format_version == 1` and `schema_version <= CURRENT_SCHEMA_VERSION`.
2. **Prepare target database**: Open database via `Store::open()` (creates fresh schema). Verify the database is empty (entry count == 0) -- refuse import into a populated database to prevent data corruption.
3. **Ingest rows**: Read JSONL line-by-line. Group by `_table`. Insert into SQLite using direct SQL (not through Store API) within a single transaction. Tables are processed in the order they appear (dependency-ordered by nan-001's export contract).
4. **Hash chain validation** (unless `--skip-hash-validation`): After all entries are loaded, validate: (a) content hash correctness by recomputing from title+content, (b) chain integrity by checking that every non-empty `previous_hash` has a matching `content_hash` in the dataset.
5. **Re-embed all entries**: Initialize `OnnxProvider`, iterate all entries, embed each (batch for efficiency), insert into a fresh `VectorIndex`, persist to disk.
6. **Commit and report**: Commit the transaction. Print summary (entry count, tables restored, entries re-embedded, validation results).

### Why Direct SQL Instead of Store API

Same rationale as nan-001 (ADR-003): the Store API auto-generates IDs, timestamps, content hashes, and confidence. Import must preserve original values verbatim. Direct SQL INSERT is the correct approach, matching the v5-to-v6 migration pattern.

### Why Require Empty Database

Merging would require ID remapping (what if imported entry ID 5 conflicts with existing entry ID 5?), correction chain reconciliation, co-access merge logic, and counter reconciliation. This is not needed for backup/restore or cross-project seeding. The user creates a fresh project directory (or clears the existing one) before import.

### Why Re-embed Rather Than Import Embeddings

1. Embeddings are model-specific. If the import target runs a different model version (or the model was updated between export and import), stale embeddings would silently degrade search quality.
2. The export format deliberately excludes embeddings (they are derived data, not knowledge).
3. Re-embedding is deterministic given the same model, so import produces a consistent vector index.
4. For a typical knowledge base (< 1000 entries), re-embedding takes seconds with the ONNX model.

### Embedding Batch Strategy

Read all entries from the database after insertion, batch them in groups of 64, embed each batch, insert into VectorIndex. This is the same batch pattern used by the server's background embedding task.

### Error Handling

- Invalid header or unsupported format_version: error to stderr, exit 1. No partial import.
- Schema version too new: error to stderr with message about upgrading the binary. Exit 1.
- Non-empty target database: error to stderr, exit 1. Message suggests using a fresh project directory.
- JSON parse error on any line: error to stderr with line number. Exit 1. No partial import.
- Hash chain validation failure: warning to stderr with details of broken chains. Exit 1 unless `--skip-hash-validation`.
- Content hash mismatch: warning to stderr with entry IDs. Exit 1 unless `--skip-hash-validation`.
- Embedding failure: error to stderr. Exit 1.
- All errors cause transaction rollback -- no partial state.

## Acceptance Criteria

- AC-01: `unimatrix-server import --input <path>` CLI subcommand exists and restores a knowledge base from a nan-001 export file
- AC-02: `--force` flag drops all existing data (entries, tags, co-access, etc.) before import, enabling restore into a populated database
- AC-03: The header line is validated: format_version must equal 1; schema_version must be <= CURRENT_SCHEMA_VERSION
- AC-04: Import rejects a format_version != 1 with an error message naming the unsupported version
- AC-05: Import rejects a schema_version greater than CURRENT_SCHEMA_VERSION with an error message suggesting binary upgrade
- AC-06: Import into a non-empty database (entries count > 0) is rejected with an error message unless `--force` is specified, in which case all existing data is dropped first
- AC-07: All 8 tables from the export are restored: entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log, counters
- AC-08: Imported entries preserve all 26 columns exactly, including confidence, helpful_count, unhelpful_count, access_count, last_accessed_at, content_hash, previous_hash, version, trust_source, pre_quarantine_status
- AC-09: Counter values (next_entry_id, next_signal_id, etc.) are restored from the export, ensuring no ID collision with post-import inserts
- AC-10: All imported entries are re-embedded using the current ONNX model (all-MiniLM-L6-v2, 384-dim) and inserted into a fresh HNSW vector index
- AC-11: After import, `unimatrix-server` (no subcommand) starts and serves MCP queries against the imported data with working semantic search
- AC-12: Hash chain integrity validation: for each entry with non-empty `previous_hash`, a matching `content_hash` exists in the imported dataset (unless `--skip-hash-validation`)
- AC-13: Content hash validation: `compute_content_hash(title, content)` matches the stored `content_hash` for every entry (unless `--skip-hash-validation`)
- AC-14: `--skip-hash-validation` flag bypasses hash chain and content hash checks, emitting a warning instead of an error
- AC-15: Round-trip test: export a populated database, import into a fresh database, export again -- the two exports are identical (excluding `exported_at` timestamp)
- AC-16: Import of an empty export (header + counters only, no entries) succeeds and produces a valid empty database
- AC-17: Import of a database with 500 entries completes re-embedding in under 60 seconds
- AC-18: The import subcommand does not require a running MCP server (opens database directly, like export and hook)
- AC-19: The `--project-dir` flag on the root CLI is respected by the import subcommand
- AC-20: Exit code is 0 on success, non-zero on any error. Errors and warnings are written to stderr. Summary is written to stderr.
- AC-21: A malformed JSONL line produces an error with the line number
- AC-22: The entire import is atomic: any failure rolls back the transaction, leaving no partial state
- AC-23: Unit tests verify JSONL deserialization for each table type including edge cases (null fields, empty strings, unicode, max integer values)
- AC-24: Integration test verifies full round-trip: create database with data across all 8 tables, export, import into fresh database, verify all data matches
- AC-25: Progress reporting to stderr during import: entry insertion count and re-embedding progress
- AC-26: The import operation itself is recorded in the audit log after restoring the exported audit log (provenance entry)
- AC-27: `--force` on a populated database drops all existing data and proceeds with import successfully

## Constraints

1. **Schema version 11**: Import reads and validates against `CURRENT_SCHEMA_VERSION` (11). No support for importing older schema versions -- the user must re-export from a current binary.
2. **Single-threaded, synchronous**: Import follows the hook/export pattern -- no tokio runtime. `OnnxProvider::new()` and `embed_batch()` are synchronous. Direct SQL writes are synchronous. This simplifies error handling and transaction management.
3. **ONNX model required**: Unlike export (read-only, no model needed), import requires the embedding model for re-embedding. `OnnxProvider::new()` will download the model if not cached (~80MB). This is a network dependency for first-time use.
4. **Memory**: Re-embedding batches entries (64 at a time) to bound memory. The HNSW index is built incrementally. The JSONL file is read line-by-line (not loaded entirely into memory).
5. **No new crate dependencies**: serde_json (parsing), unimatrix-store (database), unimatrix-embed (ONNX), unimatrix-vector (HNSW) are all existing workspace dependencies.
6. **Database exclusivity**: Import opens the database via `Store::open()`. If a server is running, SQLite's WAL mode allows concurrent reads but the write transaction will contend. Import should warn if a server is running (check PID file) and recommend stopping it first.
7. **Foreign key enforcement**: PRAGMA foreign_keys = ON (set by Store::open). The dependency-ordered emission from nan-001 means entries are inserted before entry_tags, satisfying FK constraints. If the export is re-ordered, FK violations will cause the import to fail (which is correct behavior).
8. **Vector persistence**: After building the HNSW index, it must be saved to the `vector/` directory so the server can load it on next startup. This uses the existing VectorIndex persistence API.
9. **Format stability**: format_version 1 is the only supported version. If nan-001 ever increments format_version, import must be updated to handle the new format.

## Resolved Questions

1. **Stdin support**: Deferred. File-only for now.
2. **Existing data handling**: `--force` flag supported — drops all existing data before import.
3. **Progress reporting**: Included. Emit progress to stderr (entry count, re-embedding progress).
4. **Adaptation state**: MicroLoRA reset after import is acceptable. Not part of export/import contract.
5. **Audit log for import**: Yes — the import operation itself is recorded in the audit log after restoring the exported audit log, providing provenance.

## Tracking

https://github.com/dug-21/unimatrix/issues/217
