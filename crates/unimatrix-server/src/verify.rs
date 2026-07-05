//! `verify` CLI subcommand — on-demand cross-version hash-chain check (nxs-014).
//!
//! Reads the live project database READ-ONLY, loads ALL entries (every status,
//! incl. `Deprecated` predecessors — R-02), runs the transport-agnostic
//! [`unimatrix_store::chain_verify::verify_entries`] oracle over them, prints a
//! human-readable report to stdout (naming every offending entry id on a break —
//! AC-09), and maps the outcome to the process exit code:
//!
//! - clean corpus  ⇒ `Ok(())` ⇒ `main()` exits 0
//! - break found   ⇒ `Err(..)` ⇒ `main()` prints and exits non-zero
//!
//! No running server is required — a direct DB read, mirroring `export`/`import`.
//! The sync entry drives a Tokio runtime internally (pattern #4577), so the
//! pre-Tokio dispatch in `main()` calls it directly like the other sync
//! subcommands. See ARCHITECTURE §Component Interactions (CLI) and ADR-001.

use std::path::Path;

use unimatrix_store::SqlxStore;

use crate::project;

/// CLI entry (production): resolve the project DB under `~/.unimatrix`, verify the
/// hash chain read-only, print the report, and return `Err` on a non-clean report.
///
/// Errors (path resolution, DB open, query failure, or a non-clean report) surface
/// as `Err(Box<dyn Error>)` — `main()` prints and exits non-zero. Never panics.
pub fn run_verify(project_dir: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
    run_verify_inner(project_dir, None)
}

/// Test-isolation variant: route path resolution to an explicit `base_dir` instead
/// of `~/.unimatrix/` (mirrors `run_import_with_base`). Keeps test data hermetic.
pub fn run_verify_with_base(
    project_dir: Option<&Path>,
    base_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    run_verify_inner(project_dir, Some(base_dir))
}

/// Runtime bridge, identical shape to `run_import_inner` (entry #4577). When called
/// from inside an existing runtime, `block_in_place` avoids nesting; otherwise a
/// fresh multi-thread runtime is built. verify does no `block_in_place` of its own,
/// but multi_thread matches import and is safe on both arms.
fn run_verify_inner(
    project_dir: Option<&Path>,
    base_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            tokio::task::block_in_place(|| handle.block_on(run_verify_async(project_dir, base_dir)))
        }
        Err(_) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(run_verify_async(project_dir, base_dir))
        }
    }
}

/// Async implementation: resolve db path, open READ-ONLY, load ALL entries, run the
/// pure core, print, and map the report to the exit contract.
async fn run_verify_async(
    project_dir: Option<&Path>,
    base_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Resolve the db path (same resolver as import/export).
    let paths = project::ensure_data_directory(project_dir, base_dir)?;

    // 2. Open READ-ONLY (R-10): a mis-pointed path can never corrupt data, and a
    //    future edit cannot silently widen this to read-write without failing the
    //    read-only test.
    let store = SqlxStore::open_readonly(&paths.db_path).await?;

    // 3. Load ALL entries incl. Deprecated predecessors (R-02). query_all_entries
    //    has no status filter — the successors' supersedes targets are present.
    let entries = store.query_all_entries().await?;

    // 4. PURE CORE — no I/O, no CLI/MCP types (C-07).
    let report = unimatrix_store::chain_verify::verify_entries(&entries);

    // 5. Print the human-readable report (names every offending id on break;
    //    a checked/skipped summary on clean).
    println!("{}", report.describe());

    // 6. Fail-loud exit contract (NFR-06, AC-09, R-12): clean ⇒ Ok ⇒ exit 0;
    //    non-clean ⇒ Err ⇒ main exits non-zero. Never Ok on a populated report
    //    (the #5180 green-on-detect trap). The offending ids reach the operator
    //    via the describe() print in step 5.
    if report.is_clean() {
        Ok(())
    } else {
        Err(format!(
            "chain verification failed: {} violation(s)",
            report.violations.len()
        )
        .into())
    }
}
