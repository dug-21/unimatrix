# Knowledge Export

## Problem Statement

Unimatrix stores all knowledge in a SQLite database under `~/.unimatrix/{hash}/`. There is no way to extract this data in a portable, text-based format. This blocks three use cases:

1. **Backup/restore** -- Users cannot back up their knowledge base before destructive operations (schema migrations, model upgrades, experimentation). The v5-to-v6 migration creates `.v5-backup` files, but these are raw SQLite snapshots tied to a specific schema version -- not portable.
2. **Cross-project knowledge transfer** -- Knowledge accumulated in one project cannot be moved to another project (different hash, different database).
3. **nan-002 dependency** -- Knowledge Import (nan-002) requires a well-defined input format to consume. The export format IS the import contract. Getting this wrong means either data loss on round-trip or a breaking format change later.

The Platform Hardening milestone requires this as a prerequisite for first multi-repo deployments.

## Goals

1. Export the knowledge base to a text-only file that preserves every field needed for lossless knowledge restore -- including learned confidence scores, usage signals, and correction chains
2. Exclude derived data (embeddings, HNSW index) -- these are re-computed on import from content
3. Exclude ephemeral operational data (observations, sessions, injection_log, query_log, etc.) -- these have ~60 day lifespans and rebuild naturally through usage
4. Include long-term knowledge data: entries (with all confidence/usage fields), tags, correction chains (content_hash/previous_hash), co-access pairs, feature_entries, outcome_index
5. Include security/audit data: agent_registry (enrollment/trust), audit_log (append-only compliance record)
6. Include counters for ID continuity
7. Produce deterministic output for a given database state (sorted by primary key within each table)
8. Implement as a CLI subcommand (`unimatrix-server export`) following the existing `hook` subcommand pattern
9. Design the format so nan-002 can consume it for lossless restore with hash chain integrity validation

## Non-Goals

- **No import functionality** -- That is nan-002. This feature produces the file; nan-002 consumes it.
- **No incremental/differential export** -- Full dump only. Incremental export is a future optimization.
- **No embedding export** -- Embeddings are derived data (384-dim f32 vectors from the ONNX model). They are re-generated on import. The vector_map table (entry_id -> hnsw_data_id) is internal to the HNSW index and excluded.
- **No operational data export** -- Sessions, observations, injection_log, signal_queue, query_log, observation_metrics, observation_phase_metrics, shadow_evaluations, and topic_deliveries are excluded. These are ephemeral (~60 day lifespan), rebuilt through usage, and not required for knowledge continuity.
- **No compression** -- The export is plain text. Users can pipe through gzip/zstd themselves. Keeps the format inspectable.
- **No streaming/partial export** -- The entire knowledge base is exported atomically. No table-level selection.
- **No MCP tool** -- This is a CLI subcommand, not exposed via MCP. Export requires filesystem access that MCP tools do not have.
- **No remote/network export** -- Local filesystem only.
- **No schema migration during export** -- Export reads the current schema version and writes it into the header. If the database is on an old schema, the user must run the server first (which migrates on startup).

## Background Research

### Existing Codebase Patterns

**CLI subcommand pattern** (main.rs): The binary uses clap with `#[derive(Subcommand)]`. Currently has one subcommand (`Hook { event: String }`). The hook subcommand runs synchronously (no tokio runtime) for latency. Export can follow the same sync pattern -- no tokio needed for sequential SQL reads and file writes.

**Database access**: `Store::open()` handles migration, table creation, and PRAGMA configuration. Export needs read-only access. The existing `Store::lock_conn()` method provides direct `&Connection` access for raw SQL queries. ADR-003 from the nxs-008 migration (Unimatrix #335) explicitly chose direct redb/SQL access over the Store API for export operations -- the same principle applies here.

**Project path resolution**: `unimatrix_engine::project::detect_project_root()` and `ProjectPaths` provide the standard path to `~/.unimatrix/{hash}/unimatrix.db`. The `--project-dir` flag already exists on the CLI struct. Export reuses this.

**JSON-Lines format precedent**: The nxs-008 redb-to-SQLite migration (Unimatrix #333, #343) established JSON-Lines as the intermediate format for cross-backend data migration. That migration successfully used JSONL to transfer entries, co-access, sessions, injection_log, signal_queue, agent_registry, and audit_log between storage backends. The same format is appropriate here.

### SQLite Schema (v11) -- Tables in Scope

8 tables exported (knowledge + security/audit + counters):

**Knowledge tables (core data model):**
- `entries` -- 26 columns, primary knowledge store. Includes confidence (f64), helpful_count, unhelpful_count, access_count, last_accessed -- all learned signals that must survive backup/restore. Also includes content_hash and previous_hash for correction chain integrity.
- `entry_tags` -- Junction table (entry_id, tag). Tags are NOT stored in entries column.
- `co_access` -- (entry_id_a, entry_id_b, count, last_updated). CHECK constraint: entry_id_a < entry_id_b. Preserves learned pair-wise usage patterns.
- `feature_entries` -- (feature_id, entry_id). Maps features to entries.
- `outcome_index` -- (feature_cycle, entry_id). Maps outcomes to entries.

**Security/audit tables:**
- `agent_registry` -- Agent enrollment records. capabilities and allowed_topics/allowed_categories stored as JSON strings.
- `audit_log` -- Append-only audit trail. target_ids stored as JSON string. Long-term compliance record.

**Infrastructure:**
- `counters` -- (name, value). Includes schema_version, next_entry_id, next_signal_id, next_log_id, next_audit_event_id.

**Excluded (derived or ephemeral):**
- `vector_map` -- Internal HNSW mapping. Derived, rebuilt from embeddings.
- `sessions`, `observations`, `injection_log`, `signal_queue`, `query_log` -- Ephemeral event data (~60 day lifespan).
- `observation_metrics`, `observation_phase_metrics`, `topic_deliveries` -- Aggregates computed from ephemeral observations.
- `shadow_evaluations` -- Neural extraction internals with BLOB column.

### Why Confidence Survives Without Operational Data

The confidence system's learned state is fully captured in the exported tables:
- **Base score**: entries.confidence (f64) -- the composite score used for ranking
- **Helpfulness**: entries.helpful_count, entries.unhelpful_count -- Wilson score inputs
- **Usage/freshness**: entries.access_count, entries.last_accessed -- decay inputs
- **Corrections**: entries.content_hash, entries.previous_hash, entries.version -- chain integrity
- **Trust source**: entries.trust_source -- auto vs human provenance
- **Co-access patterns**: co_access table -- pair-wise boosting data with counts

On restore, the system serves content with the same confidence scores, same helpfulness rankings, same co-access boosts. Future updates continue from these baselines -- no cold start.

### Format Design

**JSON-Lines (JSONL)** is the right choice:
- Each line is a self-contained JSON object -- streamable, parseable line-by-line
- Human-readable and inspectable (unlike bincode/protobuf)
- Text-only by definition (no binary data -- BLOB tables excluded)
- Established precedent in this codebase (nxs-008 migration)
- Handles all SQL types: integers, floats, strings, nulls, JSON-encoded arrays

**File structure**: A single JSONL file with a header line followed by table rows. Each line has a `_table` discriminator field:

```jsonl
{"_header": true, "schema_version": 11, "exported_at": 1741644000, "entry_count": 53, "format_version": 1}
{"_table": "counters", "name": "schema_version", "value": 11}
{"_table": "entries", "id": 1, "title": "...", ...all 26 columns...}
{"_table": "entry_tags", "entry_id": 1, "tag": "rust"}
{"_table": "co_access", "entry_id_a": 1, "entry_id_b": 5, "count": 3, "last_updated": 1700000000}
...
```

**Why a single file, not per-table files**: Atomic export. A directory of files introduces partial-export failure modes. A single file either exists complete or does not.

**Why `_table` discriminator, not separate sections**: Allows streaming import without buffering. The importer processes each line independently based on `_table`.

**No BLOB handling needed**: With shadow_evaluations excluded, all columns across the 8 exported tables are text/integer/real/null. No base64 encoding required.

**Ordering**: Rows within each table are ordered by primary key. Tables are emitted in dependency order (entries before entry_tags, entries before co_access) to allow streaming import with foreign key enforcement.

### Hash Chain Integrity

Entries form correction chains via `content_hash` and `previous_hash`. The export preserves these fields verbatim. On import (nan-002), the hash chain can be validated: for each entry where `previous_hash` is non-empty, there should exist another entry (or historical version) whose `content_hash` matches. The export does not validate chains -- it dumps raw data. Validation is nan-002's responsibility.

## Proposed Approach

### New CLI Subcommand

Add `Export` variant to the `Command` enum in `main.rs`:

```rust
Export {
    /// Output file path. Defaults to stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
}
```

The export subcommand:
1. Resolves project paths (reuses existing `--project-dir` flag)
2. Opens the database read-only via `Store::open()` (which runs migration if needed)
3. Reads 8 tables in dependency order
4. Writes JSONL to output file or stdout
5. Exits with code 0 on success, non-zero on error

### Implementation Location

New module `crates/unimatrix-server/src/export.rs` containing:
- `pub fn run_export(db_path: &Path, output: Option<&Path>) -> Result<()>`
- Per-table export functions that read via raw SQL and write JSONL
- Header generation with metadata (schema_version, entry_count, timestamp, format_version)

This follows the pattern of `uds/hook.rs` being a self-contained module for the hook subcommand.

### Direct SQL Access

Per ADR-003 (#335), export uses `Store::lock_conn()` for direct SQL queries rather than going through the Store API or service layer. Rationale: the Store API returns Rust types (EntryRecord, etc.) that would need re-serialization; direct SQL-to-JSON avoids the intermediate representation and guarantees no data is lost in type conversion.

### Dependency Order

Tables emitted in this order (satisfies foreign key constraints for streaming import):
1. Header
2. counters
3. entries
4. entry_tags (FK: entries.id)
5. co_access
6. feature_entries
7. outcome_index
8. agent_registry
9. audit_log

### Error Handling

- Database open failure: error message to stderr, exit 1
- Individual table read failure: error message to stderr, exit 1. No partial exports.
- Output write failure: error message to stderr, exit 1
- Empty tables: emit zero rows for that table (header still includes the table count of 0). This is fine -- nan-002 handles empty sections.

## Acceptance Criteria

- AC-01: `unimatrix-server export` CLI subcommand exists and produces JSONL output to stdout by default
- AC-02: `unimatrix-server export --output <path>` writes to the specified file instead of stdout
- AC-03: The first line of output is a header object with `schema_version`, `exported_at` (unix timestamp), `entry_count`, and `format_version` (integer, initially 1)
- AC-04: Every non-header line contains a `_table` field identifying which table the row belongs to
- AC-05: All 8 knowledge/audit tables are exported with all columns preserved (entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log, counters)
- AC-06: Entries table export includes all 26 columns with confidence, helpful_count, unhelpful_count, access_count, and other learned signals preserved exactly
- AC-07: Rows within each table are ordered by primary key (ascending)
- AC-08: Tables are emitted in dependency order: entries before entry_tags
- AC-09: Null SQL values are represented as JSON null (not empty string, not omitted)
- AC-10: Export of a freshly-initialized empty database produces a valid JSONL file with header and counter rows only (no entries, no errors)
- AC-11: Export of a database with 500 entries completes in under 5 seconds
- AC-12: The export subcommand does not require a running MCP server (opens database directly, like the hook subcommand)
- AC-13: The `--project-dir` flag on the root CLI is respected by the export subcommand for project path resolution
- AC-14: Output is deterministic: exporting the same database twice produces byte-identical output (assuming no writes between exports)
- AC-15: Exit code is 0 on success, non-zero on any error. Errors are written to stderr.
- AC-16: Unit tests verify JSONL serialization for each table type including edge cases (null optional fields, empty strings, unicode content, maximum integer values)
- AC-17: Integration test creates a database with representative data across all 8 exported tables, exports it, and verifies the output parses as valid JSONL with correct row counts and field values
- AC-18: Operational tables (sessions, observations, injection_log, signal_queue, query_log, observation_metrics, observation_phase_metrics, shadow_evaluations, topic_deliveries) are NOT present in export output

## Constraints

1. **Schema version 11**: Export reads the current schema. No need to support exporting older schema versions -- `Store::open()` migrates to current schema on open.
2. **Single-threaded, synchronous**: Like the hook subcommand, export does not need tokio. Sequential SQL reads are fine. The database is opened with WAL mode (read-only access does not block the running server).
3. **No new crate dependencies**: serde_json (already present) handles JSON serialization. No BLOB columns in scope, so no base64 needed.
4. **Server not required**: Export opens the database directly. If a server is running, SQLite WAL mode allows concurrent reads. If no server is running, export still works.
5. **Memory**: Export streams rows to output. Does not load entire tables into memory. Each table is read and written row-by-row.
6. **Format stability**: `format_version: 1` in the header. If the format changes in the future, increment this version. nan-002 checks format_version for compatibility.
7. **No embeddings**: vector_map is excluded. Embeddings are not stored in SQLite (they are in the HNSW in-memory index, persisted to `vector/` directory). Export is text-only.
8. **Confidence continuity**: All learned signals (confidence, helpful_count, unhelpful_count, access_count, last_accessed, co_access counts) are preserved exactly. On restore, the system serves content identically -- no cold start, no relearning.

## Tracking

https://github.com/dug-21/unimatrix/issues/209
