# nxs-006: SQLite Cutover

## Problem Statement

Unimatrix currently ships a dual-backend storage engine (redb and SQLite) gated by Cargo feature flags. nxs-005 delivered the SQLite backend with full parity testing, but the system still defaults to redb. There is one production database (the live Unimatrix knowledge base) that must be migrated from redb format to SQLite before SQLite can become the default.

Until the production data is migrated, SQLite cannot become the default backend and nxs-007 (redb removal, schema normalization, server decoupling) cannot proceed.

## Goals

1. Build data migration tooling: export subcommand (redb-compiled binary reads all 17 tables, writes intermediate format) and import subcommand (sqlite-compiled binary reads intermediate format, writes SQLite database)
2. Migrate the one production database from redb to SQLite with verified row counts per table
3. Switch the default backend from redb to SQLite (flip the feature flag default)
4. Verify all MCP tools return identical results against the migrated SQLite database

## Non-Goals

- **No redb code removal** -- deleting redb implementation files, removing cfg gates, flattening sqlite/ module, removing redb from Cargo.toml -- all moves to nxs-007. The redb code must remain compilable so the export subcommand works and as a backout path.
- **No schema normalization** -- decomposing bincode blobs into SQL columns, eliminating index tables, or adding SQL JOINs is nxs-007.
- **No server decoupling** -- refactoring the server to stop bypassing the Store API is nxs-007.
- **No compat layer changes** -- the transitional compat layer stays as-is. Cleanup is nxs-007.
- **No new storage features** -- no new tables, columns, or query patterns.
- **No changes to HNSW vector index** -- VECTOR_MAP bridge table moves with the rest; in-memory HNSW is unchanged.
- **No changes to MCP tool behavior** -- all 10 tools must return identical results.

## Background Research

### Current Dual-Backend Architecture (nxs-005 Output)

The store crate uses compile-time feature flags to select between backends:
- **Default (redb)**: Top-level modules db.rs (532), read.rs (924), write.rs (1,939), migration.rs (1,421), query.rs (318), counter.rs (56) = 5,190 lines. Plus redb-gated sections in shared modules (error.rs, lib.rs, schema.rs, sessions.rs, injection_log.rs, signal.rs) ~2,400 lines.
- **SQLite (`backend-sqlite`)**: sqlite/ submodule with 13 files totaling 2,987 lines. Includes 742 lines of transitional compat layer (compat.rs 184, compat_handles.rs 426, compat_txn.rs 132).
- **Shared**: schema.rs (676, types and redb table definitions), error.rs (210, dual-gated error variants), hash.rs (78), lib.rs (87, dual-gated re-exports).

### Data Migration Requirements

One production database must be migrated. Tables (17 total):
- entries, topic_index, category_index, tag_index, time_index, status_index
- vector_map, counters, agent_registry, audit_log
- feature_entries, co_access, outcome_index, observation_metrics
- signal_queue, sessions, injection_log

The intermediate format must preserve exact binary content (bincode blobs) for all record types. Row counts per table serve as the primary verification mechanism. The export runs on a redb-compiled binary; the import runs on the sqlite-compiled binary. This avoids needing both backends compiled into one binary.

### Server Feature Flags

unimatrix-server Cargo.toml has:
- `default = ["mcp-briefing", "redb"]`
- `backend-sqlite = ["unimatrix-store/backend-sqlite"]`
- `redb = { workspace = true, optional = true }` -- the server directly depends on redb for the `DatabaseAlreadyOpen` error variant in main.rs

After cutover: flip default to `["mcp-briefing", "backend-sqlite"]`, keep redb available for export subcommand.

## Proposed Approach

### Wave 1: Data Migration Tooling

Build `unimatrix-server export` and `unimatrix-server import` subcommands. The export subcommand reads all 17 tables from the redb database and writes them to a JSON-lines intermediate file (one section per table, with table name, row count, and base64-encoded bincode blobs for each row). The import subcommand reads the intermediate file and writes to SQLite. Both report per-table row counts for verification.

The export subcommand only compiles when the redb feature is active. The import subcommand only compiles when backend-sqlite is active. This keeps the two backends separate -- no need for both in one binary.

### Wave 2: Production Migration

Run export on the current production database (redb-compiled binary). Run import to create the SQLite database (sqlite-compiled binary). Verify row counts match. Verify MCP tool behavior against the new database. Back up the original redb file.

### Wave 3: Default Flip

Change the default feature flags so SQLite is the default backend:
- unimatrix-store: default features include `backend-sqlite`
- unimatrix-server: default features become `["mcp-briefing", "backend-sqlite"]`
- Verify `cargo build` and `cargo test` pass with the new defaults
- Verify all integration tests pass against SQLite

The redb backend remains compilable (for the export subcommand and as a backout path) but is no longer the default.

## Acceptance Criteria

- AC-01: Export subcommand reads all 17 tables from a redb database and writes an intermediate file with per-table row counts
- AC-02: Import subcommand reads the intermediate file and creates a SQLite database with identical per-table row counts
- AC-03: Production database migrated successfully (row counts verified, MCP tools return identical results)
- AC-04: SQLite is the default backend (`cargo build` without feature flags uses SQLite)
- AC-05: redb backend remains compilable via explicit feature flag (backout path preserved)
- AC-06: All existing store unit tests pass with the new default (`cargo test -p unimatrix-store`)
- AC-07: All existing server tests pass with the new default
- AC-08: All integration tests pass against the SQLite backend

## Constraints

- **One production database**: There is exactly one redb database to migrate. The migration tooling is one-time but must remain compilable until nxs-007 removes redb entirely.
- **No simultaneous compilation**: The export and import subcommands compile under different feature flags. There is no need to compile both backends into one binary.
- **Backward compatibility**: After default flip, the system uses SQLite by default. redb databases can still be opened by compiling with the redb feature (for export or backout).
- **Test infrastructure is cumulative**: Extend existing test helpers; do not create new isolated scaffolding.

## Open Questions

1. **Intermediate format**: JSON-lines with base64-encoded blobs is proposed. Alternative: bincode-serialized dump file. JSON-lines is human-inspectable and debuggable; bincode is more compact.
2. **Export/import as subcommands vs standalone binary**: Proposed as subcommands on unimatrix-server for simplicity. Alternative: separate `unimatrix-migrate` binary.

## Revised nxs-007 Scope (expanded)

nxs-007 now absorbs the cleanup work originally in nxs-006:
- Remove redb backend implementation (~7,590 lines)
- Remove transitional compat layer (~742 lines)
- Remove all cfg gates from store and server crates
- Flatten sqlite/ module to crate root
- Remove redb from workspace dependencies
- Make rusqlite unconditional
- **Plus** the original nxs-007 scope: server decoupling + schema normalization

## Tracking

GitHub Issue will be created during Session 1 synthesis phase.
