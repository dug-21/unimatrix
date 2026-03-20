# Pseudocode: main.rs (CLI Wiring)

**Location**: `crates/unimatrix-server/src/main.rs`

## Purpose

Add `Command::Snapshot` and `Command::Eval { command: EvalCommand }` variants to the
existing `Command` enum and dispatch them in the sync block before the tokio runtime
(C-09, C-10, ADR-005). `eval/mod.rs` owns the `EvalCommand` enum and the dispatcher.
This file documents only the `main.rs` additions; it does not repeat the existing code.

## Files Modified

- `crates/unimatrix-server/src/main.rs` — add two Command variants and two dispatch arms
- `crates/unimatrix-server/src/eval/mod.rs` — new file: EvalCommand enum + run_eval_command

## main.rs Changes

### Command enum additions (after `Stop`, before any future variants)

```rust
// In the existing Command enum in main.rs:

#[derive(Debug, Subcommand)]
enum Command {
    // ... existing variants: Hook, Export, Import, Version, ModelDownload, Serve, Stop ...

    /// Take a full-fidelity snapshot of the active database using VACUUM INTO.
    ///
    /// The snapshot is a self-contained SQLite file containing ALL tables.
    /// It is the input to `unimatrix eval scenarios` and `unimatrix eval run`.
    ///
    /// WARNING: The snapshot contains all database content including agent_id,
    /// session_id, and query history. Do not commit snapshots to version control
    /// or share outside your development environment. (NFR-07)
    ///
    /// The snapshot can be taken while the daemon is running. WAL-mode SQLite
    /// guarantees isolation: VACUUM INTO reads a consistent point-in-time snapshot.
    Snapshot {
        /// Output file path for the snapshot SQLite file (required).
        #[arg(long)]
        out: PathBuf,
    },

    /// Offline evaluation harness for Unimatrix intelligence changes.
    ///
    /// Subcommands: scenarios, run, report
    ///
    /// Memory note: each profile in `eval run` loads a separate vector index.
    /// For large snapshots (50k entries) with multiple profiles, ensure adequate RAM.
    /// Recommended: <= 2 candidate profiles on machines with 8 GB RAM.
    Eval {
        #[command(subcommand)]
        command: EvalCommand,
    },
}
```

### Dispatch arms in `main()` (in the sync block, before tokio runtime init)

```rust
// In the existing sync dispatch match block in main():
// These arms are placed alongside Hook, Export, Import, Version, ModelDownload, Stop.
// C-10: all sync paths BEFORE any tokio runtime init.

        Some(Command::Snapshot { out }) => {
            // Sync path: dispatched pre-tokio (C-09).
            // run_snapshot uses block_export_sync internally for async sqlx.
            return unimatrix_server::snapshot::run_snapshot(
                cli.project_dir.as_deref(),
                &out,
            );
        }

        Some(Command::Eval { command: eval_cmd }) => {
            // Sync path: dispatched pre-tokio (C-10, ADR-005).
            // run_eval_command uses block_export_sync internally for async subcommands.
            return unimatrix_server::eval::run_eval_command(
                eval_cmd,
                cli.project_dir.as_deref(),
            );
        }
```

### `mod.rs` additions

Add these two module declarations to `crates/unimatrix-server/src/lib.rs` (or wherever
`export` is declared):

```rust
pub mod snapshot;
pub mod eval;
```

## eval/mod.rs

**Location**: `crates/unimatrix-server/src/eval/mod.rs`

```rust
//! Evaluation harness module (nan-007).
//!
//! Provides offline A/B evaluation for Unimatrix intelligence changes.
//! All eval subcommands are dispatched pre-tokio; async work uses block_export_sync.
//!
//! Module tree:
//!   eval/mod.rs      — EvalCommand enum, dispatcher
//!   eval/profile.rs  — EvalProfile, EvalServiceLayer, AnalyticsMode, EvalError
//!   eval/scenarios.rs — D2: query_log scan → JSONL
//!   eval/runner.rs    — D3: in-process A/B replay, metrics
//!   eval/report.rs    — D4: Markdown aggregation, zero-regression check

mod profile;
mod scenarios;
mod runner;
mod report;

pub use profile::{AnalyticsMode, EvalError, EvalProfile, EvalServiceLayer};
pub use runner::{ScenarioResult, ProfileResult, ComparisonMetrics, RankChange, ScoredEntry};
pub use scenarios::{ScenarioRecord, ScenarioContext, ScenarioBaseline, ScenarioSource};

use std::path::{Path, PathBuf};
use clap::Subcommand;

/// Eval subcommands.
///
/// Dispatched pre-tokio via run_eval_command(). Async work uses block_export_sync internally.
#[derive(Debug, Subcommand)]
pub enum EvalCommand {
    /// Extract eval scenarios from a snapshot database.
    ///
    /// Reads query_log from the snapshot and writes one JSONL line per scenario.
    /// The snapshot must not be the active daemon database (live-DB path guard enforced).
    Scenarios {
        /// Path to snapshot SQLite file (required).
        #[arg(long)]
        db: PathBuf,

        /// Output JSONL file path (required).
        #[arg(long)]
        out: PathBuf,

        /// Maximum number of scenarios to extract.
        #[arg(long)]
        limit: Option<usize>,

        /// Filter by retrieval source: mcp, uds, or all (default: all).
        #[arg(long, value_enum, default_value_t = ScenarioSource::All)]
        retrieval_mode: ScenarioSource,
    },

    /// Replay eval scenarios through one or more profile configurations in-process.
    ///
    /// Writes one JSON result file per scenario to --out directory.
    /// Profiles are named by the [profile].name field in their TOML files.
    /// The first profile listed is treated as the baseline for comparison metrics.
    ///
    /// Memory note: each profile loads a separate vector index. Use <= 2 profiles
    /// on machines with 8 GB RAM and large snapshots.
    Run {
        /// Path to snapshot SQLite file (required). Must not be the active daemon DB.
        #[arg(long)]
        db: PathBuf,

        /// Path to scenarios JSONL file (required, output of `eval scenarios`).
        #[arg(long)]
        scenarios: PathBuf,

        /// Comma-separated paths to profile TOML files (required, at least one).
        /// First profile is treated as the baseline.
        #[arg(long)]
        configs: String,

        /// Output directory for per-scenario result JSON files (required).
        #[arg(long)]
        out: PathBuf,

        /// K for P@K metric computation (default: 5, must be >= 1).
        #[arg(long, default_value_t = 5)]
        k: usize,
    },

    /// Aggregate eval results into a Markdown report.
    ///
    /// Reads all *.json files from --results directory and writes a Markdown report
    /// with five sections: summary, notable ranking changes, latency distribution,
    /// entry-level analysis, and zero-regression check.
    ///
    /// Always exits 0; no automated pass/fail gate logic is applied.
    Report {
        /// Directory containing per-scenario result JSON files (required).
        #[arg(long)]
        results: PathBuf,

        /// Output Markdown file path (required).
        #[arg(long)]
        out: PathBuf,

        /// Optional: path to scenarios JSONL for annotating queries in the report.
        #[arg(long)]
        scenarios: Option<PathBuf>,
    },
}

/// Dispatch an EvalCommand variant.
///
/// Called from main() in the pre-tokio sync block (C-10, ADR-005).
/// Async subcommands (Scenarios, Run) use block_export_sync internally.
/// Report is fully synchronous with no async bridge needed.
pub fn run_eval_command(
    cmd: EvalCommand,
    project_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        EvalCommand::Scenarios {
            db, out, limit, retrieval_mode,
        } => {
            // block_export_sync bridge is inside run_scenarios (async sqlx)
            scenarios::run_scenarios(&db, retrieval_mode, limit, &out)
        }

        EvalCommand::Run {
            db, scenarios, configs, out, k,
        } => {
            // Parse comma-separated config paths:
            let config_paths: Vec<PathBuf> = configs
                .split(',')
                .map(|s| PathBuf::from(s.trim()))
                .collect();
            if config_paths.is_empty() {
                return Err("--configs must name at least one profile TOML file".into());
            }
            // block_export_sync bridge is inside run_eval (async sqlx + EvalServiceLayer)
            runner::run_eval(&db, &scenarios, &config_paths, k, &out)
        }

        EvalCommand::Report {
            results, out, scenarios,
        } => {
            // Fully synchronous; no block_export_sync needed
            report::run_report(&results, scenarios.as_deref(), &out)
        }
    }
}
```

## Cargo.toml Change (unimatrix-server)

One change required in `crates/unimatrix-server/Cargo.toml`:

```toml
# Before (existing):
unimatrix-engine = { path = "../unimatrix-engine" }

# After (nan-007, ADR-003):
# production-safe; required by eval runner for kendall_tau and ranking metrics
unimatrix-engine = { path = "../unimatrix-engine", features = ["test-support"] }
```

This enables `unimatrix_engine::test_scenarios::kendall_tau()` from within the eval
runner's production binary code. The `test-support` feature is guarded by
`#[cfg(any(test, feature = "test-support"))]` in unimatrix-engine. The comment on
the Cargo.toml line is mandatory (ADR-003: reviewer friction if removed; R-03 mitigation).

## lib.rs Changes

Ensure the following module declarations exist in
`crates/unimatrix-server/src/lib.rs`:

```rust
pub mod snapshot;  // new
pub mod eval;      // new
```

Both modules must be `pub` so that `main.rs` can reference them as
`unimatrix_server::snapshot::run_snapshot` and `unimatrix_server::eval::run_eval_command`.

## Error Handling

| Dispatch arm | Error behavior |
|-------------|----------------|
| `EvalCommand::Scenarios` | Propagates from `run_scenarios` → main() → stderr + non-zero exit |
| `EvalCommand::Run` | Propagates from `run_eval`; `EvalError` types are `Display` for user messages |
| `EvalCommand::Report` | Propagates from `run_report`; always exits 0 if report produced |
| `--configs` parse (empty string) | Immediate Err with message before any file I/O |

## Key Test Scenarios

1. **Help text** (AC-15): `unimatrix --help` output contains `snapshot`; `unimatrix eval --help`
   contains `scenarios`, `run`, `report`.

2. **Snapshot --help**: contains NFR-07 content-sensitivity warning.

3. **eval --help**: contains memory note about vector index per profile.

4. **Dispatch pre-tokio**: invoke `run_eval_command(EvalCommand::Report{...})` from a
   sync context (no existing runtime); assert no runtime panic (R-11).

5. **--configs comma-separated**: `--configs a.toml,b.toml` parses into two PathBuf entries.

6. **--configs empty string**: returns Err before any file I/O.

7. **All three eval subcommands registered**: `unimatrix eval scenarios --help`,
   `unimatrix eval run --help`, `unimatrix eval report --help` all print usage without
   error (AC-15).

8. **Cargo.toml feature flag**: `cargo build --release` with the test-support feature
   compiles without error; removing the feature causes a compile error in runner.rs
   (R-03 mitigation).

## Knowledge Stewardship

Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — ADR-005 (#2588, nested eval subcommand via clap 4.x inner enum) directly governs EvalCommand placement and dispatch. ADR-003 (#2586, test-support feature for kendall_tau) governs the Cargo.toml change. ADR-004 (#2587, eval in unimatrix-server) governs module placement. All three followed exactly.
Queried: /uni-query-patterns for "block_export_sync async bridge pattern" — #2126 and #1758 confirm the pre-tokio dispatch pattern. Command::Snapshot and Command::Eval dispatch arms are placed before the tokio runtime init, consistent with existing Hook, Export, Import arms in main.rs.
Queried: /uni-query-patterns for "evaluation harness patterns conventions" (category: pattern) — no results applicable to CLI wiring. Clap 4.x inner enum subcommand pattern (#2588, ADR-005) is the governing decision; no deviation.
Stored: nothing novel to store — pseudocode agents are read-only; patterns are consumed not created
