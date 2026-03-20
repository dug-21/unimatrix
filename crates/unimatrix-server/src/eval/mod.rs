//! Evaluation harness module (nan-007).
//!
//! Provides offline A/B evaluation infrastructure for Unimatrix intelligence
//! changes. All eval commands operate against a frozen snapshot database without
//! a running daemon. The module is structured as:
//!
//! - `profile` — `EvalProfile`, `EvalServiceLayer`, `AnalyticsMode`, `EvalError`
//! - `scenarios` — query_log scan → JSONL (D2)
//! - `runner` — per-profile in-process replay + metric computation (D3)
//! - `report` — Markdown aggregation + zero-regression check (nan-007 D4)
//!
//! `run_eval_command` dispatches `EvalCommand` variants pre-tokio (C-09, ADR-005).

use std::path::{Path, PathBuf};

pub mod profile;
pub mod report;
pub mod runner;
pub mod scenarios;

// Re-export core eval types for downstream modules.
pub use profile::{AnalyticsMode, EvalError, EvalProfile, EvalServiceLayer};
pub use report::run_report;
pub use runner::{
    ComparisonMetrics, ProfileResult, RankChange, ScenarioResult, ScoredEntry, run_eval,
};
pub use scenarios::{ScenarioBaseline, ScenarioContext, ScenarioRecord, ScenarioSource};

/// Nested subcommand enum for `unimatrix eval`.
///
/// ADR-005: nested `Command::Eval { command: EvalCommand }` via clap 4.x inner enum.
/// All variants are dispatched pre-tokio via `run_eval_command`. Async work uses
/// `block_export_sync` internally (C-09).
#[derive(Debug, clap::Subcommand)]
pub enum EvalCommand {
    /// Extract scenario records from a snapshot database.
    ///
    /// Scans `query_log`, builds baselines from stored result IDs and scores,
    /// and writes one JSONL line per record. Supports `--source mcp|uds|all`.
    Scenarios {
        /// Snapshot database path.
        #[arg(long)]
        db: PathBuf,
        /// Filter by source: mcp, uds, or all (default: all).
        #[arg(long, default_value = "all")]
        source: ScenarioSource,
        /// Limit the number of scenarios extracted.
        #[arg(long)]
        limit: Option<usize>,
        /// Output JSONL file path.
        #[arg(short, long)]
        out: PathBuf,
    },

    /// Run in-process A/B evaluation across one or more profile configs.
    ///
    /// Constructs one `EvalServiceLayer` per profile, replays each scenario,
    /// computes metrics (P@K, MRR, Kendall tau, latency delta), and writes
    /// per-scenario JSON result files.
    ///
    /// Memory note: each profile loads a separate vector index. Use <= 2 profiles
    /// on machines with 8 GB RAM and large snapshots.
    Run {
        /// Snapshot database path. Must not be the active daemon DB.
        #[arg(long)]
        db: PathBuf,
        /// JSONL scenarios file (output of `eval scenarios`).
        #[arg(long)]
        scenarios: PathBuf,
        /// Comma-separated list of profile TOML paths (at least one required).
        /// First profile is treated as the baseline.
        #[arg(long)]
        configs: String,
        /// Results output directory for per-scenario JSON files.
        #[arg(short, long)]
        out: PathBuf,
        /// Top-K for P@K metric (default: 5, must be >= 1).
        #[arg(long, default_value = "5")]
        k: usize,
    },

    /// Aggregate per-scenario JSON results into a Markdown report.
    ///
    /// Produces five Markdown sections: summary, notable ranking changes,
    /// latency distribution, entry-level analysis, and zero-regression check.
    ///
    /// Always exits 0 — no automated pass/fail gate logic is applied (C-07).
    Report {
        /// Directory containing per-scenario JSON result files.
        #[arg(long)]
        results: PathBuf,
        /// Optional JSONL scenarios file for annotating queries in the report.
        #[arg(long)]
        scenarios: Option<PathBuf>,
        /// Output Markdown report path.
        #[arg(short, long)]
        out: PathBuf,
    },
}

/// Dispatch an `EvalCommand` variant.
///
/// Called from `main()` in the pre-tokio sync block (C-10, ADR-005).
/// `Scenarios` and `Run` use `block_export_sync` internally for async sqlx.
/// `Report` is fully synchronous with no async bridge needed.
///
/// # Errors
///
/// Propagates errors from the dispatched function. For `Run`, an empty
/// `--configs` string is rejected immediately before any file I/O.
pub fn run_eval_command(
    cmd: EvalCommand,
    _project_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        EvalCommand::Scenarios {
            db,
            source,
            limit,
            out,
        } => {
            // block_export_sync bridge is inside run_scenarios (async sqlx).
            scenarios::run_scenarios(&db, source, limit, &out)
        }

        EvalCommand::Run {
            db,
            scenarios,
            configs,
            out,
            k,
        } => {
            // Parse comma-separated config paths.
            let config_paths: Vec<PathBuf> = configs
                .split(',')
                .map(|s| PathBuf::from(s.trim()))
                .filter(|p| !p.as_os_str().is_empty())
                .collect();
            if config_paths.is_empty() {
                return Err("--configs must name at least one profile TOML file".into());
            }
            // block_export_sync bridge is inside run_eval (async sqlx + EvalServiceLayer).
            runner::run_eval(&db, &scenarios, &config_paths, k, &out)
        }

        EvalCommand::Report {
            results,
            scenarios,
            out,
        } => {
            // Fully synchronous — no block_export_sync needed (C-07, FR-29).
            report::run_report(&results, scenarios.as_deref(), &out)
        }
    }
}
