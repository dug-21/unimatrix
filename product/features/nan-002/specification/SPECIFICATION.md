# nan-002: Knowledge Import — Specification

## Objective

Provide a CLI subcommand (`unimatrix-server import`) that restores a Unimatrix knowledge base from a nan-001 JSONL export dump, re-embeds all entries with the current ONNX model, and validates data integrity. This completes the backup/restore cycle required for cross-project knowledge transfer and multi-repo deployment resilience in the Platform Hardening milestone.

---

## Functional Requirements

### FR-01: CLI Subcommand

Add an `Import` variant to the `Command` enum in `crates/unimatrix-server/src/main.rs` with:
- `--input <path>` (required): Path to the JSONL export file.
- `--skip-hash-validation` (optional): Bypass hash chain and content hash verification.
- `--force` (optional): Drop all existing data before import, enabling restore into a populated database.

The subcommand runs synchronously (no tokio runtime), matching the `Hook` and `Export` subcommand pattern. The `--project-dir` root flag is respected for database location.

### FR-02: Header Validation

Parse the first line of the JSONL file as a header object. Validate:
- `format_version` equals `1`. If not, reject with an error naming the unsupported version.
- `schema_version` is <= `CURRENT_SCHEMA_VERSION` (currently 11). If greater, reject with a message suggesting the user upgrade the binary.
- `_header` field is `true`.

Validation failures terminate import immediately with exit code non-zero, no partial state.

### FR-03: Database Pre-Flight

Before any data insertion:
1. Open the database via `Store::open()` (which applies migrations, guaranteeing the target is at `CURRENT_SCHEMA_VERSION`).
2. Check if the database is empty (entry count == 0). If non-empty and `--force` is not specified, reject with an error suggesting `--force` or a fresh project directory.
3. If `--force` is specified on a non-empty database, drop all data from all 8 importable tables plus vector_map, then proceed. Log the count of dropped entries to stderr.
4. Check for a running MCP server via PID file / flock (using existing vnc-004 mechanisms). If detected, emit a warning to stderr recommending the user stop the server before import. Do not block — proceed with warning. (SR-07)

### FR-04: JSONL Ingestion

Read the file line-by-line (not loaded entirely into memory). For each data line:
1. Parse as JSON. On parse failure, emit error with the 1-indexed line number and abort.
2. Dispatch by `_table` field to the appropriate table inserter.
3. Insert using direct SQL INSERT (not through Store API) to preserve original IDs, timestamps, confidence, content_hash, version, and all other fields verbatim. (Rationale: Store API auto-generates IDs, timestamps, hashes, and confidence defaults.)

All insertions occur within a single database transaction. Any failure rolls back the entire transaction — no partial state.

### FR-05: Table Restoration

Restore all 8 tables from the export format:

| Table | Key columns | Notes |
|-------|-------------|-------|
| `counters` | name, value | Restores next_entry_id, next_signal_id, schema_version |
| `entries` | 26 columns | All fields preserved verbatim (see FR-06) |
| `entry_tags` | entry_id, tag | FK to entries |
| `co_access` | entry_id_a, entry_id_b, count, last_seen | Learned signal |
| `feature_entries` | feature_cycle, entry_id | FK to entries |
| `outcome_index` | feature_cycle, entry_id | FK to entries |
| `agent_registry` | agent_id, trust_level, capabilities, ... | Security/identity data |
| `audit_log` | All audit fields | Append-only log, preserved for provenance |

Tables are inserted in the dependency order emitted by nan-001 export (counters, entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log). This satisfies foreign key constraints.

### FR-06: Entry Field Preservation

All 26 entry columns must be preserved exactly as exported:
`id`, `title`, `content`, `topic`, `category`, `source`, `status`, `confidence`, `created_at`, `updated_at`, `last_accessed_at`, `access_count`, `supersedes`, `superseded_by`, `correction_count`, `embedding_dim`, `created_by`, `modified_by`, `content_hash`, `previous_hash`, `version`, `feature_cycle`, `trust_source`, `helpful_count`, `unhelpful_count`, `pre_quarantine_status`.

No column may be auto-generated, defaulted, or transformed during import.

### FR-07: Counter Restoration

Counter values (`next_entry_id`, `next_signal_id`, `schema_version`, and any other counters present in the export) are restored from the export dump. This ensures post-import inserts do not collide with imported entry IDs.

### FR-08: Hash Chain Validation

Unless `--skip-hash-validation` is specified, after all entries are loaded:

1. **Content hash validation**: For each entry, recompute `compute_content_hash(title, content)` using the existing `crates/unimatrix-store/src/hash.rs` function. Compare against the stored `content_hash`. Report mismatches with entry ID and title.
2. **Chain integrity validation**: For each entry with a non-empty `previous_hash`, verify that another entry in the imported dataset has a matching `content_hash`. Report broken chains with entry ID and the unresolved `previous_hash`.

Validation failures cause exit code non-zero with details to stderr. When `--skip-hash-validation` is specified, emit a warning to stderr noting that validation was skipped.

### FR-09: Re-Embedding

After entry insertion and hash validation:
1. Initialize `OnnxProvider` directly (synchronous, no `EmbedServiceHandle`).
2. Read all entries from the database.
3. Batch entries in groups of 64.
4. For each batch, call `embed_entries(provider, entries, ": ")` to produce 384-dim f32 vectors.
5. Insert each (entry_id, embedding) pair into a fresh `VectorIndex`.
6. Persist the HNSW index to the `vector/` directory within the project data directory.

Re-embedding guarantees vector consistency with the currently installed model, regardless of what model produced the original knowledge.

### FR-10: Audit Provenance Entry

After restoring the exported audit log and committing, write a provenance entry to the audit log recording the import operation itself. This entry includes the import timestamp, source file path, and entry count. This provides an audit trail showing when the database was restored from an export.

### FR-11: Progress Reporting

Emit progress to stderr during import:
- Entry insertion count (e.g., "Inserted 245 entries")
- Re-embedding progress (e.g., "Embedding batch 3/8 (192/500 entries)")
- Final summary: tables restored, entries imported, entries re-embedded, validation outcome

### FR-12: Error Handling

All error conditions produce a non-zero exit code. Errors and warnings go to stderr. Summary goes to stderr. Specific error cases:

| Condition | Behavior | Exit |
|-----------|----------|------|
| Invalid/missing header | Error with details | 1 |
| Unsupported format_version | Error naming the version | 1 |
| schema_version > CURRENT | Error suggesting binary upgrade | 1 |
| Non-empty DB without --force | Error suggesting --force or fresh dir | 1 |
| JSON parse error | Error with 1-indexed line number | 1 |
| Content hash mismatch | Error with entry IDs (unless --skip-hash-validation) | 1 |
| Broken hash chain | Error with entry IDs and unresolved hash (unless --skip-hash-validation) | 1 |
| Embedding failure | Error with details | 1 |
| Successful import | Summary to stderr | 0 |

Transaction rollback occurs on any failure — no partial state is left.

---

## Non-Functional Requirements

### NFR-01: Performance

Import of a database with 500 entries completes re-embedding in under 60 seconds on standard hardware. Batch size of 64 entries bounds per-batch latency. (AC-17)

### NFR-02: Memory

- JSONL file is read line-by-line (streaming), not loaded entirely into memory.
- Re-embedding batches entries (64 at a time) to bound memory.
- HNSW index is built incrementally via `VectorIndex::insert()`.

### NFR-03: Atomicity

The entire import is wrapped in a single database transaction. Any failure at any stage (parsing, insertion, validation, embedding) rolls back the transaction completely. The database is left in the state it was before import began.

### NFR-04: Compatibility

- Requires ONNX model (all-MiniLM-L6-v2, ~80MB). First-time use downloads the model if not cached. Error messaging must be clear when model is unavailable. (SR-01)
- No new crate dependencies — uses existing workspace crates (serde_json, unimatrix-store, unimatrix-embed, unimatrix-vector).
- Runs on the same platforms as the existing CLI (Linux x64, macOS arm64/x64).

### NFR-05: No Server Required

The import subcommand opens the database directly and does not require a running MCP server, consistent with the export and hook subcommands. (AC-18)

---

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `unimatrix-server import --input <path>` CLI subcommand exists and restores a knowledge base from a nan-001 export file | Integration test: export populated DB, import into fresh DB, verify data |
| AC-02 | `--force` flag drops all existing data before import, enabling restore into a populated database | Integration test: populate DB, import with --force, verify old data gone and new data present |
| AC-03 | Header line is validated: format_version must equal 1; schema_version must be <= CURRENT_SCHEMA_VERSION | Unit test: valid headers pass, invalid headers produce errors |
| AC-04 | Import rejects format_version != 1 with an error naming the unsupported version | Unit test: header with format_version 2, verify error message contains "2" |
| AC-05 | Import rejects schema_version > CURRENT_SCHEMA_VERSION with error suggesting binary upgrade | Unit test: header with schema_version 999, verify error message suggests upgrade |
| AC-06 | Import into non-empty database rejected without --force; accepted with --force | Integration test: both paths exercised |
| AC-07 | All 8 tables restored: entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log, counters | Integration test: round-trip export/import, query each table, verify row counts match |
| AC-08 | Imported entries preserve all 26 columns exactly | Integration test: compare each column of each entry before export and after import |
| AC-09 | Counter values restored; post-import inserts do not collide | Integration test: import, insert new entry, verify its ID > max imported ID |
| AC-10 | All entries re-embedded with current ONNX model (384-dim) and inserted into HNSW index | Integration test: after import, perform semantic search, verify results returned |
| AC-11 | After import, MCP server starts and serves queries with working semantic search | Integration test: import, start server, issue context_search, verify results |
| AC-12 | Hash chain validation: non-empty previous_hash entries have matching content_hash in dataset | Unit test: valid chain passes; broken chain produces error |
| AC-13 | Content hash validation: recomputed hash matches stored content_hash | Unit test: valid entry passes; tampered content produces error with entry ID |
| AC-14 | --skip-hash-validation bypasses checks with warning to stderr | Integration test: import with tampered hash and --skip-hash-validation, verify warning emitted and import succeeds |
| AC-15 | Round-trip: export, import, re-export produces identical output (excluding exported_at) | Integration test: byte-level comparison of two exports after normalizing exported_at |
| AC-16 | Empty export (header + counters, no entries) imports successfully | Unit/integration test: import empty export, verify valid empty database |
| AC-17 | 500-entry import completes re-embedding in under 60 seconds | Performance test with timer assertion |
| AC-18 | Import does not require a running MCP server | Verified by design: subcommand opens database directly |
| AC-19 | --project-dir flag respected by import subcommand | Integration test: import with --project-dir pointing to non-default location |
| AC-20 | Exit code 0 on success, non-zero on error; errors/warnings to stderr | Integration test: check exit codes for success and failure cases |
| AC-21 | Malformed JSONL line produces error with line number | Unit test: corrupt line 5, verify error says "line 5" |
| AC-22 | Entire import is atomic: failure rolls back transaction | Integration test: inject failure mid-import, verify database unchanged |
| AC-23 | Unit tests verify JSONL deserialization for each table type including edge cases | Unit tests: null fields, empty strings, unicode, max integer values per table type |
| AC-24 | Integration test: full round-trip with data across all 8 tables | Integration test: create comprehensive test data, export, import, verify all tables |
| AC-25 | Progress reporting to stderr during import | Integration test: capture stderr, verify progress messages present |
| AC-26 | Import operation recorded in audit log after restoring exported audit log | Integration test: after import, query audit_log for import provenance entry |
| AC-27 | --force on populated database drops existing data and imports successfully | Integration test: populate, force-import, verify clean import |

---

## Domain Models

### Import Pipeline Stages

The import process follows a strict sequential pipeline:

```
[File Open] -> [Header Parse & Validate] -> [Pre-Flight Checks] -> [BEGIN TRANSACTION]
    -> [JSONL Line-by-Line Ingestion] -> [Hash Validation] -> [COMMIT]
    -> [Re-Embed All Entries] -> [Persist Vector Index] -> [Audit Provenance] -> [Summary]
```

Transaction boundary: The database transaction wraps ingestion through commit. Re-embedding and vector persistence occur after commit because they write to the vector index (separate from the SQLite transaction). If re-embedding fails after commit, the database contains valid data but lacks a vector index — the server will rebuild on startup.

### JSONL Record Types

**Header Record** (line 1):
```json
{"_header": true, "schema_version": 11, "exported_at": 1741234567, "entry_count": 245, "format_version": 1}
```

**Data Records** (lines 2+): Each has a `_table` discriminator field. The remaining fields are the column values for that table row.

| `_table` value | Description | FK dependencies |
|----------------|-------------|-----------------|
| `"counters"` | Key-value counter pairs | None |
| `"entries"` | Knowledge entries (26 columns) | None |
| `"entry_tags"` | Tag associations | entries.id |
| `"co_access"` | Co-access pair counts | entries.id (both sides) |
| `"feature_entries"` | Feature-to-entry mapping | entries.id |
| `"outcome_index"` | Outcome tracking index | entries.id |
| `"agent_registry"` | Enrolled agents with trust levels | None |
| `"audit_log"` | Immutable audit trail | None |

### Key Terms

- **Export dump**: A JSONL file produced by `unimatrix-server export` (nan-001), containing a header line followed by data lines for 8 tables.
- **Format version**: Integer (currently 1) identifying the JSONL structure. Breaking changes to the export format increment this.
- **Schema version**: Integer (currently 11) identifying the database schema. The import binary's `CURRENT_SCHEMA_VERSION` is the maximum importable schema version.
- **Content hash**: SHA-256 hex digest of `"{title}: {content}"` (with edge cases for empty title/content). Used for integrity verification and correction chain linking.
- **Correction chain**: A sequence of entries linked by `previous_hash` -> `content_hash`. Represents the correction history of knowledge.
- **Re-embedding**: The process of computing fresh vector embeddings for all imported entries using the locally-installed ONNX model, ensuring vector consistency regardless of the model version used during export.
- **Provenance entry**: An audit log record created by the import operation itself, providing a traceable record that the database was restored from an export.

---

## User Workflows

### Workflow 1: Backup and Restore

1. User exports knowledge base: `unimatrix-server export --output backup.jsonl`
2. Database corruption or loss occurs.
3. User creates a fresh project directory (or uses `--force`).
4. User restores: `unimatrix-server import --input backup.jsonl`
5. Import re-embeds entries, validates integrity, prints summary.
6. User starts the server normally. All knowledge, confidence, and audit history preserved.

### Workflow 2: Cross-Project Knowledge Seeding

1. Team exports mature project's knowledge: `unimatrix-server export --output team-knowledge.jsonl`
2. Team starts a new repository.
3. Team imports into the new project: `unimatrix-server import --input team-knowledge.jsonl --project-dir /new/repo`
4. New project starts with full accumulated knowledge instead of cold start.

### Workflow 3: Forced Restore Over Existing Data

1. User has a running project with a degraded knowledge base.
2. User stops the MCP server.
3. User restores from a known-good export: `unimatrix-server import --input known-good.jsonl --force`
4. `--force` drops existing data, imports fresh data.
5. Server restart serves the restored knowledge.

### Workflow 4: Import with Integrity Warning

1. User receives a knowledge dump from an external source.
2. User runs: `unimatrix-server import --input external.jsonl`
3. Hash validation detects content tampering — import fails with entry-level details.
4. User investigates or chooses to bypass: `unimatrix-server import --input external.jsonl --skip-hash-validation`
5. Import succeeds with a warning that integrity was not verified.

---

## Constraints

1. **Schema version 11**: Import validates against `CURRENT_SCHEMA_VERSION` (11). No support for importing older schema versions. Users with older exports must re-export from a current binary. (SR-05: document that exports should be taken immediately before binary upgrades.)
2. **Single-threaded, synchronous**: Follows the hook/export subcommand pattern — no tokio runtime. `OnnxProvider::new()` and `embed_batch()` are synchronous. Direct SQL writes are synchronous.
3. **ONNX model required**: Unlike export, import requires the embedding model (~80MB download on first use). Clear error messaging required when model is unavailable. (SR-01)
4. **Direct SQL, not Store API**: Import uses direct SQL INSERT to preserve original field values. Any future Store schema changes (new columns, new constraints) must be mirrored in import code. Mitigate by sharing row struct definitions between export and import. (SR-02, SR-08)
5. **Database exclusivity**: Import should warn if a server holds the database (via PID file check). SQLite WAL mode allows concurrent reads but write transactions contend. Use existing vnc-004 PidGuard/flock mechanisms. (SR-07)
6. **Foreign key enforcement**: `PRAGMA foreign_keys = ON` (set by `Store::open()`). Dependency-ordered emission from nan-001 satisfies FK constraints. Re-ordered exports cause FK violation failures (correct behavior).
7. **Vector persistence**: HNSW index must be saved to the `vector/` directory after building so the server loads it on next startup.
8. **Format stability**: Only `format_version` 1 is supported. Future format versions require import code updates.
9. **No stdin**: Input is file path only. Stdin deferred to future version. (SR-06: the pipe example in non-goals requires stdin support, which is out of scope.)
10. **Destructive --force safety**: `--force` on a non-empty database logs the count of entries being dropped to stderr before proceeding. (SR-04: the only safety net is a prior export. Consider `--force --yes` double-opt-in in future iteration.)

---

## Dependencies

### Internal Crates

| Crate | Usage |
|-------|-------|
| `unimatrix-store` | `Store::open()`, `lock_conn()`, `compute_content_hash()`, `CURRENT_SCHEMA_VERSION` |
| `unimatrix-embed` | `OnnxProvider::new()`, `embed_entries()` for re-embedding |
| `unimatrix-vector` | `VectorIndex::new()`, `VectorIndex::insert()`, persistence API |
| `unimatrix-server` | CLI framework (`clap`), `project::ensure_data_directory()` |

### External Crates (Existing)

| Crate | Usage |
|-------|-------|
| `serde_json` | JSONL parsing |
| `clap` | CLI argument parsing |
| `rusqlite` | Direct SQL INSERT statements |

### Existing Components

- **nan-001 export format**: The JSONL contract (format_version 1, 8 tables, 26 entry columns). Import is the consuming side. Shared row struct definitions should be used by both export and import to prevent format drift. (SR-08, SR-09)
- **vnc-004 PID lifecycle**: PidGuard/flock mechanism for detecting a running server.
- **hash.rs**: `compute_content_hash()` for content hash validation.
- **Export test infrastructure**: Existing integration tests in `export.rs` that create populated databases and verify JSONL output. Import tests should follow the same patterns and reuse test helpers.

---

## NOT in Scope

- **Merge/append mode**: Import is full restore only. Merging overlapping knowledge bases (ID remapping, conflict resolution) is a fundamentally different problem.
- **Incremental import**: The entire dump is imported atomically. Resumable/partial import is a future optimization.
- **MCP tool exposure**: Import is CLI-only. It requires filesystem access, database exclusivity, and embedding model initialization unsuitable for MCP.
- **Stdin input (`--input -`)**: File path only. Stdin deferred.
- **Remote/network import**: Local filesystem only.
- **Format version negotiation**: Unsupported format versions fail immediately. No backward/forward compatibility logic.
- **Schema migration of import data**: Data must be at the current schema version. Older exports must be re-exported with a current binary.
- **Adaptation state import**: MicroLoRA weights and prototype adjustments (crt-006) are separate files, not part of the JSONL export/import contract.
- **Operational data import**: Sessions, observations, injection_log, query_log are excluded from the export format.
- **Decompression**: Input is plain JSONL. User decompresses before import.
- **Confirmation prompts for --force**: A future iteration may add `--force --yes` double-opt-in. Current scope uses `--force` as a single flag with stderr logging of dropped entry count.
- **--skip-embedding dry-run mode**: Suggested by SR-01 as a risk mitigation. Deferred — not in SCOPE.md acceptance criteria.

---

## Knowledge Stewardship

- Queried: SCOPE.md and SCOPE-RISK-ASSESSMENT.md directly for nan-002 domain -- used as primary inputs
- Queried: export.rs implementation for format contract details -- 8 tables, header structure, column lists confirmed
- Queried: migration.rs for CURRENT_SCHEMA_VERSION -- confirmed value 11
- Queried: main.rs for CLI subcommand pattern -- confirmed Hook and Export variants, synchronous execution model
