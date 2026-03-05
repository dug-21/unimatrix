# nxs-006 Pseudocode Overview

## Component Interaction

```
migrate-module (unimatrix-store)
  format.rs  -- shared types (TableHeader, DataRow, KeyType, ValueType)
  mod.rs     -- TableDescriptor enum, ALL_TABLES const, public API
  export.rs  -- #[cfg(not(backend-sqlite))]: redb -> JSON-lines
  import.rs  -- #[cfg(backend-sqlite)]: JSON-lines -> SQLite

cli-subcommands (unimatrix-server)
  main.rs    -- Export + Import variants on Command enum, cfg-gated

feature-flag-flip (store + engine + server Cargo.toml, engine project.rs)
  Cargo.toml edits + project.rs cfg-gate
```

## Data Flow

```
[redb database]
    |
    v  (export: redb read txn -> iterate ALL_TABLES)
[JSON-lines file]
    |
    v  (import: Store::open() then raw SQL INSERT per table)
[SQLite database]
```

## Shared Types

All components share types defined in `migrate/format.rs`:
- `TableHeader` -- JSON object marking start of a table section
- `DataRow` -- JSON object for one row (key + value)
- `KeyType` / `ValueType` -- enums classifying table schemas
- `write_header()` / `write_row()` / `read_line()` -- JSON-lines I/O helpers

The `TableDescriptor` enum in `migrate/mod.rs` classifies all 17 tables by their key/value types and provides the table name. The `ALL_TABLES` const array is the single source of truth for table enumeration.

## Integration Harness Plan

Existing test infrastructure at `product/test/infra-001/` provides integration test suites. For nxs-006:

1. **Smoke tests**: The existing smoke suite should continue passing -- nxs-006 does not change any MCP tool behavior.
2. **New integration tests**: Two cfg-gated integration test files in `crates/unimatrix-store/tests/`:
   - `migrate_export.rs` -- compiled under redb (no backend-sqlite): creates populated redb, exports, verifies file format
   - `migrate_import.rs` -- compiled under backend-sqlite: reads exported file, imports into SQLite, verifies all data
3. **Compilation matrix**: Verified by building with different feature flag combinations (no runtime test needed, just `cargo build` / `cargo check`).
4. **Round-trip testing**: Since both backends cannot be compiled simultaneously, the round-trip test uses a shared fixture file approach -- export test writes a known intermediate file, import test reads it.

## Patterns Used

- **cfg-gating pattern**: Already established in `lib.rs` for backend selection. Export/import follow the same pattern.
- **Store::open() for schema creation**: Established in sqlite/db.rs. Import reuses this.
- **pub(crate) conn access**: Established in sqlite/db.rs. Import uses `store.conn.lock()` for raw SQL.
- **Clap subcommand pattern**: Established with `Hook` variant in main.rs. Export/Import follow same pattern.
- **redb table definition constants**: All 17 defined in schema.rs. Export uses these directly.

## Open Questions

None -- all design decisions resolved via ADRs #333-336.
