# Test Plan: engine-extraction

## Risks Covered

| Risk | Severity | Test Coverage |
|------|----------|--------------|
| R-01 | Critical | Existing 1199 tests as regression gate after each extraction step |
| R-02 | Critical | Verify no stale source files remain; re-export path identity |
| R-21 | Medium | ProjectPaths extension (socket_path field) |

## Unit Tests

### ProjectPaths Extension (R-21)

Location: `crates/unimatrix-engine/src/project.rs` (within `#[cfg(test)]` module)

1. **test_ensure_creates_dirs_with_socket_path**: After `ensure_data_directory`, verify `paths.socket_path` ends with `"unimatrix.sock"` and is within `paths.data_dir`.

2. **test_socket_path_deterministic**: Call `ensure_data_directory` twice with the same project dir. Assert `socket_path` is identical both times.

3. **test_socket_path_in_data_dir**: Verify `paths.socket_path == paths.data_dir.join("unimatrix.sock")`.

## Regression Tests

### Engine Extraction Verification (R-01, R-02)

These are NOT new tests. They are verification steps using the existing test suite:

1. **After project.rs extraction**: `cargo test --workspace` -- all 1199 tests pass. No test modifications.

2. **After confidence.rs + coaccess.rs extraction**: `cargo test --workspace` -- all 1199 tests pass. No test modifications.

3. **Re-export verification**: After extraction, the following import paths must compile and resolve correctly:
   - `unimatrix_server::confidence::compute_confidence`
   - `unimatrix_server::coaccess::compute_search_boost`
   - `unimatrix_server::project::ensure_data_directory`
   - `unimatrix_engine::confidence::compute_confidence`
   - `unimatrix_engine::coaccess::compute_search_boost`
   - `unimatrix_engine::project::ensure_data_directory`

4. **No stale source files** (R-02): After extraction, verify via shell check:
   ```
   test ! -f crates/unimatrix-server/src/confidence.rs
   test ! -f crates/unimatrix-server/src/coaccess.rs
   test ! -f crates/unimatrix-server/src/project.rs
   ```

## Integration Test Coverage

No NEW integration tests for engine-extraction. The existing 174 integration tests serve as the regression gate. If any integration test requires modification during extraction, that modification must be reviewed as a potential behavior change.

## Edge Cases

- `ensure_data_directory` with a path that produces a very long socket path (>108 chars, Unix socket limit): Document behavior (bind will fail with ENAMETOOLONG at UDS listener startup, not at ProjectPaths creation).

## Assertions

- `paths.socket_path.ends_with("unimatrix.sock")` -- correct filename
- `paths.socket_path.parent() == Some(&paths.data_dir)` -- correct directory
- `paths.socket_path.to_string_lossy().contains(&paths.project_hash)` -- hash in path
