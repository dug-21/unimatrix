//! Evaluation harness module (nan-007).
//!
//! Provides offline A/B evaluation infrastructure for Unimatrix intelligence
//! changes. All eval commands operate against a frozen snapshot database without
//! a running daemon. The module is structured as:
//!
//! - `profile` — `EvalProfile`, `EvalServiceLayer`, `AnalyticsMode`, `EvalError`
//! - `scenarios` — query_log scan → JSONL (Wave 2)
//! - `runner` — per-profile in-process replay + metric computation (Wave 2)
//! - `report` — Markdown aggregation + zero-regression check (Wave 2)
//!
//! CLI wiring (`Command::Eval`, `run_eval_command`) is added in Wave 3 (main.rs).

pub mod profile;

// Wave 2 modules (not yet implemented):
// pub mod scenarios;
// pub mod runner;
// pub mod report;

// Re-export core eval types for downstream modules.
pub use profile::{AnalyticsMode, EvalError, EvalProfile, EvalServiceLayer};

/// Nested subcommand enum for `unimatrix eval`.
///
/// Wired into clap in Wave 3 (`main.rs`). Defined here so downstream modules
/// can reference the enum shape without touching `main.rs` prematurely.
///
/// ADR-005: nested `Command::Eval { command: EvalCommand }` via clap 4.x.
#[derive(Debug, clap::Subcommand)]
pub enum EvalCommand {
    /// Extract scenario records from a snapshot database.
    ///
    /// Scans `query_log`, joins with `entries` to build baselines, and writes
    /// one JSONL line per record. Implemented in Wave 2.
    Scenarios {
        /// Snapshot database path.
        #[arg(long)]
        db: std::path::PathBuf,
        /// Filter by source: mcp, uds, or all.
        #[arg(long, default_value = "all")]
        source: String,
        /// Limit the number of scenarios extracted.
        #[arg(long)]
        limit: Option<usize>,
        /// Output JSONL file path.
        #[arg(short, long)]
        out: std::path::PathBuf,
    },

    /// Run in-process A/B evaluation across one or more profile configs.
    ///
    /// Constructs one `EvalServiceLayer` per profile, replays each scenario,
    /// computes metrics (P@K, MRR, Kendall tau, latency delta), and writes
    /// per-scenario JSON result files. Implemented in Wave 2.
    Run {
        /// Snapshot database path.
        #[arg(long)]
        db: std::path::PathBuf,
        /// JSONL scenarios file.
        #[arg(long)]
        scenarios: std::path::PathBuf,
        /// Comma-separated list of profile TOML paths.
        #[arg(long)]
        configs: String,
        /// Results output directory.
        #[arg(short, long)]
        out: std::path::PathBuf,
        /// Top-K for P@K metric.
        #[arg(long, default_value = "5")]
        k: usize,
    },

    /// Aggregate per-scenario JSON results into a Markdown report.
    ///
    /// Produces five Markdown sections with summary table, ranking changes,
    /// latency distribution, entry-level analysis, and zero-regression check.
    /// Exits 0 regardless of regression count (C-07). Implemented in Wave 2.
    Report {
        /// Directory containing per-scenario JSON result files.
        #[arg(long)]
        results: std::path::PathBuf,
        /// Optional JSONL scenarios file for additional context.
        #[arg(long)]
        scenarios: Option<std::path::PathBuf>,
        /// Output Markdown report path.
        #[arg(short, long)]
        out: std::path::PathBuf,
    },
}
