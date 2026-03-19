## ADR-001: VACUUM INTO via rusqlite (Synchronous, Pre-Tokio)

### Context

The `unimatrix snapshot` subcommand must produce a complete SQLite copy of the database
using `VACUUM INTO`. The snapshot command is dispatched before the tokio runtime per the
C-10 rule in `main.rs`, putting it in the same dispatch group as `export`, `import`,
`version`, and `hook`.

Two implementation options were evaluated:

**Option A — rusqlite synchronous**: Open a `rusqlite::Connection`, call
`conn.execute("VACUUM INTO ?1", [out_path])`. No tokio runtime. No async framing.
rusqlite is already a transitive dependency of `unimatrix-store` (bundled SQLite).

**Option B — sqlx async with `block_export_sync`**: Reuse the `block_export_sync`
helper from `export.rs` to create a current-thread tokio runtime, then issue
`VACUUM INTO` through a sqlx connection. This adds a runtime-creation step for a
single DDL statement.

### Decision

Use rusqlite synchronous (Option A).

`VACUUM INTO` is a single DDL statement, not a query pipeline. It requires no
connection pool, no transaction management, no row iteration — only one synchronous
call. rusqlite's `Connection::execute()` handles this directly. Creating a tokio
runtime (via `block_export_sync`) for a single DDL call is unnecessary overhead with
no benefit.

The snapshot subcommand opens a rusqlite connection to the source database, confirms
the output path is not the same inode as the source, and executes `VACUUM INTO`. The
resulting file is a fully defragmented, self-consistent copy of all tables at the
moment of execution. SQLite WAL mode provides isolation: `VACUUM INTO` reads a
consistent snapshot even while the daemon is running; the resulting copy reflects the
state at the start of the operation.

**Live-DB path guard — applies to both `snapshot` and `eval run`**

The guard is required on every command that opens a database file supplied by the
user. Both `unimatrix snapshot` (output path) and `eval run` (input `db_path`)
must resolve the user-supplied path via `std::fs::canonicalize()` and compare the
result against the active daemon's DB path (obtained from `ProjectPaths`). If they
resolve to the same path, the command must return an error (`EvalError::LiveDbPath`
for `eval run`; a structured CLI error for `snapshot`) before any pool or connection
is opened.

Rationale: `?mode=ro` on the sqlx pool prevents writes but does not prevent
`eval run` from operating against the live production database — which would silently
produce eval results contaminated by ongoing writes. The path guard is the only
layer that catches this before any I/O occurs.

### Consequences

- Snapshot stays fully synchronous — consistent with the C-10 dispatch rule.
- No tokio runtime is created for the snapshot subcommand.
- rusqlite and sqlx coexist in `unimatrix-store`; the snapshot uses rusqlite for
  this single command while all other DB operations use sqlx.
- The snapshot operating mode (against a live daemon's WAL-mode database) is safe
  and does not require the daemon to be stopped first. This must be documented in
  the CLI help text.
- The snapshot database must be opened read-only by downstream consumers — this is
  enforced by `SqliteConnectOptions::read_only(true)` in the eval engine, not by
  rusqlite in the snapshot command itself.
- `eval run` carries the same live-DB path guard as `snapshot`. `EvalError::LiveDbPath`
  is the structured error variant returned when the guard fires. Implementation agents
  must not skip this guard on the assumption that `?mode=ro` is sufficient protection.
