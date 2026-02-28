# Pseudocode: error-path

## Purpose

Add DatabaseLocked error variant to ServerError and replace process::exit(1) in open_store_with_retry with proper error propagation.

## File: crates/unimatrix-server/src/error.rs

### New variant in ServerError enum

```
pub enum ServerError {
    // ... existing variants ...

    /// Database is locked by another process after exhausting retries.
    DatabaseLocked(PathBuf),
}
```

Note: Requires `use std::path::PathBuf;` import at top of file.

### Display impl addition

Add arm to existing Display match:

```
ServerError::DatabaseLocked(path) =>
    write!(f, "database is locked by another process: {}", path.display())
```

### ErrorData From<ServerError> impl addition

Add arm to existing match (use ERROR_INTERNAL code):

```
ServerError::DatabaseLocked(path) => ErrorData::new(
    ERROR_INTERNAL,
    format!(
        "Database is locked by another process at {}. Kill the other unimatrix-server process, or run: lsof {}",
        path.display(),
        path.display()
    ),
    None,
)
```

### std::error::Error source() — no change needed

DatabaseLocked has no inner error, so the existing `_ => None` arm covers it.

## File: crates/unimatrix-server/src/main.rs

### Modified function: open_store_with_retry

Change the `process::exit(1)` branch to return a proper error:

```
fn open_store_with_retry(db_path: &Path) -> Result<Arc<Store>, Box<dyn std::error::Error>>:
    for attempt in 1..=DB_OPEN_MAX_ATTEMPTS:
        match Store::open(db_path):
            Ok(s) => return Ok(Arc::new(s)),
            Err(StoreError::Database(redb::DatabaseError::DatabaseAlreadyOpen)):
                if attempt < DB_OPEN_MAX_ATTEMPTS:
                    tracing::warn!(
                        attempt,
                        max_attempts = DB_OPEN_MAX_ATTEMPTS,
                        "database locked by another process, retrying in {}s",
                        DB_OPEN_RETRY_DELAY.as_secs()
                    )
                    std::thread::sleep(DB_OPEN_RETRY_DELAY)
                else:
                    // CHANGED: return error instead of process::exit(1)
                    return Err(ServerError::DatabaseLocked(db_path.to_path_buf()).into())
            Err(e) => return Err(ServerError::Core(CoreError::Store(e)).into()),

    unreachable!()
```

### Remove import

Remove `use std::process` if it was only used for `process::exit(1)`. Check for other usages first (std::process::id is used in pidfile.rs, not main.rs).

## Error Handling

- DatabaseLocked propagates up to main() which returns Result<(), Box<dyn std::error::Error>>
- This means PidGuard::drop will run (if PidGuard was acquired before the error)
- The error message includes the path and actionable hint (same info as the old eprintln)

## Key Test Scenarios

1. DatabaseLocked Display includes the path
2. DatabaseLocked converts to ErrorData with ERROR_INTERNAL code
3. DatabaseLocked ErrorData message includes the path and "lsof" hint
4. No process::exit calls remain in server crate (grep verification)
