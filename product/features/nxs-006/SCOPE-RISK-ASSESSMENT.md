# nxs-006: Scope Risk Assessment

## Scope Summary

nxs-006 is a data migration and default-flip feature with three deliverables:
1. Export/import CLI subcommands for one-way redb-to-SQLite migration
2. Production database migration with row-count verification
3. Default feature flag flip (SQLite becomes default, redb remains compilable)

The scope is narrow and well-bounded. No code removal, no refactoring, no schema changes.

---

## Risk Catalog

### SR-01: Database Filename Mismatch After Default Flip
**Severity: HIGH** | **Likelihood: CERTAIN**

The database path is hardcoded to `unimatrix.redb` in `crates/unimatrix-engine/src/project.rs:94`. After the default flip to SQLite, the system will attempt to open `unimatrix.redb` with `rusqlite::Connection::open()`. SQLite will happily create a new empty database at that path (or try to open the redb binary file and fail with a "not a database" error if the old file still exists).

**Impact**: Either silent data loss (empty DB created) or startup crash (SQLite rejects redb format). Both are production-breaking.

**Mitigation**: The architect must decide:
- (a) Change `project.rs` to return a different `db_path` based on the active feature flag (`unimatrix.db` for SQLite, `unimatrix.redb` for redb).
- (b) Change `project.rs` unconditionally to `unimatrix.db` and have the import subcommand write to that path.
- (c) Make the db_path configurable, with the migration tool handling the rename.

This risk requires an architectural decision (ADR).

### SR-02: Intermediate Format Data Fidelity
**Severity: HIGH** | **Likelihood: LOW**

The export must preserve exact binary content of all 17 tables. The intermediate format (JSON-lines with base64-encoded blobs) involves an encode/decode step that could silently corrupt data if:
- Base64 encoding/decoding has off-by-one errors on binary blob boundaries
- Key types (composite keys like `(&str, u64)` for TOPIC_INDEX) are not serialized and deserialized with exact type fidelity
- The `u64` to `i64` conversion in SQLite (which uses signed integers) loses data for values above `i64::MAX` (9,223,372,036,854,775,807)

**Impact**: Corrupted knowledge base entries, broken correction chains, invalid vector mappings.

**Mitigation**:
- Round-trip test: export a redb test database, import to SQLite, export again, compare intermediate files byte-for-byte.
- Per-record verification: after import, read every entry back and compare bincode blobs against the intermediate file.
- Explicit test for `u64::MAX` and `i64::MAX` boundary values in counters and entry IDs.

### SR-03: Table Schema Divergence Between Backends
**Severity: MEDIUM** | **Likelihood: LOW**

The redb and SQLite backends evolved in parallel (nxs-005). If any table schema differs between the two backends (column order, constraints, default values), the import could create a SQLite database that behaves differently from a fresh SQLite database created by `Store::open()`.

Specific concern: the `observation_metrics` table exists in the redb backend (added post-nxs-005) and in the SQLite DDL, but it is NOT in the compat layer's table constants. If the export includes this table but the import path does not handle it, data is lost.

**Impact**: Subtle behavioral differences or missing data after migration.

**Mitigation**:
- The import subcommand must create the database using `Store::open()` (which runs `create_tables()` and `migrate_if_needed()`), then insert data via direct SQL, not through the Store API.
- Explicit enumeration of all 17 tables in both export and import code with compile-time enforcement (array of table descriptors).

### SR-04: Multimap Table Semantics
**Severity: MEDIUM** | **Likelihood: LOW**

Two tables use redb's `MultimapTableDefinition` (TAG_INDEX, FEATURE_ENTRIES), which means one key maps to multiple values. The export must iterate ALL values per key and preserve them individually. If the export treats these as regular tables (one value per key), data is silently dropped.

**Impact**: Lost tag associations and feature-entry mappings after migration.

**Mitigation**:
- Explicit handling of multimap tables in the export code using redb's `MultimapTable::iter()` which yields `(key, MultimapValue)` pairs.
- Row count verification: multimap tables count each (key, value) pair as one row.
- Parity test: create entries with multiple tags, export/import, verify all tags survive.

### SR-05: Counter State Consistency
**Severity: MEDIUM** | **Likelihood: LOW**

The counters table contains critical state: `next_entry_id`, `next_signal_id`, `next_log_id`, `next_audit_event_id`, `schema_version`, and status counters (`total_active`, `total_deprecated`, etc.). If counter values are imported incorrectly, the next Store write operation could generate duplicate IDs or incorrect status counts.

**Impact**: Duplicate entry IDs (catastrophic), incorrect statistics (cosmetic).

**Mitigation**:
- Export all counter key-value pairs explicitly.
- After import, verify `next_entry_id` is greater than the maximum entry ID in the entries table.
- Verify `schema_version` matches the expected version (5).

### SR-06: Co-Access Key Ordering Invariant
**Severity: LOW** | **Likelihood: LOW**

The CO_ACCESS table enforces `entry_id_a < entry_id_b` via a SQLite CHECK constraint. If the export produces rows where `entry_id_a >= entry_id_b`, the import will fail with a constraint violation. The redb backend enforces this at the application level (`co_access_key()` function), not at the database level.

**Impact**: Import fails on co-access data.

**Mitigation**:
- The export reads keys as-is from redb (which stores them correctly because `co_access_key()` always orders them).
- Add a validation step in the import that checks the ordering invariant before inserting.

### SR-07: Feature Flag Interaction Complexity
**Severity: MEDIUM** | **Likelihood: MEDIUM**

The current feature flag system has unexpected interactions:
- `unimatrix-server` has a `redb` feature (default) that brings in `redb` as an optional dependency AND also does NOT set `backend-sqlite` on the store.
- The server's `backend-sqlite` feature sets `unimatrix-store/backend-sqlite`.
- The two features are mutually exclusive at the store level (cfg gates) but there is no `mutually_exclusive_features` declaration.

After the default flip, the new default will be `["mcp-briefing", "backend-sqlite"]`. But if someone compiles with `--no-default-features --features redb`, they should get the redb backend. This requires the feature flag graph to be correct.

**Impact**: Compilation errors or wrong backend selected after the flip.

**Mitigation**:
- Document the feature flag matrix (default = SQLite, explicit `redb` = redb backend).
- Test compilation with: (a) default features, (b) `--no-default-features --features redb,mcp-briefing`, (c) `--features backend-sqlite` explicitly.
- Verify that the server's `redb` feature correctly does NOT set `backend-sqlite` on the store.

### SR-08: Concurrent Access During Migration
**Severity: LOW** | **Likelihood: MEDIUM**

If the Unimatrix MCP server is running while the export subcommand tries to open the same redb database, the export may fail with `DatabaseAlreadyOpen` (redb's exclusive lock). The migration process requires the server to be stopped.

**Impact**: Export fails, no data loss.

**Mitigation**:
- Document that the server must be stopped before running `unimatrix-server export`.
- The export subcommand should check for a running server (PID file check) and abort with a clear error message.

---

## Top 3 Risks for Architect Attention

1. **SR-01 (Database Filename Mismatch)**: CERTAIN to cause issues. Requires an ADR to decide how `project.rs` handles the filename transition from `unimatrix.redb` to `unimatrix.db`.

2. **SR-02 (Intermediate Format Data Fidelity)**: Data corruption in production is unrecoverable. The intermediate format design and round-trip testing strategy need architectural review.

3. **SR-07 (Feature Flag Interaction)**: The feature flag graph is non-trivial and the default flip changes behavior for all downstream consumers. The architect should specify the exact flag configuration for each compilation scenario.
