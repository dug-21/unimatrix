# nan-001: Knowledge Export — Architecture

## System Overview

Knowledge Export adds a CLI subcommand (`unimatrix-server export`) that reads the SQLite database and writes a JSONL file containing all long-term knowledge, security/audit, and counter data. This is a read-only, synchronous operation that follows the existing `hook` subcommand pattern — no tokio runtime, no MCP server dependency.

The export format is the contract that nan-002 (Knowledge Import) will consume. Format stability is the primary design concern.

### Position in the System

```
unimatrix-server binary
├── (no subcommand) → tokio MCP server
├── hook <event>    → sync UDS dispatch (existing)
└── export          → sync JSONL dump (new, this feature)
```

The export subcommand shares only path resolution with the server. It opens the database directly via `Store::open()`, acquires a read transaction, and streams rows to output. No service layer, no vector index, no embedding pipeline.

## Component Breakdown

### 1. CLI Extension (`main.rs`)

**Responsibility**: Add `Export` variant to the `Command` enum, dispatch to the export module.

**Changes**:
- Add `Export { output: Option<PathBuf> }` to `Command` enum
- Add match arm in `main()` that calls `export::run_export()`
- Follows the hook pattern: sync path, no tokio

### 2. Export Module (`crates/unimatrix-server/src/export.rs`)

**Responsibility**: Orchestrate the full export — open database, begin transaction, iterate tables in order, write JSONL.

**Public interface**:
```rust
/// Run the export subcommand.
///
/// Opens the database, wraps the read in a single transaction for snapshot
/// consistency, and writes JSONL to `output` (or stdout if None).
pub fn run_export(
    project_dir: Option<&Path>,
    output: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>>
```

**Internal structure**:
- `write_header()` — emit the header line with metadata
- `export_counters()` — emit counter rows
- `export_entries()` — emit all 26 columns per entry, ordered by id
- `export_entry_tags()` — emit (entry_id, tag) pairs, ordered by (entry_id, tag)
- `export_co_access()` — emit pairs, ordered by (entry_id_a, entry_id_b)
- `export_feature_entries()` — emit (feature_id, entry_id), ordered by (feature_id, entry_id)
- `export_outcome_index()` — emit (feature_cycle, entry_id), ordered by (feature_cycle, entry_id)
- `export_agent_registry()` — emit agent records, ordered by agent_id
- `export_audit_log()` — emit audit records, ordered by event_id

Each function takes `&Connection` and `&mut impl Write`, reads via a prepared SQL statement, and writes one JSON line per row.

### 3. Row Serialization

**Responsibility**: Convert each SQL row to a JSON line with the `_table` discriminator.

**Approach**: Use `serde_json::Map<String, Value>` (which is backed by `BTreeMap` internally when the `preserve_order` feature is NOT enabled; see ADR-003 for the chosen approach). Each per-table function builds the map explicitly from column values, ensuring:
- Column names map 1:1 to JSON keys (no renaming)
- SQL NULL becomes `serde_json::Value::Null` (not omitted)
- SQL INTEGER becomes `Value::Number`
- SQL REAL becomes `Value::Number` (f64, 15+ significant digits via serde_json default)
- SQL TEXT becomes `Value::String`
- JSON-encoded TEXT columns (capabilities, allowed_topics, allowed_categories, target_ids) are emitted as their raw string value — no double-encoding, no parsing

## Component Interactions

```
main.rs
  │
  ├─ Cli::parse() → Command::Export { output }
  │
  └─ export::run_export(project_dir, output)
       │
       ├─ project::ensure_data_directory(project_dir, None) → ProjectPaths
       │
       ├─ Store::open(&paths.db_path) → Store
       │
       ├─ store.lock_conn() → MutexGuard<Connection>
       │
       ├─ conn.execute_batch("BEGIN DEFERRED") → snapshot isolation
       │
       ├─ write_header(&conn, &mut writer)
       │    └─ SELECT value FROM counters WHERE name = 'schema_version'
       │    └─ SELECT COUNT(*) FROM entries
       │
       ├─ export_counters(&conn, &mut writer)
       │    └─ SELECT name, value FROM counters ORDER BY name
       │
       ├─ export_entries(&conn, &mut writer)
       │    └─ SELECT <all 26 columns> FROM entries ORDER BY id
       │
       ├─ export_entry_tags(&conn, &mut writer)
       │    └─ SELECT entry_id, tag FROM entry_tags ORDER BY entry_id, tag
       │
       ├─ ... (remaining tables in dependency order)
       │
       ├─ conn.execute_batch("COMMIT")
       │
       └─ Ok(())
```

### Data Flow

1. `Store::open()` handles migration and PRAGMA setup (reuses existing code)
2. `lock_conn()` acquires the connection mutex
3. `BEGIN DEFERRED` starts a read transaction — all subsequent reads see a consistent snapshot (SR-07)
4. Each table is read row-by-row via prepared statements
5. Each row is serialized to a JSON line and written to the output writer
6. `COMMIT` releases the read transaction
7. Writer is flushed

### Error Boundaries

- **Store::open() failure**: Propagated as `Box<dyn Error>`, printed to stderr, exit 1
- **SQL query failure**: Propagated immediately. No partial exports — any table failure aborts the entire export
- **Write failure**: `std::io::Error` from the writer, propagated immediately
- **Serialization failure**: `serde_json::Error` — should not occur with valid SQL types, but propagated if it does

## Technology Decisions

| Decision | Choice | ADR |
|----------|--------|-----|
| Snapshot isolation | BEGIN DEFERRED transaction | ADR-001 |
| Row serialization format | Explicit column mapping with serde_json::Value | ADR-002 |
| Key ordering for determinism | Explicit insertion order via serde_json::Map + sequential insert | ADR-003 |
| Module location | `crates/unimatrix-server/src/export.rs` | (follows hook.rs pattern) |

## Integration Points

### Dependencies (existing code reused)

| Component | Crate | What export uses |
|-----------|-------|-----------------|
| `Store::open()` | `unimatrix-store` | Database open + migration |
| `Store::lock_conn()` | `unimatrix-store` | Direct `rusqlite::Connection` access |
| `project::ensure_data_directory()` | `unimatrix-engine` (re-exported via `unimatrix-server`) | Path resolution |
| `serde_json` | (external) | JSON serialization |
| `clap` | (external) | CLI argument parsing |

### No new dependencies required

All required crates are already in the `unimatrix-server` dependency tree:
- `unimatrix-store` (for `Store`)
- `unimatrix-engine` (for `project::ensure_data_directory`, `ProjectPaths`)
- `serde_json` (already in Cargo.toml)
- `clap` (already in Cargo.toml)

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `export::run_export` | `pub fn run_export(project_dir: Option<&Path>, output: Option<&Path>) -> Result<(), Box<dyn std::error::Error>>` | New: `crates/unimatrix-server/src/export.rs` |
| `Command::Export` | `Export { #[arg(short, long)] output: Option<PathBuf> }` | Modified: `crates/unimatrix-server/src/main.rs` |
| `Store::open` | `pub fn open(path: impl AsRef<Path>) -> Result<Self>` | Existing: `crates/unimatrix-store/src/db.rs:23` |
| `Store::lock_conn` | `pub fn lock_conn(&self) -> MutexGuard<'_, Connection>` | Existing: `crates/unimatrix-store/src/db.rs:86` |
| `project::ensure_data_directory` | `pub fn ensure_data_directory(override_dir: Option<&Path>, base_dir: Option<&Path>) -> io::Result<ProjectPaths>` | Existing: `crates/unimatrix-engine/src/project.rs:142` |
| `ProjectPaths.db_path` | `pub db_path: PathBuf` | Existing: `crates/unimatrix-engine/src/project.rs:13` |

## JSONL Format Contract (v1)

### Header Line

```json
{"_header":true,"schema_version":11,"exported_at":1741644000,"entry_count":53,"format_version":1}
```

| Field | Type | Description |
|-------|------|-------------|
| `_header` | `bool` | Always `true`. Discriminator for the header line. |
| `schema_version` | `integer` | Value from `counters` table. |
| `exported_at` | `integer` | Unix timestamp (seconds) at export time. |
| `entry_count` | `integer` | `COUNT(*)` from `entries` table. |
| `format_version` | `integer` | Always `1` for this version. Incremented on breaking format changes. |

### Data Lines

Every non-header line has a `_table` field as the first key, followed by the table's columns.

### Entries Table — Column-to-JSON Mapping (26 columns)

| SQL Column | JSON Key | JSON Type | Notes |
|------------|----------|-----------|-------|
| `id` | `id` | number | PRIMARY KEY |
| `title` | `title` | string | |
| `content` | `content` | string | |
| `topic` | `topic` | string | |
| `category` | `category` | string | |
| `source` | `source` | string | |
| `status` | `status` | number | integer enum |
| `confidence` | `confidence` | number | f64, full precision |
| `created_at` | `created_at` | number | unix timestamp |
| `updated_at` | `updated_at` | number | unix timestamp |
| `last_accessed_at` | `last_accessed_at` | number | unix timestamp |
| `access_count` | `access_count` | number | |
| `supersedes` | `supersedes` | number or null | nullable FK |
| `superseded_by` | `superseded_by` | number or null | nullable FK |
| `correction_count` | `correction_count` | number | |
| `embedding_dim` | `embedding_dim` | number | |
| `created_by` | `created_by` | string | |
| `modified_by` | `modified_by` | string | |
| `content_hash` | `content_hash` | string | |
| `previous_hash` | `previous_hash` | string | |
| `version` | `version` | number | |
| `feature_cycle` | `feature_cycle` | string | |
| `trust_source` | `trust_source` | string | |
| `helpful_count` | `helpful_count` | number | |
| `unhelpful_count` | `unhelpful_count` | number | |
| `pre_quarantine_status` | `pre_quarantine_status` | number or null | nullable |

### Type Encoding Rules

1. SQL `NULL` -> JSON `null` (never omitted, never empty string)
2. SQL `INTEGER` -> JSON number (no quotes)
3. SQL `REAL` -> JSON number (serde_json serializes f64 with sufficient precision for lossless round-trip; see ADR-002)
4. SQL `TEXT` -> JSON string
5. JSON-in-TEXT columns (`capabilities`, `allowed_topics`, `allowed_categories`, `target_ids`) -> JSON string (the raw text value, not parsed/re-encoded)

### Table Emission Order

1. Header
2. `counters` (ORDER BY name)
3. `entries` (ORDER BY id)
4. `entry_tags` (ORDER BY entry_id, tag)
5. `co_access` (ORDER BY entry_id_a, entry_id_b)
6. `feature_entries` (ORDER BY feature_id, entry_id)
7. `outcome_index` (ORDER BY feature_cycle, entry_id)
8. `agent_registry` (ORDER BY agent_id)
9. `audit_log` (ORDER BY event_id)

### Excluded Tables

`vector_map`, `sessions`, `observations`, `injection_log`, `signal_queue`, `query_log`, `observation_metrics`, `observation_phase_metrics`, `shadow_evaluations`, `topic_deliveries`

## Open Questions

1. **Schema column list drift (SR-02, SR-04)**: The scope recommends deriving column lists from a shared definition. For v1, hardcoding the column list is acceptable since schema v11 is stable and the export is a new module. If schema changes land before nan-001 delivery, the column list must be updated manually. A compile-time shared definition (const array or macro) is a future enhancement, not a v1 requirement.

2. **Store::open() migration race (SR-08)**: If a server is already running, migration has already happened. If no server is running, export opens and migrates safely alone. The scope correctly notes this is low risk. No architectural mitigation needed.
