# Pseudocode: observe-files

## Purpose

Session file discovery, age computation, cleanup identification, and aggregate stats.

## File: `crates/unimatrix-observe/src/files.rs`

### Constant: OBSERVATION_DIR (ADR-004)

```
/// Default observation directory path.
/// Functions accept &Path for testability.
pub const DEFAULT_OBSERVATION_DIR: &str = "~/.unimatrix/observation";

/// Expand ~ to home directory
pub fn observation_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".unimatrix").join("observation")
    } else {
        PathBuf::from(DEFAULT_OBSERVATION_DIR)
    }
}
```

### discover_sessions

```
pub fn discover_sessions(dir: &Path) -> Result<Vec<SessionFile>> {
    if !dir.exists() {
        return Ok(vec![]);  // Missing dir -> empty, not error
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
        let modified_at = metadata.modified()
            .map_err(|e| ObserveError::Io(e))?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Session ID = filename without extension
        let session_id = path.file_stem()
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
```

### identify_expired

```
pub fn identify_expired(dir: &Path, max_age_secs: u64) -> Result<Vec<PathBuf>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let sessions = discover_sessions(dir)?;

    let expired: Vec<PathBuf> = sessions.iter()
        .filter(|s| now.saturating_sub(s.modified_at) >= max_age_secs)
        .map(|s| s.path.clone())
        .collect();

    Ok(expired)
}
```

### scan_observation_stats

```
pub fn scan_observation_stats(dir: &Path) -> Result<ObservationStats> {
    let sessions = discover_sessions(dir)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let file_count = sessions.len() as u64;
    let total_size_bytes: u64 = sessions.iter().map(|s| s.size_bytes).sum();

    let oldest_file_age_days = sessions.iter()
        .map(|s| now.saturating_sub(s.modified_at) / 86400)
        .max()
        .unwrap_or(0);

    // Files approaching 60-day cleanup (>= 45 days old)
    let approaching_cleanup: Vec<String> = sessions.iter()
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
```

## Error Handling

- Missing directory: return empty results, not error
- Unreadable files: propagate Io error
- Files with no modified time: use duration 0

## Key Test Scenarios

- Discover sessions in dir with 3 .jsonl files -> 3 SessionFiles
- Discover sessions in empty dir -> empty vec
- Discover sessions in nonexistent dir -> empty vec (not error)
- Non-.jsonl files ignored
- identify_expired: files at exactly 60 days -> included (R-08 scenario 1)
- identify_expired: files at 59 days -> excluded (R-08 scenario 2)
- identify_expired: files at 61 days -> included (R-08 scenario 3)
- scan_observation_stats: correct file_count, total_size, oldest_age
- scan_observation_stats: approaching_cleanup includes 45-59 day files (AC-35)
- scan_observation_stats: empty dir -> all zeros
