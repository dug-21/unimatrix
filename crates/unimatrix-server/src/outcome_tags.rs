//! Structured tag parsing and validation for outcome entries.
//!
//! Validates `key:value` tags against the recognized key set when `category == "outcome"`.
//! Tags without `:` are plain tags and pass through without validation.
//! Called from `context_store` only when `category == "outcome"`.

use crate::error::ServerError;

/// Recognized structured tag keys for outcome entries.
const RECOGNIZED_KEYS: &[&str] = &["type", "gate", "phase", "result", "agent", "wave"];

/// Valid workflow type values for the required `type` tag.
const VALID_TYPES: &[&str] = &["feature", "bugfix", "incident", "process"];

/// Valid outcome result values for the `result` tag.
const VALID_RESULTS: &[&str] = &["pass", "fail", "rework", "skip"];

/// Valid phase values for the `phase` tag.
const VALID_PHASES: &[&str] = &[
    "research",
    "design",
    "implementation",
    "testing",
    "validation",
];

/// Validate all tags for an outcome entry.
///
/// Rules:
/// 1. Tags without `:` are plain tags -- pass through, no validation.
/// 2. Tags with `:` are split on FIRST `:` into (key, value).
/// 3. Key must be in RECOGNIZED_KEYS -- unknown key is an error.
/// 4. The `type` tag is REQUIRED -- its absence is an error.
/// 5. Duplicate structured keys are rejected.
/// 6. Key-specific value validation applies (see `validate_tag_key_value`).
pub fn validate_outcome_tags(tags: &[String]) -> Result<(), ServerError> {
    let mut has_type = false;
    let mut seen_keys: Vec<&str> = Vec::new();

    for tag in tags {
        if let Some((key, value)) = parse_structured_tag(tag) {
            // Check for recognized key
            if !RECOGNIZED_KEYS.contains(&key) {
                return Err(ServerError::InvalidInput {
                    field: "tags".to_string(),
                    reason: format!(
                        "unknown structured tag key '{}'. Recognized keys: {}",
                        key,
                        RECOGNIZED_KEYS.join(", ")
                    ),
                });
            }

            // Check for duplicate keys
            if seen_keys.contains(&key) {
                return Err(ServerError::InvalidInput {
                    field: "tags".to_string(),
                    reason: format!("duplicate structured tag key '{}'", key),
                });
            }
            seen_keys.push(key);

            // Validate key-specific value
            validate_tag_key_value(key, value)?;

            if key == "type" {
                has_type = true;
            }
        }
        // Plain tags (no colon) pass through without validation
    }

    // Check required 'type' tag
    if !has_type {
        return Err(ServerError::InvalidInput {
            field: "tags".to_string(),
            reason: "type tag is required for outcome entries (e.g., type:feature, type:bugfix, type:incident, type:process)".to_string(),
        });
    }

    Ok(())
}

/// Parse a single tag into (key, value) if it contains ':'.
/// Returns None for plain tags (no colon).
/// Splits on FIRST colon only -- value may contain additional colons.
fn parse_structured_tag(tag: &str) -> Option<(&str, &str)> {
    tag.split_once(':')
}

/// Validate a structured tag key-value pair.
fn validate_tag_key_value(key: &str, value: &str) -> Result<(), ServerError> {
    match key {
        "type" => {
            if !VALID_TYPES.contains(&value) {
                return Err(ServerError::InvalidInput {
                    field: "tags".to_string(),
                    reason: format!(
                        "invalid type value '{}'. Valid: {}",
                        value,
                        VALID_TYPES.join(", ")
                    ),
                });
            }
        }
        "result" => {
            if !VALID_RESULTS.contains(&value) {
                return Err(ServerError::InvalidInput {
                    field: "tags".to_string(),
                    reason: format!(
                        "invalid result value '{}'. Valid: {}",
                        value,
                        VALID_RESULTS.join(", ")
                    ),
                });
            }
        }
        "phase" => {
            if !VALID_PHASES.contains(&value) {
                return Err(ServerError::InvalidInput {
                    field: "tags".to_string(),
                    reason: format!(
                        "invalid phase value '{}'. Valid: {}",
                        value,
                        VALID_PHASES.join(", ")
                    ),
                });
            }
        }
        "gate" => {
            if value.is_empty() {
                return Err(ServerError::InvalidInput {
                    field: "tags".to_string(),
                    reason: "gate tag value cannot be empty".to_string(),
                });
            }
        }
        "agent" => {
            if value.is_empty() {
                return Err(ServerError::InvalidInput {
                    field: "tags".to_string(),
                    reason: "agent tag value cannot be empty".to_string(),
                });
            }
        }
        "wave" => {
            if value.parse::<u32>().is_err() {
                return Err(ServerError::InvalidInput {
                    field: "tags".to_string(),
                    reason: format!(
                        "wave value '{}' must be a non-negative integer",
                        value
                    ),
                });
            }
        }
        _ => {
            // Already checked in validate_outcome_tags, but defensive
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    // -- Validation Acceptance --

    #[test]
    fn test_all_recognized_keys_accepted() {
        let t = tags(&[
            "type:feature",
            "gate:3a",
            "phase:implementation",
            "result:pass",
            "agent:col-001-validator",
            "wave:2",
        ]);
        assert!(validate_outcome_tags(&t).is_ok());
    }

    #[test]
    fn test_type_feature_accepted() {
        assert!(validate_outcome_tags(&tags(&["type:feature"])).is_ok());
    }

    #[test]
    fn test_type_bugfix_accepted() {
        assert!(validate_outcome_tags(&tags(&["type:bugfix"])).is_ok());
    }

    #[test]
    fn test_type_incident_accepted() {
        assert!(validate_outcome_tags(&tags(&["type:incident"])).is_ok());
    }

    #[test]
    fn test_type_process_accepted() {
        assert!(validate_outcome_tags(&tags(&["type:process"])).is_ok());
    }

    #[test]
    fn test_result_pass() {
        assert!(validate_outcome_tags(&tags(&["type:feature", "result:pass"])).is_ok());
    }

    #[test]
    fn test_result_fail() {
        assert!(validate_outcome_tags(&tags(&["type:feature", "result:fail"])).is_ok());
    }

    #[test]
    fn test_result_rework() {
        assert!(validate_outcome_tags(&tags(&["type:feature", "result:rework"])).is_ok());
    }

    #[test]
    fn test_result_skip() {
        assert!(validate_outcome_tags(&tags(&["type:feature", "result:skip"])).is_ok());
    }

    #[test]
    fn test_gate_accepts_any_nonempty_string() {
        for gate in &["3a", "custom-gate", "1b", "\u{6d4b}\u{8bd5}"] {
            let t = tags(&["type:feature", &format!("gate:{gate}")]);
            assert!(validate_outcome_tags(&t).is_ok(), "gate:{gate} should be accepted");
        }
    }

    #[test]
    fn test_agent_accepts_any_nonempty_string() {
        let t = tags(&["type:feature", "agent:col-001-agent-1-architect"]);
        assert!(validate_outcome_tags(&t).is_ok());
    }

    #[test]
    fn test_agent_with_colons_accepted() {
        // agent:col-001:agent:1 -- split on first colon, value is "col-001:agent:1"
        let t = tags(&["type:feature", "agent:col-001:agent:1"]);
        assert!(validate_outcome_tags(&t).is_ok());
    }

    #[test]
    fn test_wave_accepts_valid_integers() {
        for wave in &["0", "2", "99"] {
            let t = tags(&["type:feature", &format!("wave:{wave}")]);
            assert!(validate_outcome_tags(&t).is_ok(), "wave:{wave} should be accepted");
        }
    }

    #[test]
    fn test_mixed_plain_and_structured_tags() {
        let t = tags(&["type:feature", "important", "reviewed"]);
        assert!(validate_outcome_tags(&t).is_ok());
    }

    #[test]
    fn test_plain_tag_with_type_passes() {
        let t = tags(&["type:feature", "important"]);
        assert!(validate_outcome_tags(&t).is_ok());
    }

    #[test]
    fn test_phase_all_values() {
        for phase in &["research", "design", "implementation", "testing", "validation"] {
            let t = tags(&["type:feature", &format!("phase:{phase}")]);
            assert!(validate_outcome_tags(&t).is_ok(), "phase:{phase} should be accepted");
        }
    }

    // -- Validation Rejection --

    #[test]
    fn test_missing_type_tag_rejected() {
        let t = tags(&["gate:3a", "result:pass"]);
        let err = validate_outcome_tags(&t).unwrap_err();
        match err {
            ServerError::InvalidInput { reason, .. } => {
                assert!(reason.contains("type tag is required"));
            }
            _ => panic!("expected InvalidInput"),
        }
    }

    #[test]
    fn test_unknown_key_rejected() {
        let t = tags(&["type:feature", "severity:high"]);
        let err = validate_outcome_tags(&t).unwrap_err();
        match err {
            ServerError::InvalidInput { reason, .. } => {
                assert!(reason.contains("unknown structured tag key 'severity'"));
                assert!(reason.contains("Recognized keys"));
            }
            _ => panic!("expected InvalidInput"),
        }
    }

    #[test]
    fn test_invalid_type_value_rejected() {
        let t = tags(&["type:unknown"]);
        let err = validate_outcome_tags(&t).unwrap_err();
        match err {
            ServerError::InvalidInput { reason, .. } => {
                assert!(reason.contains("invalid type value 'unknown'"));
                assert!(reason.contains("feature, bugfix, incident, process"));
            }
            _ => panic!("expected InvalidInput"),
        }
    }

    #[test]
    fn test_invalid_result_value_rejected() {
        let t = tags(&["type:feature", "result:maybe"]);
        let err = validate_outcome_tags(&t).unwrap_err();
        match err {
            ServerError::InvalidInput { reason, .. } => {
                assert!(reason.contains("invalid result value 'maybe'"));
            }
            _ => panic!("expected InvalidInput"),
        }
    }

    #[test]
    fn test_empty_gate_value_rejected() {
        let t = tags(&["type:feature", "gate:"]);
        let err = validate_outcome_tags(&t).unwrap_err();
        match err {
            ServerError::InvalidInput { reason, .. } => {
                assert!(reason.contains("gate tag value cannot be empty"));
            }
            _ => panic!("expected InvalidInput"),
        }
    }

    #[test]
    fn test_empty_agent_value_rejected() {
        let t = tags(&["type:feature", "agent:"]);
        let err = validate_outcome_tags(&t).unwrap_err();
        match err {
            ServerError::InvalidInput { reason, .. } => {
                assert!(reason.contains("agent tag value cannot be empty"));
            }
            _ => panic!("expected InvalidInput"),
        }
    }

    #[test]
    fn test_invalid_wave_value_rejected() {
        let t = tags(&["type:feature", "wave:abc"]);
        let err = validate_outcome_tags(&t).unwrap_err();
        match err {
            ServerError::InvalidInput { reason, .. } => {
                assert!(reason.contains("must be a non-negative integer"));
            }
            _ => panic!("expected InvalidInput"),
        }
    }

    #[test]
    fn test_duplicate_key_rejected() {
        let t = tags(&["type:feature", "type:bugfix"]);
        let err = validate_outcome_tags(&t).unwrap_err();
        match err {
            ServerError::InvalidInput { reason, .. } => {
                assert!(reason.contains("duplicate structured tag key 'type'"));
            }
            _ => panic!("expected InvalidInput"),
        }
    }

    #[test]
    fn test_invalid_phase_value_rejected() {
        let t = tags(&["type:feature", "phase:unknown"]);
        assert!(validate_outcome_tags(&t).is_err());
    }

    #[test]
    fn test_empty_type_value_rejected() {
        let t = tags(&["type:"]);
        assert!(validate_outcome_tags(&t).is_err());
    }

    // -- Error Message Quality (R-09) --

    #[test]
    fn test_missing_type_error_message() {
        let t = tags(&["result:pass"]);
        let err = validate_outcome_tags(&t).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("type tag is required"));
    }

    #[test]
    fn test_unknown_key_error_message() {
        let t = tags(&["type:feature", "foo:bar"]);
        let err = validate_outcome_tags(&t).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Recognized keys"));
    }

    #[test]
    fn test_invalid_type_value_error_message() {
        let t = tags(&["type:invalid"]);
        let err = validate_outcome_tags(&t).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("feature, bugfix, incident, process"));
    }
}
