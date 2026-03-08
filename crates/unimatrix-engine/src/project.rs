//! Project root detection, hash computation, and data directory management.

use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Resolved project paths for data storage.
#[derive(Debug, Clone)]
pub struct ProjectPaths {
    /// Canonical project root directory.
    pub project_root: PathBuf,
    /// First 16 hex chars of SHA-256(canonical_path).
    pub project_hash: String,
    /// Data directory: ~/.unimatrix/{hash}/
    pub data_dir: PathBuf,
    /// Database path: ~/.unimatrix/{hash}/unimatrix.db
    pub db_path: PathBuf,
    /// Vector index directory: ~/.unimatrix/{hash}/vector/
    pub vector_dir: PathBuf,
    /// PID file path: ~/.unimatrix/{hash}/unimatrix.pid
    pub pid_path: PathBuf,
    /// Socket file path: ~/.unimatrix/{hash}/unimatrix.sock
    pub socket_path: PathBuf,
}

/// Detect the project root by walking up from cwd looking for `.git`.
///
/// If `override_dir` is provided, it is used directly (canonicalized).
/// Otherwise, walks up from the current working directory. If no `.git`
/// is found, the current working directory is used.
///
/// Handles both normal repositories (`.git` is a directory) and git
/// worktrees (`.git` is a file containing `gitdir: <path>`). For
/// worktrees, resolves through the gitdir pointer back to the main
/// repository root so all worktrees share the same project hash.
pub fn detect_project_root(override_dir: Option<&Path>) -> io::Result<PathBuf> {
    if let Some(dir) = override_dir {
        let canonical = dir.canonicalize()?;
        return resolve_worktree_root(&canonical);
    }

    let start = std::env::current_dir()?;
    let mut current = start.as_path();

    loop {
        let git_path = current.join(".git");
        if git_path.is_dir() {
            return current.to_path_buf().canonicalize();
        }
        if git_path.is_file() {
            return resolve_git_file(&git_path, current);
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }

    // No .git found — use original cwd
    start.canonicalize()
}

/// Resolve a `.git` file (worktree marker) to the main repository root.
///
/// The file contains a single line: `gitdir: <path>`. The path points to
/// `<main-repo>/.git/worktrees/<name>`. We resolve to `<main-repo>` by
/// finding the `.git` directory ancestor of the gitdir target.
fn resolve_git_file(git_file: &Path, worktree_dir: &Path) -> io::Result<PathBuf> {
    let content = fs::read_to_string(git_file)?;
    let gitdir_line = content
        .lines()
        .find(|l| l.starts_with("gitdir:"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("no gitdir line in {}", git_file.display()),
            )
        })?;

    let gitdir_raw = gitdir_line["gitdir:".len()..].trim();
    let gitdir_path = if Path::new(gitdir_raw).is_absolute() {
        PathBuf::from(gitdir_raw)
    } else {
        worktree_dir.join(gitdir_raw)
    };

    // Walk up from the gitdir target to find the `.git` directory itself.
    // Typical path: <repo>/.git/worktrees/<name> -> we want <repo>.
    let gitdir_canonical = gitdir_path.canonicalize()?;
    let mut ancestor = gitdir_canonical.as_path();
    loop {
        if ancestor.file_name().and_then(|n| n.to_str()) == Some(".git")
            && ancestor.is_dir()
            && let Some(repo_root) = ancestor.parent()
        {
            return repo_root.to_path_buf().canonicalize();
        }
        match ancestor.parent() {
            Some(parent) => ancestor = parent,
            None => break,
        }
    }

    // Fallback: if we can't resolve, use the worktree dir itself
    worktree_dir.to_path_buf().canonicalize()
}

/// If the given directory is a worktree, resolve to the main repo root.
/// Otherwise return the directory as-is.
fn resolve_worktree_root(dir: &Path) -> io::Result<PathBuf> {
    let git_path = dir.join(".git");
    if git_path.is_file() {
        resolve_git_file(&git_path, dir)
    } else {
        Ok(dir.to_path_buf())
    }
}

/// Compute a deterministic project hash from a canonical path.
///
/// Returns the first 16 hex characters of SHA-256(path_as_utf8).
pub fn compute_project_hash(project_root: &Path) -> String {
    let path_string = project_root.to_string_lossy();
    let mut hasher = Sha256::new();
    hasher.update(path_string.as_bytes());
    let digest = hasher.finalize();
    format!("{digest:x}")[..16].to_string()
}

/// Ensure the project data directory exists, creating it if needed.
///
/// `override_dir` overrides the project root (input to the path hash).
/// `base_dir` overrides the parent directory for data storage. When `None`,
/// defaults to `~/.unimatrix/`. Pass a tempdir here in tests to avoid
/// leaking directories into the real home directory.
///
/// Returns `ProjectPaths` with all resolved paths.
pub fn ensure_data_directory(
    override_dir: Option<&Path>,
    base_dir: Option<&Path>,
) -> io::Result<ProjectPaths> {
    let project_root = detect_project_root(override_dir)?;
    let project_hash = compute_project_hash(&project_root);

    let unimatrix_base = match base_dir {
        Some(dir) => dir.to_path_buf(),
        None => {
            let home = dirs::home_dir().ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "home directory not found")
            })?;
            home.join(".unimatrix")
        }
    };

    let data_dir = unimatrix_base.join(&project_hash);
    let db_path = data_dir.join("unimatrix.db");
    let vector_dir = data_dir.join("vector");
    let pid_path = data_dir.join("unimatrix.pid");
    let socket_path = data_dir.join("unimatrix.sock");

    fs::create_dir_all(&data_dir)?;
    #[cfg(unix)]
    fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o700))?;
    fs::create_dir_all(&vector_dir)?;

    Ok(ProjectPaths {
        project_root,
        project_hash,
        data_dir,
        db_path,
        vector_dir,
        pid_path,
        socket_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_root_from_dir_with_git() {
        let dir = tempfile::TempDir::new().unwrap();
        let git_dir = dir.path().join(".git");
        fs::create_dir(&git_dir).unwrap();

        let result = detect_project_root(Some(dir.path())).unwrap();
        assert_eq!(result, dir.path().canonicalize().unwrap());
    }

    #[test]
    fn test_detect_root_override() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = detect_project_root(Some(dir.path())).unwrap();
        assert_eq!(result, dir.path().canonicalize().unwrap());
    }

    #[test]
    fn test_hash_deterministic() {
        let path = Path::new("/tmp/test-project");
        let h1 = compute_project_hash(path);
        let h2 = compute_project_hash(path);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_is_16_hex_chars() {
        let path = Path::new("/some/path");
        let hash = compute_project_hash(path);
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_different_paths() {
        let h1 = compute_project_hash(Path::new("/path/a"));
        let h2 = compute_project_hash(Path::new("/path/b"));
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_lowercase_hex() {
        let hash = compute_project_hash(Path::new("/test"));
        assert!(hash.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn test_ensure_creates_dirs() {
        let project_dir = tempfile::TempDir::new().unwrap();
        let base_dir = tempfile::TempDir::new().unwrap();
        let paths =
            ensure_data_directory(Some(project_dir.path()), Some(base_dir.path())).unwrap();

        assert!(paths.data_dir.exists());
        assert!(paths.vector_dir.exists());
        assert!(paths.data_dir.starts_with(base_dir.path()));
        assert!(paths.db_path.parent().unwrap().exists());
        assert!(paths.db_path.to_string_lossy().ends_with("unimatrix.db"));
        assert!(paths.vector_dir.to_string_lossy().ends_with("vector"));
        assert!(paths.pid_path.to_string_lossy().ends_with("unimatrix.pid"));
        assert!(paths.socket_path.to_string_lossy().ends_with("unimatrix.sock"));
    }

    #[test]
    fn test_ensure_idempotent() {
        let project_dir = tempfile::TempDir::new().unwrap();
        let base_dir = tempfile::TempDir::new().unwrap();
        let paths1 =
            ensure_data_directory(Some(project_dir.path()), Some(base_dir.path())).unwrap();
        let paths2 =
            ensure_data_directory(Some(project_dir.path()), Some(base_dir.path())).unwrap();
        assert_eq!(paths1.project_hash, paths2.project_hash);
    }

    #[test]
    fn test_hash_unicode_path() {
        let hash = compute_project_hash(Path::new("/tmp/test-unicode-path"));
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_long_path() {
        let long_path = format!("/tmp/{}", "a".repeat(1000));
        let hash = compute_project_hash(Path::new(&long_path));
        assert_eq!(hash.len(), 16);
    }

    #[test]
    fn test_socket_path_in_data_dir() {
        let project_dir = tempfile::TempDir::new().unwrap();
        let base_dir = tempfile::TempDir::new().unwrap();
        let paths =
            ensure_data_directory(Some(project_dir.path()), Some(base_dir.path())).unwrap();
        assert_eq!(paths.socket_path, paths.data_dir.join("unimatrix.sock"));
    }

    #[test]
    fn test_detect_root_worktree_git_file() {
        // Simulate a worktree: main repo has .git dir, worktree has .git file
        let main_repo = tempfile::TempDir::new().unwrap();
        let git_dir = main_repo.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        let worktrees_dir = git_dir.join("worktrees").join("my-worktree");
        fs::create_dir_all(&worktrees_dir).unwrap();

        let worktree = tempfile::TempDir::new().unwrap();
        let gitdir_target = worktrees_dir.canonicalize().unwrap();
        fs::write(
            worktree.path().join(".git"),
            format!("gitdir: {}\n", gitdir_target.display()),
        )
        .unwrap();

        let result = detect_project_root(Some(worktree.path())).unwrap();
        assert_eq!(result, main_repo.path().canonicalize().unwrap());
    }

    #[test]
    fn test_worktree_same_hash_as_main_repo() {
        // A worktree and its main repo must produce the same project hash
        let main_repo = tempfile::TempDir::new().unwrap();
        let git_dir = main_repo.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        let worktrees_dir = git_dir.join("worktrees").join("feature-branch");
        fs::create_dir_all(&worktrees_dir).unwrap();

        let worktree = tempfile::TempDir::new().unwrap();
        let gitdir_target = worktrees_dir.canonicalize().unwrap();
        fs::write(
            worktree.path().join(".git"),
            format!("gitdir: {}\n", gitdir_target.display()),
        )
        .unwrap();

        let main_root = detect_project_root(Some(main_repo.path())).unwrap();
        let wt_root = detect_project_root(Some(worktree.path())).unwrap();
        assert_eq!(main_root, wt_root);

        let main_hash = compute_project_hash(&main_root);
        let wt_hash = compute_project_hash(&wt_root);
        assert_eq!(main_hash, wt_hash);
    }

    #[test]
    fn test_worktree_relative_gitdir() {
        // Worktree .git file can use a relative path
        let main_repo = tempfile::TempDir::new().unwrap();
        let git_dir = main_repo.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        let worktrees_dir = git_dir.join("worktrees").join("rel-wt");
        fs::create_dir_all(&worktrees_dir).unwrap();

        // Create worktree as a subdirectory of main repo (like .claude/worktrees/...)
        let worktree_dir = main_repo.path().join("worktrees").join("rel-wt");
        fs::create_dir_all(&worktree_dir).unwrap();
        // Relative path from worktree to main .git/worktrees/rel-wt
        fs::write(
            worktree_dir.join(".git"),
            "gitdir: ../../.git/worktrees/rel-wt\n",
        )
        .unwrap();

        let result = detect_project_root(Some(&worktree_dir)).unwrap();
        assert_eq!(result, main_repo.path().canonicalize().unwrap());
    }

    #[test]
    fn test_worktree_git_file_no_gitdir_line() {
        // A .git file without a gitdir line should fail with InvalidData
        let dir = tempfile::TempDir::new().unwrap();
        fs::write(dir.path().join(".git"), "something unexpected\n").unwrap();

        let result = detect_project_root(Some(dir.path()));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_worktree_ensure_data_dir_matches_main() {
        // ensure_data_directory from a worktree should produce the same paths
        // as from the main repo
        let main_repo = tempfile::TempDir::new().unwrap();
        let git_dir = main_repo.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        let worktrees_dir = git_dir.join("worktrees").join("data-test");
        fs::create_dir_all(&worktrees_dir).unwrap();

        let worktree = tempfile::TempDir::new().unwrap();
        let gitdir_target = worktrees_dir.canonicalize().unwrap();
        fs::write(
            worktree.path().join(".git"),
            format!("gitdir: {}\n", gitdir_target.display()),
        )
        .unwrap();

        let base_dir = tempfile::TempDir::new().unwrap();
        let main_paths =
            ensure_data_directory(Some(main_repo.path()), Some(base_dir.path())).unwrap();
        let wt_paths =
            ensure_data_directory(Some(worktree.path()), Some(base_dir.path())).unwrap();

        assert_eq!(main_paths.project_hash, wt_paths.project_hash);
        assert_eq!(main_paths.db_path, wt_paths.db_path);
        assert_eq!(main_paths.socket_path, wt_paths.socket_path);
    }

    #[test]
    fn test_ensure_no_dirs_leak_outside_base() {
        let project_dir = tempfile::TempDir::new().unwrap();
        let base_dir = tempfile::TempDir::new().unwrap();
        let paths =
            ensure_data_directory(Some(project_dir.path()), Some(base_dir.path())).unwrap();

        // All created directories must be inside base_dir, not ~/.unimatrix/
        assert!(paths.data_dir.starts_with(base_dir.path()));
        assert!(paths.vector_dir.starts_with(base_dir.path()));
        assert!(paths.db_path.starts_with(base_dir.path()));
        assert!(paths.pid_path.starts_with(base_dir.path()));
        assert!(paths.socket_path.starts_with(base_dir.path()));

        // The hash directory should NOT exist under ~/.unimatrix/
        let home = dirs::home_dir().unwrap();
        let leaked_dir = home.join(".unimatrix").join(&paths.project_hash);
        assert!(
            !leaked_dir.exists(),
            "directory leaked outside base_dir into {leaked_dir:?}"
        );
    }
}
