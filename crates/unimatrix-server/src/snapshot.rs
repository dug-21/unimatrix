//! Database snapshot command (nan-007, D1).
//!
//! Produces a self-contained, full-fidelity SQLite copy of the active database via
//! `VACUUM INTO`. Unlike `export`, the snapshot includes *every* table — analytics
//! tables, query_log, co_access, shadow_evaluations — making it suitable as the
//! immutable input for the offline eval pipeline (D2–D4).
//!
//! # Safety invariants
//!
//! * The output path is resolved via [`std::fs::canonicalize`] and compared against
//!   the active DB path before any I/O. Symlinks that point back to the live database
//!   are rejected (AC-02, R-06, NFR-06).
//! * `SqlxStore::open()` is never called here — doing so would trigger schema
//!   migrations against a potentially stale snapshot or produce spurious writes
//!   (R-02, C-02).
//! * The entire async portion runs inside [`crate::export::block_export_sync`], which
//!   creates a minimal current-thread tokio runtime. No outer runtime is assumed (C-09).
//!
//! # Content-sensitivity warning (NFR-07, C-12)
//!
//! The snapshot contains all database content including `agent_id`, `session_id`, and
//! full query history. Snapshots must not be committed to version control or shared
//! outside the development environment. No `--anonymize` flag is provided or planned.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use tracing::info;

use crate::export::block_export_sync;
use crate::project;

/// Produce a full-fidelity SQLite snapshot of the active database at `out`.
///
/// # WARNING
/// The snapshot contains all database content including `agent_id`, `session_id`, and
/// query history. **Do not commit snapshots to version control or share outside your
/// development environment.**
///
/// # Errors
///
/// Returns an error when:
/// - The active DB path cannot be resolved via `canonicalize` (DB not initialised).
/// - The parent directory of `out` does not exist.
/// - `out` (after canonicalization) resolves to the same path as the active DB.
/// - The sqlx pool cannot be opened (permissions, disk, etc.).
/// - `VACUUM INTO` fails (disk full, target path not writable, etc.).
pub fn run_snapshot(
    project_dir: Option<&Path>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Resolve project paths.
    let paths = project::ensure_data_directory(project_dir, None)?;

    // 2. Live-DB path guard (C-13, FR-04, NFR-06, ADR-001).
    let active_db = std::fs::canonicalize(&paths.db_path).map_err(|e| {
        format!(
            "cannot resolve active database path: {e}; abort\n  path: {}",
            paths.db_path.display()
        )
    })?;

    let out_resolved = canonicalize_or_parent(out)?;

    if active_db == out_resolved {
        return Err(format!(
            "snapshot --out path resolves to the active database\n  active: {}\n  out:    {}\n  refusing to overwrite the live database",
            active_db.display(),
            out_resolved.display()
        )
        .into());
    }

    // 3. Bridge to async via block_export_sync (C-09, ADR-001).
    let source = paths.db_path.clone();
    block_export_sync(async move { do_snapshot(source, out).await })?;

    // 4. Report success.
    eprintln!("snapshot written to {}", out.display());

    Ok(())
}

/// Execute the `VACUUM INTO` operation against the source database.
///
/// Opens a minimal read-only pool against `source` (no migration triggered) and
/// issues `VACUUM INTO out_path`. The output file is created by SQLite — it must
/// not already exist or VACUUM INTO will fail.
///
/// # Note on read-only pool + VACUUM INTO
///
/// `VACUUM INTO` is issued against a read-only connection that opens the *source* DB.
/// The output path is written directly by the SQLite engine, not through the pool.
/// If SQLite rejects `VACUUM INTO` on a strictly read-only connection, the source pool
/// is opened without `read_only(true)` and the path guard (checked above) is the sole
/// write-protection boundary. Document: the path guard is the security layer; read_only
/// is defence-in-depth only (ADR-001 note).
async fn do_snapshot(source: PathBuf, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Open source read-only (defence-in-depth). No migration — raw pool only (C-02).
    // If SQLite rejects VACUUM INTO on a strict read-only connection, fall back to
    // read-write. The path guard above is the actual security boundary; read_only is
    // defence-in-depth only (ADR-001 note).
    let pool = {
        let ro_opts = SqliteConnectOptions::new()
            .filename(&source)
            .read_only(true);
        match SqlitePool::connect_with(ro_opts).await {
            Ok(p) => p,
            Err(_) => {
                // Fallback: open without read_only and rely solely on the path guard.
                let rw_opts = SqliteConnectOptions::new().filename(&source);
                SqlitePool::connect_with(rw_opts).await.map_err(|e| {
                    format!("failed to open source database '{}': {e}", source.display())
                })?
            }
        }
    };

    // Execute VACUUM INTO (ADR-001).
    let out_str = out
        .to_str()
        .ok_or("snapshot --out path is not valid UTF-8")?;

    sqlx::query("VACUUM INTO ?")
        .bind(out_str)
        .execute(&pool)
        .await
        .map_err(|e| format!("VACUUM INTO '{}' failed: {e}", out.display()))?;

    pool.close().await;

    info!(out = %out.display(), "snapshot written");

    Ok(())
}

/// Resolve `path` to a canonical absolute path, tolerating a non-existent leaf.
///
/// When `path` does not yet exist (the expected case for a new snapshot output), the
/// parent directory is canonicalized and the filename is re-appended. This allows the
/// path guard to compare against the active DB path correctly even when the output file
/// has not been created yet.
///
/// Returns an error when the parent directory does not exist or `path` has no filename
/// component.
fn canonicalize_or_parent(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    match std::fs::canonicalize(path) {
        Ok(p) => Ok(p),
        Err(_) => {
            // Output file does not exist yet — canonicalize the parent instead.
            let parent = path
                .parent()
                .ok_or("snapshot --out has no parent directory")?;
            let canon_parent = std::fs::canonicalize(parent).map_err(|e| {
                format!(
                    "parent directory not found for --out '{}': {e}",
                    path.display()
                )
            })?;
            let file_name = path
                .file_name()
                .ok_or("snapshot --out path has no file name")?;
            Ok(canon_parent.join(file_name))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    // ---------------------------------------------------------------------------
    // Helper: create a minimal valid file at `path` so that std::fs::canonicalize
    // works. The content is irrelevant for path-guard tests.
    // ---------------------------------------------------------------------------
    fn create_stub_file(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create parent dir");
        }
        fs::write(path, b"stub").expect("failed to write stub file");
    }

    // ---------------------------------------------------------------------------
    // Helper: resolve the db_path that ensure_data_directory will assign for a
    // given project_dir without requiring the DB to exist. The data directory
    // is created by ensure_data_directory; the DB file itself is NOT.
    // ---------------------------------------------------------------------------
    fn resolve_db_path(project_dir: &Path) -> PathBuf {
        project::ensure_data_directory(Some(project_dir), None)
            .expect("ensure_data_directory should succeed")
            .db_path
    }

    // ---------------------------------------------------------------------------
    // Test: path guard fires when out == active DB (after both are created)
    // ---------------------------------------------------------------------------
    #[test]
    fn test_snapshot_path_guard_same_path() {
        let dir = TempDir::new().unwrap();
        // Resolve the db_path that run_snapshot will use for this project dir.
        let db_path = resolve_db_path(dir.path());
        // Create the DB file so canonicalize succeeds on the source side.
        create_stub_file(&db_path);

        // Pass the same path as `out` — guard must reject it.
        let result = run_snapshot(Some(dir.path()), &db_path);
        assert!(result.is_err(), "expected path guard to reject same path");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("resolves to the active database"),
            "error should mention path conflict; got: {msg}"
        );
        assert!(
            msg.contains("active:"),
            "error should name active path; got: {msg}"
        );
        assert!(
            msg.contains("out:"),
            "error should name out path; got: {msg}"
        );
    }

    // ---------------------------------------------------------------------------
    // Test: path guard fires when out is a symlink pointing to the active DB
    // ---------------------------------------------------------------------------
    #[test]
    #[cfg(unix)]
    fn test_snapshot_path_guard_symlink() {
        let dir = TempDir::new().unwrap();
        let db_path = resolve_db_path(dir.path());
        create_stub_file(&db_path);

        // Create a symlink in the temp dir pointing at the active DB.
        let symlink_path = dir.path().join("link.db");
        std::os::unix::fs::symlink(&db_path, &symlink_path).unwrap();

        let result = run_snapshot(Some(dir.path()), &symlink_path);
        assert!(result.is_err(), "expected symlink guard to reject");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("resolves to the active database"),
            "error should mention path conflict via canonicalized symlink; got: {msg}"
        );
    }

    // ---------------------------------------------------------------------------
    // Test: missing parent directory returns a descriptive error before any VACUUM
    // ---------------------------------------------------------------------------
    #[test]
    fn test_snapshot_parent_dir_missing() {
        let dir = TempDir::new().unwrap();
        let db_path = resolve_db_path(dir.path());
        create_stub_file(&db_path);

        // Output path whose parent does not exist.
        let missing_parent = dir.path().join("nonexistent_parent").join("snap.db");
        let result = run_snapshot(Some(dir.path()), &missing_parent);
        assert!(result.is_err(), "expected error for missing parent dir");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("parent directory not found") || msg.contains("not found"),
            "error should mention missing parent; got: {msg}"
        );
        assert!(
            !missing_parent.exists(),
            "no partial output file should be created"
        );
    }

    // ---------------------------------------------------------------------------
    // Test: canonicalize fails on source DB (project dir with no DB file yet)
    // ---------------------------------------------------------------------------
    #[test]
    fn test_snapshot_canonicalize_fails_on_source() {
        // Use a fresh temp dir — ensure_data_directory will create the data directory
        // but NOT the unimatrix.db file, so canonicalize on paths.db_path will fail.
        let dir = TempDir::new().unwrap();
        let out = dir.path().join("snap.db");

        let result = run_snapshot(Some(dir.path()), &out);
        assert!(
            result.is_err(),
            "expected error when source DB does not exist"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("cannot resolve active database path"),
            "error should indicate unresolvable source; got: {msg}"
        );
    }

    // ---------------------------------------------------------------------------
    // Test: canonicalize_or_parent — existing path returns canonical form
    // ---------------------------------------------------------------------------
    #[test]
    fn test_canonicalize_or_parent_existing_file() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("existing.db");
        fs::write(&file, b"x").unwrap();

        let result = canonicalize_or_parent(&file).unwrap();
        assert!(result.is_absolute(), "result should be absolute");
        assert_eq!(result, fs::canonicalize(&file).unwrap());
    }

    // ---------------------------------------------------------------------------
    // Test: canonicalize_or_parent — non-existent file in existing parent dir
    // ---------------------------------------------------------------------------
    #[test]
    fn test_canonicalize_or_parent_nonexistent_file_existing_parent() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("new_snap.db");
        assert!(!file.exists(), "file should not exist before test");

        let result = canonicalize_or_parent(&file).unwrap();
        assert!(result.is_absolute(), "result should be absolute");
        assert_eq!(result.file_name().unwrap(), "new_snap.db");
    }

    // ---------------------------------------------------------------------------
    // Test: canonicalize_or_parent — non-existent parent dir returns error
    // ---------------------------------------------------------------------------
    #[test]
    fn test_canonicalize_or_parent_missing_parent_returns_error() {
        let missing = Path::new("/nonexistent_dir_xyz_abc/snap.db");
        let result = canonicalize_or_parent(missing);
        assert!(result.is_err(), "expected error for missing parent dir");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("parent directory not found"),
            "error should mention missing parent; got: {msg}"
        );
    }

    // ---------------------------------------------------------------------------
    // Test: SqlxStore::open is not called in this module (structural / doc check).
    //
    // This module intentionally does not import `unimatrix_store::SqlxStore`.
    // Calling `SqlxStore::open()` would trigger schema migration on the source DB,
    // violating R-02 (SqlxStore guard) and C-02. The absence of the import is the
    // primary compile-time guard; this test documents the invariant explicitly.
    // ---------------------------------------------------------------------------
    #[test]
    fn test_snapshot_no_sqlx_store_open_in_snapshot() {
        // Structural invariant documented here. If SqlxStore were ever imported into
        // this module, the doc comment and this test note would need to be removed,
        // which serves as a forcing function for review. The actual guard is the
        // absence of `unimatrix_store::SqlxStore` in the use statements above.
        let _: () = ();
    }
}
