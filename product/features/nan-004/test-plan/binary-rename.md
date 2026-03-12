# Test Plan: C7 — Binary Rename

## Rust Unit Tests

### CLI Parsing

- `test_no_subcommand_defaults_to_server_mode`: Parse `Cli` from empty args. Assert `command` is `None` (MCP server mode).
- `test_hook_subcommand_parsed`: Parse from `["hook", "SessionStart"]`. Assert `Command::Hook { event: "SessionStart" }`.
- `test_version_subcommand_parsed`: Parse from `["version"]`. Assert `Command::Version`.
- `test_model_download_subcommand_parsed`: Parse from `["model-download"]`. Assert `Command::ModelDownload`.
- `test_export_subcommand_unchanged`: Parse from `["export", "--output", "/tmp/out.json"]`. Assert `Command::Export` with correct output path.
- `test_import_subcommand_unchanged`: Parse from `["import", "--input", "/tmp/in.json"]`. Assert `Command::Import` with correct input path.
- `test_project_dir_flag_accepted`: Parse from `["--project-dir", "/some/path", "version"]`. Assert `cli.project_dir == Some(PathBuf::from("/some/path"))`.
- `test_verbose_flag_accepted`: Parse from `["-v", "version"]`. Assert `cli.verbose == true`.

### Binary Name

- `test_binary_name_is_unimatrix`: Assert `Cli::command().get_name() == "unimatrix"` (clap metadata).

### Version Output

- `test_handle_version_prints_version`: Call `handle_version()`. Assert stdout contains `unimatrix` and the `CARGO_PKG_VERSION`.

## Integration Tests (infra-001)

The binary rename changes the executable name. The integration harness `get_binary_path()` must be updated to look for `unimatrix` instead of `unimatrix-server`. After that update:

- Run `smoke` suite to confirm MCP handshake works with renamed binary.
- Run `protocol` suite to confirm tool discovery returns all 9/10 tools.
- Run `tools` suite to confirm all tools function identically.

No new integration tests needed -- existing tests validate that the rename is transparent to the MCP protocol.

## Existing Tests

All existing `cargo test --workspace` tests must pass without modification (the binary rename does not change any library crate behavior). Only tests that reference the binary name directly may need updates.

## Risk Coverage

| Risk ID | Scenario | Test |
|---------|----------|------|
| R-12 | Binary rename breaks hook configs | `test_binary_name_is_unimatrix` + repo `.mcp.json`/`.claude/settings.json` updated |
| R-12 | Existing subcommands broken | `test_hook_subcommand_parsed`, `test_export_subcommand_unchanged`, `test_import_subcommand_unchanged` |
