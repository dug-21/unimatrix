//! Scenario types: `ScenarioSource`, `ScenarioRecord`, `ScenarioContext`, `ScenarioBaseline`.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ScenarioSource
// ---------------------------------------------------------------------------

/// Filter for `eval scenarios --source`.
///
/// Controls which `query_log` rows are included in the output JSONL based
/// on the `source` column value (`"mcp"` or `"uds"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ScenarioSource {
    /// Include only rows with `source = "mcp"`.
    Mcp,
    /// Include only rows with `source = "uds"`.
    Uds,
    /// Include all rows regardless of source.
    All,
}

impl ScenarioSource {
    /// Returns the SQL literal to match against `source`, or `None` for `All`.
    pub fn to_sql_filter(self) -> Option<&'static str> {
        match self {
            ScenarioSource::Mcp => Some("mcp"),
            ScenarioSource::Uds => Some("uds"),
            ScenarioSource::All => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ScenarioRecord and sub-types
// ---------------------------------------------------------------------------

/// A single eval scenario derived from a `query_log` row.
///
/// Written as one JSONL line per record. `expected` is always `null` for
/// query-log-sourced scenarios (hand-authored scenarios may set it non-null,
/// but that is not produced by this module).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioRecord {
    /// Unique scenario identifier, formatted as `"qlog-{query_id}"`.
    pub id: String,
    /// The query text from the log.
    pub query: String,
    /// Execution context metadata.
    pub context: ScenarioContext,
    /// Baseline search results at log time, or `null` if no results were returned.
    pub baseline: Option<ScenarioBaseline>,
    /// Source transport: `"mcp"` or `"uds"`.
    pub source: String,
    /// Hard labels for the expected result set. Always `null` for log-sourced scenarios.
    pub expected: Option<Vec<u64>>,
}

/// Execution context metadata extracted from the query log row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioContext {
    /// Agent identifier. Populated from `session_id` (no dedicated column exists).
    pub agent_id: String,
    /// Feature cycle. Empty string — not stored in `query_log`.
    pub feature_cycle: String,
    /// Session identifier from `query_log.session_id`.
    pub session_id: String,
    /// Retrieval mode: `"flexible"` or `"strict"`. Defaults to `"flexible"` if absent.
    pub retrieval_mode: String,
}

/// Baseline search results captured at query time.
///
/// `entry_ids` and `scores` are parallel arrays; their lengths are always equal
/// (enforced at extraction time per R-16).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioBaseline {
    /// Ordered list of result entry IDs.
    pub entry_ids: Vec<u64>,
    /// Similarity scores parallel to `entry_ids`.
    pub scores: Vec<f32>,
}
