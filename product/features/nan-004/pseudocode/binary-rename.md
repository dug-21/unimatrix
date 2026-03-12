# C7: Binary Rename — Pseudocode

## Purpose

Rename the binary from `unimatrix-server` to `unimatrix`. Add `Version` and `ModelDownload` subcommands. Update `.mcp.json` and `.claude/settings.json` in this repository. Per ADR-002, all changes ship in a single commit.

## Modified File: crates/unimatrix-server/Cargo.toml

Replace:
```toml
[[bin]]
name = "unimatrix-server"
path = "src/main.rs"
```

With:
```toml
[[bin]]
name = "unimatrix"
path = "src/main.rs"
```

The `[package] name` remains `unimatrix-server` (crate rename is out of scope).

## Modified File: crates/unimatrix-server/src/main.rs

### CLI Struct Changes

Replace:
```rust
#[command(name = "unimatrix-server", about = "Unimatrix MCP knowledge server")]
```
With:
```rust
#[command(name = "unimatrix", about = "Unimatrix knowledge engine")]
```

### Command Enum Changes

Add two new variants to the `Command` enum:

```rust
enum Command {
    Hook { event: String },
    Export { output: Option<PathBuf> },
    Import { input: PathBuf, skip_hash_validation: bool, force: bool },

    /// Print version and exit.
    Version,

    /// Download the ONNX embedding model to the cache directory.
    ModelDownload,
}
```

### main() Match Arm Changes

Add two new match arms to the `match cli.command` block, BEFORE the `None` arm:

```
MATCH cli.command:
    Some(Command::Hook { event }) => { ... }     // existing, unchanged
    Some(Command::Export { output }) => { ... }   // existing, unchanged
    Some(Command::Import { ... }) => { ... }      // existing, unchanged

    Some(Command::Version) => {
        // Sync path: NO tokio, NO database
        handle_version()
    }

    Some(Command::ModelDownload) => {
        // Sync path: NO tokio, NO database
        handle_model_download()
    }

    None => {
        // Async path: full server with tokio runtime
        tokio_main(cli)
    }
```

### handle_version()

```
FUNCTION handle_version() -> Result<(), Box<dyn Error>>:
    // CARGO_PKG_VERSION is already used in the codebase (main.rs:278)
    // With workspace version inheritance (C9), this automatically reflects 0.5.0
    println!("unimatrix {}", env!("CARGO_PKG_VERSION"))
    Ok(())
```

Output format: `unimatrix 0.5.0` (plain text, no JSON).

Note: The `--project-dir` flag is still parsed by clap at the Cli struct level. When `version` is called with `--project-dir`, the flag is available but handle_version() ignores it. The init command (C4) passes `--project-dir` to trigger project path detection for DB creation, but `handle_version()` itself does not open the database. The DB pre-creation happens because `ensure_data_directory` is called when the server starts -- but for the `version` subcommand, we need a separate path.

**Revised approach for DB pre-creation**: The init command (C4) calls `unimatrix version --project-dir <root>`. However, `handle_version()` as written above does NOT open the database. Two options:
1. Add a dedicated subcommand like `unimatrix init-db --project-dir <root>` that calls `ensure_data_directory` + `Store::open`.
2. Have `handle_version()` check if `--project-dir` is set and, if so, also run `ensure_data_directory` + `Store::open`.

The architecture specifies option 2: "exec `unimatrix version --project-dir <root>` (or a health subcommand) to trigger `ensure_data_directory` + `Store::open` + `migrate_if_needed`."

```
FUNCTION handle_version(project_dir: Option<PathBuf>) -> Result<(), Box<dyn Error>>:
    // If --project-dir is provided, pre-create data directory and DB
    IF let Some(dir) = project_dir:
        LET paths = project::ensure_data_directory(Some(&dir), None)?
        LET _store = Store::open(&paths.db_path)?
        // Store is dropped immediately -- we just needed the side effects
        eprintln!("database initialized at {}", paths.db_path.display())
    END IF

    println!("unimatrix {}", env!("CARGO_PKG_VERSION"))
    Ok(())
```

### handle_model_download()

See C8 for full pseudocode. Declared here to show placement in main.rs.

## Modified File: .mcp.json

Replace the `command` value:
```json
{
  "mcpServers": {
    "unimatrix": {
      "command": "/workspaces/unimatrix/target/release/unimatrix",
      "args": [],
      "env": {}
    }
  }
}
```

Note: The path changes from `.../unimatrix-server` to `.../unimatrix`.

## Modified File: .claude/settings.json

Replace all 7 hook commands. Change `unimatrix-server hook <Event>` to `unimatrix hook <Event>`.

For `UserPromptSubmit`, also drop the tee pipeline (resolved decision: NO tee for distribution):
- Before: `unimatrix-server hook UserPromptSubmit | tee -a ~/.unimatrix/injections/hooks.log`
- After: `unimatrix hook UserPromptSubmit`

All 7 hooks become:
```
unimatrix hook SessionStart
unimatrix hook Stop
unimatrix hook UserPromptSubmit
unimatrix hook PreToolUse
unimatrix hook PostToolUse
unimatrix hook SubagentStart
unimatrix hook SubagentStop
```

## Error Handling

- `handle_version()` with `--project-dir`: If `ensure_data_directory` fails (e.g., permission error), propagate the error. If `Store::open` fails, propagate the error. These are fatal for the init workflow.
- `handle_version()` without `--project-dir`: Cannot fail (just prints a string).

## Key Test Scenarios

1. `cargo build` produces a binary named `unimatrix` (not `unimatrix-server`).
2. `unimatrix version` prints `unimatrix 0.5.0` and exits 0.
3. `unimatrix version --project-dir /tmp/test-project` creates data directory and DB, then prints version.
4. `unimatrix hook SessionStart` still works (unchanged behavior).
5. `unimatrix` (no args) starts MCP server (unchanged behavior).
6. `unimatrix export` and `unimatrix import` still work.
7. `.mcp.json` references `unimatrix` binary path.
8. `.claude/settings.json` uses `unimatrix hook <Event>` for all 7 events.
9. No `tee` pipeline in any hook command.
