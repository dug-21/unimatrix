# nxs-006: Specification — SQLite Cutover

## Domain Model

### Intermediate Format Types

```rust
/// Header line for a table section in the intermediate file.
#[derive(Serialize, Deserialize)]
struct TableHeader {
    table: String,
    key_type: KeyType,
    value_type: ValueType,
    #[serde(default)]
    multimap: bool,
    row_count: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum KeyType {
    U64,         // entries, audit_log, signal_queue, injection_log
    Str,         // counters, agent_registry, sessions, observation_metrics (regular table) + tag_index, feature_entries (multimap)
    StrU64,      // topic_index, category_index, outcome_index
    U64U64,      // time_index, co_access
    U8U64,       // status_index
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ValueType {
    Blob,
    U64,
    Unit,
}

/// Data line for a single row in the intermediate file.
#[derive(Serialize, Deserialize)]
struct DataRow {
    key: serde_json::Value,   // JSON representation of the key
    value: serde_json::Value, // JSON representation of the value (base64 string for blobs, number for u64, null for unit)
}
```

### Table Classification

All 17 tables classified by key/value type and iteration pattern:

| Table | Key Type | Value Type | Multimap | redb Iteration |
|-------|----------|------------|----------|---------------|
| entries | u64 | blob | No | `table.iter()` -> `(AccessGuard<u64>, AccessGuard<&[u8]>)` |
| topic_index | (&str, u64) | () | No | `table.iter()` -> `(AccessGuard<(&str, u64)>, AccessGuard<()>)` |
| category_index | (&str, u64) | () | No | same as topic_index |
| tag_index | &str | u64 | Yes | `table.iter()` -> `(AccessGuard<&str>, MultimapValue<u64>)` |
| time_index | (u64, u64) | () | No | `table.iter()` -> `(AccessGuard<(u64, u64)>, AccessGuard<()>)` |
| status_index | (u8, u64) | () | No | `table.iter()` -> `(AccessGuard<(u8, u64)>, AccessGuard<()>)` |
| vector_map | u64 | u64 | No | `table.iter()` -> `(AccessGuard<u64>, AccessGuard<u64>)` |
| counters | &str | u64 | No | `table.iter()` -> `(AccessGuard<&str>, AccessGuard<u64>)` |
| agent_registry | &str | blob | No | `table.iter()` -> `(AccessGuard<&str>, AccessGuard<&[u8]>)` |
| audit_log | u64 | blob | No | same as entries |
| feature_entries | &str | u64 | Yes | same as tag_index |
| co_access | (u64, u64) | blob | No | `table.iter()` -> `(AccessGuard<(u64, u64)>, AccessGuard<&[u8]>)` |
| outcome_index | (&str, u64) | () | No | same as topic_index |
| observation_metrics | &str | blob | No | same as agent_registry |
| signal_queue | u64 | blob | No | same as entries |
| sessions | &str | blob | No | same as agent_registry |
| injection_log | u64 | blob | No | same as entries |

### SQLite Import SQL Patterns

Each table type maps to a specific INSERT statement:

| Key Type | Value Type | INSERT SQL |
|----------|------------|-----------|
| u64 + blob | | `INSERT INTO {table} ({pk_col}, data) VALUES (?1, ?2)` with `(key as i64, blob)` |
| u64 + u64 | | `INSERT INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)` with `(key as i64, value as i64)` |
| str + u64 | | `INSERT INTO counters (name, value) VALUES (?1, ?2)` with `(key, value as i64)` |
| str + blob | | `INSERT INTO {table} ({pk_col}, data) VALUES (?1, ?2)` with `(key, blob)` |
| (str, u64) + unit | | `INSERT INTO {table} ({pk_col}, entry_id) VALUES (?1, ?2)` with `(str_key, u64_key as i64)` |
| (u64, u64) + unit | | `INSERT INTO time_index (timestamp, entry_id) VALUES (?1, ?2)` with `(k0 as i64, k1 as i64)` |
| (u8, u64) + unit | | `INSERT INTO status_index (status, entry_id) VALUES (?1, ?2)` with `(k0, k1 as i64)` |
| (u64, u64) + blob | | `INSERT INTO co_access (entry_id_a, entry_id_b, data) VALUES (?1, ?2, ?3)` with `(k0 as i64, k1 as i64, blob)` |
| str + u64 (multimap) | | `INSERT OR IGNORE INTO {table} ({pk_col}, entry_id) VALUES (?1, ?2)` with `(key, value as i64)` |

---

## Functional Requirements

### FR-01: Export Subcommand

The `unimatrix-server export --output <path>` subcommand reads all 17 tables from the project's redb database and writes them to a JSON-lines file at the specified path.

**Preconditions**:
- The binary must be compiled without `backend-sqlite` (redb backend active).
- No running unimatrix-server instance (checked via PID file; abort with error if found).
- The redb database file must exist at the project-detected path (or `--db-path` override).

**Behavior**:
1. Resolve the database path: use `--db-path` if provided, otherwise auto-detect via `project::ensure_data_directory()`.
2. Check for PID file at the project data directory. If a valid PID exists and the process is running, abort with error message: "Cannot export while server is running. Stop the server first."
3. Open the redb database using `redb::Builder::new().create()` (read-only, no migrations).
4. Begin a read transaction.
5. For each of the 17 tables (in `ALL_TABLES` order):
   a. Open the table.
   b. Count rows.
   c. Write table header line to output file.
   d. Iterate all rows, writing one DataRow line per row.
6. Print summary to stderr: table name, row count for each table, and total rows.
7. Exit with code 0 on success, non-zero on failure.

**Output format**: JSON-lines as specified in ADR-001. One table header followed by N data rows, for each of the 17 tables.

### FR-02: Import Subcommand

The `unimatrix-server import --input <path> --output <path>` subcommand reads a JSON-lines intermediate file and creates a new SQLite database.

**Preconditions**:
- The binary must be compiled with `backend-sqlite`.
- The input file must exist and be readable.
- The output path must NOT already exist (refuse to overwrite).

**Behavior**:
1. Validate preconditions.
2. Create the SQLite database at the output path using `Store::open()` (creates tables, sets PRAGMAs).
3. Clear any auto-initialized counter values (Store::open sets counters like `next_entry_id=1`, `schema_version=5`; the import will overwrite them from the dump).
4. Parse the intermediate file line by line.
5. For each table section:
   a. Parse the table header. Record expected row_count and table name.
   b. Begin a SQLite write transaction (for batch performance).
   c. For each data row: decode key and value, execute the appropriate INSERT statement.
   d. Commit the transaction.
   e. Count rows inserted. If actual != expected, abort with error.
6. After all tables imported, run verification:
   a. For each table: `SELECT COUNT(*)` matches expected row_count.
   b. `next_entry_id` counter > MAX(id) from entries table.
   c. `schema_version` counter == 5.
7. Print summary to stderr: table name, row count for each table.
8. Exit with code 0 on success, non-zero on failure.

### FR-03: Feature Flag Default Flip

**Changes**:

1. `crates/unimatrix-store/Cargo.toml`:
   - Add `default = ["backend-sqlite"]` to `[features]`.
   - Keep `redb` as unconditional dependency (needed for export code path when compiled without `backend-sqlite`).

2. `crates/unimatrix-server/Cargo.toml`:
   - Change `default` to `["mcp-briefing", "backend-sqlite"]`.
   - Keep `redb = { workspace = true, optional = true }` (needed when compiled with `redb` feature for export).

3. `crates/unimatrix-engine/Cargo.toml`:
   - Add `[features]` section with `backend-sqlite = []`.

4. `crates/unimatrix-engine/src/project.rs`:
   - Change `db_path` assignment to:
     ```rust
     #[cfg(not(feature = "backend-sqlite"))]
     let db_path = data_dir.join("unimatrix.redb");
     #[cfg(feature = "backend-sqlite")]
     let db_path = data_dir.join("unimatrix.db");
     ```

5. `crates/unimatrix-server/Cargo.toml` feature propagation:
   - Change `backend-sqlite` to `["unimatrix-store/backend-sqlite", "unimatrix-engine/backend-sqlite"]`.

### FR-04: Compilation Matrix

After the feature flag changes, these compilation scenarios must work:

| Scenario | Store Backend | Engine db_path | Export Available | Import Available |
|----------|--------------|----------------|-----------------|-----------------|
| Default (`cargo build`) | SQLite | unimatrix.db | No | Yes |
| `--no-default-features --features mcp-briefing` | redb | unimatrix.redb | Yes | No |
| `--features backend-sqlite` (explicit) | SQLite | unimatrix.db | No | Yes |

---

## Acceptance Criteria

### AC-01: Export reads all 17 tables
The export subcommand successfully iterates all 17 tables from a redb database containing test data. Each table's row count in the output matches the actual row count in the redb database.

**Verification**: Integration test creates a redb database with data in all 17 tables, runs export, verifies the intermediate file contains 17 table headers with correct row counts.

### AC-02: Import creates equivalent SQLite database
The import subcommand reads an intermediate file and creates a SQLite database where each table's row count matches the intermediate file's declared counts.

**Verification**: Integration test runs import on the intermediate file from AC-01, verifies `SELECT COUNT(*)` for all 17 tables matches.

### AC-03: Data fidelity — blob content preserved
Bincode-encoded blobs survive the export/import cycle without modification. EntryRecord, CoAccessRecord, AgentRecord, AuditRecord, SessionRecord, InjectionLogRecord, SignalRecord, and MetricVector all deserialize correctly from the imported SQLite database.

**Verification**: Integration test creates specific records in redb, exports, imports, then reads each record from SQLite and compares field-by-field against the original.

### AC-04: Data fidelity — non-blob values preserved
Counter values (u64), vector mappings (u64 -> u64), and index entries (composite keys to unit values) survive the export/import cycle.

**Verification**: Integration test verifies counters (next_entry_id, schema_version, status totals), vector_map entries, and index entries after round-trip.

### AC-05: Multimap tables preserve all values
TAG_INDEX and FEATURE_ENTRIES multimap tables preserve all (key, value) pairs through export/import. A tag with 5 associated entry IDs retains all 5 after migration.

**Verification**: Integration test creates entries with multiple tags and feature associations, round-trips, verifies all associations survive.

### AC-06: Co-access ordering invariant preserved
CO_ACCESS rows in the imported SQLite database satisfy the CHECK constraint (entry_id_a < entry_id_b). No constraint violations during import.

**Verification**: Integration test creates co-access pairs, round-trips, verifies SQLite CHECK constraint is satisfied by querying all rows.

### AC-07: Counter consistency after import
After import, `next_entry_id` > MAX(id) from entries table. `schema_version` == 5. Status counters match the actual number of entries per status.

**Verification**: Post-import verification step (part of FR-02) plus integration test.

### AC-08: Export aborts if server is running
When a valid PID file exists with a running process, the export subcommand exits with a clear error message and non-zero exit code without touching the database.

**Verification**: Unit test or manual test (mock PID file check).

### AC-09: Import refuses to overwrite existing file
When the `--output` path already exists, the import subcommand exits with error without modifying the existing file.

**Verification**: Unit test.

### AC-10: Default compilation uses SQLite
`cargo build -p unimatrix-server` without explicit features produces a binary that opens SQLite databases (unimatrix.db filename).

**Verification**: Build test + project.rs unit test for db_path suffix.

### AC-11: redb backend remains compilable
`cargo build -p unimatrix-server --no-default-features --features mcp-briefing` produces a binary that opens redb databases and has the export subcommand available.

**Verification**: Build test.

### AC-12: All existing store tests pass with new defaults
`cargo test -p unimatrix-store` passes (with default features = backend-sqlite).

**Verification**: CI test suite.

### AC-13: All existing server tests pass with new defaults
`cargo test -p unimatrix-server` passes (with default features = mcp-briefing,backend-sqlite).

**Verification**: CI test suite.

### AC-14: Project path returns correct db filename
`ensure_data_directory()` returns a path ending in `unimatrix.db` when compiled with `backend-sqlite`, and `unimatrix.redb` when compiled without it.

**Verification**: Unit test in project.rs with cfg-conditional assertions.

### AC-15: Empty tables handled correctly
Export/import handles empty tables without error. Tables with zero rows produce a header line with `row_count: 0` and no data rows.

**Verification**: Integration test with a freshly opened database (all tables empty except counters initialized by Store::open).

---

## Constraints

1. **No Store API write methods in import**: The import inserts data via raw SQL to avoid triggering counter updates and index maintenance. The imported data includes the exact counter values from the source.

2. **Single transaction per table on import**: Each table's data is inserted in a single BEGIN/COMMIT transaction for performance. At ~53 entries, even without batching, import completes in milliseconds. But the pattern should be correct.

3. **base64 encoding**: Use standard base64 (RFC 4648, no URL-safe variant, with padding). The `base64` crate with `STANDARD` engine.

4. **Deterministic table order**: Tables are always processed in the same order (defined by `ALL_TABLES` array) so the intermediate file is deterministic.

5. **Error handling**: Both export and import abort on the first error. No partial exports or partial imports. The import creates the SQLite file first; if import fails, the partial file should be deleted (cleanup on error).

6. **No migration of HNSW data**: The HNSW graph is rebuilt from VECTOR_MAP on startup. Only the VECTOR_MAP bridge table is migrated. The vector index directory (`~/.unimatrix/{hash}/vector/`) can be deleted and will be regenerated.

7. **u64/i64 boundary**: SQLite stores integers as i64. Entry IDs above i64::MAX (2^63 - 1) would lose data. In practice, the production database has ~53 entries with IDs well within i64 range. The import should validate that no u64 value exceeds i64::MAX and abort if found.
