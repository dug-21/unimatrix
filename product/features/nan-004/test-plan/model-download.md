# Test Plan: C8 — Model Download Subcommand

## Rust Unit Tests

### Subcommand Behavior

- `test_model_download_calls_ensure_model`: Mock or instrument `ensure_model()`. Assert `handle_model_download()` invokes it. Assert return is `Ok(())` on success.
- `test_model_download_propagates_error`: If `ensure_model()` returns an error, assert `handle_model_download()` returns `Err` (exit code 1).
- `test_model_download_runs_synchronously`: Assert the function does NOT spawn a tokio runtime. It runs in the sync CLI path alongside `hook`, `export`, `import`, `version`.

### Output

- `test_model_download_prints_to_stderr`: Assert progress/status messages go to stderr, not stdout. (Postinstall captures stdout; stderr is for diagnostics.)

## Integration Tests

No MCP integration test needed. The `model-download` subcommand is a standalone CLI operation with no MCP protocol involvement.

### Shell Integration

- `test_model_download_succeeds_with_network`: Run `./target/release/unimatrix model-download`. Assert exit code 0. Assert model files exist in `~/.cache/unimatrix-embed/`.
- `test_model_download_idempotent`: Run twice. Assert second run exits 0 quickly (model already cached).

## Risk Coverage

| Risk ID | Scenario | Test |
|---------|----------|------|
| R-08 | Model download failure handling | `test_model_download_propagates_error` (postinstall catches this) |
