# nxs-007: redb Removal

## Problem Statement

After nxs-006 completes the SQLite cutover (production database migrated, default backend flipped), the redb backend code remains in the codebase as dead weight. Approximately 8,300 lines of redb-specific code, 742 lines of transitional compatibility layer, ~90 cfg gate annotations, and the redb workspace dependency all remain. This dual-backend machinery adds compile-time cost, cognitive overhead for developers, and maintenance burden for code that will never run again. The export subcommand (redb-only migration tooling from nxs-006) also becomes obsolete once migration is confirmed successful.

nxs-007 is a mechanical cleanup: delete the dead code, remove the feature gates, and flatten the module structure so the codebase reflects the single-backend reality.

## Goals

1. Remove all redb backend implementation code from unimatrix-store (~5,190 lines across db.rs, read.rs, write.rs, migration.rs, query.rs, counter.rs)
2. Remove all redb-gated sections from shared modules in unimatrix-store (error.rs, lib.rs, schema.rs, sessions.rs, injection_log.rs — cfg-gated blocks totaling ~2,400 lines)
3. Remove the transitional compat layer from sqlite/ module (~742 lines: compat.rs, compat_handles.rs, compat_txn.rs)
4. Remove all `#[cfg(feature = "backend-sqlite")]` and `#[cfg(not(feature = "backend-sqlite"))]` annotations from store, engine, and server crates — make SQLite unconditional
5. Flatten sqlite/ submodule to crate root — move SQLite implementation files up from `src/sqlite/` to `src/`, eliminating the submodule indirection
6. Remove the `backend-sqlite` feature flag from all Cargo.toml files (store, engine, server)
7. Remove `redb` from workspace dependencies (Cargo.toml at root, store, server)
8. Remove the `export` subcommand from unimatrix-server (redb-only migration code, no longer needed)
9. Remove the `migrate/export.rs` module from unimatrix-store (redb-only export logic)
10. Make `rusqlite` an unconditional (non-optional) dependency in unimatrix-store

## Non-Goals

- **No schema normalization** — decomposing bincode blobs into SQL columns, eliminating index tables, adding SQL JOINs is nxs-008
- **No server decoupling** — refactoring the server to stop bypassing the Store API is nxs-008
- **No new features** — this is purely subtractive; no new tables, columns, queries, or tool behavior
- **No behavioral changes** — all 10 MCP tools must produce identical results before and after removal
- **No import subcommand removal** — the import subcommand (SQLite-side) may be retained or removed; it is harmless but no longer needed. Decision deferred to implementation.
- **No HNSW/vector changes** — the in-memory HNSW index and VECTOR_MAP bridge table are untouched
- **No test infrastructure changes beyond cfg gate removal** — existing SQLite tests become the only tests; redb-only test paths are deleted

## Background Research

### Files to Delete (redb-only, entire files removed)

| File | Lines | Purpose |
|------|-------|---------|
| `crates/unimatrix-store/src/db.rs` | 532 | redb Store::open, database handle |
| `crates/unimatrix-store/src/read.rs` | 924 | redb read transaction impl |
| `crates/unimatrix-store/src/write.rs` | 1,939 | redb write transaction impl |
| `crates/unimatrix-store/src/migration.rs` | 1,421 | redb schema migration |
| `crates/unimatrix-store/src/query.rs` | 318 | redb query helpers |
| `crates/unimatrix-store/src/counter.rs` | 56 | redb counter operations |
| `crates/unimatrix-store/src/migrate/export.rs` | 293 | redb export (nxs-006 tooling) |
| **Subtotal** | **5,483** | |

### Files to Delete (compat layer, obsolete after removal)

| File | Lines | Purpose |
|------|-------|---------|
| `crates/unimatrix-store/src/sqlite/compat.rs` | 184 | Compatibility trait adapters |
| `crates/unimatrix-store/src/sqlite/compat_handles.rs` | 426 | Compat handle wrappers |
| `crates/unimatrix-store/src/sqlite/compat_txn.rs` | 132 | Compat transaction wrappers |
| **Subtotal** | **742** | |

### Files Requiring cfg Gate Removal (shared modules in store)

| File | Lines | cfg Annotations | Action |
|------|-------|-----------------|--------|
| `crates/unimatrix-store/src/error.rs` | 210 | 31 | Remove redb error variants, keep SQLite variants, drop all cfg gates |
| `crates/unimatrix-store/src/lib.rs` | 88 | 15 | Remove redb module declarations/re-exports, keep SQLite, drop cfg gates |
| `crates/unimatrix-store/src/schema.rs` | 676 | 19 | Remove redb table definitions (keep shared types), drop cfg gates |
| `crates/unimatrix-store/src/sessions.rs` | 682 | 7 | Remove redb session impl, keep SQLite, drop cfg gates |
| `crates/unimatrix-store/src/injection_log.rs` | 259 | 6 | Remove redb injection_log impl, keep SQLite, drop cfg gates |
| `crates/unimatrix-store/src/test_helpers.rs` | ~30 | 2 | Remove redb test helper path, keep SQLite path |
| `crates/unimatrix-store/src/migrate/mod.rs` | 215 | 8 | Remove redb migration functions, keep import, drop cfg gates |

### Files Requiring cfg Gate Removal (server crate)

| File | cfg Annotations | Action |
|------|-----------------|--------|
| `crates/unimatrix-server/src/main.rs` | 5 | Remove export subcommand, redb error handling, cfg gates |
| `crates/unimatrix-server/src/server.rs` | 4 | Remove redb transaction helper, cfg gates |
| `crates/unimatrix-server/src/services/store_correct.rs` | 3 | Remove redb correction impl, keep SQLite, drop cfg gates |
| `crates/unimatrix-server/src/services/status.rs` | 2 | Remove redb import, drop cfg gates |
| `crates/unimatrix-server/src/services/usage.rs` | 2 | Remove redb import, drop cfg gates |
| `crates/unimatrix-server/src/infra/registry.rs` | 2 | Remove redb import, drop cfg gates |
| `crates/unimatrix-server/src/infra/audit.rs` | 4 | Remove redb audit impl, keep SQLite, drop cfg gates |

### Files Requiring cfg Gate Removal (engine crate)

| File | cfg Annotations | Action |
|------|-----------------|--------|
| `crates/unimatrix-engine/src/project.rs` | 4 | Remove redb path logic, keep SQLite path, drop cfg gates |

### SQLite Module Flattening

The `crates/unimatrix-store/src/sqlite/` submodule contains 10 non-compat files (2,245 lines) that become the primary (and only) implementation:

| File | Lines | Destination after flattening |
|------|-------|------------------------------|
| `sqlite/db.rs` | 184 | `src/db.rs` (replaces deleted redb version) |
| `sqlite/read.rs` | 442 | `src/read.rs` (replaces deleted redb version) |
| `sqlite/write.rs` | 424 | `src/write.rs` (replaces deleted redb version) |
| `sqlite/write_ext.rs` | 381 | `src/write_ext.rs` (new top-level) |
| `sqlite/migration.rs` | 133 | `src/migration.rs` (replaces deleted redb version) |
| `sqlite/sessions.rs` | 302 | Merges into `src/sessions.rs` (or replaces it) |
| `sqlite/injection_log.rs` | 106 | Merges into `src/injection_log.rs` (or replaces it) |
| `sqlite/signal.rs` | 153 | Merges into `src/signal.rs` (or replaces it) |
| `sqlite/txn.rs` | 89 | `src/txn.rs` (new top-level) |
| `sqlite/mod.rs` | 31 | Deleted (re-exports absorbed into `src/lib.rs`) |

### Cargo.toml Changes

| File | Changes |
|------|---------|
| `Cargo.toml` (workspace root) | Remove `redb = "3.1"` from `[workspace.dependencies]` |
| `crates/unimatrix-store/Cargo.toml` | Remove `redb` dep, make `rusqlite` non-optional, remove `backend-sqlite` feature, remove `default` feature list |
| `crates/unimatrix-server/Cargo.toml` | Remove `redb` optional dep, remove `backend-sqlite` feature, update default features |
| `crates/unimatrix-engine/Cargo.toml` | Remove `backend-sqlite` feature entirely |

### Test File References

Several test files in the server crate reference `test.redb` as a database filename in test helpers. These are cosmetic — the filename string is used for temp file paths and the actual backend is SQLite. These should be renamed to `test.db` for clarity but the tests will function regardless.

### Estimated Removal Summary

| Category | Lines Removed |
|----------|---------------|
| redb implementation files (store) | ~5,190 |
| redb export module (migrate) | ~293 |
| Compat layer files | ~742 |
| cfg-gated blocks in shared modules (store) | ~1,200 (estimated) |
| cfg-gated blocks in server crate | ~400 (estimated) |
| cfg-gated blocks in engine crate | ~20 (estimated) |
| **Total estimated removal** | **~7,845** |

Net effect: ~2,245 lines of SQLite code move from `sqlite/` subdirectory to crate root. The codebase shrinks by approximately 5,600 net lines.

## Proposed Approach

### Wave 1: Delete redb Implementation Files

Delete the 7 redb-only files from unimatrix-store (db.rs, read.rs, write.rs, migration.rs, query.rs, counter.rs, migrate/export.rs). These files are entirely redb code with no SQLite content.

### Wave 2: Delete Compat Layer

Delete the 3 compat files (compat.rs, compat_handles.rs, compat_txn.rs) from sqlite/. Remove all compat usage from remaining SQLite code — the compat layer exists only to bridge redb's API shape; with redb gone, SQLite code uses its native types directly.

### Wave 3: Flatten SQLite Module

Move the 10 remaining sqlite/ files up to src/ root. Update mod.rs/lib.rs to reference them directly instead of through the sqlite submodule. Delete sqlite/mod.rs. Resolve any naming conflicts (e.g., sqlite/db.rs replacing the now-deleted redb db.rs).

### Wave 4: Remove cfg Gates

Systematically remove all `#[cfg(feature = "backend-sqlite")]` and `#[cfg(not(feature = "backend-sqlite"))]` annotations from store, server, and engine crates. For each annotation:
- `#[cfg(feature = "backend-sqlite")]` blocks: keep the code, remove the gate
- `#[cfg(not(feature = "backend-sqlite"))]` blocks: delete the code entirely
- Clean up any `#[cfg_attr(...)]` annotations

### Wave 5: Cargo.toml Cleanup

- Remove `redb` from workspace dependencies
- Make `rusqlite` unconditional in unimatrix-store
- Remove `backend-sqlite` feature from store, server, and engine Cargo.toml files
- Remove `redb` optional dependency from server Cargo.toml

### Wave 6: Server Cleanup

- Remove the `export` subcommand from main.rs CLI definition and handler
- Remove the `DatabaseAlreadyOpen` redb error handling from main.rs
- Rename `test.redb` references to `test.db` in test files for clarity

### Wave 7: Verification

- `cargo build` succeeds without any feature flags
- `cargo test --workspace` passes
- All 10 MCP tools produce identical results against the production SQLite database
- No references to `redb`, `backend-sqlite`, or `cfg(feature` remain in store, server, or engine crates (except comments noting the removal)

## Acceptance Criteria

- AC-01: All redb implementation files deleted from unimatrix-store (db.rs, read.rs, write.rs, migration.rs, query.rs, counter.rs — 5,190 lines)
- AC-02: redb export module deleted (migrate/export.rs — 293 lines), import module deleted (migrate/import.rs — 412 lines), format module deleted (migrate/format.rs — 330 lines)
- AC-03: Transitional compat layer deleted (compat.rs, compat_handles.rs, compat_txn.rs — 742 lines)
- AC-04: All cfg gates for `backend-sqlite` removed from store, server, and engine crates
- AC-05: sqlite/ submodule flattened — all SQLite implementation files moved to crate root, sqlite/ directory deleted
- AC-06: `backend-sqlite` feature removed from all Cargo.toml files
- AC-07: `redb` removed from workspace dependencies and all crate Cargo.toml files
- AC-08: `rusqlite` is an unconditional (non-optional) dependency in unimatrix-store
- AC-09: Export subcommand removed from unimatrix-server
- AC-10: `cargo build` succeeds without feature flags
- AC-11: `cargo test --workspace` passes — all existing SQLite tests pass
- AC-12: No functional cfg gates referencing `backend-sqlite` or `redb` remain in store, server, or engine crates
- AC-13: All 10 MCP tools produce identical results (behavioral parity verified)
- AC-14: All `test.redb` filename references renamed to `test.db` in test files
- AC-15: Import subcommand removed from unimatrix-server (along with import.rs and format.rs from migrate/)

## Constraints

- **Prerequisite: nxs-006 must be complete** — the production database must be on SQLite and the default backend must already be SQLite before redb code can be removed. This is a hard dependency.
- **No behavioral changes** — this is a subtractive-only change. Any test that passes before nxs-007 must pass after, and all MCP tools must return identical results.
- **Compat layer removal requires careful analysis** — the compat types may be referenced in SQLite code that was written to bridge the two backends. These references must be updated to use native rusqlite types.
- **Module flattening may cause merge conflicts** — since nxs-006 may still be in-flight, the module flattening should be coordinated to avoid conflicts with any late nxs-006 changes.
- **Test infrastructure is cumulative** — extend existing test helpers; do not create isolated scaffolding.
- **migrate/ entire directory removed** — import.rs, export.rs, and format.rs are all dead code post-migration. The entire migrate/ module is deleted.

## Open Questions (Resolved)

1. **Import subcommand fate** — RESOLVED: Remove import.rs and format.rs. Dead code is dead code.
2. **test.redb filename references** — RESOLVED: Rename to test.db. Quick search-and-replace, avoids confusion.
3. **Compat layer usage depth** — RESOLVED: Needs architect investigation during Phase 2a. The architect will map compat type usage and determine refactoring scope.

## Tracking

GitHub Issue: https://github.com/dug-21/unimatrix/issues/99
