# Component 4 — CLI wiring (clap `--slug`) (`unimatrix-server/src/main.rs`)

## Purpose

Add the `--slug <name>` clap argument to the `Export` and `Import` subcommands and thread it
into the `run_export`/`run_import` calls (`main.rs:556-581`). Depends on both new signatures
(Wave C). No resolution logic here — `main` only forwards the raw `Option<&str>`.

## Clap arg additions

### `Export` variant (`main.rs:332-346`) — add field

```
Export {
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// NEW
    /// Back up a named per-slug project store ({base}/<slug>/unimatrix.db), the store the
    /// running daemon uses — not the CLI's path-hash store. Base is derived from
    /// --project-dir; the in-container invocation (HOME=/data) is the expected posture.
    /// `--slug` names a store DIR under the base, not a registered [[projects]] entry.
    #[arg(long)]
    slug: Option<String>,

    #[arg(long)]
    skip_quarantined: bool,

    #[arg(long)]
    confirm: bool,
},
```

### `Import` variant (`main.rs:353-365`) — add field

```
Import {
    #[arg(short, long)]
    input: PathBuf,

    /// NEW
    /// Restore into a named per-slug project store ({base}/<slug>/unimatrix.db) and rebuild
    /// its vector index. Base is derived from --project-dir; run in-container (HOME=/data).
    /// `--slug` names a store DIR under the base, not a registered [[projects]] entry.
    /// Canonical restore sequence: `project register <slug>` → stop → import --slug → start
    /// (see README "Restore a per-slug project").
    #[arg(long)]
    slug: Option<String>,

    #[arg(long)]
    skip_hash_validation: bool,

    #[arg(long)]
    force: bool,
},
```

Help text satisfies FR-15/AC-07: (a) base from `--project-dir`, (b) in-container posture,
(c) "store dir, not registered project"; import additionally carries the README pointer.

## Dispatch wiring

### Export dispatch (`main.rs:556-568`)

```
Some(Command::Export { output, slug, skip_quarantined, confirm }) => {
    unimatrix_server::export::run_export(
        cli.project_dir.as_deref(),
        output.as_deref(),
        slug.as_deref(),          # NEW — Option<String> -> Option<&str>
        skip_quarantined,
        confirm,
    )
}
```

### Import dispatch (`main.rs:569-581`)

```
Some(Command::Import { input, slug, skip_hash_validation, force }) => {
    unimatrix_server::import::run_import(
        cli.project_dir.as_deref(),
        &input,
        slug.as_deref(),          # NEW
        skip_hash_validation,
        force,
    )
}
```

Match the argument position chosen in `export.md` / `import.md` (slug placed after
`output`/`input`). Both stay on the sync pre-tokio path (C-8) — no change to dispatch flavor.

## State machine

None. Pure clap parse + forward.

## Data flow

`--slug <name>` (raw string, untrusted) → `Option<String>` field → `.as_deref()` →
`Option<&str>` into `run_export`/`run_import`. Validation happens downstream at the funnel's
`validate_slug` edge (Component 1) — main does NOT validate.

## Error handling

None added in `main`. Errors surface from `run_export`/`run_import` and flow through the
existing `Result → process exit` mapping (non-zero exit on `Err`, message printed). AC-03/AC-04
messages therefore reach the operator unchanged.

## Key test scenarios (hints)

- **AC-07 help snapshot:** `export --help` / `import --help` show `--slug` with base-derivation,
  in-container, and store-dir-not-registered wording; import help carries the README pointer.
- **AC-05 parity:** invoking without `--slug` parses and dispatches exactly as before.
- Wiring is exercised transitively by the export/import integration tests that drive
  `run_export_with_base` / `run_import_with_base` (which take `slug` directly); a thin
  parse-level test confirms `--slug foo` populates the field.
