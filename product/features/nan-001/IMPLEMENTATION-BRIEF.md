# nan-001 Implementation Brief: Knowledge Export

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/nan-001/SCOPE.md |
| Architecture | product/features/nan-001/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-001/specification/SPECIFICATION.md |
| Risk Strategy | product/features/nan-001/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-001/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| cli-extension | pseudocode/cli-extension.md | test-plan/cli-extension.md |
| export-module | pseudocode/export-module.md | test-plan/export-module.md |
| row-serialization | pseudocode/row-serialization.md | test-plan/row-serialization.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Export the Unimatrix knowledge base to a portable, text-based JSONL file that preserves every field needed for lossless knowledge restore -- including learned confidence scores, usage signals, correction chains, agent enrollment, and audit records. The export covers 8 tables, excludes derived data (embeddings, HNSW index) and ephemeral operational data, and produces deterministic output that serves as the import contract for nan-002 (Knowledge Import). Implemented as a synchronous CLI subcommand (`unimatrix-server export`) following the existing `hook` subcommand pattern.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Snapshot isolation for concurrent reads | Wrap all 8 table reads in a single `BEGIN DEFERRED` transaction for consistent point-in-time snapshot; shared read lock does not block MCP server writes | SR-07, ADR-001 | architecture/ADR-001-snapshot-isolation.md |
| Row serialization approach | Explicit column-to-JSON mapping using `serde_json::Value` construction per table; no Rust struct derive; SQL NULL -> JSON null (never omitted); JSON-in-TEXT columns emitted as raw strings | SR-01, SR-03, ADR-002 | architecture/ADR-002-explicit-column-mapping.md |
| Deterministic key ordering | Enable `preserve_order` feature on serde_json for insertion-order determinism; keys follow SQL column declaration order; `_table` is always first key. Fallback: default BTreeMap (lexicographic) if preserve_order breaks existing tests | SR-05, ADR-003 | architecture/ADR-003-deterministic-key-ordering.md |
| Module location | New `crates/unimatrix-server/src/export.rs` following hook.rs pattern | Architecture | -- |
| Direct SQL access | Use `Store::lock_conn()` for raw SQL queries, bypassing Store API to avoid intermediate type conversion and guarantee no data loss | Scope, ADR-002 | architecture/ADR-002-explicit-column-mapping.md |
| Function signature | `run_export(project_dir: Option<&Path>, output: Option<&Path>) -> Result<(), Box<dyn std::error::Error>>` per architecture (not spec's `store: &Store` variant) | Alignment Report | -- |

## Files to Create/Modify

| Path | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/main.rs` | Modify | Add `Export { output: Option<PathBuf> }` variant to `Command` enum; add match arm dispatching to `export::run_export()` |
| `crates/unimatrix-server/src/export.rs` | Create | Export module: `run_export()` entry point, `write_header()`, 8 per-table export functions, row serialization logic |
| `crates/unimatrix-server/Cargo.toml` | Modify | Add `preserve_order` feature to serde_json dependency |
| `crates/unimatrix-server/tests/export_integration.rs` | Create | Integration tests: full export with representative data, empty database, determinism, excluded tables, ordering, --project-dir |
| `crates/unimatrix-server/src/export.rs` (unit tests) | Create | Unit tests within the module: per-table serialization, null handling, float precision, unicode, large integers, JSON-in-TEXT columns |

## Data Structures

### Header Line (first line of JSONL output)

```
{
  "_header": true,
  "schema_version": <integer>,    // From counters table
  "exported_at": <integer>,       // Unix timestamp (seconds) at export time
  "entry_count": <integer>,       // SELECT COUNT(*) FROM entries
  "format_version": 1             // Fixed for v1
}
```

### Row Line (every subsequent line)

```
{
  "_table": "<table_name>",
  "<column_1>": <value>,
  "<column_2>": <value>,
  ...
}
```

### Exported Tables (8 total, in emission order)

1. **counters** -- (name TEXT PK, value INTEGER) -- ORDER BY name
2. **entries** -- 26 columns, PK: id INTEGER -- ORDER BY id
3. **entry_tags** -- (entry_id INTEGER, tag TEXT) -- ORDER BY entry_id, tag
4. **co_access** -- (entry_id_a, entry_id_b, count, last_updated) -- ORDER BY entry_id_a, entry_id_b
5. **feature_entries** -- (feature_id TEXT, entry_id INTEGER) -- ORDER BY feature_id, entry_id
6. **outcome_index** -- (feature_cycle TEXT, entry_id INTEGER) -- ORDER BY feature_cycle, entry_id
7. **agent_registry** -- 8 columns, PK: agent_id TEXT -- ORDER BY agent_id
8. **audit_log** -- 8 columns, PK: event_id INTEGER -- ORDER BY event_id

### Entries Table -- Full 26-Column Schema

| Column | SQL Type | JSON Type | Nullable |
|--------|----------|-----------|----------|
| id | INTEGER PK | number | no |
| title | TEXT NOT NULL | string | no |
| content | TEXT NOT NULL | string | no |
| topic | TEXT NOT NULL | string | no |
| category | TEXT NOT NULL | string | no |
| source | TEXT NOT NULL | string | no |
| status | INTEGER NOT NULL DEFAULT 0 | number | no |
| confidence | REAL NOT NULL DEFAULT 0.0 | number (f64) | no |
| created_at | INTEGER NOT NULL | number | no |
| updated_at | INTEGER NOT NULL | number | no |
| last_accessed_at | INTEGER NOT NULL DEFAULT 0 | number | no |
| access_count | INTEGER NOT NULL DEFAULT 0 | number | no |
| supersedes | INTEGER | number or null | yes |
| superseded_by | INTEGER | number or null | yes |
| correction_count | INTEGER NOT NULL DEFAULT 0 | number | no |
| embedding_dim | INTEGER NOT NULL DEFAULT 0 | number | no |
| created_by | TEXT NOT NULL DEFAULT '' | string | no |
| modified_by | TEXT NOT NULL DEFAULT '' | string | no |
| content_hash | TEXT NOT NULL DEFAULT '' | string | no |
| previous_hash | TEXT NOT NULL DEFAULT '' | string | no |
| version | INTEGER NOT NULL DEFAULT 0 | number | no |
| feature_cycle | TEXT NOT NULL DEFAULT '' | string | no |
| trust_source | TEXT NOT NULL DEFAULT '' | string | no |
| helpful_count | INTEGER NOT NULL DEFAULT 0 | number | no |
| unhelpful_count | INTEGER NOT NULL DEFAULT 0 | number | no |
| pre_quarantine_status | INTEGER | number or null | yes |

### Type Encoding Rules

| SQL Type | JSON Encoding |
|----------|--------------|
| INTEGER NOT NULL | JSON number (integer) |
| INTEGER (nullable) | JSON number or null |
| REAL NOT NULL | JSON number (f64, serde_json/ryu precision) |
| TEXT NOT NULL | JSON string |
| TEXT (nullable) | JSON string or null |
| TEXT containing JSON (capabilities, allowed_topics, allowed_categories, target_ids) | JSON string (raw value, not parsed/inlined) |

## Function Signatures

### Public Entry Point

```rust
// crates/unimatrix-server/src/export.rs
pub fn run_export(
    project_dir: Option<&Path>,
    output: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>>
```

### CLI Extension

```rust
// crates/unimatrix-server/src/main.rs (addition to Command enum)
Export {
    /// Output file path. Defaults to stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
}
```

### Internal Functions (all in export.rs)

```rust
fn write_header(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>
fn export_counters(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>
fn export_entries(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>
fn export_entry_tags(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>
fn export_co_access(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>
fn export_feature_entries(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>
fn export_outcome_index(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>
fn export_agent_registry(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>
fn export_audit_log(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>
```

### Existing Interfaces Used

```rust
// crates/unimatrix-store/src/db.rs
impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self>
    pub fn lock_conn(&self) -> MutexGuard<'_, Connection>
}

// crates/unimatrix-engine/src/project.rs
pub fn ensure_data_directory(
    override_dir: Option<&Path>,
    base_dir: Option<&Path>,
) -> io::Result<ProjectPaths>

pub struct ProjectPaths {
    pub db_path: PathBuf,
    // ...
}
```

## Constraints

1. **Schema v11**: Export reads the current schema. `Store::open()` migrates on open. No support for older schema versions.
2. **Single-threaded, synchronous**: No tokio runtime. Sequential SQL reads and writes. Matches the hook subcommand pattern.
3. **No new crate dependencies**: serde_json (already present) handles JSON. Only change: add `preserve_order` feature flag.
4. **Server not required**: Export opens the database directly via `Store::open()`. WAL mode allows concurrent reads if a server is running.
5. **Memory streaming**: Rows are written to output as read. No full-table in-memory buffering.
6. **Format stability**: `format_version: 1` in header. Future format changes increment this.
7. **No embeddings**: vector_map excluded. Export is text-only.
8. **Confidence continuity**: All learned signals (confidence, helpful_count, unhelpful_count, access_count, last_accessed, co_access counts) preserved exactly.
9. **Transaction isolation**: Single `BEGIN DEFERRED` transaction wraps all reads (ADR-001).
10. **JSON-in-TEXT columns**: capabilities, allowed_topics, allowed_categories, target_ids emitted as raw strings, not parsed/re-encoded.

## Dependencies

| Dependency | Crate | Usage |
|-----------|-------|-------|
| `unimatrix-store` | existing | `Store::open()`, `Store::lock_conn()` |
| `unimatrix-engine` | existing (re-exported) | `project::ensure_data_directory()`, `ProjectPaths` |
| `serde_json` | existing, add `preserve_order` feature | JSON serialization, `Map<String, Value>` with insertion-order |
| `clap` | existing | CLI argument parsing, `#[arg]` attribute on Export variant |
| `rusqlite` | existing (transitive via unimatrix-store) | Direct `Connection` access for SQL queries |

## NOT in Scope

- **Import functionality** -- nan-002
- **Incremental/differential export** -- full dump only; future optimization
- **Embedding export** -- derived data, re-generated on import
- **Operational data export** -- sessions, observations, injection_log, signal_queue, query_log, observation_metrics, observation_phase_metrics, shadow_evaluations, topic_deliveries all excluded
- **Compression** -- plain text; users pipe through gzip/zstd
- **Streaming/partial export** -- entire knowledge base exported atomically
- **MCP tool exposure** -- CLI subcommand only
- **Remote/network export** -- local filesystem only
- **Schema migration during export** -- Store::open() handles migration before export reads
- **Atomic output file writes** -- no temp-file-then-rename; partial files may remain on error
- **Hash chain validation** -- export dumps raw data; validation is nan-002's responsibility
- **Column list derivation from shared definition** -- hardcoded for v1; future enhancement

## Alignment Status

**Overall: PASS with 2 minor WARNs**

Both variances are non-blocking and resolved by implementation guidance:

1. **serde_json key ordering ambiguity (WARN)**: Architecture text slightly ambiguous on preserve_order vs BTreeMap. ADR-003 resolves this: use `preserve_order` feature for insertion-order determinism. If preserve_order breaks existing tests, fall back to default BTreeMap (lexicographic) ordering. Either approach satisfies AC-14 determinism.

2. **run_export signature divergence (WARN)**: Spec says `run_export(store: &Store, ...)`, architecture says `run_export(project_dir: Option<&Path>, ...)`. Architecture version is correct -- run_export handles Store::open() internally, consistent with the component interaction diagram. Implementation follows the architecture signature.

No scope gaps. No scope additions. No vision variances. All 18 acceptance criteria from SCOPE.md covered. All 9 scope risks addressed in architecture and risk strategy.
