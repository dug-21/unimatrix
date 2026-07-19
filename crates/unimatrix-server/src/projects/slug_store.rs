//! The single shared slug-store resolution funnel (vnc-048, Component 1).
//!
//! Both `export` and `import` resolve a `--slug` target through this ONE funnel so
//! the three reuse invariants (ADR-001) and the pre-open existence gate (ADR-002)
//! are held structurally in a single place:
//!
//! - **One base derivation** — `paths.data_dir.parent()` via the existing fallback
//!   idiom (no `unwrap`/`expect`, C-1). This is the `.unimatrix` base by
//!   construction in all four deploy shapes.
//! - **One join site** — [`super::per_slug_data_dir`], which accepts a
//!   `&ProjectSlug` only (never a raw `&str`, C-2). Traversal is closed
//!   structurally at `ProjectSlug::try_from`.
//! - **One validation edge** — [`super::ProjectRegistry::validate_slug`] (charset +
//!   reserved), run before ANY filesystem or DB access (AC-04).
//!
//! The funnel then applies the **existence gate**: `db_path.exists()` strictly
//! before any caller reaches `SqlxStore::open` (C-3, ADR-002). `open` auto-creates
//! and migrates, so it must never be the gate. A miss fails loud naming the
//! fully-resolved absolute path + slug + next action (ADR-006). The funnel opens
//! no DB, checks no PID, reads no audit log, and mutates no filesystem — the
//! import-only live-PID and non-empty-audit gates layer on top of it in the caller.

use std::path::{Path, PathBuf};

use crate::error::ServerError;
use crate::project::ProjectPaths;

use super::{PROJECT_DB_NAME, PROJECT_VECTOR_DIR, ProjectRegistry, per_slug_data_dir};

/// Resolved per-slug store target paths (vnc-048).
///
/// Derived once by [`resolve_slug_store`]; callers use `db_path` for
/// `SqlxStore::open` and (import only) `vector_dir` as the HNSW rebuild target
/// (ADR-004). `pid_path` is NOT here — it stays base-scoped on the caller's
/// `ProjectPaths` (ADR-003/004) and must not be tidied into this struct.
#[derive(Debug)]
pub(crate) struct SlugStorePaths {
    /// The per-slug database file: `{base}/<slug>/unimatrix.db` (`PROJECT_DB_NAME`).
    pub db_path: PathBuf,
    /// The per-slug vector index dir: `{base}/<slug>/vector` (`PROJECT_VECTOR_DIR`).
    pub vector_dir: PathBuf,
}

/// Resolve the runtime's literal-slug store for `raw_slug` under the base derived
/// from `paths` (vnc-048, ADR-001/002).
///
/// Ordered contract (the reuse triad + pre-open existence gate):
/// 1. validate `raw_slug` → `ProjectSlug` (charset + reserved) — before any FS/DB.
/// 2. derive `base = paths.data_dir.parent()` via the fallback idiom (NO `unwrap`).
/// 3. join `slug_dir = per_slug_data_dir(base, &slug)` — the ONLY join site.
/// 4. derive `db_path`/`vector_dir` from the canonical name constants.
/// 5. existence gate: `db_path.exists()` else fail loud with the absolute path.
///
/// Returns [`SlugStorePaths`] on a store that already exists on disk; never opens
/// the DB and never creates a directory, file, or WAL. Every error is a
/// [`ServerError`] (converts to `Box<dyn Error>` upstream via `?`).
pub(crate) fn resolve_slug_store(
    paths: &ProjectPaths,
    raw_slug: &str,
) -> Result<SlugStorePaths, ServerError> {
    // ── Step 1: validation edge — before ANY filesystem or DB access (AC-04, C-2) ──
    // `validate_slug` = `ProjectSlug::try_from` (charset) THEN `is_reserved_slug`.
    // On Err the ServerError::Config message already names the rejected raw slug;
    // no filesystem is touched (R-08, R-14).
    let slug = ProjectRegistry::validate_slug(raw_slug)?;

    // ── Step 2: base derivation — the ONE derivation, NO unwrap (C-1) ──
    // Exact idiom from projects.rs `ProjectRegistry::resolve` / main.rs. `parent()`
    // is the `.unimatrix` base by construction (ensure_data_directory builds
    // data_dir = base.join(hash)). The None branch (data_dir at fs root) falls back
    // to data_dir itself; the existence gate below then fails loud — never unwrap.
    let base = paths
        .data_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| paths.data_dir.clone());

    // ── Step 3: join site — the ONE join, &ProjectSlug only (C-2) ──
    // A raw &str cannot reach here: `per_slug_data_dir` takes `&ProjectSlug`, the
    // proof `slug` passed the validation edge. Traversal closed structurally.
    let slug_dir = per_slug_data_dir(&base, &slug);

    // ── Step 4: derive store paths from the canonical name constants (no literals) ──
    let db_path = slug_dir.join(PROJECT_DB_NAME);
    let vector_dir = slug_dir.join(PROJECT_VECTOR_DIR);

    // ── Step 5: EXISTENCE GATE — strictly before any SqlxStore::open (C-3, AC-03) ──
    // `exists()` is a pure read: no store, dir, WAL, or output file is created
    // (R-02, R-14). This is also where a host bind-mount base miss surfaces — the
    // printed absolute path distinguishes a base miss from a typo (ADR-006).
    if !db_path.exists() {
        return Err(ServerError::Config(format!(
            "no store found for slug '{slug}' at {abs}: expected the runtime's \
             per-slug store. Run `project register {slug}` and restart, or check \
             --project-dir.",
            slug = slug,
            abs = db_path.display()
        )));
    }

    // ── Step 6: success ──
    Ok(SlugStorePaths {
        db_path,
        vector_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    /// Build a `ProjectPaths` whose `data_dir` is `data_dir` and whose other fields
    /// are inert placeholders derived from it. The funnel only reads `.data_dir`.
    fn paths_with_data_dir(data_dir: PathBuf) -> ProjectPaths {
        ProjectPaths {
            project_root: PathBuf::from("/nonexistent/project/root"),
            project_hash: "0000000000000000".to_string(),
            db_path: data_dir.join(PROJECT_DB_NAME),
            vector_dir: data_dir.join(PROJECT_VECTOR_DIR),
            pid_path: data_dir.join("unimatrix.pid"),
            socket_path: data_dir.join("unimatrix.sock"),
            mcp_socket_path: data_dir.join("unimatrix-mcp.sock"),
            log_path: data_dir.join("unimatrix.log"),
            data_dir,
        }
    }

    /// Snapshot the immediate children of `dir` (empty set if `dir` is absent).
    fn children(dir: &Path) -> BTreeSet<PathBuf> {
        match std::fs::read_dir(dir) {
            Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
            Err(_) => BTreeSet::new(),
        }
    }

    /// Seed a real per-slug store (`base/<slug>/unimatrix.db`) so the existence gate
    /// passes; returns the slug dir.
    fn seed_store(base: &Path, slug: &str) -> PathBuf {
        let slug_dir = base.join(slug);
        std::fs::create_dir_all(&slug_dir).expect("create slug dir");
        std::fs::write(slug_dir.join(PROJECT_DB_NAME), b"seed").expect("write db");
        slug_dir
    }

    // ── Base derivation (R-05, C-1, NFR-4) — AC-01 ────────────────────────────

    #[test]
    fn test_resolve_slug_store_base_is_data_dir_parent() {
        let base = TempDir::new().unwrap();
        // data_dir is a path-hash dir under the base; base == data_dir.parent().
        let data_dir = base.path().join("deadbeefdeadbeef");
        seed_store(base.path(), "alpha");

        let paths = paths_with_data_dir(data_dir);
        let resolved = resolve_slug_store(&paths, "alpha").expect("resolve ok");

        // The resolved db lives under `{base}/<slug>/` — its parent is the slug dir.
        assert_eq!(
            resolved.db_path.parent().unwrap(),
            base.path().join("alpha")
        );
    }

    #[test]
    fn test_resolve_slug_store_parent_none_uses_fallback_no_unwrap() {
        // data_dir with no parent (`.parent() == None`) must NOT panic — the
        // fallback idiom clones data_dir. Root "/" has parent None.
        let paths = paths_with_data_dir(PathBuf::from("/"));
        // Guard: the fallback path is exercised (no panic), and the existence gate
        // then fails loud because no store exists at the resolved path.
        let err = resolve_slug_store(&paths, "alpha").expect_err("missing store errors");
        assert!(matches!(err, ServerError::Config(_)));
    }

    #[test]
    fn test_resolve_slug_store_in_container_shape_derivation() {
        // In-container shape 1: data_dir == /data/.unimatrix/<hash>.
        // No FS at /data in CI — assert the derivation, expect the existence-gate Err.
        let paths = paths_with_data_dir(PathBuf::from("/data/.unimatrix/deadbeefdeadbeef"));
        let err = resolve_slug_store(&paths, "alpha").expect_err("no store in CI");
        let ServerError::Config(msg) = err else {
            panic!("expected ServerError::Config, got {err:?}");
        };
        // Resolved slug dir parent is the .unimatrix base; db_path names it.
        assert!(
            msg.contains("/data/.unimatrix/alpha/unimatrix.db"),
            "message must name the resolved absolute db_path: {msg}"
        );
    }

    #[test]
    fn test_resolve_slug_store_local_dev_shape_derivation() {
        // Local-dev shape 2: data_dir under a ~/.unimatrix-style base.
        let base = TempDir::new().unwrap();
        let unimatrix_base = base.path().join(".unimatrix");
        let data_dir = unimatrix_base.join("deadbeefdeadbeef");
        seed_store(&unimatrix_base, "beta");

        let paths = paths_with_data_dir(data_dir);
        let resolved = resolve_slug_store(&paths, "beta").expect("resolve ok");

        // db_path = `{unimatrix_base}/<slug>/unimatrix.db`; two parents up is the base.
        assert_eq!(
            resolved.db_path.parent().unwrap().parent().unwrap(),
            unimatrix_base
        );
    }

    // ── Validation edge (R-08, C-2, NFR-8) — AC-04 ────────────────────────────
    //
    // `per_slug_data_dir` accepts only `&ProjectSlug`; passing a raw `&str` will
    // not compile. That is the NFR-8 traversal-closure proof by construction — no
    // runtime test can smuggle a raw string into the join site.

    #[test]
    fn test_resolve_slug_store_rejects_charset_invalid() {
        let base = TempDir::new().unwrap();
        let data_dir = base.path().join("deadbeefdeadbeef");
        let paths = paths_with_data_dir(data_dir);
        let before = children(base.path());

        for raw in ["Foo!", "a_b", "-lead", "UPPER", &"a".repeat(64)] {
            let err = resolve_slug_store(&paths, raw).expect_err("charset reject");
            assert!(matches!(err, ServerError::Config(_)), "raw {raw:?}");
        }
        // Validation is pre-FS: no directory/file created under the base.
        assert_eq!(
            children(base.path()),
            before,
            "no FS side effects on reject"
        );
    }

    #[test]
    fn test_resolve_slug_store_rejects_traversal() {
        let base = TempDir::new().unwrap();
        let data_dir = base.path().join("deadbeefdeadbeef");
        let paths = paths_with_data_dir(data_dir);
        let before = children(base.path());

        for raw in ["../x", "..", "%2e%2e", "/abs", "a/b", "a\\b", "a\0b"] {
            let err = resolve_slug_store(&paths, raw).expect_err("traversal reject");
            assert!(matches!(err, ServerError::Config(_)), "raw {raw:?}");
        }
        // Zero filesystem touch — the raw &str never reached per_slug_data_dir.
        assert_eq!(
            children(base.path()),
            before,
            "no FS side effects on traversal"
        );
    }

    #[test]
    fn test_resolve_slug_store_rejects_reserved() {
        let base = TempDir::new().unwrap();
        let data_dir = base.path().join("deadbeefdeadbeef");
        let paths = paths_with_data_dir(data_dir);
        let before = children(base.path());

        for raw in ["v1", "health", "observe", "tools"] {
            let err = resolve_slug_store(&paths, raw).expect_err("reserved reject");
            assert!(matches!(err, ServerError::Config(_)), "raw {raw:?}");
        }
        assert_eq!(
            children(base.path()),
            before,
            "no FS side effects on reserved"
        );
    }

    #[test]
    fn test_resolve_slug_store_accepts_boundary_lengths() {
        let base = TempDir::new().unwrap();
        let data_dir = base.path().join("deadbeefdeadbeef");
        let paths = paths_with_data_dir(data_dir);

        // 1-byte and 63-byte valid slugs pass validation, then hit the existence
        // gate (no store seeded) → Err, proving validation admitted them.
        for raw in ["a", &"a".repeat(63)] {
            let err = resolve_slug_store(&paths, raw).expect_err("existence gate");
            let ServerError::Config(msg) = err else {
                panic!("expected ServerError::Config, got {err:?}");
            };
            assert!(
                msg.contains("no store found"),
                "raw len {}: {msg}",
                raw.len()
            );
        }
    }

    // ── Existence gate before open (R-02, C-3) — AC-03 ────────────────────────

    #[test]
    fn test_resolve_slug_store_missing_db_errors_before_open() {
        let base = TempDir::new().unwrap();
        let data_dir = base.path().join("deadbeefdeadbeef");
        let paths = paths_with_data_dir(data_dir);
        let slug_dir = base.path().join("alpha");
        let before = children(&slug_dir); // empty — slug dir does not exist

        let err = resolve_slug_store(&paths, "alpha").expect_err("missing db errors");
        let ServerError::Config(msg) = err else {
            panic!("expected ServerError::Config, got {err:?}");
        };
        let expected = slug_dir.join(PROJECT_DB_NAME);
        assert!(
            msg.contains(&expected.display().to_string()),
            "message must contain the resolved absolute db_path: {msg}"
        );
        // Nothing created at the slug dir: no db, no vector/, no -wal/-shm.
        assert_eq!(
            children(&slug_dir),
            before,
            "existence gate created nothing"
        );
        assert!(!slug_dir.exists(), "slug dir must not be created");
    }

    #[test]
    fn test_resolve_slug_store_vector_only_dir_is_missing_store() {
        let base = TempDir::new().unwrap();
        let data_dir = base.path().join("deadbeefdeadbeef");
        let paths = paths_with_data_dir(data_dir);
        // vector/ present but unimatrix.db absent — the gate is on the db file.
        let slug_dir = base.path().join("alpha");
        std::fs::create_dir_all(slug_dir.join(PROJECT_VECTOR_DIR)).unwrap();

        let err = resolve_slug_store(&paths, "alpha").expect_err("vector-only is missing");
        assert!(matches!(err, ServerError::Config(_)));
        // The db file was not created by the gate.
        assert!(!slug_dir.join(PROJECT_DB_NAME).exists());
    }

    // ── Host bind-mount fail-loud corner (R-06, SR-11, C-7) — AC-03 ───────────

    #[test]
    fn test_resolve_slug_store_host_base_miss_fails_loud_with_resolved_path() {
        // Base derives to a directory where the slug store does NOT exist
        // (simulating host $HOME/.unimatrix ≠ container base). The error names the
        // resolved absolute path actually tried, distinguishing a base miss.
        let host_base = TempDir::new().unwrap();
        let data_dir = host_base.path().join("hosthashhosthash");
        let paths = paths_with_data_dir(data_dir);

        let err = resolve_slug_store(&paths, "alpha").expect_err("host base miss");
        let ServerError::Config(msg) = err else {
            panic!("expected ServerError::Config, got {err:?}");
        };
        let tried = host_base.path().join("alpha").join(PROJECT_DB_NAME);
        assert!(
            msg.contains(&tried.display().to_string()),
            "must name the resolved absolute path tried: {msg}"
        );
    }

    // ── SlugStorePaths shape ──────────────────────────────────────────────────

    #[test]
    fn test_slug_store_paths_fields() {
        let base = TempDir::new().unwrap();
        let data_dir = base.path().join("deadbeefdeadbeef");
        let slug_dir = seed_store(base.path(), "gamma");
        let paths = paths_with_data_dir(data_dir);

        let resolved = resolve_slug_store(&paths, "gamma").expect("resolve ok");
        assert_eq!(resolved.db_path, slug_dir.join(PROJECT_DB_NAME));
        assert_eq!(resolved.vector_dir, slug_dir.join(PROJECT_VECTOR_DIR));
        // Concrete constant values (guards against a second literal drifting).
        assert_eq!(resolved.db_path, slug_dir.join("unimatrix.db"));
        assert_eq!(resolved.vector_dir, slug_dir.join("vector"));
    }
}
