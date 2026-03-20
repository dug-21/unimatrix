# Pseudocode: snapshot.rs (D1)

**Location**: `crates/unimatrix-server/src/snapshot.rs`

## Purpose

Produce a self-contained, full-fidelity SQLite copy of the active database via
`VACUUM INTO`. This is the data-collection entry point for the offline eval pipeline.
The snapshot includes every table in the schema — no exclusions. The function is
synchronous at the CLI level (pre-tokio, C-09) but uses `block_export_sync` from
`export.rs` to issue the async sqlx call.

No rusqlite. No migration. No daemon required.

## Dependencies

| Dependency | Location | Role |
|------------|----------|------|
| `block_export_sync` | `crates/unimatrix-server/src/export.rs` | Async-to-sync bridge |
| `sqlx::SqlitePool` + `SqliteConnectOptions` | sqlx | Raw pool, no migration |
| `project::ensure_data_directory` | `crates/unimatrix-server/src/project.rs` | Resolve ProjectPaths |
| `ProjectPaths.db_path` | `unimatrix_engine::project` | Active daemon DB path for guard |
| `std::fs::canonicalize` | stdlib | Live-DB path guard |

## Function: `pub fn run_snapshot`

```
pub fn run_snapshot(
    project_dir: Option<&Path>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>

BODY:
  1. Resolve project paths:
       paths = project::ensure_data_directory(project_dir, None)?

  2. Live-DB path guard (C-13, FR-04, NFR-06, ADR-001):
       active_db = canonicalize(paths.db_path)?
         -- if canonicalize fails: return Err with message
         --   "cannot resolve active database path: {err}; abort"

       out_resolved = canonicalize_or_parent(out)?
         -- canonicalize_or_parent: try canonicalize(out);
         --   if out does not exist yet, canonicalize(out.parent()?) then join(out.file_name())
         -- if parent resolution fails: return Err with message naming the missing parent dir

       if active_db == out_resolved:
         return Err with message:
           "snapshot --out path resolves to the active database\n  active: {active_db}\n  out:    {out_resolved}\n  refusing to overwrite the live database"

  3. Bridge to async via block_export_sync:
       block_export_sync(async {
         do_snapshot(paths.db_path, out).await
       })?

  4. Print success to stderr:
       eprintln!("snapshot written to {}", out.display())

  5. return Ok(())
```

## Function: `async fn do_snapshot` (private)

```
async fn do_snapshot(
    source: PathBuf,   -- paths.db_path
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>

BODY:
  1. Open read-only pool against source DB (raw, no migration, C-02):
       opts = SqliteConnectOptions::new()
                .filename(&source)
                .read_only(true)   -- source is opened read-only; VACUUM INTO writes out
       pool = SqlitePool::connect_with(opts).await?

  NOTE: VACUUM INTO is issued against the READ-ONLY pool that opens the SOURCE db.
        The output path is a new file created by SQLite; the pool itself is read-only.
        This is correct: VACUUM INTO on a read-only connection reads the source and
        writes to the specified output path via SQLite internal mechanism.

  ALTERNATIVE APPROACH (if VACUUM INTO rejects ro connection):
       If sqlx / SQLite rejects VACUUM INTO on a read_only connection,
       open source with read_only(false) and rely on the path guard (step 2 above)
       as the only write-protection. Document this in a comment. The path guard is
       the security boundary; read_only on the source is defense-in-depth only.

  2. Execute VACUUM INTO (ADR-001):
       sqlx::query("VACUUM INTO ?")
         .bind(out.to_str().ok_or("out path is not valid UTF-8")?)
         .execute(&pool)
         .await?

  3. Close pool:
       pool.close().await

  4. return Ok(())
```

## Helper: `fn canonicalize_or_parent` (private)

```
fn canonicalize_or_parent(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>>

BODY:
  match std::fs::canonicalize(path) {
    Ok(p) => return Ok(p),
    Err(_) => {
      -- out file does not exist yet (expected for new snapshot paths)
      parent = path.parent()
               .ok_or("snapshot --out has no parent directory")?
      canon_parent = std::fs::canonicalize(parent)
                     .map_err(|e| format!("parent directory not found: {e}"))?
      return Ok(canon_parent.join(path.file_name().ok_or("--out path has no file name")?))
    }
  }
```

## Error Handling

| Failure Point | Behavior |
|--------------|----------|
| `canonicalize(paths.db_path)` fails | Err with "cannot resolve active database path" — non-zero exit |
| Parent dir of `--out` does not exist | Err before VACUUM INTO — non-zero exit |
| `--out` resolves to active DB path | Err naming both paths — non-zero exit (AC-02) |
| `SqlitePool::connect_with` fails | Propagated; likely permissions or file not found |
| `VACUUM INTO` fails | Propagated; may mean out path is read-only or disk full |
| `--out` path not valid UTF-8 | Err before query execution |

All errors propagate to `main()` which prints to stderr and exits non-zero via `?`.
No panics in this module.

## Key Test Scenarios

Coverage required by RISK-TEST-STRATEGY.md:

1. **Happy path**: snapshot writes a valid SQLite file at `--out`; open the result and
   `SELECT name FROM sqlite_master WHERE type='table'` returns all expected table names
   (AC-01).

2. **Live-DB path guard — exact path**: pass live DB path as `--out`; assert non-zero
   exit and stderr contains both resolved paths (AC-02, R-06).

3. **Live-DB path guard — symlink**: create a symlink pointing to the active DB;
   pass symlink as `--out`; assert same rejection after canonicalization (R-06, NFR-06).

4. **Live-DB path guard — relative path**: pass a relative path that resolves to the
   active DB; assert rejection (R-06).

5. **Missing parent directory**: pass `--out /nonexistent/dir/snap.db`; assert non-zero
   exit before any VACUUM IO (RISK-TEST-STRATEGY edge case).

6. **`canonicalize` fails on source path** (e.g. DB not found): assert non-zero exit
   with descriptive error.

7. **NFR-07 help text**: `unimatrix snapshot --help` output contains content-sensitivity
   warning mentioning `agent_id`, `session_id`, and storage guidance.

8. **WAL isolation**: snapshot taken while daemon is writing; SHA-256 of source DB
   unchanged; snapshot is a valid SQLite file (documented, not unit-tested per R-18).

## Notes for Implementer

- `VACUUM INTO` is a DDL statement that creates an entirely new database file at the
  output path. It does not modify the source. The source pool can be opened read-only
  as a safety layer, but if that blocks VACUUM INTO, open without read_only and rely
  on the path guard.
- `block_export_sync` is exported from `export.rs` — make it `pub(crate)` if it is
  currently `pub(crate)` or private; implementer must verify visibility.
- The help text for `--help` must include: "WARNING: The snapshot contains all database
  content including agent_id, session_id, and query history. Do not commit snapshots
  to version control or share outside your development environment." (NFR-07, C-12).
- No `--anonymize` flag is implemented or planned (C-12).

## Knowledge Stewardship

Queried: /uni-query-patterns for "snapshot vacuum database patterns" — 5 results; #1097 (ADR-001: Snapshot Isolation for Knowledge Export) is the closest prior art. That ADR uses transaction-scoped reads; nan-007 uses VACUUM INTO for full-DB copy. Approach is consistent with the isolation principle; different mechanism for different scope.
Queried: /uni-query-patterns for "block_export_sync async bridge pattern" — 5 results; #2126 (block_in_place pattern) and #1758 (extract spawn_blocking body into named sync helper for testability) are directly applicable. block_export_sync in export.rs follows this established convention.
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — ADR-001 (#2602) directly governs this module: sqlx + block_on wrapper + live-DB path guard. Followed exactly.
Stored: nothing novel to store — pseudocode agents are read-only; patterns are consumed not created
