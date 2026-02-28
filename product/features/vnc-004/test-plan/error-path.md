# Test Plan: error-path

## Unit Tests

Tests in `crates/unimatrix-server/src/error.rs` module `tests`.

### test_database_locked_display

- Arrange: Create ServerError::DatabaseLocked(PathBuf::from("/tmp/test.redb"))
- Act: format!("{err}")
- Assert: Display output contains "locked" and "/tmp/test.redb"
- Assert: Display output does NOT contain "ServerError" or Rust type names
- Risks: R-05

### test_database_locked_error_data_code

- Arrange: Create ServerError::DatabaseLocked(PathBuf::from("/data/unimatrix.redb"))
- Act: Convert to ErrorData via .into()
- Assert: ErrorData code is ERROR_INTERNAL (-32603)
- Risks: R-05

### test_database_locked_error_data_message

- Arrange: Create ServerError::DatabaseLocked(PathBuf::from("/data/unimatrix.redb"))
- Act: Convert to ErrorData via .into()
- Assert: ErrorData message contains the path "/data/unimatrix.redb"
- Assert: ErrorData message contains "lsof" hint
- Risks: R-05

### test_no_process_exit_in_crate (AC-01 verification)

- This is a grep-based verification, not a unit test
- Run in Stage 3c: `grep -r "process::exit" crates/unimatrix-server/src/`
- Assert: No matches found
- Risks: AC-01

## Existing Tests

All existing error.rs tests remain valid and unchanged. The new DatabaseLocked variant does not affect any existing match arms because the From<ServerError> for ErrorData implementation is exhaustive.
