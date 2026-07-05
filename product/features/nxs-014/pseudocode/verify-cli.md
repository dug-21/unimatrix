# Component 4 — verify-cli

**Files:** `crates/unimatrix-server/src/verify.rs` (new) + `crates/unimatrix-server/src/main.rs` (`Command` enum
+ dispatch) + `crates/unimatrix-server/src/lib.rs` (`pub mod verify;`).
**Source of truth:** ARCHITECTURE §Component Interactions (CLI) + §Error boundaries, SPEC FR-08, sync-subcommand
pattern (entry #4577 / procedure #1192).
**Traces:** FR-08; AC-09; R-10, R-12; C-07.
**Depends on:** Component 1 (`chain_verify::verify_entries`).

## Purpose

Expose the verify core as an on-demand `unimatrix verify` subcommand that reads the live project DB
read-only, runs `verify_entries` over ALL entries, prints a human-readable report, and sets the process exit
code (0 = clean, non-zero = break). No running server required — direct DB read, mirroring `Export`/`Import`.

## main.rs — `Command::Verify` variant + dispatch

Add to the `Command` enum (`main.rs:162`), mirroring `Export`/`Import` (no required args in v1 — project dir
comes from the shared `cli.project_dir`):

```
/// Verify cross-version hash chain integrity of the project database.
Verify {},
```

Add the dispatch arm in `main()` alongside the other sync subcommands (near `:411`), sync path, NO tokio at
the call site (the handler owns its runtime, like `run_import`):

```
Some(Command::Verify {}) => {
    // Sync path, like Import/Export. run_verify returns Err on a non-clean report; the existing
    // main() error handling prints it and exits non-zero. Exit 0 on Ok (clean corpus). (AC-09)
    unimatrix_server::verify::run_verify(cli.project_dir.as_deref())
}
```

(If `main()`'s Result→exit mapping already turns `Err` into a non-zero exit and prints, that satisfies AC-09.
If a specific exit code / id-naming needs to reach stdout on the clean path, `run_verify` prints the summary
itself before returning `Ok`.)

## lib.rs

```
pub mod verify;
```

## verify.rs — handler

Mirror `run_import`'s structure (public sync entry + a `_with_base` test-isolation variant + an async impl),
per pattern #4577. The core work is async (`open_readonly`, `query_all_entries`); the sync wrapper drives a
runtime.

```
use std::path::Path;
use unimatrix_store::SqlxStore;

/// CLI entry (production): resolve project dir under ~/.unimatrix, verify, print, Err on non-clean.
pub fn run_verify(project_dir: Option<&Path>) -> Result<(), Box<dyn std::error::Error>>:
    run_verify_inner(project_dir, None)

/// Test-isolation variant: route path resolution to an explicit base_dir (mirrors run_import_with_base).
pub fn run_verify_with_base(project_dir: Option<&Path>, base_dir: &Path)
        -> Result<(), Box<dyn std::error::Error>>:
    run_verify_inner(project_dir, Some(base_dir))

fn run_verify_inner(project_dir: Option<&Path>, base_dir: Option<&Path>)
        -> Result<(), Box<dyn std::error::Error>>:
    // Runtime bridge, identical shape to run_import_inner (entry #4577). verify does NO block_in_place,
    // so a current-thread runtime is sufficient in the Err arm; multi_thread is also fine and matches import.
    match tokio::runtime::Handle::try_current():
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(run_verify_async(project_dir, base_dir)))
        Err(_)     => tokio::runtime::Builder::new_multi_thread().enable_all().build()?
                          .block_on(run_verify_async(project_dir, base_dir))

async fn run_verify_async(project_dir: Option<&Path>, base_dir: Option<&Path>)
        -> Result<(), Box<dyn std::error::Error>>:
    // 1. Resolve db path (same resolver as import/export)
    let paths = unimatrix_server::project::ensure_data_directory(project_dir, base_dir)?

    // 2. Open READ-ONLY (R-10 security: a mis-pointed path cannot corrupt data; assert read-only in tests)
    let store = SqlxStore::open_readonly(&paths.db_path).await?

    // 3. Load ALL entries incl. Deprecated predecessors (R-02) — query_all_entries has no status filter
    let entries = store.query_all_entries().await?

    // 4. PURE CORE
    let report = unimatrix_store::chain_verify::verify_entries(&entries)

    // 5. Print human-readable report to stdout (names every offending id on break; summary on clean)
    println!("{}", report.describe())

    // 6. Fail-loud exit contract (NFR-06, AC-09): Ok(clean) -> main exits 0; Err(break) -> main exits non-zero
    if report.is_clean():
        Ok(())
    else:
        Err(format!("chain verification failed: {} violation(s)", report.violations.len()).into())
```

**Exit-code contract (AC-09, R-10):** clean ⇒ `Ok(())` ⇒ `main()` exits 0; non-clean ⇒ `Err` ⇒ `main()`
prints and exits non-zero. The offending id(s) reach the operator via the `report.describe()` print in step 5
(a count-only failure fails AC-04/AC-09 — the print names each id). Do NOT return `Ok` on a non-clean report
(the #5180 green-on-detect trap, R-12).

**Read-only open (R-10):** step 2 uses `open_readonly`; a test asserts the open is read-only so a future edit
cannot silently widen it to read-write. A missing/invalid `project_dir` surfaces as a clean `Err` from
`ensure_data_directory`/`open_readonly` (step 1/2 `?`), never a panic.

## Data flow

```
in:  project_dir: Option<&Path> (operator-supplied, or default ~/.unimatrix)
     paths = ensure_data_directory -> db_path
     store = open_readonly(db_path)
     entries = query_all_entries() (ALL statuses)
core: verify_entries(&entries) -> ChainReport
out: stdout = report.describe(); exit 0 (clean) | non-zero (break, ids named)
```

## Error handling

- Path resolution / DB open / query failures → `?` into `Box<dyn Error>` → `Err` → main exits non-zero,
  cleanly (no panic, R-10).
- Non-clean report → `Err` (fail-loud, NFR-06). Clean → `Ok(())`.
- No `.unwrap()` in non-test code; all fallible I/O via `?`.

## Key test scenarios (hints)

1. **AC-09a clean corpus.** Seed a clean corrected chain via `run_verify_with_base`; assert `Ok(())` and the
   printed summary states entries/legacy checked.
2. **AC-09b tampered corpus.** Mutate a superseded entry's content in the DB; run; assert `Err` AND the
   printed output NAMES the offending `entry_id` (not just a count) (R-10, #5180).
3. **Deprecated predecessor via CLI (R-02).** Clean chain whose predecessor is Deprecated; assert `Ok(())`
   (loader returns all statuses).
4. **Read-only open (R-10).** Assert the CLI opens the DB via `open_readonly` (a mis-pointed path cannot
   write). Assert a missing project dir errors cleanly (no panic).
5. **Empty DB.** Fresh project, no corrections; assert `Ok(())`, summary sane.
6. **Exit-code wiring.** Assert the `main()` mapping turns the clean `Ok` into exit 0 and the non-clean `Err`
   into non-zero (R-12 — no fail-silent).
