# nxs-007: Specification -- redb Removal

## Overview

This specification defines the exact deletions, modifications, and verifications required to remove the redb backend from the Unimatrix workspace. Every change is traced to an acceptance criterion (AC) from SCOPE.md. The specification is organized by wave (per ADR-003) to ensure each step produces a compilable state.

---

## Domain Model

nxs-007 has no new domain concepts. It removes one domain concept (the redb storage backend) and simplifies the remaining domain:

**Before**: Two storage backends (redb, SQLite) selected via compile-time feature flag, with a compat layer bridging the API difference.

**After**: One storage backend (SQLite), no feature flag, no compat bridging needed. The compat types survive as the server's typed table API until nxs-008.

---

## Wave 1: Delete redb Implementation Files (AC-01)

### Files to Delete

| File | Lines | Verification |
|------|-------|-------------|
| `crates/unimatrix-store/src/db.rs` | 532 | File gone, no compile error |
| `crates/unimatrix-store/src/read.rs` | 924 | File gone, no compile error |
| `crates/unimatrix-store/src/write.rs` | 1,939 | File gone, no compile error |
| `crates/unimatrix-store/src/migration.rs` | 1,421 | File gone, no compile error |
| `crates/unimatrix-store/src/query.rs` | 318 | File gone, no compile error |
| `crates/unimatrix-store/src/counter.rs` | 56 | File gone, no compile error |

### lib.rs Changes

Remove these module declarations (all gated with `#[cfg(not(feature = "backend-sqlite"))]`):
```rust
// DELETE these 6 lines:
#[cfg(not(feature = "backend-sqlite"))]
mod db;
#[cfg(not(feature = "backend-sqlite"))]
mod counter;
#[cfg(not(feature = "backend-sqlite"))]
mod migration;
#[cfg(not(feature = "backend-sqlite"))]
mod write;
#[cfg(not(feature = "backend-sqlite"))]
mod read;
#[cfg(not(feature = "backend-sqlite"))]
mod query;
```

Remove these re-exports (all gated with `#[cfg(not(feature = "backend-sqlite"))]`):
```rust
// DELETE these lines (approx 46-57):
#[cfg(not(feature = "backend-sqlite"))]
pub use schema::{AGENT_REGISTRY, AUDIT_LOG, COUNTERS};
#[cfg(not(feature = "backend-sqlite"))]
pub use schema::{ENTRIES, TOPIC_INDEX, ...};
#[cfg(not(feature = "backend-sqlite"))]
pub use schema::{SIGNAL_QUEUE, SESSIONS, INJECTION_LOG};
#[cfg(not(feature = "backend-sqlite"))]
pub use schema::CO_ACCESS;
#[cfg(not(feature = "backend-sqlite"))]
pub use counter::{next_entry_id, increment_counter};
#[cfg(not(feature = "backend-sqlite"))]
pub use db::Store;
```

**Compilation gate**: `cargo check -p unimatrix-store` succeeds.

---

## Wave 2: Delete migrate/ Directory (AC-02, AC-09, AC-15)

### Files to Delete

| File | Lines | Purpose |
|------|-------|---------|
| `crates/unimatrix-store/src/migrate/export.rs` | 293 | redb export |
| `crates/unimatrix-store/src/migrate/import.rs` | 412 | SQLite import |
| `crates/unimatrix-store/src/migrate/format.rs` | 330 | Shared intermediate format |
| `crates/unimatrix-store/src/migrate/mod.rs` | 215 | Module root |

### lib.rs Changes

```rust
// DELETE this line:
pub mod migrate;
```

### Server main.rs Changes

Remove the `Export` and `Import` variants from the `Command` enum:

```rust
// DELETE these variants and their doc comments:
/// Export all tables from the redb database to a JSON-lines file.
Export {
    output: PathBuf,
    #[arg(long)]
    db_path: Option<PathBuf>,
},
/// Import tables from a JSON-lines file into a new SQLite database.
Import {
    input: PathBuf,
    output: PathBuf,
},
```

Remove the match arms in the command handler:

```rust
// DELETE these arms:
Some(Command::Export { output, db_path }) => {
    run_export(output, db_path, cli.project_dir)
}
Some(Command::Import { input, output }) => {
    run_import(input, output)
}
```

Remove the `run_export` and `run_import` functions entirely (approximately lines 344-407 in current main.rs).

Remove the `use unimatrix_store::migrate` import if present.

**Compilation gate**: `cargo check -p unimatrix-store -p unimatrix-server` succeeds.

---

## Wave 3: Flatten sqlite/ Module to Crate Root (AC-05)

### File Moves

| Source | Destination | Notes |
|--------|-------------|-------|
| `sqlite/db.rs` | `src/db.rs` | Replaces deleted redb db.rs |
| `sqlite/read.rs` | `src/read.rs` | Replaces deleted redb read.rs |
| `sqlite/write.rs` | `src/write.rs` | Replaces deleted redb write.rs |
| `sqlite/write_ext.rs` | `src/write_ext.rs` | New top-level file |
| `sqlite/migration.rs` | `src/migration.rs` | Replaces deleted redb migration.rs |
| `sqlite/txn.rs` | `src/txn.rs` | New top-level file |
| `sqlite/compat.rs` | `src/tables.rs` | Renamed per ADR-001 |
| `sqlite/compat_handles.rs` | `src/handles.rs` | Renamed per ADR-001 |
| `sqlite/compat_txn.rs` | `src/dispatch.rs` | Renamed per ADR-001 |

### Files to Delete

| File | Reason |
|------|--------|
| `sqlite/mod.rs` | Module root no longer needed |
| `sqlite/sessions.rs` | Merged into root sessions.rs (Wave 4) |
| `sqlite/injection_log.rs` | Merged into root injection_log.rs (Wave 4) |
| `sqlite/signal.rs` | Merged into root signal.rs (Wave 4) |

### lib.rs Changes

Replace:
```rust
#[cfg(feature = "backend-sqlite")]
mod sqlite;
```

With:
```rust
mod db;
mod txn;
mod read;
mod write;
mod write_ext;
mod migration;
mod tables;
mod handles;
mod dispatch;
```

Replace SQLite re-exports:
```rust
// BEFORE:
#[cfg(feature = "backend-sqlite")]
pub use sqlite::Store;
#[cfg(feature = "backend-sqlite")]
pub use sqlite::{SqliteReadTransaction, SqliteWriteTransaction};
#[cfg(feature = "backend-sqlite")]
#[allow(unused_imports)]
pub use sqlite::{
    ENTRIES, TOPIC_INDEX, ...
};

// AFTER:
pub use db::Store;
pub use txn::{SqliteReadTransaction, SqliteWriteTransaction};
pub use tables::{
    SqliteTableDef, SqliteMultimapDef,
    ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX,
    STATUS_INDEX, VECTOR_MAP, COUNTERS, OUTCOME_INDEX, AUDIT_LOG,
    AGENT_REGISTRY, FEATURE_ENTRIES, CO_ACCESS, SIGNAL_QUEUE,
    SESSIONS, INJECTION_LOG,
    next_entry_id, increment_counter, decrement_counter,
    BlobGuard, U64Guard, UnitGuard, CompositeKeyGuard, U64KeyGuard,
    RangeResult,
};
pub use handles::{
    TableU64Blob, TableStrU64, TableStrBlob,
    TableStrU64Comp, TableU64U64Comp, TableU8U64Comp,
    TableU64U64, MultimapStrU64,
};
pub use dispatch::{TableSpec, MultimapSpec};
```

### Path Updates in Moved Files

All moved files use `super::` to reference sibling modules. Since they move from `sqlite/` submodule to root, these paths must change:

| File | Before | After |
|------|--------|-------|
| db.rs | `use super::txn::...` | `use crate::txn::...` |
| db.rs | `use super::migration::...` | `use crate::migration::...` |
| read.rs | `use super::db::Store` | `use crate::db::Store` |
| read.rs | `use super::txn::...` | `use crate::txn::...` |
| write.rs | `use super::db::Store` | `use crate::db::Store` |
| write.rs | `use super::txn::...` | `use crate::txn::...` |
| write_ext.rs | `use super::db::Store` | `use crate::db::Store` |
| write_ext.rs | `use super::txn::...` | `use crate::txn::...` |
| migration.rs | `use super::db::Store` | `use crate::db::Store` |
| tables.rs | `use super::txn::...` | `use crate::txn::...` |
| handles.rs | `use super::compat::...` | `use crate::tables::...` |
| handles.rs | `use super::txn::...` | `use crate::txn::...` |
| dispatch.rs | `use super::compat::...` | `use crate::tables::...` |
| dispatch.rs | `use super::compat_handles::...` | `use crate::handles::...` |
| dispatch.rs | `use super::txn::...` | `use crate::txn::...` |

Also update visibility modifiers: `pub(crate)` modules in sqlite/mod.rs become `mod` in lib.rs (private by default). The `pub(crate)` on functions/structs in moved files may need adjustment since they are no longer in a submodule.

**Compilation gate**: `cargo check -p unimatrix-store` succeeds.

---

## Wave 4: Merge Shared Modules (AC-04 partial)

### sessions.rs Merge

**Keep from root sessions.rs** (lines 1-67):
- Module doc comment
- `use serde::{Deserialize, Serialize};` (remove redb imports)
- Constants: `TIMED_OUT_THRESHOLD_SECS`, `DELETE_THRESHOLD_SECS`
- Types: `SessionRecord`, `SessionLifecycleStatus`, `GcStats`
- Remove: `#[cfg(not(feature = "backend-sqlite"))] use redb::{...}` (line 8)
- Remove: `#[cfg(not(feature = "backend-sqlite"))] use crate::db::Store` (lines 12-13)
- Remove: `#[cfg(not(feature = "backend-sqlite"))] use crate::error::{Result, StoreError}` (lines 14-15, but keep the import without cfg)
- Remove: `#[cfg(not(feature = "backend-sqlite"))] use crate::schema::{INJECTION_LOG, SESSIONS}` (line 16)

**Keep one copy of serialization helpers** (from root, without cfg gates):
- `serialize_session` (remove `#[cfg(not(feature = "backend-sqlite"))]`)
- `deserialize_session` (remove `#[cfg(not(feature = "backend-sqlite"))]`)

**Delete from root sessions.rs** (lines 87-308):
- The entire `#[cfg(not(feature = "backend-sqlite"))] impl Store { ... }` block (redb implementation)

**Append from sqlite/sessions.rs** (lines 29-302):
- The `impl Store { ... }` block containing insert_session, update_session, get_session, scan_sessions_by_feature, scan_sessions_by_feature_with_status, gc_sessions
- Update: `use super::db::Store` -> `use crate::db::Store`
- Remove: `use crate::sessions::{GcStats, SessionLifecycleStatus, SessionRecord}` (types are now in the same file)
- Remove: duplicate serialize/deserialize helpers (keep the ones from root)

**Delete from root sessions.rs** (lines 310-682):
- The entire `#[cfg(test)] #[cfg(not(feature = "backend-sqlite"))] mod tests { ... }` block (redb-only tests)

### injection_log.rs Merge

**Keep from root injection_log.rs**:
- Module doc comment
- `use serde::{Deserialize, Serialize};` (remove redb imports)
- `use crate::error::{Result, StoreError};` (without cfg gate)
- Type: `InjectionLogRecord`
- Serialization helpers: `serialize_injection_log`, `deserialize_injection_log` (remove cfg gates, remove `#[cfg_attr(feature = "backend-sqlite", allow(dead_code))]`)

**Delete**: Entire `#[cfg(not(feature = "backend-sqlite"))] impl Store { ... }` block (redb implementation)

**Append from sqlite/injection_log.rs**: The `impl Store { ... }` block
- Update paths: `use super::db::Store` -> `use crate::db::Store`
- Remove: `use crate::injection_log::InjectionLogRecord` (now in same file)
- Remove: duplicate serialize helper (keep root's version)

**Delete**: redb-only test module if present

### signal.rs Merge

Root signal.rs has NO redb impl block (no cfg-gated methods). It contains only types and serialization helpers. No redb imports to remove.

**Append from sqlite/signal.rs**: The `impl Store { ... }` block
- Update paths: `use super::db::Store` -> `use crate::db::Store`
- Remove: `use crate::signal::{SignalRecord, SignalType, deserialize_signal, serialize_signal}` (now in same file)

**Compilation gate**: `cargo check -p unimatrix-store` succeeds.

---

## Wave 5: Remove cfg Gates from Store Crate (AC-04)

### error.rs

Delete these variants from `StoreError` enum:
- `Database(redb::DatabaseError)` -- and its Display, Error::source, From impl
- `Transaction(redb::TransactionError)` -- and its Display, Error::source, From impl
- `Table(redb::TableError)` -- and its Display, Error::source, From impl
- `Storage(redb::StorageError)` -- and its Display, Error::source, From impl
- `Commit(redb::CommitError)` -- and its Display, Error::source, From impl
- `Compaction(redb::CompactionError)` -- and its Display, Error::source, From impl

Remove `#[cfg(feature = "backend-sqlite")]` gate from:
- `Sqlite(rusqlite::Error)` variant
- Its Display arm
- Its Error::source arm
- Its `From<rusqlite::Error>` impl

Remove `use redb;` if present.

**Post-state StoreError**:
```rust
pub enum StoreError {
    EntryNotFound(u64),
    Sqlite(rusqlite::Error),
    Serialization(String),
    Deserialization(String),
    InvalidStatus(u8),
}
```

### schema.rs

Delete lines 1-91 (all redb table definitions gated with `#[cfg(not(feature = "backend-sqlite"))]`).
Delete the `use redb::{MultimapTableDefinition, TableDefinition};` import (line 2, cfg-gated).
Keep everything from line 92 onward (Status enum, EntryRecord, NewEntry, QueryFilter, TimeRange, DatabaseConfig, serialization helpers, CoAccessRecord, tests).

### test_helpers.rs

Remove the cfg gate on the SQLite path. Delete the redb path.

**Before** (approximately):
```rust
#[cfg(not(feature = "backend-sqlite"))]
let path = dir.path().join("test.redb");
#[cfg(feature = "backend-sqlite")]
let path = dir.path().join("test.redb");  // renamed to test.db in Wave 7
```

**After**:
```rust
let path = dir.path().join("test.redb");  // renamed to test.db in Wave 7
```

### lib.rs

Remove comment "Backend-specific modules: redb (default) or SQLite (feature-gated)" and similar.
Remove any remaining `#[cfg(...)]` gates referencing `backend-sqlite`.

**Compilation gate**: `cargo check -p unimatrix-store` succeeds without any feature flags.

---

## Wave 6: Remove cfg Gates + redb Dependency from Engine, Server, and Workspace (AC-04, AC-06, AC-07, AC-08)

### unimatrix-engine/src/project.rs

Remove cfg gates around db_path:
```rust
// BEFORE:
#[cfg(feature = "backend-sqlite")]
let db_path = data_dir.join("unimatrix.db");
#[cfg(not(feature = "backend-sqlite"))]
let db_path = data_dir.join("unimatrix.redb");

// AFTER:
let db_path = data_dir.join("unimatrix.db");
```

Remove cfg-gated test assertion:
```rust
// BEFORE:
#[cfg(feature = "backend-sqlite")]
assert!(paths.db_path.to_string_lossy().ends_with("unimatrix.db"));
#[cfg(not(feature = "backend-sqlite"))]
assert!(paths.db_path.to_string_lossy().ends_with("unimatrix.redb"));

// AFTER:
assert!(paths.db_path.to_string_lossy().ends_with("unimatrix.db"));
```

### unimatrix-engine/Cargo.toml

Delete entire `[features]` section:
```toml
# DELETE:
[features]
backend-sqlite = []
```

### unimatrix-server/src/main.rs

Remove the `DatabaseAlreadyOpen` retry block in `open_with_retries`:
```rust
// DELETE this arm:
#[cfg(not(feature = "backend-sqlite"))]
Err(StoreError::Database(redb::DatabaseError::DatabaseAlreadyOpen)) => {
    // ... retry logic ...
}
```

Remove `use unimatrix_store::StoreError;` if it was only used for the redb match arm (check if used elsewhere first).

Remove `DB_OPEN_MAX_ATTEMPTS` and `DB_OPEN_RETRY_DELAY` constants if they are only used by the removed retry block. If the retry logic is still needed for SQLite (e.g., SQLITE_BUSY), keep it but simplify.

### unimatrix-server/Cargo.toml

```toml
# BEFORE:
redb = { workspace = true, optional = true }
...
[features]
default = ["mcp-briefing", "backend-sqlite"]
mcp-briefing = []
backend-sqlite = ["unimatrix-store/backend-sqlite", "unimatrix-engine/backend-sqlite"]

# AFTER:
# (redb line deleted)
...
[features]
default = ["mcp-briefing"]
mcp-briefing = []
# (backend-sqlite deleted)
```

### unimatrix-store/Cargo.toml

```toml
# BEFORE:
[features]
default = ["backend-sqlite"]
test-support = ["dep:tempfile"]
backend-sqlite = ["dep:rusqlite"]

[dependencies]
redb = { workspace = true }
rusqlite = { version = "0.34", features = ["bundled"], optional = true }

# AFTER:
[features]
test-support = ["dep:tempfile"]

[dependencies]
rusqlite = { version = "0.34", features = ["bundled"] }
# (redb line deleted, rusqlite no longer optional, default features removed)
```

### Workspace Cargo.toml

```toml
# DELETE from [workspace.dependencies]:
redb = "3.1"
```

**Compilation gate**: `cargo check --workspace` succeeds. `cargo test --workspace` passes.

---

## Wave 7: Cosmetic Cleanup (AC-14)

### test.redb Rename

Global search-and-replace across the workspace:
- Pattern: `test.redb`
- Replacement: `test.db`

Affected crates and approximate locations:
- `crates/unimatrix-store/src/test_helpers.rs` (1 occurrence)
- `crates/unimatrix-store/src/injection_log.rs` (1 occurrence, if tests remain)
- `crates/unimatrix-core/src/adapters.rs` (3 occurrences)
- `crates/unimatrix-core/src/async_wrappers.rs` (2 occurrences)
- `crates/unimatrix-vector/src/test_helpers.rs` (1 occurrence)
- `crates/unimatrix-server/src/server.rs` (1 occurrence)
- `crates/unimatrix-server/src/infra/audit.rs` (2 occurrences)
- `crates/unimatrix-server/src/infra/shutdown.rs` (5 occurrences)
- `crates/unimatrix-server/src/infra/registry.rs` (1 occurrence)
- `crates/unimatrix-server/src/mcp/identity.rs` (1 occurrence)
- `crates/unimatrix-server/src/services/briefing.rs` (1 occurrence)
- `crates/unimatrix-server/src/services/gateway.rs` (2 occurrences)
- `crates/unimatrix-server/src/services/usage.rs` (1 occurrence)
- `crates/unimatrix-server/src/uds/listener.rs` (1 occurrence)
- `crates/unimatrix-server/src/error.rs` (3 occurrences in test assertions)

Special case: `crates/unimatrix-store/src/sessions.rs` has `reopen.redb` in a test. Rename to `reopen.db`.

Note: `crates/unimatrix-store/src/db.rs` has many `test.redb` references but these are in redb-only tests that were deleted in Wave 1. No rename needed.

### Comment Cleanup

Update or remove comments that reference the redb backend:
- lib.rs: "Backend-specific modules: redb (default) or SQLite (feature-gated)" -> remove
- tables.rs (formerly compat.rs): Replace "TEMPORARY: will be removed when the server migrates to the Store API" with "Server table handle API. Will be replaced in nxs-008 when the server migrates to the Store API directly."
- handles.rs (formerly compat_handles.rs): Same comment update
- dispatch.rs (formerly compat_txn.rs): Same comment update
- wire.rs in engine crate: "Reserved for col-010: once INJECTION_LOG persists to redb" -> update to "SQLite" or remove

**Verification gate**: `grep -r "test\.redb" crates/` returns zero results. `grep -r "backend-sqlite" crates/` returns zero results. `cargo test --workspace` passes.

---

## Acceptance Criteria Trace

| AC | Wave | Verification |
|----|------|-------------|
| AC-01: redb impl files deleted | Wave 1 | 6 files removed, `cargo check` passes |
| AC-02: migrate/ deleted | Wave 2 | 4 files removed (export + import + format + mod) |
| AC-03: compat layer handled | Wave 3 | Files relocated and renamed, not deleted (ADR-001) |
| AC-04: cfg gates removed | Waves 4-6 | `grep -r "cfg.*backend.sqlite" crates/` returns 0 |
| AC-05: sqlite/ flattened | Wave 3 | `sqlite/` directory does not exist |
| AC-06: backend-sqlite feature removed | Wave 6 | Not in any Cargo.toml |
| AC-07: redb removed from deps | Wave 6 | Not in any Cargo.toml |
| AC-08: rusqlite unconditional | Wave 6 | Not optional in store Cargo.toml |
| AC-09: export subcommand removed | Wave 2 | Not in server CLI |
| AC-10: cargo build succeeds | Wave 6 | `cargo build --workspace` |
| AC-11: cargo test passes | Wave 6 | `cargo test --workspace` |
| AC-12: no cfg gates remain | Wave 6 | Grep verification |
| AC-13: behavioral parity | Wave 6 | All existing tests pass unchanged |
| AC-14: test.redb renamed | Wave 7 | Grep verification |
| AC-15: import subcommand removed | Wave 2 | Not in server CLI |

---

## Constraints and Prerequisites

1. **nxs-006 must be merged and complete** before any nxs-007 implementation begins
2. **No behavioral changes**: every MCP tool returns identical results
3. **Test infrastructure is cumulative**: no new test scaffolding, only deletion of redb-specific tests
4. **AC-03 revision**: The compat layer is retained per ADR-001. AC-03 from SCOPE.md is satisfied by relocating and renaming (the "transitional" aspect is removed), not by deletion. Full deletion is deferred to nxs-008.
