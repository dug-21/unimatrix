//! Shared observation types for the Unimatrix pipeline.
//!
//! These types originated in unimatrix-observe and were moved here (col-013 ADR-002)
//! so that multiple crates can consume them without depending on unimatrix-observe.

use serde::{Deserialize, Serialize};

/// Hook event type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookType {
    PreToolUse,
    PostToolUse,
    SubagentStart,
    SubagentStop,
}

/// A single normalized observation record from a Claude Code hook event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationRecord {
    /// Unix epoch milliseconds.
    pub ts: u64,
    /// Hook event type.
    pub hook: HookType,
    /// Claude Code session identifier.
    pub session_id: String,
    /// Tool name (PreToolUse/PostToolUse) or agent type (SubagentStart). None for SubagentStop.
    pub tool: Option<String>,
    /// Tool input object or prompt snippet (as Value::String). None for SubagentStop.
    pub input: Option<serde_json::Value>,
    /// Response byte count (PostToolUse only).
    pub response_size: Option<u64>,
    /// First 500 chars of response (PostToolUse only).
    pub response_snippet: Option<String>,
}

/// A parsed session with its records.
#[derive(Debug, Clone)]
pub struct ParsedSession {
    /// Session ID.
    pub session_id: String,
    /// Parsed observation records, sorted by timestamp.
    pub records: Vec<ObservationRecord>,
}

/// Aggregate statistics about observation data.
#[derive(Debug, Clone)]
pub struct ObservationStats {
    /// Number of observation records.
    pub record_count: u64,
    /// Number of distinct sessions with observations.
    pub session_count: u64,
    /// Age of oldest observation record in days.
    pub oldest_record_age_days: u64,
    /// Session IDs with records approaching 60-day cleanup (45-59 days old).
    pub approaching_cleanup: Vec<String>,
}
