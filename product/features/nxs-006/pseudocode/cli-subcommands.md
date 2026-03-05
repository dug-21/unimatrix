# Pseudocode: cli-subcommands

## File: `crates/unimatrix-server/src/main.rs`

### Changes to Command enum

```rust
#[derive(Subcommand)]
enum Command {
    Hook { event: String },

    /// Export all tables from the redb database to a JSON-lines file.
    #[cfg(not(feature = "backend-sqlite"))]
    Export {
        /// Path to the output JSON-lines file.
        #[arg(long)]
        output: PathBuf,
        /// Override the database path.
        #[arg(long)]
        db_path: Option<PathBuf>,
    },

    /// Import tables from a JSON-lines file into a new SQLite database.
    #[cfg(feature = "backend-sqlite")]
    Import {
        /// Path to the input JSON-lines file.
        #[arg(long)]
        input: PathBuf,
        /// Path for the output SQLite database file.
        #[arg(long)]
        output: PathBuf,
    },
}
```

### Changes to main() match

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Hook { event }) => {
            unimatrix_server::uds::hook::run(event, cli.project_dir)
        }
        #[cfg(not(feature = "backend-sqlite"))]
        Some(Command::Export { output, db_path }) => {
            run_export(output, db_path, cli.project_dir)
        }
        #[cfg(feature = "backend-sqlite")]
        Some(Command::Import { input, output }) => {
            run_import(input, output)
        }
        None => {
            tokio_main(cli)
        }
    }
}
```

### Export handler (sync, no tokio)

```rust
#[cfg(not(feature = "backend-sqlite"))]
fn run_export(
    output: PathBuf,
    db_path: Option<PathBuf>,
    project_dir: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Resolve database path
    let resolved_db = if let Some(p) = db_path {
        p
    } else {
        let paths = project::ensure_data_directory(project_dir.as_deref(), None)?;
        // Check for running server (PID file)
        if let Ok(contents) = std::fs::read_to_string(&paths.pid_path) {
            if let Ok(pid) = contents.trim().parse::<u32>() {
                if pidfile::is_unimatrix_process(pid) {
                    return Err("Cannot export while server is running. Stop the server first.".into());
                }
            }
        }
        paths.db_path
    };

    // 2. Validate database exists
    if !resolved_db.exists() {
        return Err(format!("database not found: {}", resolved_db.display()).into());
    }

    // 3. Run export
    eprintln!("Exporting from: {}", resolved_db.display());
    let summary = unimatrix_store::migrate::export::export(&resolved_db, &output)?;

    // 4. Print summary
    summary.print_to_stderr();
    eprintln!("Export complete: {}", output.display());
    Ok(())
}
```

### Import handler (sync, no tokio)

```rust
#[cfg(feature = "backend-sqlite")]
fn run_import(
    input: PathBuf,
    output: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Validate input exists
    if !input.exists() {
        return Err(format!("input file not found: {}", input.display()).into());
    }

    // 2. Validate output does not exist
    if output.exists() {
        return Err(format!("output already exists (will not overwrite): {}", output.display()).into());
    }

    // 3. Run import
    eprintln!("Importing from: {}", input.display());
    let summary = unimatrix_store::migrate::import::import(&input, &output)?;

    // 4. Print summary
    summary.print_to_stderr();
    eprintln!("Import complete: {}", output.display());
    Ok(())
}
```

## Cargo.toml changes (unimatrix-server)

```toml
[features]
default = ["mcp-briefing", "backend-sqlite"]
mcp-briefing = []
backend-sqlite = ["unimatrix-store/backend-sqlite", "unimatrix-engine/backend-sqlite"]

# redb stays optional, only needed when NOT using backend-sqlite
# The "redb" feature is no longer in the default list
```

Note: The `redb` dependency in unimatrix-server is currently optional and used only for the `DatabaseAlreadyOpen` error match in `open_store_with_retry`. That match arm is already `#[cfg(not(feature = "backend-sqlite"))]` gated. With the default flip, the redb feature on the server is no longer in defaults, and the error match arm only compiles under the redb path.
