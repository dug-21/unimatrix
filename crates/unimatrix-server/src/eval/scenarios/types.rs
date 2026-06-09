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
// Property-based assertions (nan-018, ADR-004) — shared on-disk types
// ---------------------------------------------------------------------------

/// A stable handle to a corpus entry: a fixture alias such as `"chainA.head"`.
///
/// Authored against in fixture TOML and resolved to a concrete entry id at load
/// time by the corpus loader (`eval/corpus/`). Using an alias rather than a
/// literal id means property assertions survive a re-snapshot / id renumber
/// (nan-018 R-10).
pub type EntryRef = String;

/// Property-based ground truth for a primary-corpus scenario (nan-018, ADR-004).
///
/// The primary fixture corpus asserts *outcomes*, never literal ids (C-04):
/// every assertion below is authored against a corpus alias (`EntryRef`) and
/// resolved at load. This is the on-disk shape; evaluation lives in
/// `eval/runner/trust.rs::evaluate_trust`. The three property families are:
///
/// - `redirect_to_head` — the terminal-active chain head must be present at or
///   above each of its queried (superseded) members.
/// - `forbidden_absent` — each alias must be absent from the result top-k.
/// - `rank_below` — for `(A, B)`, A must rank strictly below B (or A absent).
///
/// Field names and shape are the load-bearing Integration Surface
/// (architecture §6); downstream components import this type rather than
/// redefining it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExpectedAssertions {
    /// Chain-head aliases that must surface at/above their superseded members.
    #[serde(default)]
    pub redirect_to_head: Vec<EntryRef>,
    /// Aliases that must NOT appear in the result top-k.
    #[serde(default)]
    pub forbidden_absent: Vec<EntryRef>,
    /// `(A, B)` pairs where A must rank strictly below B.
    #[serde(default)]
    pub rank_below: Vec<(EntryRef, EntryRef)>,
}

impl ExpectedAssertions {
    /// Every alias referenced by any assertion in this set.
    ///
    /// Used by the loader to validate that each referenced alias resolves
    /// (nan-018 R-10) — a missing alias is a hard load error, never a silent
    /// vacuous pass.
    pub fn referenced_aliases(&self) -> impl Iterator<Item = &EntryRef> {
        self.redirect_to_head
            .iter()
            .chain(self.forbidden_absent.iter())
            .chain(self.rank_below.iter().flat_map(|(a, b)| [a, b]))
    }

    /// True when this set carries no assertions of any kind.
    pub fn is_empty(&self) -> bool {
        self.redirect_to_head.is_empty()
            && self.forbidden_absent.is_empty()
            && self.rank_below.is_empty()
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
    /// Property-based ground truth for primary-corpus scenarios (nan-018, ADR-004).
    ///
    /// Additive and orthogonal to `expected`: log-sourced scenarios never set it,
    /// and the primary fixture corpus uses `assertions` and NEVER `expected`
    /// (C-04, loader-enforced). `#[serde(default)]` so existing JSONL without the
    /// field deserializes unchanged (backward wire-compat).
    #[serde(default)]
    pub assertions: Option<ExpectedAssertions>,
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
    /// Workflow phase from `query_log.phase` (col-028). Absent from JSONL when `None`
    /// (ADR-001: null phase must not emit `"phase":null` — backward wire-compat).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
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
