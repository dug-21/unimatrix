# nxs-006: Implementation Brief — SQLite Cutover

## Objective

Migrate the production Unimatrix knowledge base from redb to SQLite and make SQLite the default backend. No code removal — redb remains compilable.

---

## Implementation Waves

### Wave 1: Intermediate Format + Migration Module (Store Crate)

**Goal**: Build the `migrate/` module in unimatrix-store with shared format types, export logic, and import logic.

**Files to create**:
- `crates/unimatrix-store/src/migrate/mod.rs` — Module root, `TableDescriptor` enum, `ALL_TABLES` const array (17 entries)
- `crates/unimatrix-store/src/migrate/format.rs` — `TableHeader`, `DataRow` serde types, JSON-lines read/write helpers
- `crates/unimatrix-store/src/migrate/export.rs` — `#[cfg(not(feature = "backend-sqlite"))]` export function: open redb, iterate tables, write JSON-lines
- `crates/unimatrix-store/src/migrate/import.rs` — `#[cfg(feature = "backend-sqlite")]` import function: read JSON-lines, create SQLite via Store::open(), insert via raw SQL, verify

**Files to modify**:
- `crates/unimatrix-store/Cargo.toml` — Add `base64` and `serde_json` dependencies. Add `default = ["backend-sqlite"]` to features.
- `crates/unimatrix-store/src/lib.rs` — Add `pub mod migrate;`

**Dependencies to add**:
- `base64 = "0.22"` (for blob encoding in intermediate format)
- `serde_json = { workspace = true }` (for JSON-lines format)

**Key implementation details**:
- Export opens redb using `redb::Builder::new().create()` (ADR-003). Does NOT call Store::open() to avoid migrations.
- Import calls `Store::open()` to create correct schema, then uses `store.conn.lock()` for raw SQL inserts (ADR-004).
- All 17 tables enumerated in `ALL_TABLES` const with compile-time completeness.
- Multimap tables (TAG_INDEX, FEATURE_ENTRIES) iterate all values per key, one output line per (key, value) pair.
- u64 values validated against i64::MAX before insertion into SQLite.
- Counter values imported as-is (overwriting Store::open() defaults).
- Import deletes the output file on error (no partial databases left behind).

**Test files to create**:
- `crates/unimatrix-store/src/migrate/format.rs` — inline unit tests for JSON serialization round-trips
- `crates/unimatrix-store/tests/migrate_export.rs` — integration test (cfg-gated for redb): create populated redb, export, verify file format
- `crates/unimatrix-store/tests/migrate_import.rs` — integration test (cfg-gated for sqlite): read exported file, import, verify row counts and data fidelity

### Wave 2: CLI Subcommands (Server Crate)

**Goal**: Wire export/import as `unimatrix-server export` and `unimatrix-server import` subcommands.

**Files to modify**:
- `crates/unimatrix-server/src/main.rs` — Add `Export` and `Import` variants to `Command` enum (cfg-gated). Add match arms that call into `unimatrix_store::migrate::export()` and `unimatrix_store::migrate::import()`.
- `crates/unimatrix-server/Cargo.toml` — Change default features to `["mcp-briefing", "backend-sqlite"]`. Propagate backend-sqlite to engine: `backend-sqlite = ["unimatrix-store/backend-sqlite", "unimatrix-engine/backend-sqlite"]`.

**Key implementation details**:
- Export subcommand: resolves db_path (--db-path flag or auto-detect), checks PID file, calls export.
- Import subcommand: validates --input exists, validates --output does not exist, calls import.
- Both are synchronous (no tokio runtime needed, like Hook subcommand).
- Both print per-table row counts to stderr.

### Wave 3: Feature Flag Default Flip (Engine + Store + Server)

**Goal**: Make SQLite the default backend. Change the database filename for the SQLite backend.

**Files to modify**:
- `crates/unimatrix-engine/Cargo.toml` — Add `[features]` section with `backend-sqlite = []`.
- `crates/unimatrix-engine/src/project.rs` — Cfg-gate db_path: `unimatrix.db` under backend-sqlite, `unimatrix.redb` without.

**Key implementation details**:
- The engine's `backend-sqlite` feature is just a marker — no dependencies, just enables `#[cfg(feature = "backend-sqlite")]` in project.rs.
- The server's `backend-sqlite` feature propagates to both store and engine.
- After this wave, `cargo build` produces a binary that opens `unimatrix.db` (SQLite).
- `cargo build --no-default-features --features mcp-briefing` produces a binary that opens `unimatrix.redb` (redb) and has the export subcommand.

### Wave 4: Production Migration (Manual)

**Goal**: Migrate the live Unimatrix knowledge base.

**Steps**:
1. Stop the running unimatrix-server instance.
2. Build the export binary: `cargo build -p unimatrix-server --no-default-features --features mcp-briefing`
3. Run export: `./target/debug/unimatrix-server export --output /tmp/unimatrix-export.jsonl`
4. Review the intermediate file: check row counts, spot-check entries.
5. Build the import binary: `cargo build -p unimatrix-server` (uses default features = backend-sqlite)
6. Run import: `./target/debug/unimatrix-server import --input /tmp/unimatrix-export.jsonl --output ~/.unimatrix/{hash}/unimatrix.db`
7. Verify: start the server, run context_status, check entry counts match.
8. Back up the old redb file: `mv ~/.unimatrix/{hash}/unimatrix.redb ~/.unimatrix/{hash}/unimatrix.redb.bak`

**Backout procedure**:
1. Stop the server.
2. Restore redb: `mv ~/.unimatrix/{hash}/unimatrix.redb.bak ~/.unimatrix/{hash}/unimatrix.redb`
3. Build with redb: `cargo build -p unimatrix-server --no-default-features --features mcp-briefing,redb`
4. Start the server with the redb binary.

---

## ADR References

| ADR | Entry ID | Summary |
|-----|----------|---------|
| ADR-001 | #333 | JSON-lines intermediate format with base64 blobs |
| ADR-002 | #334 | Cfg-gated database filename (unimatrix.db vs unimatrix.redb) |
| ADR-003 | #335 | Export uses direct redb access, not Store API |
| ADR-004 | #336 | Import uses Store::open() then raw SQL |

---

## Dependency Graph

```
Wave 1 (migrate module) ──> Wave 2 (CLI subcommands) ──> Wave 3 (default flip)
                                                              |
                                                              v
                                                    Wave 4 (production migration)
```

Waves 1-3 are code changes (can be in one commit or multiple).
Wave 4 is a manual operational step.

---

## Estimated Scope

| Wave | New Files | Modified Files | New Lines (est.) | Deleted Lines |
|------|-----------|----------------|------------------|---------------|
| 1 | 4 (migrate module) + 2 (tests) | 2 (Cargo.toml, lib.rs) | ~600-800 | 0 |
| 2 | 0 | 2 (main.rs, server Cargo.toml) | ~80 | 0 |
| 3 | 0 | 3 (engine Cargo.toml, project.rs, server Cargo.toml) | ~15 | ~5 |
| **Total** | **6** | **7** | **~700-900** | **~5** |

---

## Risk Mitigations (Implementation-Level)

1. **Test the round-trip before touching production**: Wave 1 tests must pass before Wave 4 runs.
2. **Back up before migrating**: The old redb file must be preserved as `.redb.bak`.
3. **Verify via context_status**: After import, run the MCP server and call `context_status` to compare entry counts, category distributions, and confidence stats against the pre-migration state.
4. **HNSW rebuild**: After migration, the vector index directory can be deleted. The server will rebuild the HNSW graph from VECTOR_MAP on startup. This is normal and expected.
