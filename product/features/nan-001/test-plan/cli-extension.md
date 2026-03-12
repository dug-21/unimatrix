# Test Plan: cli-extension

Component: CLI dispatch in `crates/unimatrix-server/src/main.rs`
Risks covered: R-10, R-15

## Unit Tests

None. The CLI extension is clap-derived declarative code. Testing is done at the integration level by invoking the binary.

## Integration Tests

All tests below are in `crates/unimatrix-server/tests/export_integration.rs` and invoke the binary via `std::process::Command` or call `export::run_export()` directly.

### T-CL-01: Export subcommand produces JSONL to stdout (AC-01, AC-12)

**Risks**: R-15 (wiring)
**Setup**: Create a temp database with at least one entry via `Store::open()` + insert.
**Action**: Invoke `unimatrix-server export --project-dir <temp>` (or call `run_export(Some(temp_dir), None)`).
**Assert**:
- Exit code is 0.
- Stdout contains at least 2 lines (header + at least one counter row).
- Each line parses as valid JSON.
- First line has `"_header": true`.
- No server process is running (AC-12 verified implicitly).

### T-CL-02: --output flag writes to file (AC-02)

**Risks**: R-10 (partial write)
**Setup**: Create a temp database with data. Create a temp output path.
**Action**: Invoke `run_export(Some(temp_dir), Some(output_path))`.
**Assert**:
- Exit code is 0.
- Output file exists and is non-empty.
- File content parses as valid JSONL (each line is valid JSON).
- First line has `"_header": true`.
- Content is identical to what stdout would produce (same database, same timestamp caveat).

### T-CL-03: --project-dir flag resolves to correct database (AC-13, R-15)

**Risks**: R-15 (--project-dir not wired)
**Setup**:
- Create temp dir A with a database containing entry titled "alpha".
- Create temp dir B with a database containing entry titled "beta".
**Action**: Call `run_export(Some(dir_a), None)` and `run_export(Some(dir_b), None)`.
**Assert**:
- Export from dir_a contains "alpha" in an entries row, does NOT contain "beta".
- Export from dir_b contains "beta" in an entries row, does NOT contain "alpha".
- This proves --project-dir is actually respected, not silently ignored.

### T-CL-04: Non-writable output path returns non-zero exit (AC-15, R-10)

**Risks**: R-10 (partial output on error)
**Setup**: Create a temp database. Set output path to a read-only directory or `/dev/null/../nonexistent`.
**Action**: Call `run_export(Some(temp_dir), Some(bad_path))`.
**Assert**:
- Returns `Err(...)`.
- Error message mentions the path or I/O failure.
- No partial file left at the path (or if it exists, the caller knows it is invalid).

### T-CL-05: Database open failure returns non-zero exit (AC-15)

**Risks**: R-10 (error handling)
**Setup**: Point project_dir to a non-existent directory (or a directory with no database file -- depends on how `ensure_data_directory` handles this).
**Action**: Call `run_export(Some(nonexistent_dir), None)`.
**Assert**:
- Returns `Err(...)`.
- Error message references the database or path.

## Edge Cases

### T-CL-06: Export with no subcommand arguments (defaults)

**Setup**: Create a temp database.
**Action**: Call `run_export(None, None)` (both defaults).
**Assert**:
- Either succeeds using the default project directory, or returns a clear error if no default project exists.
- Does not panic.
