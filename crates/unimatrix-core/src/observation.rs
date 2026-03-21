//! Shared observation types for the Unimatrix pipeline.
//!
//! These types originated in unimatrix-observe and were moved here (col-013 ADR-002)
//! so that multiple crates can consume them without depending on unimatrix-observe.

use serde::{Deserialize, Serialize};

/// Well-known event type strings for the "claude-code" domain pack.
/// These are string constants for documentation only.
/// Use `event_type: String` and `source_domain: String` in all hot paths.
pub mod hook_type {
    pub const PRETOOLUSE: &str = "PreToolUse";
    pub const POSTTOOLUSE: &str = "PostToolUse";
    pub const SUBAGENTSTART: &str = "SubagentStart";
    pub const SUBAGENTSTOPPED: &str = "SubagentStop";
}

/// A single normalized observation record from a hook event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationRecord {
    /// Unix epoch milliseconds.
    pub ts: u64,
    /// Event type string (e.g., "PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop").
    /// Replaces the former `hook: HookType` field (col-023 ADR-001).
    pub event_type: String,
    /// Source domain for this event (e.g., "claude-code", "sre", "unknown").
    /// Set server-side at the ingest boundary; never client-declared.
    pub source_domain: String,
    /// Session identifier.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// T-OR-01: event_type and source_domain fields replace hook.
    /// This test will not compile if `hook: HookType` still exists on the struct.
    #[test]
    fn test_observation_record_has_event_type_and_source_domain() {
        let record = ObservationRecord {
            ts: 1_000_000,
            event_type: "PostToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: "sess-1".to_string(),
            tool: None,
            input: None,
            response_size: None,
            response_snippet: None,
        };
        assert_eq!(record.event_type, "PostToolUse");
        assert_eq!(record.source_domain, "claude-code");
    }

    /// T-OR-02: hook_type constants are &str, not enum variants.
    #[test]
    fn test_hook_type_constants_are_str() {
        assert_eq!(hook_type::PRETOOLUSE, "PreToolUse");
        assert_eq!(hook_type::POSTTOOLUSE, "PostToolUse");
        assert_eq!(hook_type::SUBAGENTSTART, "SubagentStart");
        assert_eq!(hook_type::SUBAGENTSTOPPED, "SubagentStop");

        // Compile-time type assertions: these must be &str, not enum variants.
        let _: &str = hook_type::PRETOOLUSE;
        let _: &str = hook_type::POSTTOOLUSE;
        let _: &str = hook_type::SUBAGENTSTART;
        let _: &str = hook_type::SUBAGENTSTOPPED;
    }

    /// T-OR-03: Serialization round-trip — JSON has event_type and source_domain, not hook.
    #[test]
    fn test_observation_record_serde_round_trip() {
        let record = ObservationRecord {
            ts: 1_000_000,
            event_type: "PostToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: "s1".to_string(),
            tool: None,
            input: None,
            response_size: None,
            response_snippet: None,
        };

        let v = serde_json::to_value(&record).expect("serialize");

        assert_eq!(v["event_type"], "PostToolUse");
        assert_eq!(v["source_domain"], "claude-code");
        assert!(
            v.get("hook").is_none(),
            "serialized JSON must not have 'hook' key"
        );

        let deserialized: ObservationRecord = serde_json::from_value(v).expect("deserialize");
        assert_eq!(deserialized.ts, record.ts);
        assert_eq!(deserialized.event_type, record.event_type);
        assert_eq!(deserialized.source_domain, record.source_domain);
        assert_eq!(deserialized.session_id, record.session_id);
    }

    /// T-OR-04: All existing fields are present (compile-time structural check).
    #[test]
    fn test_observation_record_all_fields_present() {
        let record = ObservationRecord {
            ts: 42,
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: "sess".to_string(),
            tool: Some("Bash".to_string()),
            input: Some(serde_json::json!({"cmd": "ls"})),
            response_size: Some(1024),
            response_snippet: Some("output".to_string()),
        };
        // Access every field to ensure none were accidentally removed.
        let _ = record.ts;
        let _ = &record.event_type;
        let _ = &record.source_domain;
        let _ = &record.session_id;
        let _ = &record.tool;
        let _ = &record.input;
        let _ = record.response_size;
        let _ = &record.response_snippet;
    }
}
