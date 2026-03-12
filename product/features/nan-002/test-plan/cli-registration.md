# nan-002: Test Plan -- cli-registration

## Component Scope

CLI argument parsing and dispatch in `crates/unimatrix-server/src/main.rs`. The `Command::Import` variant with `--input`, `--skip-hash-validation`, and `--force` arguments.

## Unit Tests

### test_import_command_parse_required_args
- Parse `["import", "--input", "backup.jsonl"]`
- Assert `Command::Import` with `input == PathBuf::from("backup.jsonl")`, `skip_hash_validation == false`, `force == false`

### test_import_command_parse_all_flags
- Parse `["import", "--input", "backup.jsonl", "--skip-hash-validation", "--force"]`
- Assert all three flags set correctly

### test_import_command_parse_short_input
- Parse `["import", "-i", "backup.jsonl"]`
- Assert `input == PathBuf::from("backup.jsonl")`

### test_import_command_missing_input_errors
- Parse `["import"]` without `--input`
- Assert clap returns an error (required argument missing)

### test_import_command_with_project_dir
- Parse `["--project-dir", "/tmp/proj", "import", "--input", "backup.jsonl"]`
- Assert `project_dir == Some(PathBuf::from("/tmp/proj"))` and `Command::Import` with correct input

## Integration Tests

### test_import_dispatches_to_run_import (AC-01)
- Create a valid JSONL export file (header + counters)
- Execute the binary with `import --input <path>`
- Assert exit code 0
- Risks: R-14

### test_import_project_dir_resolution (AC-19)
- Create two temp directories: dir_a, dir_b
- Export from dir_a, import into dir_b using `--project-dir dir_b`
- Verify database exists in dir_b, not in default location
- Risks: R-14

### test_import_exit_codes (AC-20)
- Success case: valid import -> exit code 0
- Bad header: format_version 99 -> exit code non-zero
- Non-empty DB without --force -> exit code non-zero
- Malformed JSONL -> exit code non-zero
- Assert at least 3 distinct failure cases produce non-zero exit

## Risk Coverage

| Risk | Tests | Coverage |
|------|-------|----------|
| R-04 (--force safety) | test_import_command_parse_all_flags, integration tests in import-pipeline.md | Partial (CLI parsing only; behavior tested in import-pipeline) |
| R-14 (--project-dir mismatch) | test_import_command_with_project_dir, test_import_project_dir_resolution | Full |
