# cli-extension: Command Enum Extension

## Purpose

Add an `Export` variant to the `Command` enum in `main.rs` and a match arm that dispatches to `export::run_export()`. This follows the existing `Hook` subcommand pattern: synchronous execution, no tokio runtime.

## Files Modified

- `crates/unimatrix-server/src/main.rs`

## Changes to Command Enum

```
// Add to the existing Command enum (currently has Hook only)
enum Command {
    Hook { event: String },

    // NEW:
    /// Export the knowledge base to JSONL format.
    Export {
        /// Output file path. Defaults to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}
```

Source: Architecture Integration Surface -- `Command::Export` variant.

## Changes to main() Match

The existing match in `main()` handles `Some(Command::Hook { .. })` and `None`. Add a new arm for `Export`:

```
match cli.command {
    Some(Command::Hook { event }) => {
        // existing hook dispatch (unchanged)
        unimatrix_server::uds::hook::run(event, cli.project_dir)
    }

    // NEW:
    Some(Command::Export { output }) => {
        // Sync path: NO tokio, like Hook
        unimatrix_server::export::run_export(
            cli.project_dir.as_deref(),
            output.as_deref(),
        )
    }

    None => {
        // Async path: full server with tokio runtime (unchanged)
        tokio_main(cli)
    }
}
```

Key points:
- `cli.project_dir` is `Option<PathBuf>`, passed as `Option<&Path>` via `.as_deref()`
- `output` is `Option<PathBuf>`, passed as `Option<&Path>` via `.as_deref()`
- No tokio runtime needed -- `run_export` is fully synchronous
- Error propagation: `run_export` returns `Result<(), Box<dyn std::error::Error>>`, which matches `main()`'s return type

## No Additional Imports

The import `use unimatrix_server::export;` is not needed in `main.rs` since the call uses the fully qualified path `unimatrix_server::export::run_export`. The `export` module just needs to be declared `pub mod export;` in the server crate's `lib.rs`.

## Error Handling

- `run_export` returns `Result<(), Box<dyn std::error::Error>>`
- On error, the `?` propagation in `main()` prints the error to stderr and exits with non-zero code (same as Hook behavior)
- No special error wrapping needed

## Key Test Scenarios

1. **CLI parsing**: `unimatrix-server export` parses without error
2. **CLI parsing with output flag**: `unimatrix-server export --output /tmp/export.jsonl` correctly populates `output` field
3. **CLI parsing with short flag**: `unimatrix-server export -o /tmp/export.jsonl` works
4. **Existing subcommands unaffected**: `unimatrix-server hook SessionStart` still works
5. **Server mode unaffected**: `unimatrix-server` (no subcommand) still enters tokio server mode
6. **project-dir wired**: `unimatrix-server --project-dir /tmp/proj export` passes project_dir to run_export
