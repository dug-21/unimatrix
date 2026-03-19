## ADR-005: Nested eval Subcommand via Clap 4.x Inner Enum

### Context

`unimatrix eval` must expose three sub-subcommands: `scenarios`, `run`, `report`. This
is a three-level CLI: `unimatrix eval <subcommand> [args]`. The existing `Command` enum
in `main.rs` has one level of subcommands (Hook, Export, Import, Version, ModelDownload,
Serve, Stop). Adding `eval` at the same level with nested dispatching requires either
flat variants (`EvalScenarios { ... }`, `EvalRun { ... }`, `EvalReport { ... }`) or a
nested enum structure.

Two options were evaluated:

**Option A — Flat variants with name prefix**: Add `EvalScenarios`, `EvalRun`,
`EvalReport` as direct variants of `Command`. Avoids nesting but produces unwieldy
variant names and does not produce the `unimatrix eval --help` grouping behaviour that
users expect.

**Option B — Nested enum with `#[command(subcommand)]`**: Add `Eval { command:
EvalCommand }` to `Command` where `EvalCommand` is a new `Subcommand` enum with
`Scenarios`, `Run`, and `Report` variants. Clap 4.x natively supports nested
subcommand enums via `#[command(subcommand)]` on the inner field.

### Decision

Nested enum (Option B).

Clap 4.x supports arbitrary nesting via `#[command(subcommand)]`. The `Command::Eval`
variant carries a `command: EvalCommand` field. The dispatch in `main()` is:

```rust
Some(Command::Eval { command: eval_cmd }) => {
    return run_eval_command(eval_cmd, cli.project_dir.as_deref());
}
```

This dispatch arm is placed in the sync block (before the tokio runtime) alongside the
other sync subcommands, satisfying C-10. The `run_eval_command` function uses
`block_export_sync` internally for subcommands that need async sqlx.

The `EvalCommand` enum:
```rust
#[derive(Debug, Subcommand)]
enum EvalCommand {
    /// Extract eval scenarios from a snapshot database.
    Scenarios { db: PathBuf, source: ..., limit: ..., out: PathBuf },
    /// Replay scenarios through profile configs in-process.
    Run { db: PathBuf, scenarios: PathBuf, configs: String, out: PathBuf, k: usize },
    /// Aggregate eval results into a Markdown report.
    Report { results: PathBuf, scenarios: Option<PathBuf>, out: PathBuf },
}
```

The `snapshot` subcommand is a direct `Command::Snapshot { out: PathBuf }` variant —
it is at the same level as `export`, not nested under `eval`. This matches the product
specification and keeps the separation between data collection (`snapshot`) and
analysis (`eval *`) visible at the CLI level.

### Consequences

- `unimatrix eval --help` correctly lists the three subcommands.
- `unimatrix eval scenarios --help`, `unimatrix eval run --help`,
  `unimatrix eval report --help` each display their own argument documentation.
- C-10 dispatch ordering is preserved: the entire `Eval` arm is dispatched before the
  tokio runtime. Async work inside `run_eval_command` uses `block_export_sync`.
- AC-15 (all new subcommands visible in `--help`) is satisfied by clap's automatic
  generation from the enum and doc comments.
- Adding future `eval live` or `eval compare` subcommands requires only a new variant
  in `EvalCommand` — the dispatch structure is already in place.
