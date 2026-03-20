//! Scenario extraction from a snapshot database (D2, nan-007).
//!
//! Scans the `query_log` table in a read-only snapshot and writes one JSONL
//! line per row as a `ScenarioRecord`. Supports filtering by `source`
//! (`mcp`, `uds`, `all`) and an optional row limit.
//!
//! This module never calls `SqlxStore::open()` (C-02). All DB access uses
//! a raw `SqlitePool` opened with `SqliteConnectOptions::read_only(true)`.
//! Async sqlx queries are bridged to the synchronous CLI dispatch path via
//! `block_export_sync` (C-09, ADR-005).

mod extract;
mod output;
mod types;

#[cfg(test)]
mod tests;

pub use output::run_scenarios;
pub use types::{ScenarioBaseline, ScenarioContext, ScenarioRecord, ScenarioSource};
