# Test Plan: cli-subcommands

## Verification Approach

The CLI subcommands are thin wrappers around the migrate module functions. The heavy testing is in the migrate-module test plan. CLI-level testing focuses on argument parsing and error handling.

## Test Cases

### T-15: PID file prevents concurrent export (R-08)
- Export subcommand checks PID file before proceeding
- If a running unimatrix process is detected, export aborts with clear error
- If PID file is stale (non-unimatrix process), export proceeds

### Build verification (T-06 partial)
- `cargo build -p unimatrix-server` (default = backend-sqlite): Import subcommand is available, Export is not
- `cargo build -p unimatrix-server --no-default-features --features mcp-briefing`: Export subcommand is available, Import is not

### Import refuses overwrite (AC-09)
- Call import with `--output` pointing to an existing file
- Verify error message and non-zero exit

### Import validates input exists
- Call import with `--input` pointing to a non-existent file
- Verify error message and non-zero exit

### Export validates database exists
- Call export with `--db-path` pointing to a non-existent file
- Verify error message and non-zero exit

## Notes

Most CLI tests are effectively integration tests of the migrate module. The subcommand layer adds minimal logic (path resolution, PID check, error formatting). The server's existing test infrastructure does not include CLI integration tests (the Hook subcommand is also tested via the UDS hook module, not CLI).

For nxs-006, the primary verification method for CLI behavior is manual testing during Wave 4 (production migration). The automated tests focus on the migrate module's correctness.
