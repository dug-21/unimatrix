# nxs-006: Architecture — SQLite Cutover

## Overview

nxs-006 delivers three things: (1) CLI migration tooling to move data from redb to SQLite, (2) production database migration, and (3) a feature flag default flip making SQLite the default backend. No code is removed; no refactoring occurs. The redb backend remains compilable as a backout path and for the export subcommand.

---

## Component Architecture

### Component 1: Migration Module (`crates/unimatrix-store/src/migrate/`)

A new module within unimatrix-store containing the export and import logic. This module lives in the store crate (not the server crate) because it needs direct access to redb table definitions and SQLite connection internals.

```
crates/unimatrix-store/src/migrate/
  mod.rs          -- module root, TableDescriptor enum, intermediate format types
  export.rs       -- #[cfg(not(feature = "backend-sqlite"))] redb export logic
  import.rs       -- #[cfg(feature = "backend-sqlite")] SQLite import logic
  format.rs       -- Shared intermediate format (serde types, JSON-lines I/O)
```

**Rationale**: Placing migration in the store crate gives it access to both backend internals through cfg-gating. The server crate just wires up the CLI subcommands.

#### Intermediate Format (ADR-001)

JSON-lines file with one JSON object per line. Each line represents either a table header or a data row.

```jsonl
{"table":"entries","key_type":"u64","value_type":"blob","row_count":53}
{"key":"42","value":"<base64-encoded bincode blob>"}
{"key":"43","value":"<base64-encoded bincode blob>"}
...
{"table":"topic_index","key_type":"str_u64","value_type":"unit","row_count":53}
{"key":["auth",42],"value":null}
...
{"table":"tag_index","key_type":"str","value_type":"u64","multimap":true,"row_count":106}
{"key":"rust","value":42}
{"key":"rust","value":43}
...
{"table":"co_access","key_type":"u64_u64","value_type":"blob","row_count":368}
{"key":[5,10],"value":"<base64>"}
...
```

Key design decisions:
- **JSON-lines not binary**: human-inspectable, debuggable, diffable. At ~53 entries the file size is negligible (~100KB).
- **Base64 for blobs**: safe text encoding, no escaping issues. Standard base64 (RFC 4648).
- **Table header with row_count**: enables pre-verification before import begins. Import aborts if actual rows != declared row_count.
- **Typed keys**: key_type field documents the key structure. The import parser uses this to deserialize keys correctly.
- **Multimap flag**: TAG_INDEX and FEATURE_ENTRIES set `"multimap": true`. Each (key, value) pair gets its own line.

#### Table Descriptor Enum

All 17 tables are enumerated in a single const array to ensure export and import handle exactly the same set:

```rust
pub(crate) const ALL_TABLES: &[TableDescriptor] = &[
    TableDescriptor::U64Blob { name: "entries" },
    TableDescriptor::StrU64Unit { name: "topic_index" },
    TableDescriptor::StrU64Unit { name: "category_index" },
    TableDescriptor::MultimapStrU64 { name: "tag_index" },
    TableDescriptor::U64U64Unit { name: "time_index" },
    TableDescriptor::U8U64Unit { name: "status_index" },
    TableDescriptor::U64U64 { name: "vector_map" },
    TableDescriptor::StrU64 { name: "counters" },
    TableDescriptor::StrBlob { name: "agent_registry" },
    TableDescriptor::U64Blob { name: "audit_log" },
    TableDescriptor::MultimapStrU64 { name: "feature_entries" },
    TableDescriptor::U64U64Blob { name: "co_access" },
    TableDescriptor::StrU64Unit { name: "outcome_index" },
    TableDescriptor::StrBlob { name: "observation_metrics" },
    TableDescriptor::U64Blob { name: "signal_queue" },
    TableDescriptor::StrBlob { name: "sessions" },
    TableDescriptor::U64Blob { name: "injection_log" },
];
```

#### Export Path (redb -> JSON-lines)

Only compiles under `#[cfg(not(feature = "backend-sqlite"))]`:

1. Open the redb database at the given path (using `redb::Database::open()` not `Store::open()`, to avoid table creation side effects).
2. Begin a read transaction.
3. For each table in `ALL_TABLES`:
   a. Open the table (or multimap table).
   b. Write the table header line (name, key_type, value_type, row_count).
   c. Iterate all rows, writing one JSON line per row.
   d. For blob values: base64-encode the raw bytes.
   e. For composite keys `(&str, u64)`: serialize as `["string", number]`.
   f. For composite keys `(u64, u64)`: serialize as `[number, number]`.
   g. For composite keys `(u8, u64)`: serialize as `[number, number]`.
   h. For unit values: serialize as `null`.
4. Print summary: table name, expected row count, actual row count.

**Critical**: The export must use `redb::Database::open()` (read-only mode if available, or builder) rather than `Store::open()` to avoid running migrations that could modify data. Actually, redb's `Builder::create()` opens existing databases without modifications, and the migration only runs on read-time anyway. Use `Store::open()` for simplicity since it opens the database and ensures tables are accessible. The export is read-only after opening.

No wait -- `Store::open()` runs `migrate_if_needed()` which writes. Use `redb::Builder::new().create()` directly to get a read-only handle. Then access tables by definition.

Actually, `redb::Builder::new().create()` opens for read+write. And `migrate_if_needed` only writes if the schema version is behind. For production export, the schema is already current. So using `Store::open()` is safe and simpler. But let's not take the risk: open the redb::Database directly, skip migrations.

**Decision**: Open using `redb::Builder::new().create()` directly. Skip `migrate_if_needed()`. The export only reads.

#### Import Path (JSON-lines -> SQLite)

Only compiles under `#[cfg(feature = "backend-sqlite")]`:

1. Create the SQLite database at the output path using `Store::open()` (runs `create_tables()` and `migrate_if_needed()` to ensure correct schema).
2. Read the JSON-lines file.
3. For each table section:
   a. Parse the table header line. Record expected row_count.
   b. Parse data rows. Insert each row using direct SQL (not Store API methods) for performance and to avoid business logic side effects (counters, indexes, etc. are imported as-is).
   c. Count actual rows inserted. Compare to expected row_count. Abort on mismatch.
4. After all tables imported, verify total counts match.

**Critical**: The import uses `Store::open()` to create the database (so tables and PRAGMAs are set up correctly), then inserts data using raw SQL on the underlying connection. This bypasses the Store API's write methods which would trigger counter updates and index maintenance -- we want exact data copy, not re-insertion.

Access to the underlying connection: `Store` has `pub(crate) conn: Mutex<Connection>`. The import module is in the same crate, so it can access `store.conn.lock()` directly.

#### Verification

After import, the import tool runs a verification pass:
1. For each table: `SELECT COUNT(*) FROM {table}` must equal the header's row_count.
2. Counter verification: `next_entry_id` must be > MAX(id) from entries table.
3. Schema version check: `schema_version` counter must equal 5.

### Component 2: CLI Subcommands (`crates/unimatrix-server/src/main.rs`)

Add `Export` and `Import` variants to the existing `Command` enum:

```rust
#[derive(Subcommand)]
enum Command {
    Hook { event: String },

    /// Export all tables from the redb database to a JSON-lines file.
    #[cfg(not(feature = "backend-sqlite"))]
    Export {
        /// Path to the output JSON-lines file.
        #[arg(long)]
        output: PathBuf,
        /// Override the database path (default: auto-detected project database).
        #[arg(long)]
        db_path: Option<PathBuf>,
    },

    /// Import tables from a JSON-lines file into a new SQLite database.
    #[cfg(feature = "backend-sqlite")]
    Import {
        /// Path to the input JSON-lines file (from export).
        #[arg(long)]
        input: PathBuf,
        /// Path for the output SQLite database file.
        #[arg(long)]
        output: PathBuf,
    },
}
```

Both subcommands are synchronous (like Hook). No tokio runtime needed.

The Export subcommand:
1. Resolves the database path (from `--db-path` flag or auto-detected via `project::ensure_data_directory()`).
2. Checks for a running server (PID file) and aborts with a clear error if found.
3. Calls `unimatrix_store::migrate::export()`.

The Import subcommand:
1. Validates the input file exists.
2. Validates the output path does not already exist (safety: refuse to overwrite).
3. Calls `unimatrix_store::migrate::import()`.
4. Reports success with table-by-table row counts.

### Component 3: Feature Flag Default Flip

Changes to Cargo.toml files:

**`crates/unimatrix-store/Cargo.toml`**:
```toml
[features]
default = ["backend-sqlite"]
test-support = ["dep:tempfile"]
backend-sqlite = ["dep:rusqlite"]
```

The redb dependency becomes unconditional but unused at runtime when `backend-sqlite` is active (it is needed for the export code path which is cfg-gated).

Wait -- this is wrong. If `backend-sqlite` is the default, the redb modules (db.rs, read.rs, write.rs, etc.) are NOT compiled. But we need them to remain compilable for the export path. The export is cfg-gated with `#[cfg(not(feature = "backend-sqlite"))]`. So when building with `backend-sqlite` (new default), the export code is excluded and the redb code is excluded. When building WITHOUT `backend-sqlite` (explicit `--no-default-features`), the redb code compiles and the export code compiles.

This means: to compile the export tool, you build with `--no-default-features` (or `--no-default-features --features redb` on the server).

**`crates/unimatrix-server/Cargo.toml`**:
```toml
[features]
default = ["mcp-briefing", "backend-sqlite"]
mcp-briefing = []
backend-sqlite = ["unimatrix-store/backend-sqlite"]
redb = ["dep:redb"]
```

Remove `redb` from default features. Add `backend-sqlite` to defaults.

**`crates/unimatrix-engine/src/project.rs`** (ADR-002):

The `db_path` must change based on the active backend:

```rust
#[cfg(not(feature = "backend-sqlite"))]
let db_path = data_dir.join("unimatrix.redb");
#[cfg(feature = "backend-sqlite")]
let db_path = data_dir.join("unimatrix.db");
```

But `unimatrix-engine` does not currently have a `backend-sqlite` feature. It needs one, forwarded from the server:

**`crates/unimatrix-engine/Cargo.toml`**:
```toml
[features]
backend-sqlite = []
```

**`crates/unimatrix-server/Cargo.toml`** updated:
```toml
backend-sqlite = ["unimatrix-store/backend-sqlite", "unimatrix-engine/backend-sqlite"]
```

This way, when the server is compiled with `backend-sqlite`, the engine produces `unimatrix.db` as the db_path, and the store opens SQLite. When compiled without it (for export), the engine produces `unimatrix.redb` and the store opens redb.

---

## ADR Summary

### ADR-001: JSON-Lines Intermediate Format

**Decision**: Use JSON-lines with base64-encoded binary blobs as the intermediate format for data migration.

**Context**: Two options considered: (a) JSON-lines with base64, (b) bincode-serialized dump file. The production database has ~53 entries, so file size is irrelevant. JSON-lines is human-inspectable, debuggable, and allows partial verification (grep for specific entries). Bincode would require custom tooling to inspect.

**Consequences**: Adds `base64` and `serde_json` as dependencies to unimatrix-store (serde_json is already a workspace dep). The format is one-time -- used for a single migration, then the tooling is removed in nxs-007.

### ADR-002: Database Filename Transition

**Decision**: Use a cfg-gated database filename: `unimatrix.redb` for the redb backend, `unimatrix.db` for the SQLite backend. The cfg gate is on the `backend-sqlite` feature in unimatrix-engine's `project.rs`.

**Context**: The database path is hardcoded to `unimatrix.redb` in `project.rs:94`. After the default flip to SQLite, opening `unimatrix.redb` with SQLite would either create an empty database (if the old file was removed) or fail with "not a database" (if the old redb file still exists). A cfg-gated filename ensures each backend opens the correct file.

**Risk addressed**: SR-01 (Database Filename Mismatch). This is the critical risk.

**Consequences**: The `backend-sqlite` feature must propagate from server -> engine (new feature flag on engine crate). After nxs-007 removes redb, the cfg gate is removed and `unimatrix.db` becomes unconditional. The import subcommand produces a `unimatrix.db` file; the human places it at `~/.unimatrix/{hash}/unimatrix.db`.

### ADR-003: Export Uses Direct redb Access, Not Store API

**Decision**: The export subcommand opens the redb database using `redb::Builder::new().create()` directly, bypassing `Store::open()`. This avoids running `migrate_if_needed()` which could modify the production database during export.

**Context**: `Store::open()` calls `migrate_if_needed()` which writes to the database if the schema version is behind. While the production database should already be at the current schema version, the export tool should be read-only for safety.

**Consequences**: The export code must manually construct redb table definitions (using the constants from `schema.rs`). This is straightforward since the table definitions are already available.

### ADR-004: Import Uses Store::open() Then Raw SQL

**Decision**: The import subcommand creates the SQLite database via `Store::open()` (to get correct table DDL and PRAGMAs), then inserts data using direct SQL on the underlying `Mutex<Connection>`.

**Context**: Using Store API write methods (e.g., `insert_entry()`) would trigger counter updates, index maintenance, and other business logic. We want an exact data copy, not a re-insertion. But we need the tables to exist with correct schemas, which `Store::open()` handles via `create_tables()`.

**Consequences**: The import module accesses `store.conn` directly (pub(crate) visibility within the store crate). This is acceptable because the import is a one-time migration tool, not a long-lived API.

---

## Integration Surface

### Crate Dependency Changes

| Crate | Change |
|-------|--------|
| unimatrix-store | Add `base64` dep. Add `serde_json` dep. New `migrate/` module. |
| unimatrix-engine | Add `backend-sqlite` feature (no deps). Cfg-gate db_path in project.rs. |
| unimatrix-server | Add Export/Import CLI subcommands. Change default features. |

### File Changes

| File | Change Type |
|------|-------------|
| `crates/unimatrix-store/Cargo.toml` | Add default features, add base64/serde_json deps |
| `crates/unimatrix-store/src/lib.rs` | Add `mod migrate;` |
| `crates/unimatrix-store/src/migrate/mod.rs` | New: module root, types |
| `crates/unimatrix-store/src/migrate/format.rs` | New: intermediate format types |
| `crates/unimatrix-store/src/migrate/export.rs` | New: redb export (cfg-gated) |
| `crates/unimatrix-store/src/migrate/import.rs` | New: SQLite import (cfg-gated) |
| `crates/unimatrix-engine/Cargo.toml` | Add backend-sqlite feature |
| `crates/unimatrix-engine/src/project.rs` | Cfg-gate db_path filename |
| `crates/unimatrix-server/Cargo.toml` | Change default features, propagate backend-sqlite to engine |
| `crates/unimatrix-server/src/main.rs` | Add Export/Import subcommands |

### What Does NOT Change

- No changes to any Store API methods
- No changes to any MCP tool implementation
- No changes to UDS listener
- No changes to vector index
- No changes to test files (existing tests continue to pass)
- No changes to any file outside the three crates listed above
- No removal of any existing code

---

## Verification Strategy

1. **Unit tests** for intermediate format serialization/deserialization (round-trip each key type)
2. **Integration test**: create a redb test database with data in all 17 tables, export, import, verify row counts and blob content
3. **Feature flag compilation matrix**: verify all four combinations compile:
   - default (backend-sqlite): server + store + engine compile, import available
   - no-default-features: redb backend, export available
   - explicit backend-sqlite: same as default
   - explicit redb (no backend-sqlite): same as no-default-features
4. **Project path test**: verify `ensure_data_directory()` returns `.db` suffix under backend-sqlite and `.redb` suffix without it
