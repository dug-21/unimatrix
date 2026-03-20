## ADR-001: VACUUM INTO via sqlx + block_on Wrapper

### Context

The `unimatrix snapshot` subcommand must produce a complete SQLite copy of the database
using `VACUUM INTO`. The snapshot command is dispatched before the tokio runtime per the
C-10 rule in `main.rs`, putting it in the same dispatch group as `export`, `import`,
`version`, and `hook`.

Two implementation options were evaluated:

**Option A — rusqlite synchronous**: Open a `rusqlite::Connection`, call
`conn.execute("VACUUM INTO ?1", [out_path])`. No tokio runtime. No async framing.

**Option B — sqlx async with `block_export_sync`**: Reuse the `block_export_sync`
helper from `export.rs` to create a current-thread tokio runtime, then issue
`sqlx::query("VACUUM INTO ?").bind(path).execute(&pool).await` through an existing
sqlx pool. The same bridge pattern is already used by `eval scenarios` and `eval run`.

Option A was the original choice. It is **invalid**: rusqlite was fully removed from all
crates in nxs-011 (PR #299). There is no rusqlite dependency in any `Cargo.toml`.
Reintroducing it would undo that migration and add a second SQLite bundling boundary
alongside sqlx's bundled SQLite — two copies of the same C library linked into one
binary.

### Decision

Use sqlx with the `block_export_sync` wrapper (Option B).

`VACUUM INTO` is valid SQL that sqlx executes directly:

```rust
sqlx::query("VACUUM INTO ?").bind(out_path).execute(&pool).await?;
```

The snapshot subcommand calls `block_export_sync` (the same helper already defined in
`export.rs`) to bridge from the synchronous CLI dispatch to the async sqlx call. This
creates a `tokio::runtime::Builder::new_current_thread()` runtime for the duration of
the snapshot — identical to what `export`, `eval scenarios`, and `eval run` already do.
The pool opened for snapshot is a minimal read-only pool against the source database
path; no migration is triggered (raw `SqlitePool` via `SqliteConnectOptions`, not
`SqlxStore::open()`).

Note on Postgres portability: `VACUUM INTO` is SQLite-specific and has no sqlx
abstraction. If the project ever moves to Postgres, the snapshot command requires a full
rewrite (pg_dump / physical backup) regardless of whether rusqlite or sqlx is used
here. This is a known limitation of the SQLite-native approach, not a design flaw
introduced by choosing sqlx.

**Live-DB path guard — applies to both `snapshot` and `eval run`**

The guard is required on every command that opens a database file supplied by the user.
Both `unimatrix snapshot` (output path) and `eval run` (input `db_path`) must resolve
the user-supplied path via `std::fs::canonicalize()` and compare the result against the
active daemon's DB path (obtained from `ProjectPaths`). If they resolve to the same
path, the command must return an error (`EvalError::LiveDbPath` for `eval run`; a
structured CLI error for `snapshot`) before any pool or connection is opened.

Rationale: `?mode=ro` on the sqlx pool prevents writes but does not prevent `eval run`
from operating against the live production database — which would silently produce eval
results contaminated by ongoing writes. The path guard is the only layer that catches
this before any I/O occurs.

### Consequences

- Snapshot uses the same sqlx + `block_export_sync` pattern as `export.rs`, `eval
  scenarios`, and `eval run`. One consistent async-bridge pattern across all pre-tokio
  subcommands.
- No rusqlite dependency is introduced. The nxs-011 migration (PR #299) remains intact.
- A single current-thread tokio runtime is created for the duration of the snapshot
  call — negligible overhead for a one-shot DDL operation.
- `VACUUM INTO` is SQLite-specific. Any future Postgres migration requires a full
  snapshot command rewrite at that time; this is documented as a known limitation.
- The snapshot operating mode (against a live daemon's WAL-mode database) is safe and
  does not require the daemon to be stopped first. WAL provides isolation: `VACUUM INTO`
  reads a consistent snapshot even while the daemon is running. Document in CLI help text.
- The snapshot database must be opened read-only by downstream consumers — enforced by
  `SqliteConnectOptions::read_only(true)` in the eval engine.
- `eval run` carries the same live-DB path guard as `snapshot`. `EvalError::LiveDbPath`
  is the structured error variant returned when the guard fires. Implementation agents
  must not skip this guard on the assumption that `?mode=ro` is sufficient protection.
