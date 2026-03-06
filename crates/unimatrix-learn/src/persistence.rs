//! Atomic file persistence helpers.
//!
//! Provides save/load with atomic write (temp + rename) for crash safety.

use std::fs;
use std::path::Path;

/// Save data to a file atomically using temp-file + rename.
pub fn save_atomic(data: &[u8], dir: &Path, filename: &str) -> Result<(), String> {
    let target = dir.join(filename);
    let tmp_path = dir.join(format!("{filename}.tmp"));

    fs::create_dir_all(dir).map_err(|e| format!("create_dir_all failed: {e}"))?;
    fs::write(&tmp_path, data).map_err(|e| format!("write failed: {e}"))?;
    fs::rename(&tmp_path, &target).map_err(|e| format!("rename failed: {e}"))?;

    Ok(())
}

/// Load data from a file. Returns `None` if the file is missing or empty.
pub fn load_file(dir: &Path, filename: &str) -> Result<Option<Vec<u8>>, String> {
    let path = dir.join(filename);

    if !path.exists() {
        return Ok(None);
    }

    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("Failed to read {}: {e}", path.display());
            return Ok(None);
        }
    };

    if bytes.is_empty() {
        tracing::warn!("{} is empty", path.display());
        return Ok(None);
    }

    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-LC-06: save_atomic and load_file roundtrip
    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::TempDir::new().expect("tmpdir");
        save_atomic(b"test data", dir.path(), "test.bin").expect("save");
        let loaded = load_file(dir.path(), "test.bin").expect("load");
        assert_eq!(loaded, Some(b"test data".to_vec()));

        // No .tmp file remaining
        assert!(!dir.path().join("test.bin.tmp").exists());
    }

    // T-LC-07: load_file missing file returns None
    #[test]
    fn load_missing_file() {
        let dir = tempfile::TempDir::new().expect("tmpdir");
        let loaded = load_file(dir.path(), "nonexistent.bin").expect("load");
        assert!(loaded.is_none());
    }

    #[test]
    fn load_empty_file() {
        let dir = tempfile::TempDir::new().expect("tmpdir");
        fs::write(dir.path().join("empty.bin"), b"").expect("write");
        let loaded = load_file(dir.path(), "empty.bin").expect("load");
        assert!(loaded.is_none());
    }
}
