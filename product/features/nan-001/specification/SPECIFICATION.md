# nan-001 Specification: Knowledge Export

## Objective

Export the Unimatrix knowledge base to a portable, text-based JSONL file that preserves every field needed for lossless knowledge restore -- including learned confidence scores, usage signals, correction chains, agent enrollment, and audit records. The export covers 8 tables (knowledge + security/audit + counters), excludes derived data (embeddings, HNSW index) and ephemeral operational data, and produces deterministic output suitable as the import contract for nan-002.

## Functional Requirements

### FR-01: CLI Subcommand

- FR-01.1: Add an `Export` variant to the `Command` enum in `crates/unimatrix-server/src/main.rs`, following the existing `Hook` subcommand pattern.
- FR-01.2: The subcommand is invoked as `unimatrix-server export`.
- FR-01.3: Accept an optional `--output <path>` (`-o <path>`) argument. When provided, write JSONL to the specified file path. When omitted, write to stdout.
- FR-01.4: Respect the existing `--project-dir` flag on the root CLI struct for project path resolution.
- FR-01.5: Open the database via `Store::open()` (which runs migration if needed). Do not require a running MCP server.
- FR-01.6: Run synchronously (no tokio runtime), matching the `Hook` subcommand pattern.
- FR-01.7: Exit with code 0 on success. Exit with non-zero on any error. Write all error messages to stderr.

### FR-02: JSONL Header

- FR-02.1: The first line of output is a JSON object with `"_header": true` and the following fields:
  - `schema_version` (integer): Value of the `schema_version` counter from the database.
  - `exported_at` (integer): Unix timestamp (seconds since epoch) at the moment of export.
  - `entry_count` (integer): Count of rows in the `entries` table.
  - `format_version` (integer): Always `1` for this release.
- FR-02.2: No other line in the output contains `"_header": true`.

### FR-03: Table Row Format

- FR-03.1: Every non-header line is a JSON object containing a `"_table"` field whose value is the SQL table name (one of: `counters`, `entries`, `entry_tags`, `co_access`, `feature_entries`, `outcome_index`, `agent_registry`, `audit_log`).
- FR-03.2: Each row object contains one key per SQL column in that table, using the exact SQL column name as the JSON key (see Field Mappings below).
- FR-03.3: No extra keys beyond `_table` and the column keys.

### FR-04: Table Export Order

Tables are emitted in this fixed dependency order (satisfies foreign key constraints for streaming import):

1. `counters`
2. `entries`
3. `entry_tags` (FK: entries.id)
4. `co_access`
5. `feature_entries`
6. `outcome_index`
7. `agent_registry`
8. `audit_log`

### FR-05: Row Ordering Within Tables

- FR-05.1: `counters` -- ordered by `name` ASC (TEXT primary key).
- FR-05.2: `entries` -- ordered by `id` ASC (INTEGER primary key).
- FR-05.3: `entry_tags` -- ordered by `entry_id` ASC, then `tag` ASC (composite primary key).
- FR-05.4: `co_access` -- ordered by `entry_id_a` ASC, then `entry_id_b` ASC (composite primary key).
- FR-05.5: `feature_entries` -- ordered by `feature_id` ASC, then `entry_id` ASC (composite primary key).
- FR-05.6: `outcome_index` -- ordered by `feature_cycle` ASC, then `entry_id` ASC (composite primary key).
- FR-05.7: `agent_registry` -- ordered by `agent_id` ASC (TEXT primary key).
- FR-05.8: `audit_log` -- ordered by `event_id` ASC (INTEGER primary key).

### FR-06: Empty Table Handling

- FR-06.1: If a table contains zero rows, emit zero rows for that table. The header still reports the actual `entry_count`. No error is raised.
- FR-06.2: An empty database (freshly initialized) produces a valid JSONL file with the header line and counter rows only (schema_version, next_entry_id, next_signal_id, next_log_id, next_audit_event_id).

### FR-07: Transaction Isolation

- FR-07.1: The entire export (all 8 table reads) executes within a single SQLite read transaction (`BEGIN DEFERRED`) to guarantee a consistent snapshot across all tables. This addresses SR-07 from the risk assessment.
- FR-07.2: The transaction is committed (or rolled back) after all tables have been read and written.

### FR-08: Excluded Tables

The following tables are NOT exported (no `_table` rows for any of these appear in the output):

- `vector_map` (derived -- internal HNSW mapping)
- `sessions` (ephemeral operational data)
- `observations` (ephemeral operational data)
- `injection_log` (ephemeral operational data)
- `signal_queue` (ephemeral operational data)
- `query_log` (ephemeral operational data)
- `observation_metrics` (ephemeral aggregate data)
- `observation_phase_metrics` (ephemeral aggregate data)
- `shadow_evaluations` (neural extraction internals, contains BLOB column)
- `topic_deliveries` (ephemeral aggregate data)

### FR-09: Implementation Location

- FR-09.1: New module `crates/unimatrix-server/src/export.rs` containing the export logic.
- FR-09.2: Public entry function: `pub fn run_export(store: &Store, output: Option<&Path>) -> Result<()>`.
- FR-09.3: Uses `Store::lock_conn()` for direct SQL access (per ADR-003 from nxs-008). Does not go through the Store API or service layer to avoid intermediate type conversion and guarantee no data is lost.

## Non-Functional Requirements

### NFR-01: Performance

- Export of a database with 500 entries completes in under 5 seconds on commodity hardware.

### NFR-02: Memory

- Export streams rows to output. Each table is read via a SQL query and rows are written one at a time. The export does not load all rows from all tables into memory simultaneously. Per-table result sets may be buffered by rusqlite's row iterator.

### NFR-03: Determinism (AC-14, SR-05)

- Exporting the same database twice (with no writes between exports) produces byte-identical output.
- This requires:
  - Fixed table emission order (FR-04).
  - Fixed row ordering within tables (FR-05).
  - Fixed JSON key ordering: use `serde_json::to_string` on a struct with `#[derive(Serialize)]` (field order is declaration order) or a `BTreeMap<String, Value>` -- NOT a `HashMap`. Key order must be stable and reproducible.
  - The `exported_at` timestamp must be excluded from the determinism guarantee (it changes between runs). Determinism means: if `exported_at` were held constant, output is byte-identical.

### NFR-04: Float Precision (SR-03)

- f64 values (entries.confidence) are serialized using serde_json's default `to_string`, which produces up to 17 significant digits -- sufficient for lossless f64 round-trip per IEEE 754.
- Test coverage verifies round-trip equality for edge-case floats: 0.0, 1.0, 0.123456789012345, f64::MIN_POSITIVE, and a value with maximum mantissa bits set.

### NFR-05: Error Handling

- Database open failure: error message to stderr, exit non-zero.
- SQL query failure on any table: error message to stderr, exit non-zero. No partial output (transaction ensures consistency; output file should be treated as invalid on error).
- Output write failure (e.g., disk full, broken pipe): error message to stderr, exit non-zero.
- If `--output` is specified and the export fails mid-write, the partial output file may remain on disk. The caller is responsible for cleanup. (Atomic write via temp-file-then-rename is a future enhancement, not in scope.)

### NFR-06: Compatibility

- The export subcommand shares the binary with the MCP server. Changes to `main.rs` and the clap structure must not regress server startup. The export module is self-contained with minimal shared code paths beyond path resolution and `Store::open()`.

### NFR-07: No New Dependencies

- No new crate dependencies. `serde_json` (already present) handles all JSON serialization. No BLOB columns are in scope, so no base64 crate is needed.

## Field Mappings

Each table below lists every column with its SQL type, the JSON key (identical to the SQL column name), the JSON type encoding, and nullability. This is the format contract that nan-002 will depend on (SR-01).

### Type Encoding Rules

| SQL Type | JSON Encoding | Notes |
|----------|--------------|-------|
| INTEGER NOT NULL | JSON number (integer) | No fractional part. |
| INTEGER (nullable) | JSON number or `null` | |
| REAL NOT NULL | JSON number (float) | serde_json default precision (up to 17 significant digits). |
| TEXT NOT NULL | JSON string | Includes empty strings (`""`). |
| TEXT (nullable) | JSON string or `null` | |
| TEXT containing JSON (e.g., `capabilities`, `target_ids`) | JSON string | The JSON-encoded content is stored as a string value, NOT inlined as a JSON object/array. This avoids double-encoding ambiguity. The importer stores the string as-is back into SQLite. |

### Table: counters

| SQL Column | SQL Type | JSON Key | JSON Type | Notes |
|-----------|----------|----------|-----------|-------|
| name | TEXT PRIMARY KEY | `name` | string | e.g., "schema_version", "next_entry_id" |
| value | INTEGER NOT NULL | `value` | number (integer) | |

ORDER BY: `name ASC`

### Table: entries

| SQL Column | SQL Type | JSON Key | JSON Type | Notes |
|-----------|----------|----------|-----------|-------|
| id | INTEGER PRIMARY KEY | `id` | number (integer) | |
| title | TEXT NOT NULL | `title` | string | |
| content | TEXT NOT NULL | `content` | string | May contain newlines, unicode. |
| topic | TEXT NOT NULL | `topic` | string | |
| category | TEXT NOT NULL | `category` | string | |
| source | TEXT NOT NULL | `source` | string | |
| status | INTEGER NOT NULL DEFAULT 0 | `status` | number (integer) | 0=Active, 1=Deprecated, 2=Superseded, 3=Quarantined. |
| confidence | REAL NOT NULL DEFAULT 0.0 | `confidence` | number (float) | f64 precision. |
| created_at | INTEGER NOT NULL | `created_at` | number (integer) | Unix timestamp (seconds). |
| updated_at | INTEGER NOT NULL | `updated_at` | number (integer) | Unix timestamp (seconds). |
| last_accessed_at | INTEGER NOT NULL DEFAULT 0 | `last_accessed_at` | number (integer) | Unix timestamp (seconds). |
| access_count | INTEGER NOT NULL DEFAULT 0 | `access_count` | number (integer) | |
| supersedes | INTEGER (nullable) | `supersedes` | number or null | Entry ID reference. |
| superseded_by | INTEGER (nullable) | `superseded_by` | number or null | Entry ID reference. |
| correction_count | INTEGER NOT NULL DEFAULT 0 | `correction_count` | number (integer) | |
| embedding_dim | INTEGER NOT NULL DEFAULT 0 | `embedding_dim` | number (integer) | |
| created_by | TEXT NOT NULL DEFAULT '' | `created_by` | string | Agent ID. |
| modified_by | TEXT NOT NULL DEFAULT '' | `modified_by` | string | Agent ID. |
| content_hash | TEXT NOT NULL DEFAULT '' | `content_hash` | string | SHA-256 hex digest for correction chain. |
| previous_hash | TEXT NOT NULL DEFAULT '' | `previous_hash` | string | Points to predecessor's content_hash. |
| version | INTEGER NOT NULL DEFAULT 0 | `version` | number (integer) | |
| feature_cycle | TEXT NOT NULL DEFAULT '' | `feature_cycle` | string | |
| trust_source | TEXT NOT NULL DEFAULT '' | `trust_source` | string | "human", "auto", or "". |
| helpful_count | INTEGER NOT NULL DEFAULT 0 | `helpful_count` | number (integer) | Wilson score input. |
| unhelpful_count | INTEGER NOT NULL DEFAULT 0 | `unhelpful_count` | number (integer) | Wilson score input. |
| pre_quarantine_status | INTEGER (nullable) | `pre_quarantine_status` | number or null | Status before quarantine. |

ORDER BY: `id ASC`

### Table: entry_tags

| SQL Column | SQL Type | JSON Key | JSON Type | Notes |
|-----------|----------|----------|-----------|-------|
| entry_id | INTEGER NOT NULL | `entry_id` | number (integer) | FK to entries.id. |
| tag | TEXT NOT NULL | `tag` | string | |

ORDER BY: `entry_id ASC, tag ASC`

### Table: co_access

| SQL Column | SQL Type | JSON Key | JSON Type | Notes |
|-----------|----------|----------|-----------|-------|
| entry_id_a | INTEGER NOT NULL | `entry_id_a` | number (integer) | CHECK: entry_id_a < entry_id_b. |
| entry_id_b | INTEGER NOT NULL | `entry_id_b` | number (integer) | |
| count | INTEGER NOT NULL DEFAULT 1 | `count` | number (integer) | |
| last_updated | INTEGER NOT NULL | `last_updated` | number (integer) | Unix timestamp (seconds). |

ORDER BY: `entry_id_a ASC, entry_id_b ASC`

### Table: feature_entries

| SQL Column | SQL Type | JSON Key | JSON Type | Notes |
|-----------|----------|----------|-----------|-------|
| feature_id | TEXT NOT NULL | `feature_id` | string | |
| entry_id | INTEGER NOT NULL | `entry_id` | number (integer) | |

ORDER BY: `feature_id ASC, entry_id ASC`

### Table: outcome_index

| SQL Column | SQL Type | JSON Key | JSON Type | Notes |
|-----------|----------|----------|-----------|-------|
| feature_cycle | TEXT NOT NULL | `feature_cycle` | string | |
| entry_id | INTEGER NOT NULL | `entry_id` | number (integer) | |

ORDER BY: `feature_cycle ASC, entry_id ASC`

### Table: agent_registry

| SQL Column | SQL Type | JSON Key | JSON Type | Notes |
|-----------|----------|----------|-----------|-------|
| agent_id | TEXT PRIMARY KEY | `agent_id` | string | |
| trust_level | INTEGER NOT NULL | `trust_level` | number (integer) | 0=System, 1=Privileged, 2=Internal, 3=Restricted. |
| capabilities | TEXT NOT NULL DEFAULT '[]' | `capabilities` | string | JSON-encoded array stored as string. |
| allowed_topics | TEXT (nullable) | `allowed_topics` | string or null | JSON-encoded array stored as string, or null. |
| allowed_categories | TEXT (nullable) | `allowed_categories` | string or null | JSON-encoded array stored as string, or null. |
| enrolled_at | INTEGER NOT NULL | `enrolled_at` | number (integer) | Unix timestamp (seconds). |
| last_seen_at | INTEGER NOT NULL | `last_seen_at` | number (integer) | Unix timestamp (seconds). |
| active | INTEGER NOT NULL DEFAULT 1 | `active` | number (integer) | 0 or 1 (boolean as integer). |

ORDER BY: `agent_id ASC`

### Table: audit_log

| SQL Column | SQL Type | JSON Key | JSON Type | Notes |
|-----------|----------|----------|-----------|-------|
| event_id | INTEGER PRIMARY KEY | `event_id` | number (integer) | |
| timestamp | INTEGER NOT NULL | `timestamp` | number (integer) | Unix timestamp (seconds). |
| session_id | TEXT NOT NULL | `session_id` | string | |
| agent_id | TEXT NOT NULL | `agent_id` | string | |
| operation | TEXT NOT NULL | `operation` | string | |
| target_ids | TEXT NOT NULL DEFAULT '[]' | `target_ids` | string | JSON-encoded array stored as string. |
| outcome | INTEGER NOT NULL | `outcome` | number (integer) | |
| detail | TEXT NOT NULL DEFAULT '' | `detail` | string | |

ORDER BY: `event_id ASC`

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `unimatrix-server export` CLI subcommand exists and produces JSONL output to stdout by default. | Integration test: invoke binary, capture stdout, parse each line as JSON. |
| AC-02 | `unimatrix-server export --output <path>` writes to the specified file instead of stdout. | Integration test: invoke with --output, verify file exists and content matches. |
| AC-03 | First line is a header with `_header`, `schema_version`, `exported_at`, `entry_count`, `format_version` (=1). | Integration test: parse first line, assert all fields present with correct types. |
| AC-04 | Every non-header line contains a `_table` field identifying the source table. | Integration test: parse all lines, assert `_table` present on every non-header line. |
| AC-05 | All 8 tables are exported with all columns preserved. | Integration test: create database with data in all 8 tables, export, verify all 8 `_table` values appear and row counts match. |
| AC-06 | Entries table includes all 26 columns with confidence, helpful_count, unhelpful_count, access_count, and learned signals preserved exactly. | Integration test: insert entry with specific values for all 26 columns, export, verify each field in the JSONL row matches. |
| AC-07 | Rows within each table are ordered by primary key ascending. | Integration test: insert entries with IDs out of order, verify export orders them by id ASC. |
| AC-08 | Tables are emitted in dependency order: entries before entry_tags. | Integration test: collect `_table` values in order from output, verify entries appears before entry_tags. |
| AC-09 | Null SQL values are represented as JSON `null` (not empty string, not omitted). | Unit test: insert entry with `supersedes = NULL` and `pre_quarantine_status = NULL`, export, verify JSON has `"supersedes": null`. |
| AC-10 | Empty database produces valid JSONL with header and counter rows only. | Integration test: open fresh database, export, verify header + 5 counter rows, zero entry/tag/etc. rows. |
| AC-11 | Export of 500 entries completes in under 5 seconds. | Benchmark test: populate 500 entries with tags and co-access, measure wall-clock time. |
| AC-12 | Export subcommand does not require a running MCP server. | Integration test: export with no server running, verify success. |
| AC-13 | `--project-dir` flag is respected by the export subcommand. | Integration test: create database in a non-default directory, invoke with --project-dir, verify export reads from that database. |
| AC-14 | Output is deterministic: two exports of the same database produce byte-identical output (excluding `exported_at`). | Integration test: export twice with mocked/fixed timestamp, compare byte-for-byte. |
| AC-15 | Exit code is 0 on success, non-zero on error. Errors written to stderr. | Integration test: verify exit code; test with invalid path, verify non-zero exit and stderr message. |
| AC-16 | Unit tests verify JSONL serialization for each table type including edge cases: null optional fields, empty strings, unicode content, maximum integer values. | Unit tests in export module. |
| AC-17 | Integration test creates database with representative data across all 8 tables, exports, and verifies valid JSONL with correct row counts and field values. | Integration test. |
| AC-18 | Excluded tables (sessions, observations, injection_log, signal_queue, query_log, observation_metrics, observation_phase_metrics, shadow_evaluations, topic_deliveries, vector_map) do NOT appear in output. | Integration test: populate excluded tables, export, verify no `_table` values from the excluded set. |

## Domain Models

### Header Object

```
{
  "_header": true,
  "schema_version": <integer>,    // From counters table
  "exported_at": <integer>,       // Unix timestamp at export time
  "entry_count": <integer>,       // SELECT COUNT(*) FROM entries
  "format_version": 1             // Fixed for v1 format
}
```

The header is the first line. It identifies the export format and provides metadata for the importer to validate compatibility before processing rows.

### Row Object

```
{
  "_table": "<table_name>",
  "<column_1>": <value>,
  "<column_2>": <value>,
  ...
}
```

Each row maps 1:1 to a SQL row. The `_table` discriminator allows streaming import: the importer processes each line independently based on `_table` without needing section markers or buffering.

### Key Domain Terms

- **Entry**: A knowledge record (decision, pattern, convention, lesson) in the `entries` table. Has 26 columns including learned signals (confidence, access_count, helpful/unhelpful counts).
- **Correction Chain**: A sequence of entries linked by `content_hash` and `previous_hash`. Represents the evolution of a knowledge record over time.
- **Confidence**: A composite f64 score [0.0, 1.0] computed from 6 factors (base, usage, freshness, helpfulness, correction, trust). Persisted in `entries.confidence`.
- **Co-Access Pair**: Two entries accessed together in the same context, tracked in `co_access` with a count and timestamp. Invariant: `entry_id_a < entry_id_b`.
- **Agent Registry**: Enrollment records for agents with trust levels and capabilities. Protected agents ("system", "human") are included in export.
- **Audit Log**: Append-only compliance trail. Every security-relevant operation is logged.
- **Format Version**: Integer in the header. v1 is this specification. If the format changes, increment to v2. nan-002 checks `format_version` for compatibility.

## User Workflows

### Workflow 1: Backup Before Destructive Operation

1. User runs `unimatrix-server export --output backup.jsonl`
2. Export opens the database, reads all 8 tables in a single transaction, writes JSONL.
3. User performs destructive operation (schema migration, model upgrade, experimentation).
4. If needed, user restores from `backup.jsonl` via nan-002 (future).

### Workflow 2: Cross-Project Knowledge Transfer

1. User runs `unimatrix-server export --output knowledge.jsonl` in project A.
2. User copies `knowledge.jsonl` to project B.
3. User runs `unimatrix-server import knowledge.jsonl` in project B (nan-002, future).
4. Project B now has project A's knowledge with all confidence scores and learned signals intact.

### Workflow 3: Inspect Knowledge Base

1. User runs `unimatrix-server export` (no --output, writes to stdout).
2. User pipes output through `jq` for filtering/inspection: e.g., `unimatrix-server export | jq 'select(._table == "entries")'`.

### Workflow 4: Concurrent Export While Server Running

1. MCP server is running and serving agents.
2. User runs `unimatrix-server export --output backup.jsonl` in a separate terminal.
3. Export opens the database in WAL mode, acquires a read transaction, reads a consistent snapshot.
4. Server continues to serve agents without interruption. Export does not block writes.

## Constraints

1. **Schema version 11**: Export reads the current schema. `Store::open()` migrates to current schema on open. No need to support older schema versions.
2. **Single-threaded, synchronous**: No tokio runtime. Sequential SQL reads, sequential writes. Matches the Hook subcommand pattern.
3. **No new crate dependencies**: serde_json handles JSON. No BLOB columns in scope means no base64.
4. **Server not required**: Export opens the database directly. WAL mode allows concurrent reads if the server is running.
5. **Memory streaming**: Rows are written to output as they are read from each table. No full-table in-memory buffering of all tables simultaneously.
6. **Format stability**: `format_version: 1` in the header. Future format changes increment this version.
7. **No embeddings**: vector_map excluded. Embeddings are not in SQLite; they are in the HNSW in-memory index persisted to the `vector/` directory.
8. **Confidence continuity**: All learned signals preserved exactly. On restore, the system serves content identically -- no cold start.
9. **JSON-string-in-JSON columns**: Columns containing JSON data (`capabilities`, `allowed_topics`, `allowed_categories`, `target_ids`) are exported as JSON strings, not parsed/inlined as JSON values. This prevents double-encoding issues and ensures the importer stores the exact string back into SQLite.
10. **Transaction isolation**: A single `BEGIN DEFERRED` transaction wraps all reads. This provides snapshot isolation, ensuring rows from different tables reflect the same logical database state even if the MCP server writes concurrently.

## Dependencies

- **rusqlite** (existing): Direct SQL access via `Store::lock_conn()`.
- **serde_json** (existing): JSON serialization.
- **clap** (existing): CLI argument parsing.
- **unimatrix-store** (existing): `Store::open()`, `Store::lock_conn()`.
- **unimatrix-server::project** (existing): `ensure_data_directory()` for project path resolution.

## NOT in Scope

- **Import functionality** -- That is nan-002.
- **Incremental/differential export** -- Full dump only. Future optimization.
- **Embedding export** -- Derived data, re-generated on import.
- **Operational data export** -- Sessions, observations, injection_log, signal_queue, query_log, observation_metrics, observation_phase_metrics, shadow_evaluations, topic_deliveries are excluded.
- **Compression** -- Plain text. Users pipe through gzip/zstd themselves.
- **Streaming/partial export** -- Entire knowledge base exported atomically. No table-level selection.
- **MCP tool exposure** -- CLI subcommand only, not an MCP tool.
- **Remote/network export** -- Local filesystem only.
- **Schema migration during export** -- Export reads the current schema version after `Store::open()` has migrated.
- **Atomic output file writes** -- No temp-file-then-rename pattern. Partial files may remain on error.
- **Hash chain validation** -- Export dumps raw data. Validation is nan-002's responsibility.
- **Column list derivation from shared definition** (SR-02, SR-04) -- Noted as a desirable architectural property but not a functional requirement for v1. The architect may choose to implement this pattern; it is not mandated.
