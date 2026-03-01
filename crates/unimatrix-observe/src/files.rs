//! Session file discovery, age computation, cleanup identification, and stats.

use std::path::{Path, PathBuf};

use crate::error::{ObserveError, Result};
use crate::types::{ObservationStats, SessionFile};

/// Default observation directory path (unexpanded).
pub const DEFAULT_OBSERVATION_DIR: &str = "~/.unimatrix/observation";

/// Return the observation directory path, expanding ~ to $HOME.
pub fn observation_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home)
            .join(".unimatrix")
            .join("observation")
    } else {
        PathBuf::from(DEFAULT_OBSERVATION_DIR)
    }
}

/// Discover session files in the observation directory.
///
/// Returns SessionFile metadata sorted by modified_at (oldest first).
/// Missing directory returns empty vec (not an error).
pub fn discover_sessions(dir: &Path) -> Result<Vec<SessionFile>> {
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut sessions = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only .jsonl files
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }

        let metadata = entry.metadata()?;
        let modified_at = metadata
            .modified()
            .map_err(ObserveError::Io)?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        sessions.push(SessionFile {
            path,
            session_id,
            size_bytes: metadata.len(),
            modified_at,
        });
    }

    // Sort by modified_at (oldest first)
    sessions.sort_by_key(|s| s.modified_at);

    Ok(sessions)
}

/// Identify session files that have exceeded their max age.
///
/// Files with age >= max_age_secs are considered expired.
pub fn identify_expired(dir: &Path, max_age_secs: u64) -> Result<Vec<PathBuf>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let sessions = discover_sessions(dir)?;

    let expired: Vec<PathBuf> = sessions
        .iter()
        .filter(|s| now.saturating_sub(s.modified_at) >= max_age_secs)
        .map(|s| s.path.clone())
        .collect();

    Ok(expired)
}

/// Scan observation directory and return aggregate stats.
pub fn scan_observation_stats(dir: &Path) -> Result<ObservationStats> {
    let sessions = discover_sessions(dir)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let file_count = sessions.len() as u64;
    let total_size_bytes: u64 = sessions.iter().map(|s| s.size_bytes).sum();

    let oldest_file_age_days = sessions
        .iter()
        .map(|s| now.saturating_sub(s.modified_at) / 86400)
        .max()
        .unwrap_or(0);

    // Files approaching 60-day cleanup (>= 45 days old, < 60 days)
    let approaching_cleanup: Vec<String> = sessions
        .iter()
        .filter(|s| {
            let age_days = now.saturating_sub(s.modified_at) / 86400;
            age_days >= 45 && age_days < 60
        })
        .map(|s| s.session_id.clone())
        .collect();

    Ok(ObservationStats {
        file_count,
        total_size_bytes,
        oldest_file_age_days,
        approaching_cleanup,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observation_dir_expands_home() {
        let dir = observation_dir();
        // Should not start with ~ if HOME is set
        let dir_str = dir.to_string_lossy();
        if std::env::var_os("HOME").is_some() {
            assert!(
                !dir_str.starts_with('~'),
                "should expand ~: {dir_str}"
            );
            assert!(dir_str.ends_with(".unimatrix/observation"));
        }
    }

    #[test]
    fn test_discover_sessions_in_dir_with_files() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("sess-1.jsonl"), "line1\n").unwrap();
        std::fs::write(dir.path().join("sess-2.jsonl"), "line2\n").unwrap();
        std::fs::write(dir.path().join("sess-3.jsonl"), "line3\n").unwrap();

        let sessions = discover_sessions(dir.path()).unwrap();
        assert_eq!(sessions.len(), 3);
    }

    #[test]
    fn test_discover_sessions_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let sessions = discover_sessions(dir.path()).unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_discover_sessions_nonexistent_dir() {
        let sessions = discover_sessions(Path::new("/nonexistent/path")).unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_discover_sessions_non_jsonl_ignored() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("sess-1.jsonl"), "data\n").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "text\n").unwrap();
        std::fs::write(dir.path().join("data.json"), "{}").unwrap();

        let sessions = discover_sessions(dir.path()).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "sess-1");
    }

    #[test]
    fn test_discover_sessions_metadata() {
        let dir = tempfile::TempDir::new().unwrap();
        let content = "line1\nline2\nline3\n";
        std::fs::write(dir.path().join("test-session.jsonl"), content).unwrap();

        let sessions = discover_sessions(dir.path()).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "test-session");
        assert_eq!(sessions[0].size_bytes, content.len() as u64);
        assert!(sessions[0].modified_at > 0);
    }

    #[test]
    fn test_identify_expired_none() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("recent.jsonl"), "data\n").unwrap();

        // Use a very large max_age so nothing is expired
        let expired = identify_expired(dir.path(), 999_999_999).unwrap();
        assert!(expired.is_empty());
    }

    #[test]
    fn test_identify_expired_all() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("old.jsonl"), "data\n").unwrap();

        // Use max_age of 0 seconds so everything is expired
        let expired = identify_expired(dir.path(), 0).unwrap();
        assert_eq!(expired.len(), 1);
    }

    #[test]
    fn test_identify_expired_nonexistent_dir() {
        let expired = identify_expired(Path::new("/nonexistent"), 86400).unwrap();
        assert!(expired.is_empty());
    }

    #[test]
    fn test_scan_observation_stats_with_files() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("s1.jsonl"), "data1\n").unwrap();
        std::fs::write(dir.path().join("s2.jsonl"), "data22\n").unwrap();

        let stats = scan_observation_stats(dir.path()).unwrap();
        assert_eq!(stats.file_count, 2);
        assert_eq!(stats.total_size_bytes, 6 + 7); // "data1\n" + "data22\n"
    }

    #[test]
    fn test_scan_observation_stats_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let stats = scan_observation_stats(dir.path()).unwrap();
        assert_eq!(stats.file_count, 0);
        assert_eq!(stats.total_size_bytes, 0);
        assert_eq!(stats.oldest_file_age_days, 0);
        assert!(stats.approaching_cleanup.is_empty());
    }

    #[test]
    fn test_scan_observation_stats_nonexistent_dir() {
        let stats = scan_observation_stats(Path::new("/nonexistent")).unwrap();
        assert_eq!(stats.file_count, 0);
    }
}
