# Agent Report: nan-004-agent-3-binary-rename

## Component
C7: Binary Rename

## Status
COMPLETE

## Files Modified
1. `crates/unimatrix-server/Cargo.toml` -- `[[bin]] name` changed from `unimatrix-server` to `unimatrix`
2. `crates/unimatrix-server/src/main.rs` -- CLI name/about updated, doc comments updated, `handle_version()` enhanced with `--project-dir` DB pre-creation, `Debug` derive added to `Command` enum, 10 unit tests added
3. `crates/unimatrix-server/src/infra/pidfile.rs` -- `is_unimatrix_process()` now recognizes both `unimatrix` and `unimatrix-server` binary names, comments updated
4. `crates/unimatrix-server/src/error.rs` -- Error message updated from `unimatrix-server` to `unimatrix`
5. `crates/unimatrix-server/src/import/mod.rs` -- Error message updated from `unimatrix-server` to `unimatrix`
6. `.mcp.json` -- Binary path updated from `unimatrix-server` to `unimatrix`
7. `.claude/settings.json` -- All 7 hook commands updated from `unimatrix-server hook` to `unimatrix hook`; UserPromptSubmit tee pipeline dropped per pseudocode

## Tests
- 10 passed, 0 failed (binary unit tests)
- Full workspace: all pass (1 pre-existing flaky HNSW test in unimatrix-vector intermittently fails, unrelated)

### Test Cases Added
1. `test_binary_name_is_unimatrix` -- clap metadata
2. `test_no_subcommand_defaults_to_server_mode` -- None = MCP server
3. `test_hook_subcommand_parsed` -- Hook variant
4. `test_version_subcommand_parsed` -- Version variant
5. `test_model_download_subcommand_parsed` -- ModelDownload variant
6. `test_export_subcommand_unchanged` -- Export backward compat
7. `test_import_subcommand_unchanged` -- Import backward compat
8. `test_project_dir_flag_accepted` -- --project-dir flag
9. `test_verbose_flag_accepted` -- -v flag
10. `test_handle_version_prints_version` -- handle_version(None) returns Ok

## Notes
- The C9 (version sync) and C8 (model download) agents had already added the `Version`/`ModelDownload` enum variants, match arms, and handler functions before this agent ran. This agent focused on the binary rename itself, `handle_version` enhancement with `--project-dir` support, config file updates, pidfile process detection update, and unit tests.
- `handle_version(project_dir)` now accepts `Option<PathBuf>` and when `--project-dir` is provided, calls `ensure_data_directory` + `Store::open` to pre-create the DB (used by `npx unimatrix init`).
- `is_unimatrix_process()` in pidfile.rs recognizes both `unimatrix` and `unimatrix-server` so it can detect stale processes from either binary name.

## Issues
None.

## Knowledge Stewardship
- Queried: no /query-patterns call (knowledge server unavailable in agent context)
- Stored: nothing novel to store -- the binary rename is a straightforward mechanical change with no gotchas beyond updating pidfile detection to match both names
