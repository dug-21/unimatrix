# nxs-007: Architecture -- redb Removal

## Overview

nxs-007 removes all redb backend code from the Unimatrix workspace, completing the transition to SQLite-only storage begun by nxs-005 (dual backend) and nxs-006 (cutover). The architecture is subtractive: no new abstractions, no new modules, no behavioral changes. The key architectural decisions concern the compat layer fate, the module flattening strategy, and the merge order for shared modules.

---

## ADR-001: Retain Compat Layer Types, Relocate to Store Crate Root

**Status**: Proposed
**Context**: The compat layer (compat.rs, compat_handles.rs, compat_txn.rs) provides typed table handles, guard wrappers, and `open_table`/`open_multimap_table` dispatch that the server uses in 90+ call sites. Deleting these types would require rewriting the entire server database layer -- which is nxs-008's scope (server decoupling + schema normalization).

**Decision**: The compat layer types are retained but relocated. The three compat files move from `src/sqlite/` to the store crate root alongside the flattened SQLite implementation. The "compat" naming is updated:
- `compat.rs` -> `tables.rs` (table definitions, constants, guard types, counter helpers)
- `compat_handles.rs` -> `handles.rs` (typed table handle implementations)
- `compat_txn.rs` -> `dispatch.rs` (TableSpec/MultimapSpec traits, open_table dispatch)
- `txn.rs` -> `txn.rs` (SqliteReadTransaction, SqliteWriteTransaction -- retained as-is)

The public API surface (type names, method signatures) remains identical. Only the module path changes: `sqlite::compat::BlobGuard` becomes `crate::tables::BlobGuard`, but since lib.rs re-exports all of these at the crate root, downstream code (the server) does not change.

**Rationale**: Deleting ~742 lines of compat code would require changing ~90 server call sites, which is exactly what nxs-008 will do. Doing it in nxs-007 conflates mechanical cleanup with server refactoring. The compat types are legitimate SQLite code -- they just have misleading names.

**Consequences**:
- Server code compiles unchanged (zero modifications to server crate for compat removal)
- The "transitional" / "TEMPORARY" comments in compat files must be updated to reflect that these types persist until nxs-008
- nxs-008 becomes the feature that removes these types by migrating server to the Store API

## ADR-002: Flatten-and-Merge Strategy for Shared Modules

**Status**: Proposed
**Context**: Three modules exist as both root-level files (containing shared types + redb impl) and sqlite/ files (containing SQLite impl). These are: sessions, injection_log, signal.

**Decision**: For each shared module, the post-nxs-007 file structure is:

1. **sessions.rs**: Keep shared types (SessionRecord, SessionLifecycleStatus, GcStats, constants) from current root sessions.rs. Replace the redb `impl Store` block with the SQLite `impl Store` block from sqlite/sessions.rs. The serialization helpers are duplicated between the two files (identical logic) -- keep one copy. Delete the redb-only test module (`#[cfg(not(feature = "backend-sqlite"))] mod tests`).

2. **injection_log.rs**: Keep shared types (InjectionLogRecord) and shared serialization helpers (serialize_injection_log, deserialize_injection_log) from current root injection_log.rs. Replace the redb `impl Store` block with the SQLite `impl Store` block from sqlite/injection_log.rs. Delete the redb-only test module.

3. **signal.rs**: The root signal.rs contains ONLY shared types and serialization helpers -- no redb `impl Store` block (signal operations were added after the SQLite backend existed, so the redb impl lives elsewhere or was never created). The sqlite/signal.rs contains the `impl Store` block for signal operations. Merge: keep root signal.rs types, append the `impl Store` block from sqlite/signal.rs.

**Merge procedure for each file**:
1. Start with the current root file
2. Remove all `#[cfg(not(feature = "backend-sqlite"))]` blocks (redb imports, redb Store methods, redb serialization helpers, redb-only tests)
3. Remove all `#[cfg(feature = "backend-sqlite")]` gates (keep the code, drop the gate)
4. Remove all `#[cfg_attr(feature = "backend-sqlite", ...)]` annotations
5. Append the SQLite `impl Store` block from the sqlite/ version
6. Deduplicate serialization helpers (keep one copy, remove the other)
7. Update `use` paths: `super::db::Store` becomes `crate::db::Store` (since both are now at root)

**Rationale**: This approach preserves type definitions in their original location (where they are documented and tested) while absorbing the SQLite implementation. It avoids creating new files or changing the public API.

**Consequences**:
- Three merged files, each containing types + SQLite impl + tests
- The redb-only tests (sessions.rs ~340 lines, injection_log.rs ~130 lines) are deleted since they test methods on the redb Store
- The SQLite-equivalent tests already exist in the server/integration test suite

## ADR-003: Wave Execution Order

**Status**: Proposed
**Context**: The removal involves interdependent file deletions, module restructuring, and Cargo.toml changes. Incorrect ordering causes intermediate compilation failures.

**Decision**: Seven waves, each producing a compilable state:

**Wave 1: Delete redb-only implementation files**
Delete: db.rs, read.rs, write.rs, migration.rs, query.rs, counter.rs (root-level redb files)
Update lib.rs: remove `mod db`, `mod counter`, `mod migration`, `mod write`, `mod read`, `mod query` declarations (the `#[cfg(not(feature = "backend-sqlite"))]` gated ones)
Also update lib.rs: remove the redb re-exports block (lines 46-57: table constants, counter helpers, `pub use db::Store`)
State: compiles with `backend-sqlite` feature only (which is the default)

**Wave 2: Delete migrate/ directory**
Delete entire `src/migrate/` directory (export.rs, import.rs, format.rs, mod.rs)
Update lib.rs: remove `pub mod migrate;`
Update server main.rs: remove Export and Import subcommand variants, their handler functions (run_export, run_import), and the `use unimatrix_store::migrate` import
State: compiles, export/import subcommands removed

**Wave 3: Flatten sqlite/ module to crate root**
Move sqlite/db.rs -> src/db.rs
Move sqlite/read.rs -> src/read.rs
Move sqlite/write.rs -> src/write.rs
Move sqlite/write_ext.rs -> src/write_ext.rs
Move sqlite/migration.rs -> src/migration.rs
Rename compat files:
  sqlite/compat.rs -> src/tables.rs
  sqlite/compat_handles.rs -> src/handles.rs
  sqlite/compat_txn.rs -> src/dispatch.rs
Move sqlite/txn.rs -> src/txn.rs
Delete sqlite/mod.rs
Update lib.rs: replace `#[cfg(feature = "backend-sqlite")] mod sqlite;` with direct module declarations: `mod db; mod txn; mod read; mod write; mod write_ext; mod migration; mod tables; mod handles; mod dispatch;`
Update lib.rs re-exports: replace `pub use sqlite::Store` with `pub use db::Store`, replace `pub use sqlite::{SqliteReadTransaction, SqliteWriteTransaction}` with `pub use txn::*`, and update the compat re-exports to reference `tables::*`, `handles::*`, `dispatch::*`
Update `use super::` paths in moved files to `use crate::` since they are no longer in a submodule

**Wave 4: Merge shared modules**
Merge sessions.rs (root types + sqlite/sessions.rs impl) per ADR-002
Merge injection_log.rs (root types + sqlite/injection_log.rs impl) per ADR-002
Merge signal.rs (root types + sqlite/signal.rs impl) per ADR-002
Delete the now-redundant sqlite/ files (already moved in Wave 3, so this is confirming the directory is empty)
Delete sqlite/ directory
State: compiles, no more sqlite/ submodule

**Wave 5: Remove cfg gates from store crate**
Remove all `#[cfg(feature = "backend-sqlite")]` and `#[cfg(not(feature = "backend-sqlite"))]` from:
- error.rs: delete redb error variants, keep Sqlite variant, remove gates
- schema.rs: delete redb table definitions (lines 1-91), keep shared types, remove `use redb` import
- test_helpers.rs: remove cfg gate on SQLite path, delete redb path
- lib.rs: remove any remaining cfg gates
State: store crate compiles unconditionally without feature flags

**Wave 6: Remove cfg gates from engine + server crates, remove redb dependency**
Engine: project.rs -- remove cfg gates, keep SQLite db path, delete redb path, update test assertion
Server: main.rs -- remove the `DatabaseAlreadyOpen` retry logic (cfg-gated redb block), keep the SQLite open path
Server: remove `redb = { workspace = true, optional = true }` from Cargo.toml
Server: remove `backend-sqlite` feature and update default features to just `["mcp-briefing"]`
Engine: remove `backend-sqlite` feature from Cargo.toml
Store: make `rusqlite` unconditional (remove `optional = true`), remove `backend-sqlite` feature, remove `default` feature list, remove `redb = { workspace = true }` dependency
Workspace root: remove `redb = "3.1"` from `[workspace.dependencies]`
State: entire workspace compiles, no redb dependency anywhere

**Wave 7: Cosmetic cleanup**
Rename all `test.redb` to `test.db` across the workspace (40+ locations in 6 crates)
Remove or update comments referencing redb backend (e.g., "Backend-specific modules: redb (default) or SQLite (feature-gated)")
Remove "TEMPORARY" comments from retained compat types (now tables.rs, handles.rs, dispatch.rs); replace with note about nxs-008
Update crate-level documentation if present

**Rationale**: Each wave boundary is a compilation checkpoint. Waves 1-2 are pure deletion (lowest risk). Wave 3 is the structural change (highest risk -- path rewrites). Wave 4 is merge work. Waves 5-6 are cfg gate removal. Wave 7 is cosmetic. This ordering means if any wave fails, the previous wave's state is still valid.

---

## Component Architecture (Post nxs-007)

### unimatrix-store after flattening

```
crates/unimatrix-store/src/
  lib.rs           -- module root, re-exports
  db.rs            -- Store struct, open/lock_conn (from sqlite/db.rs)
  txn.rs           -- SqliteReadTransaction, SqliteWriteTransaction (from sqlite/txn.rs)
  read.rs          -- Store read methods (from sqlite/read.rs)
  write.rs         -- Store write methods (from sqlite/write.rs)
  write_ext.rs     -- Store extended write methods (from sqlite/write_ext.rs)
  migration.rs     -- SQLite schema migration (from sqlite/migration.rs)
  tables.rs        -- Table constants, guard types, counter helpers (from sqlite/compat.rs)
  handles.rs       -- Typed table handle impls (from sqlite/compat_handles.rs)
  dispatch.rs      -- TableSpec/MultimapSpec traits (from sqlite/compat_txn.rs)
  schema.rs        -- Shared types: EntryRecord, Status, NewEntry, etc. (cleaned, no redb defs)
  error.rs         -- StoreError (Sqlite variant only)
  hash.rs          -- Content hash computation (unchanged)
  sessions.rs      -- SessionRecord types + SQLite impl Store methods (merged)
  injection_log.rs -- InjectionLogRecord types + SQLite impl Store methods (merged)
  signal.rs        -- SignalRecord types + SQLite impl Store methods (merged)
  test_helpers.rs  -- Test helper (SQLite-only path)
```

### unimatrix-store Cargo.toml (post nxs-007)

```toml
[package]
name = "unimatrix-store"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[features]
test-support = ["dep:tempfile"]

[dependencies]
rusqlite = { version = "0.34", features = ["bundled"] }
serde = { workspace = true }
serde_json = { workspace = true }
bincode = { workspace = true }
base64 = "0.22"
sha2 = "0.10"
tempfile = { version = "3", optional = true }

[dev-dependencies]
tempfile = "3"
```

### unimatrix-server Cargo.toml changes

- Remove `redb = { workspace = true, optional = true }`
- Remove `backend-sqlite` feature
- Change `default = ["mcp-briefing", "backend-sqlite"]` to `default = ["mcp-briefing"]`
- Remove `unimatrix-store/backend-sqlite` and `unimatrix-engine/backend-sqlite` from feature propagation

### unimatrix-engine Cargo.toml changes

- Remove `[features]` section entirely (only contained `backend-sqlite = []`)

### Workspace Cargo.toml changes

- Remove `redb = "3.1"` from `[workspace.dependencies]`

---

## Integration Surface

### What the server sees (before vs. after)

| Import path (before) | Import path (after) | Change? |
|---|---|---|
| `unimatrix_store::Store` | `unimatrix_store::Store` | No change (re-export) |
| `unimatrix_store::SqliteReadTransaction` | `unimatrix_store::SqliteReadTransaction` | No change (re-export) |
| `unimatrix_store::SqliteWriteTransaction` | `unimatrix_store::SqliteWriteTransaction` | No change (re-export) |
| `unimatrix_store::ENTRIES` | `unimatrix_store::ENTRIES` | No change (re-export) |
| `unimatrix_store::BlobGuard` | `unimatrix_store::BlobGuard` | No change (re-export) |
| `unimatrix_store::TableSpec` | `unimatrix_store::TableSpec` | No change (re-export) |
| `unimatrix_store::open_table(...)` | `unimatrix_store::open_table(...)` | No change (method on txn) |
| `unimatrix_store::migrate::export::export` | DELETED | Subcommand removed |
| `unimatrix_store::migrate::import::import` | DELETED | Subcommand removed |

**Key finding**: The server crate requires ZERO changes to its database access code. All changes to the server are:
1. Remove Export/Import CLI subcommands and handlers
2. Remove `redb` optional dependency and `backend-sqlite` feature
3. Remove the `DatabaseAlreadyOpen` retry block from `open_with_retries`
4. Rename `test.redb` in test files

### What the engine crate sees

| Before | After |
|---|---|
| `#[cfg(feature = "backend-sqlite")] let db_path = ...join("unimatrix.db")` | `let db_path = ...join("unimatrix.db")` |
| `#[cfg(not(feature = "backend-sqlite"))] let db_path = ...join("unimatrix.redb")` | DELETED |
| `backend-sqlite` feature in Cargo.toml | Feature section removed |

---

## Risk Mitigations (Traced from Scope Risk Assessment)

| Risk | Mitigation in Architecture |
|---|---|
| SR-01: Compat layer depth | ADR-001: Retain and relocate, do not delete |
| SR-02: Module flattening collisions | ADR-002: Explicit merge plan per file |
| SR-03: test.redb references | Wave 7: Global search-and-replace, post-build verification |
| SR-04: Migrate module dependencies | Wave 2: Delete entire directory atomically with lib.rs + server changes |
| SR-05: Schema shared types | Wave 5: Delete only cfg-gated blocks, preserve shared types |
| SR-06: Error variant references | Wave 6: Grep verification before removing variants |
| SR-07: nxs-006 merge conflict | Prerequisite: nxs-006 must be merged before implementation begins |
