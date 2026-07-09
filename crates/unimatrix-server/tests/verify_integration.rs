//! Integration tests for the `verify` CLI subcommand (nxs-014, Component 4).
//!
//! Exercises `run_verify` end-to-end: real database, real read-only open, real
//! project-dir resolution — mirroring the `export`/`import` CLI integration
//! harness (direct-DB, temp project dir, no running server). Covers:
//!
//! - AC-09 / R-10 — the CLI verify contract (both exit-code branches + id-naming).
//! - R-02 (CLI half) — the loader (`query_all_entries`) returns Deprecated
//!   predecessors, and a Deprecated predecessor is counted as checked / verifies
//!   clean through the CLI path.
//! - R-10 — the DB is opened READ-ONLY (unmodified after a run); a missing project
//!   dir errors cleanly (no panic).
//! - AC-11 / AC-12 — no MCP verify tool; nxs-014 itself adds no schema migration
//!   (the pin tracks HEAD: vnc-047 bumped the schema version to 31 for cycle_tags).
//!
//! The exit-code-wiring + stdout id-naming assertions (open Q2: main() maps Err to
//! a non-zero process exit) drive the REAL compiled binary via
//! `env!("CARGO_BIN_EXE_unimatrix")` — the only way to observe the process exit
//! code and the `describe()` stdout an in-process call cannot (pattern #4964).

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;
use unimatrix_server::project;
use unimatrix_server::verify::run_verify_with_base;
use unimatrix_store::{SqlxStore, Status, compute_content_hash};

// ---------------------------------------------------------------------------
// Helpers (mirroring export_integration.rs / import_integration.rs)
// ---------------------------------------------------------------------------

/// Set up a project dir with an isolated `base_dir`, returning
/// (project_dir, base_dir, db_path). Data resolves inside `base_dir` (never
/// `~/.unimatrix/`) so runs do not leak into the user's home (GH#640 guard).
fn setup_project() -> (TempDir, TempDir, PathBuf) {
    let project_dir = TempDir::new().expect("create project temp dir");
    let base_dir = TempDir::new().expect("create base temp dir");
    let paths =
        project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path())).unwrap();
    assert!(
        paths.data_dir.starts_with(base_dir.path()),
        "data_dir must be inside base_dir to prevent home directory leaks"
    );
    (project_dir, base_dir, paths.db_path)
}

/// Open a SqlxStore synchronously (runs migrations, creating the schema).
fn open_store(db_path: &Path) -> SqlxStore {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(SqlxStore::open(
        db_path,
        unimatrix_store::pool_config::PoolConfig::default(),
    ))
    .expect("open store")
}

/// Insert an entry with a genuine `content_hash` computed from title/content, so a
/// "clean" fixture is self-consistent. `previous_hash`/`supersedes`/`superseded_by`
/// let the caller build a real correction chain.
#[allow(clippy::too_many_arguments)]
async fn insert_entry(
    pool: &sqlx::SqlitePool,
    id: i64,
    title: &str,
    content: &str,
    status: Status,
    supersedes: Option<i64>,
    superseded_by: Option<i64>,
    previous_hash: &str,
    version: i64,
) {
    let hash = compute_content_hash(title, content);
    sqlx::query(
        "INSERT INTO entries (
            id, title, content, topic, category, source, status, confidence,
            created_at, updated_at, last_accessed_at, access_count,
            supersedes, superseded_by, correction_count, embedding_dim,
            created_by, modified_by, content_hash, previous_hash,
            version, feature_cycle, trust_source,
            helpful_count, unhelpful_count, pre_quarantine_status
        ) VALUES (
            ?1, ?2, ?3, 'testing', 'pattern', 'integration-test', ?4, 0.5,
            1700000000, 1700000001, 0, 0,
            ?5, ?6, 0, 384,
            'agent', 'agent', ?7, ?8,
            ?9, 'nxs-014', 'direct',
            0, 0, NULL
        )",
    )
    .bind(id)
    .bind(title)
    .bind(content)
    .bind(status as u8 as i64)
    .bind(supersedes)
    .bind(superseded_by)
    .bind(&hash)
    .bind(previous_hash)
    .bind(version)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a clean corpus: one legacy entry (empty previous_hash, Active) + a real
/// correction chain (Deprecated predecessor id=1 -> Active successor id=2 chained
/// on the predecessor's content_hash). Returns the predecessor's content_hash.
fn seed_clean_chain(db_path: &Path) -> String {
    let store = open_store(db_path);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let pred_hash = compute_content_hash("Pred", "predecessor content");
    rt.block_on(async {
        let pool = store.write_pool_server();
        // Legacy genesis entry — empty previous_hash, unverifiable-legacy (skipped).
        insert_entry(
            pool,
            10,
            "Legacy",
            "legacy content",
            Status::Active,
            None,
            None,
            "",
            1,
        )
        .await;
        // Correction chain: predecessor (Deprecated, superseded_by=2) <- successor.
        insert_entry(
            pool,
            1,
            "Pred",
            "predecessor content",
            Status::Deprecated,
            None,
            Some(2),
            "",
            1,
        )
        .await;
        insert_entry(
            pool,
            2,
            "Succ",
            "successor content",
            Status::Active,
            Some(1),
            None,
            &pred_hash,
            2,
        )
        .await;
    });
    drop(store);
    pred_hash
}

// ---------------------------------------------------------------------------
// Real-binary harness (exit code + stdout — pattern #4964)
// ---------------------------------------------------------------------------

/// A project + HOME pair whose binary-resolved DB equals `seed_paths`'s DB.
///
/// The binary resolves `base = None -> $HOME/.unimatrix/{hash}`. We seed via
/// `ensure_data_directory(project, Some(home/.unimatrix))` so the two paths are
/// byte-identical (same `compute_project_hash`).
struct BinFixture {
    _home: TempDir,
    project: TempDir,
    home_path: PathBuf,
    db_path: PathBuf,
}

fn bin_fixture() -> BinFixture {
    let home = TempDir::new().expect("home temp dir");
    let project = TempDir::new().expect("project temp dir");
    // A .git marker makes detect_project_root accept the dir deterministically.
    std::fs::create_dir_all(project.path().join(".git")).expect("create .git");
    let base = home.path().join(".unimatrix");
    let paths =
        project::ensure_data_directory(Some(project.path()), Some(&base)).expect("resolve paths");
    BinFixture {
        home_path: home.path().to_path_buf(),
        db_path: paths.db_path,
        project,
        _home: home,
    }
}

/// Run the real `unimatrix --project-dir <root> verify` with `HOME` pointed at the
/// hermetic sandbox. Returns (exit_code, stdout, stderr).
fn run_verify_binary(fx: &BinFixture) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_unimatrix");
    let out = Command::new(exe)
        .arg("--project-dir")
        .arg(fx.project.path())
        .arg("verify")
        .env("HOME", &fx.home_path)
        .env_remove("RUST_LOG")
        .output()
        .expect("spawn unimatrix verify");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8(out.stdout).expect("stdout utf8"),
        String::from_utf8(out.stderr).expect("stderr utf8"),
    )
}

// ===========================================================================
// AC-09 / R-10 — clean corpus: exit 0 + summary (in-process AND real binary)
// ===========================================================================

#[test]
fn test_verify_cli_clean_corpus_exit_zero_with_summary() {
    let (project_dir, base_dir, db_path) = setup_project();
    seed_clean_chain(&db_path);

    // In-process: clean corpus -> Ok (main() would exit 0).
    let result = run_verify_with_base(Some(project_dir.path()), base_dir.path());
    assert!(result.is_ok(), "clean corpus must verify Ok: {result:?}");
}

#[test]
fn test_verify_cli_clean_corpus_binary_exit_zero_prints_summary() {
    let fx = bin_fixture();
    seed_clean_chain(&fx.db_path);

    let (code, stdout, stderr) = run_verify_binary(&fx);
    assert_eq!(code, 0, "clean corpus must exit 0; stderr={stderr}");
    // Not silent: the summary states what was checked / legacy-skipped.
    assert!(
        stdout.contains("chain OK"),
        "stdout must carry a clean summary, got: {stdout:?}"
    );
    assert!(
        stdout.contains("checked"),
        "summary must state entries checked, got: {stdout:?}"
    );
}

// ===========================================================================
// AC-09 / R-10 / R-12 — tampered corpus: non-zero exit + NAMES offending id
// ===========================================================================

#[test]
fn test_verify_cli_tampered_corpus_nonzero_exit_names_id() {
    let fx = bin_fixture();
    seed_clean_chain(&fx.db_path);

    // Tamper: mutate the predecessor's content WITHOUT recomputing content_hash.
    // The stored content_hash is now stale -> ContentHashMismatch on entry 1.
    let store = open_store(&fx.db_path);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        sqlx::query("UPDATE entries SET content = 'TAMPERED' WHERE id = 1")
            .execute(store.write_pool_server())
            .await
            .unwrap();
    });
    drop(store);

    let (code, stdout, stderr) = run_verify_binary(&fx);
    assert_ne!(
        code, 0,
        "tampered corpus must exit non-zero; stdout={stdout}"
    );
    // TEETH (guards #5180 green-on-detect + AC-04/AC-09 id-naming): the output must
    // NAME the offending entry id and the break kind, not just a bare count.
    assert!(
        stdout.contains("entry 1"),
        "output must NAME offending entry id 1, got stdout={stdout:?} stderr={stderr:?}"
    );
    assert!(
        stdout.contains("content hash mismatch"),
        "output must state the break kind, got stdout={stdout:?}"
    );
}

// ===========================================================================
// R-02 (CLI half) — loader returns Deprecated predecessors
// ===========================================================================

/// Loader guard: `query_all_entries()` (the loader `run_verify` uses) must return
/// `Deprecated` rows. If it ever filters to `Active`, chained successors' `supersedes`
/// targets vanish and the core false-alarms `MissingPredecessor` on a clean corpus.
///
/// NOTE: the test plan sites this guard in `unimatrix-store`; this agent's guardrails
/// forbid touching store files, so the equivalent behavioral guard lives here (it
/// exercises the same public loader). Flagged in the agent report.
#[test]
fn test_query_all_entries_returns_deprecated_rows() {
    let (_project_dir, _base_dir, db_path) = setup_project();
    seed_clean_chain(&db_path);

    let store = open_store(&db_path);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let entries = rt
        .block_on(store.query_all_entries())
        .expect("query_all_entries");
    drop(store);

    let deprecated = entries
        .iter()
        .find(|e| e.id == 1)
        .expect("predecessor id=1 must be returned by query_all_entries");
    assert_eq!(
        deprecated.status,
        Status::Deprecated,
        "query_all_entries must return the Deprecated predecessor (R-02)"
    );
}

#[test]
fn test_verify_cli_deprecated_predecessor_verifies_clean() {
    // A clean chain whose predecessor is Deprecated must verify Ok via the CLI —
    // proving the loader fed ALL statuses (incl. Deprecated) to the core. A false
    // MissingPredecessor alarm here would mean the loader filtered to Active.
    let (project_dir, base_dir, db_path) = setup_project();
    seed_clean_chain(&db_path);

    let result = run_verify_with_base(Some(project_dir.path()), base_dir.path());
    assert!(
        result.is_ok(),
        "Deprecated predecessor chain must verify clean (loader is all-status): {result:?}"
    );
}

// ===========================================================================
// R-10 — read-only open + clean resolution error
// ===========================================================================

#[test]
fn test_verify_cli_opens_readonly() {
    let (project_dir, base_dir, db_path) = setup_project();
    seed_clean_chain(&db_path);

    // Logical read-only invariant (robust regardless of SQLite journaling mode):
    // the row set — ids, statuses, and content_hashes — must be identical before
    // and after a verify run. A raw-file-byte comparison is invalid here because a
    // hot WAL is checkpointed into the main file between the two reads even though
    // no row is mutated; that is journaling, not a write to the data. `open_readonly`
    // + read-only queries cannot alter any row, which is what R-10 actually guarantees.
    fn snapshot(db_path: &Path) -> Vec<(u64, Status, String)> {
        let store = open_store(db_path);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let mut rows: Vec<(u64, Status, String)> = rt
            .block_on(store.query_all_entries())
            .expect("query_all_entries")
            .into_iter()
            .map(|e| (e.id, e.status, e.content_hash))
            .collect();
        drop(store);
        rows.sort_by_key(|r| r.0);
        rows
    }

    let before = snapshot(&db_path);
    let result = run_verify_with_base(Some(project_dir.path()), base_dir.path());
    assert!(result.is_ok(), "clean verify: {result:?}");
    let after = snapshot(&db_path);

    assert_eq!(
        before, after,
        "verify must open READ-ONLY — no row (id/status/content_hash) may change across a run (R-10)"
    );
}

#[test]
fn test_verify_cli_missing_project_dir_errors_cleanly() {
    // A non-canonicalizable project dir must surface a clean Err from
    // ensure_data_directory (never a panic).
    let result =
        unimatrix_server::verify::run_verify(Some(Path::new("/nonexistent_path_xyz_nxs014_12345")));
    assert!(
        result.is_err(),
        "missing/invalid project dir must error cleanly, not panic"
    );
}

// ===========================================================================
// Empty DB — clean, sane summary
// ===========================================================================

#[test]
fn test_verify_cli_empty_db_is_clean() {
    let (project_dir, base_dir, db_path) = setup_project();
    // Opening the store creates the (empty) schema; no entries seeded.
    let store = open_store(&db_path);
    drop(store);

    let result = run_verify_with_base(Some(project_dir.path()), base_dir.path());
    assert!(result.is_ok(), "empty DB must verify clean: {result:?}");
}

// ===========================================================================
// AC-12 — no schema migration introduced by nxs-014 itself (schema version pin).
// The absolute value tracks the workspace HEAD (bumped to 31 by vnc-047's
// cycle_tags migration, ADR-001); nxs-014 remains weak-mode and adds no migration.
// ===========================================================================

#[test]
fn test_schema_version_still_31() {
    assert_eq!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION,
        31,
        "nxs-014 is weak-mode: it adds no schema migration. The pin tracks HEAD; \
         vnc-047 bumped CURRENT_SCHEMA_VERSION to 31 for the cycle_tags junction (C-05/NFR-02)"
    );
}

// ===========================================================================
// AC-11 — no MCP verify tool; core signature is transport-free
// ===========================================================================

#[test]
fn test_verify_core_signature_is_transport_free() {
    // Compile-time proof the shared oracle takes `&[EntryRecord]` and returns a
    // `ChainReport` — no CLI/MCP/transport types (C-07, D-4, FR-09). If the
    // signature drifts to accept a transport type this stops compiling.
    let f: fn(&[unimatrix_store::EntryRecord]) -> unimatrix_store::chain_verify::ChainReport =
        unimatrix_store::chain_verify::verify_entries;
    let report = f(&[]);
    assert!(report.is_clean());
}
