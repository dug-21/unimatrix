//! Project root detection, hash computation, and data directory management.

use std::fs;
use std::io;
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
    /// Database path: ~/.unimatrix/{hash}/unimatrix.redb
    pub db_path: PathBuf,
    /// Vector index directory: ~/.unimatrix/{hash}/vector/
    pub vector_dir: PathBuf,
    /// PID file path: ~/.unimatrix/{hash}/unimatrix.pid
    pub pid_path: PathBuf,
    /// Socket file path: ~/.unimatrix/{hash}/unimatrix.sock
    pub socket_path: PathBuf,
}

/// Detect the project root by walking up from cwd looking for `.git/`.
///
/// If `override_dir` is provided, it is used directly (canonicalized).
/// Otherwise, walks up from the current working directory. If no `.git/`
/// directory is found, the current working directory is used.
pub fn detect_project_root(override_dir: Option<&Path>) -> io::Result<PathBuf> {
    if let Some(dir) = override_dir {
        return dir.canonicalize();
    }

    let start = std::env::current_dir()?;
    let mut current = start.as_path();

    loop {
        if current.join(".git").is_dir() {
            return current.to_path_buf().canonicalize();
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }

    // No .git found — use original cwd
    start.canonicalize()
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
/// Returns `ProjectPaths` with all resolved paths.
pub fn ensure_data_directory(override_dir: Option<&Path>) -> io::Result<ProjectPaths> {
    let project_root = detect_project_root(override_dir)?;
    let project_hash = compute_project_hash(&project_root);

    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory not found"))?;

    let data_dir = home.join(".unimatrix").join(&project_hash);
    let db_path = data_dir.join("unimatrix.redb");
    let vector_dir = data_dir.join("vector");
    let pid_path = data_dir.join("unimatrix.pid");
    let socket_path = data_dir.join("unimatrix.sock");

    fs::create_dir_all(&data_dir)?;
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
        let dir = tempfile::TempDir::new().unwrap();
        let paths = ensure_data_directory(Some(dir.path())).unwrap();

        assert!(paths.data_dir.exists());
        assert!(paths.vector_dir.exists());
        assert!(paths.db_path.parent().unwrap().exists());
        assert!(paths.db_path.to_string_lossy().ends_with("unimatrix.redb"));
        assert!(paths.vector_dir.to_string_lossy().ends_with("vector"));
        assert!(paths.pid_path.to_string_lossy().ends_with("unimatrix.pid"));
        assert!(paths.socket_path.to_string_lossy().ends_with("unimatrix.sock"));
    }

    #[test]
    fn test_ensure_idempotent() {
        let dir = tempfile::TempDir::new().unwrap();
        let paths1 = ensure_data_directory(Some(dir.path())).unwrap();
        let paths2 = ensure_data_directory(Some(dir.path())).unwrap();
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
        let dir = tempfile::TempDir::new().unwrap();
        let paths = ensure_data_directory(Some(dir.path())).unwrap();
        assert_eq!(paths.socket_path, paths.data_dir.join("unimatrix.sock"));
    }
}
