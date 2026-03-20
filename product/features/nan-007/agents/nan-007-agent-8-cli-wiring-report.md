# Agent Report: nan-007-agent-8-cli-wiring

## Task

Wire CLI in `src/main.rs` and complete `src/eval/mod.rs` dispatch per ADR-005.

## Files Modified

- `crates/unimatrix-server/src/eval/mod.rs` — added `run_eval_command()` dispatcher; updated module doc; switched `std::path::PathBuf` imports to use-level import
- `crates/unimatrix-server/src/main.rs` — added `Command::Snapshot { out }` and `Command::Eval { command: EvalCommand }` variants with full doc comments (NFR-07 content-sensitivity warning on Snapshot); added pre-tokio dispatch arms for both
- `crates/unimatrix-server/src/main_tests.rs` — added 13 new CLI wiring tests per the test plan

## What Was Already Done

All components were already implemented by other agents before this agent ran:

- `eval/mod.rs`: `EvalCommand` enum with all three variants was already present; only `run_eval_command()` was missing
- `eval/scenarios.rs`, `eval/runner.rs`, `eval/report/mod.rs`: fully implemented
- `snapshot.rs`: fully implemented
- `lib.rs`: already had `pub mod eval` and `pub mod snapshot`
- `Cargo.toml`: already had `unimatrix-engine` with `features = ["test-support"]` and the required comment (ADR-003)

## Implementation Notes

### run_eval_command dispatch

The `--configs` comma-split in `EvalCommand::Run` filters empty strings via `.filter(|p| !p.as_os_str().is_empty())` so that a trailing comma or whitespace-only entry after split does not produce phantom paths. An empty `config_paths` vec after filtering returns `Err` with a user-readable message before any I/O.

`_project_dir` is accepted by `run_eval_command` to match the integration surface defined in the architecture, though the current dispatch does not use it (the internal functions resolve the project dir themselves).

### Dispatch placement in main()

`Command::Snapshot` and `Command::Eval` arms are placed between `Command::Stop` and `Command::Serve` — in the sync block before the tokio runtime, consistent with C-10 ordering.

### Test for render_long_help

The `test_snapshot_help_includes_content_sensitivity_warning` test uses `render_long_help()` (not `render_help()`). The content-sensitivity warning is in the second paragraph of the `Command::Snapshot` doc comment; clap's `render_help()` only returns the first paragraph. `render_long_help()` returns the full doc comment, matching what `unimatrix snapshot --help` shows.

## Tests

- 42 total binary tests (29 pre-existing + 13 new)
- 13 new tests pass: all CLI parsing, help visibility, content-sensitivity warning, sync dispatch, --configs validation

## Build

```
cargo build --workspace  →  Finished, 0 errors
cargo test --workspace --lib --bins  →  all pass, 0 failures
```

Pre-existing failure: doc test in `infra/config.rs` (line 21) — a `~` in a doc comment interpreted as a code block. Pre-dates this feature; confirmed by stash + re-test.

## Knowledge Stewardship

- Queried: /uni-query-patterns for `clap CLI subcommand patterns` (category: pattern) — found #1952 (clap top-level flags must precede subcommand in spawned args) and #1191 (sync CLI subcommand pattern). Both confirmed the pre-tokio dispatch pattern already in use.
- Queried: /uni-query-patterns for `nan-007 architectural decisions` (category: decision, topic: nan-007) — found ADR-001 through ADR-005; all followed exactly.
- Stored: entry #2617 "Use render_long_help() not render_help() when asserting on multi-paragraph clap doc comment content in tests" via /uni-store-pattern. Gotcha: `render_help()` silently omits content from second+ paragraphs in clap doc comments, causing tests that assert on warning text to give false negatives.
