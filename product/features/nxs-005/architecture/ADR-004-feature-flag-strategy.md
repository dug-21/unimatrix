## ADR-004: Cargo Feature Flag for Backend Selection

### Context

The user confirmed a feature flag approach: both backends coexist, redb is the default, SQLite is opt-in via feature flag. redb removal is deferred to nxs-006.

### Decision

Add a Cargo feature `backend-sqlite` to the unimatrix-store crate:

```toml
[features]
default = []
backend-sqlite = ["dep:rusqlite"]
test-support = ["dep:tempfile"]

[dependencies]
redb = { workspace = true }
rusqlite = { version = "0.34", features = ["bundled"], optional = true }
```

Module structure:

```
crates/unimatrix-store/src/
  lib.rs              -- conditional re-exports
  db.rs               -- redb Store (unchanged)
  read.rs             -- redb reads (unchanged)
  write.rs            -- redb writes (unchanged)
  sqlite/
    mod.rs            -- SQLite Store struct
    db.rs             -- Connection management, table creation
    read.rs           -- SQL read operations
    write.rs          -- SQL write operations
  schema.rs           -- shared (EntryRecord, NewEntry, etc.)
  error.rs            -- extended with Sqlite variant under cfg
  migration.rs        -- shared migration logic (schema version detection)
  counter.rs          -- redb counter helpers (unchanged)
  signal.rs           -- shared signal types
  sessions.rs         -- redb session ops (unchanged), SQLite equivalents in sqlite/
  injection_log.rs    -- redb injection ops (unchanged), SQLite equivalents in sqlite/
  query.rs            -- redb query logic (unchanged)
  hash.rs             -- shared (unchanged)
  test_helpers.rs     -- extended with backend-aware test store creation
```

The public type `Store` is selected at compile time:

```rust
// lib.rs
#[cfg(not(feature = "backend-sqlite"))]
mod db;
#[cfg(not(feature = "backend-sqlite"))]
pub use db::Store;

#[cfg(feature = "backend-sqlite")]
mod sqlite;
#[cfg(feature = "backend-sqlite")]
pub use sqlite::Store;
```

**Mutual exclusion**: The two backends are mutually exclusive at compile time. Enabling `backend-sqlite` replaces the Store implementation entirely. Both backends exist in source but only one is compiled.

**Test execution**: `cargo test -p unimatrix-store` tests the default (redb) backend. `cargo test -p unimatrix-store --features backend-sqlite` tests the SQLite backend. CI should run both.

### Consequences

- No runtime backend selection -- compile-time only. This is simpler and avoids trait object overhead.
- Shared types (EntryRecord, NewEntry, QueryFilter, etc.) remain in schema.rs -- no duplication.
- Error type must handle both backends via cfg-gated variants.
- The redb code is completely untouched -- zero regression risk for existing users.
- Downstream crates (unimatrix-core, unimatrix-server) that import `unimatrix_store::Store` get whichever backend is compiled. They need no cfg annotations themselves (except for the transaction type change in ADR-001).
