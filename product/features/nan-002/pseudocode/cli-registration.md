# nan-002: cli-registration -- Pseudocode

## Purpose

Add the `Import` subcommand variant to the existing `Command` enum in `main.rs` and wire the match arm to dispatch to `import::run_import()`. Follows the same sync-path pattern as `Hook` and `Export`.

## Files Modified

- `crates/unimatrix-server/src/main.rs` -- add `Command::Import` variant + match arm
- `crates/unimatrix-server/src/lib.rs` -- register `pub mod format;` and `pub mod import;`

## Command Enum Addition

```
// Add to the existing Command enum, after the Export variant:

/// Import a knowledge base from a JSONL export file.
///
/// Reads a nan-001 export dump, restores all 8 tables via direct SQL,
/// re-embeds entries with the current ONNX model, and builds a fresh
/// HNSW vector index. Synchronous path, no tokio runtime.
Import {
    /// Input JSONL file path (required).
    #[arg(short, long)]
    input: PathBuf,

    /// Skip content hash and chain integrity validation.
    #[arg(long)]
    skip_hash_validation: bool,

    /// Drop all existing data before import.
    #[arg(long)]
    force: bool,
}
```

## Match Arm Addition

```
// In the main() function, inside `match cli.command`:

Some(Command::Import { input, skip_hash_validation, force }) => {
    // Sync path: NO tokio, like Hook and Export
    unimatrix_server::import::run_import(
        cli.project_dir.as_deref(),
        &input,
        skip_hash_validation,
        force,
    )
}
```

## lib.rs Module Registration

```
// Add after `pub mod export;`:
pub mod format;
pub mod import;
```

## Error Handling

- `run_import` returns `Result<(), Box<dyn std::error::Error>>`, matching `run_export`.
- The main function propagates errors to the caller, which causes process exit with code 1.

## Key Test Scenarios

1. `unimatrix-server import --input test.jsonl` parses correctly and dispatches to `run_import`.
2. `unimatrix-server import` without `--input` produces a clap error (required argument missing).
3. `--force` and `--skip-hash-validation` are optional and default to `false`.
4. `--project-dir` root flag is respected and passed through to `run_import`.
5. Import subcommand runs on the sync path (no tokio runtime initialized).
