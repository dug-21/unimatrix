# Pseudocode: outcome-tags (server crate)

## Purpose

New module `outcome_tags.rs` providing structured tag parsing and validation for outcome entries. Called from context_store only when category == "outcome".

## File: crates/unimatrix-server/src/outcome_tags.rs

```
use crate::error::ServerError;

// -- Recognized structured tag keys --

const RECOGNIZED_KEYS: &[&str] = &["type", "gate", "phase", "result", "agent", "wave"];

// -- Valid enum values --

const VALID_TYPES: &[&str] = &["feature", "bugfix", "incident", "process"];
const VALID_RESULTS: &[&str] = &["pass", "fail", "rework", "skip"];
const VALID_PHASES: &[&str] = &["research", "design", "implementation", "testing", "validation"];

// -- Public entry point --

/// Validate all tags for an outcome entry.
///
/// Rules:
/// 1. Tags without ':' are plain tags -- pass through, no validation.
/// 2. Tags with ':' are split on FIRST ':' into (key, value).
/// 3. Key must be in RECOGNIZED_KEYS -- unknown key is an error.
/// 4. The "type" tag is REQUIRED -- its absence is an error.
/// 5. Duplicate structured keys are rejected.
/// 6. Key-specific value validation:
///    - type: must be in VALID_TYPES
///    - result: must be in VALID_RESULTS
///    - phase: must be in VALID_PHASES
///    - gate: any non-empty string
///    - agent: any non-empty string
///    - wave: must parse as non-negative integer
pub fn validate_outcome_tags(tags: &[String]) -> Result<(), ServerError> {
    let mut has_type = false;
    let mut seen_keys: Vec<&str> = Vec::new();

    for tag in tags {
        // Parse structured tag
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
        // else: plain tag, no validation
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
                    reason: format!("wave value '{}' must be a non-negative integer", value),
                });
            }
        }
        _ => {
            // Already checked in validate_outcome_tags, but defensive
        }
    }
    Ok(())
}
```

## File: crates/unimatrix-server/src/lib.rs

Add module declaration:

```
pub mod outcome_tags;
```

## Invariants

- Validation only fires for category == "outcome" (caller responsibility in tools.rs)
- Plain tags (no colon) pass through without validation
- Split on first colon only -- values may contain colons (e.g., agent:col-001:agent:1)
- Case-sensitive: "Type:Feature" would be rejected (unknown key "Type")
- Error messages are descriptive: list recognized keys / valid values
