//! Input validation for all tool parameters.
//!
//! Pure functions -- no I/O, no state. Each function takes parameter references
//! and returns Result<(), ServerError>.

use std::collections::HashSet;

use unimatrix_store::Status;

use crate::error::ServerError;
use crate::infra::registry::{Capability, TrustLevel};
use crate::mcp::tools::{
    BriefingParams, CorrectParams, DeprecateParams, EnrollParams, GetParams, LookupParams,
    QuarantineParams, RetrospectiveParams, SearchParams, StatusParams, StoreParams,
};

const MAX_AGENT_ID_LEN: usize = 100;
const MAX_TITLE_LEN: usize = 200;
const MAX_CONTENT_LEN: usize = 50_000;
const MAX_TOPIC_LEN: usize = 100;
const MAX_CATEGORY_LEN: usize = 50;
const MAX_TAG_LEN: usize = 50;
const MAX_TAGS_COUNT: usize = 20;
const MAX_QUERY_LEN: usize = 1_000;
const MAX_SOURCE_LEN: usize = 200;
const MAX_REASON_LEN: usize = 1_000;
const MAX_FEATURE_LEN: usize = 100;
const MAX_FEATURE_CYCLE_LEN: usize = 128;
const MAX_ROLE_LEN: usize = 100;
const MAX_TASK_LEN: usize = 1_000;
const DEFAULT_MAX_TOKENS: usize = 3_000;
const MIN_MAX_TOKENS: usize = 500;
const MAX_MAX_TOKENS: usize = 10_000;
const MAX_K: usize = 100;
const MAX_LIMIT: usize = 100;
const DEFAULT_K: usize = 5;
const DEFAULT_LIMIT: usize = 10;

fn check_length(field_name: &str, value: &str, max: usize) -> Result<(), ServerError> {
    if value.chars().count() > max {
        return Err(ServerError::InvalidInput {
            field: field_name.to_string(),
            reason: format!("exceeds {max} characters"),
        });
    }
    Ok(())
}

fn check_control_chars(
    field_name: &str,
    value: &str,
    allow_newline_tab: bool,
) -> Result<(), ServerError> {
    for ch in value.chars() {
        let code = ch as u32;
        if code <= 0x1F {
            if allow_newline_tab && (ch == '\n' || ch == '\t') {
                continue;
            }
            return Err(ServerError::InvalidInput {
                field: field_name.to_string(),
                reason: format!("contains control character U+{code:04X}"),
            });
        }
    }
    Ok(())
}

fn validate_string_field(
    field_name: &str,
    value: &str,
    max: usize,
    allow_newline_tab: bool,
) -> Result<(), ServerError> {
    check_length(field_name, value, max)?;
    check_control_chars(field_name, value, allow_newline_tab)?;
    Ok(())
}

/// Convert i64 ID (from JSON) to u64, rejecting negative values.
pub fn validated_id(id: i64) -> Result<u64, ServerError> {
    if id < 0 {
        return Err(ServerError::InvalidInput {
            field: "id".to_string(),
            reason: "must be non-negative".to_string(),
        });
    }
    Ok(id as u64)
}

/// Validate and default the `k` parameter (search result count).
pub fn validated_k(k: Option<i64>) -> Result<usize, ServerError> {
    match k {
        None => Ok(DEFAULT_K),
        Some(v) if v <= 0 => Err(ServerError::InvalidInput {
            field: "k".to_string(),
            reason: "must be positive".to_string(),
        }),
        Some(v) if v as usize > MAX_K => Err(ServerError::InvalidInput {
            field: "k".to_string(),
            reason: format!("exceeds maximum {MAX_K}"),
        }),
        Some(v) => Ok(v as usize),
    }
}

/// Validate and default the `limit` parameter (lookup result count).
pub fn validated_limit(limit: Option<i64>) -> Result<usize, ServerError> {
    match limit {
        None => Ok(DEFAULT_LIMIT),
        Some(v) if v <= 0 => Err(ServerError::InvalidInput {
            field: "limit".to_string(),
            reason: "must be positive".to_string(),
        }),
        Some(v) if v as usize > MAX_LIMIT => Err(ServerError::InvalidInput {
            field: "limit".to_string(),
            reason: format!("exceeds maximum {MAX_LIMIT}"),
        }),
        Some(v) => Ok(v as usize),
    }
}

/// Parse a status string into a Status enum (case-insensitive).
pub fn parse_status(s: &str) -> Result<Status, ServerError> {
    match s.to_lowercase().as_str() {
        "active" => Ok(Status::Active),
        "deprecated" => Ok(Status::Deprecated),
        "proposed" => Ok(Status::Proposed),
        "quarantined" => Ok(Status::Quarantined),
        _ => Err(ServerError::InvalidInput {
            field: "status".to_string(),
            reason: "must be active, deprecated, proposed, or quarantined".to_string(),
        }),
    }
}

fn validate_optional_tags(tags: &Option<Vec<String>>) -> Result<(), ServerError> {
    if let Some(tags) = tags {
        if tags.len() > MAX_TAGS_COUNT {
            return Err(ServerError::InvalidInput {
                field: "tags".to_string(),
                reason: format!("exceeds {MAX_TAGS_COUNT} tags"),
            });
        }
        for tag in tags {
            validate_string_field("tags", tag, MAX_TAG_LEN, false)?;
        }
    }
    Ok(())
}

/// Validate context_search parameters.
pub fn validate_search_params(params: &SearchParams) -> Result<(), ServerError> {
    validate_string_field("query", &params.query, MAX_QUERY_LEN, false)?;
    if let Some(topic) = &params.topic {
        validate_string_field("topic", topic, MAX_TOPIC_LEN, false)?;
    }
    if let Some(category) = &params.category {
        validate_string_field("category", category, MAX_CATEGORY_LEN, false)?;
    }
    validate_optional_tags(&params.tags)?;
    Ok(())
}

/// Validate context_lookup parameters.
pub fn validate_lookup_params(params: &LookupParams) -> Result<(), ServerError> {
    if let Some(topic) = &params.topic {
        validate_string_field("topic", topic, MAX_TOPIC_LEN, false)?;
    }
    if let Some(category) = &params.category {
        validate_string_field("category", category, MAX_CATEGORY_LEN, false)?;
    }
    validate_optional_tags(&params.tags)?;
    if let Some(status) = &params.status {
        parse_status(status)?;
    }
    if let Some(id) = params.id {
        validated_id(id)?;
    }
    Ok(())
}

/// Validate context_store parameters.
pub fn validate_store_params(params: &StoreParams) -> Result<(), ServerError> {
    if let Some(title) = &params.title {
        validate_string_field("title", title, MAX_TITLE_LEN, true)?;
    }
    validate_string_field("content", &params.content, MAX_CONTENT_LEN, true)?;
    validate_string_field("topic", &params.topic, MAX_TOPIC_LEN, false)?;
    validate_string_field("category", &params.category, MAX_CATEGORY_LEN, false)?;
    validate_optional_tags(&params.tags)?;
    if let Some(source) = &params.source {
        validate_string_field("source", source, MAX_SOURCE_LEN, false)?;
    }
    if let Some(fc) = &params.feature_cycle {
        validate_string_field("feature_cycle", fc, MAX_FEATURE_CYCLE_LEN, false)?;
    }
    Ok(())
}

/// Validate context_get parameters.
pub fn validate_get_params(params: &GetParams) -> Result<(), ServerError> {
    validated_id(params.id)?;
    Ok(())
}

/// Validate context_correct parameters.
pub fn validate_correct_params(params: &CorrectParams) -> Result<(), ServerError> {
    validated_id(params.original_id)?;
    validate_string_field("content", &params.content, MAX_CONTENT_LEN, true)?;
    if let Some(reason) = &params.reason {
        validate_string_field("reason", reason, MAX_REASON_LEN, true)?;
    }
    if let Some(topic) = &params.topic {
        validate_string_field("topic", topic, MAX_TOPIC_LEN, false)?;
    }
    if let Some(category) = &params.category {
        validate_string_field("category", category, MAX_CATEGORY_LEN, false)?;
    }
    validate_optional_tags(&params.tags)?;
    if let Some(title) = &params.title {
        validate_string_field("title", title, MAX_TITLE_LEN, true)?;
    }
    Ok(())
}

/// Validate context_deprecate parameters.
pub fn validate_deprecate_params(params: &DeprecateParams) -> Result<(), ServerError> {
    validated_id(params.id)?;
    if let Some(reason) = &params.reason {
        validate_string_field("reason", reason, MAX_REASON_LEN, true)?;
    }
    Ok(())
}

/// Quarantine action enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuarantineAction {
    Quarantine,
    Restore,
}

/// Parse quarantine action string (default: "quarantine").
pub fn parse_quarantine_action(action: &Option<String>) -> Result<QuarantineAction, ServerError> {
    match action {
        None => Ok(QuarantineAction::Quarantine),
        Some(s) => match s.to_lowercase().as_str() {
            "quarantine" => Ok(QuarantineAction::Quarantine),
            "restore" => Ok(QuarantineAction::Restore),
            _ => Err(ServerError::InvalidInput {
                field: "action".to_string(),
                reason: "must be 'quarantine' or 'restore'".to_string(),
            }),
        },
    }
}

/// Validate context_quarantine parameters.
pub fn validate_quarantine_params(params: &QuarantineParams) -> Result<(), ServerError> {
    validated_id(params.id)?;
    if let Some(reason) = &params.reason {
        validate_string_field("reason", reason, MAX_REASON_LEN, true)?;
    }
    parse_quarantine_action(&params.action)?;
    Ok(())
}

/// Validate context_status parameters.
pub fn validate_status_params(params: &StatusParams) -> Result<(), ServerError> {
    if let Some(topic) = &params.topic {
        validate_string_field("topic", topic, MAX_TOPIC_LEN, false)?;
    }
    if let Some(category) = &params.category {
        validate_string_field("category", category, MAX_CATEGORY_LEN, false)?;
    }
    Ok(())
}

/// Validate context_briefing parameters.
pub fn validate_briefing_params(params: &BriefingParams) -> Result<(), ServerError> {
    validate_string_field("role", &params.role, MAX_ROLE_LEN, false)?;
    validate_string_field("task", &params.task, MAX_TASK_LEN, true)?;
    if let Some(feature) = &params.feature {
        validate_string_field("feature", feature, MAX_FEATURE_LEN, false)?;
    }
    Ok(())
}

/// Validate the optional `feature` parameter for usage tracking.
pub fn validate_feature(feature: &Option<String>) -> Result<(), ServerError> {
    if let Some(f) = feature {
        validate_string_field("feature", f, MAX_FEATURE_LEN, false)?;
    }
    Ok(())
}

/// Validate the optional `helpful` parameter for usage tracking.
///
/// No validation needed -- Option<bool> is self-validating from deserialization.
pub fn validate_helpful(helpful: &Option<bool>) -> Result<(), ServerError> {
    let _ = helpful;
    Ok(())
}

/// Validate and default the `max_tokens` parameter for context_briefing.
pub fn validated_max_tokens(max_tokens: Option<i64>) -> Result<usize, ServerError> {
    match max_tokens {
        None => Ok(DEFAULT_MAX_TOKENS),
        Some(v) if v < MIN_MAX_TOKENS as i64 => Err(ServerError::InvalidInput {
            field: "max_tokens".to_string(),
            reason: format!("minimum is {MIN_MAX_TOKENS}"),
        }),
        Some(v) if v > MAX_MAX_TOKENS as i64 => Err(ServerError::InvalidInput {
            field: "max_tokens".to_string(),
            reason: format!("maximum is {MAX_MAX_TOKENS}"),
        }),
        Some(v) => Ok(v as usize),
    }
}

// -- col-022 / crt-025: cycle lifecycle validation --

/// Shared event type constant for cycle_start events (ADR-001, R-04 mitigation).
pub const CYCLE_START_EVENT: &str = "cycle_start";

/// Shared event type constant for cycle_stop events (ADR-001, R-04 mitigation).
pub const CYCLE_STOP_EVENT: &str = "cycle_stop";

/// Shared event type constant for cycle_phase_end events (crt-025).
pub const CYCLE_PHASE_END_EVENT: &str = "cycle_phase_end";

/// Validation limits for cycle parameters.
const MAX_CYCLE_TOPIC_LEN: usize = 128;
const MAX_PHASE_LEN: usize = 64;
const MAX_OUTCOME_LEN: usize = 512;

/// The type of cycle event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleType {
    Start,
    /// Phase transition event (crt-025). Maps from "phase-end" wire value.
    PhaseEnd,
    Stop,
}

/// Validated cycle parameters, produced by `validate_cycle_params`.
#[derive(Debug, Clone)]
pub struct ValidatedCycleParams {
    pub cycle_type: CycleType,
    pub topic: String,
    /// Normalized phase string (lowercase, trimmed). Set on phase-end events.
    pub phase: Option<String>,
    /// Free-form outcome text (max 512 chars). Set on phase-end events.
    pub outcome: Option<String>,
    /// Normalized next-phase string (lowercase, trimmed). Set on start/phase-end events.
    pub next_phase: Option<String>,
}

/// Structural check for feature cycle identifiers.
///
/// Duplicated from `unimatrix-observe::attribution` (private fn) to avoid
/// promoting a private function to pub for a single consumer. The function
/// is trivial and the validation module already has overlapping checks.
///
/// Rules: non-empty, max 128 chars, contains hyphen, no leading/trailing
/// hyphens, only `[a-zA-Z0-9\-_.]`.
fn is_valid_feature_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_CYCLE_TOPIC_LEN
        && s.contains('-')
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Validate cycle parameters for both MCP tool and hook handler (ADR-004, C-02).
///
/// Returns `Result<ValidatedCycleParams, String>` (not `ServerError`) because
/// the hook handler needs a plain string error and does not use `ServerError`.
pub fn validate_cycle_params(
    type_str: &str,
    topic: &str,
    phase: Option<&str>,
    outcome: Option<&str>,
    next_phase: Option<&str>,
) -> Result<ValidatedCycleParams, String> {
    // Step 1: Validate type (case-sensitive, lowercase only)
    let cycle_type = match type_str {
        "start" => CycleType::Start,
        "phase-end" => CycleType::PhaseEnd,
        "stop" => CycleType::Stop,
        other => {
            return Err(format!(
                "invalid type '{other}': must be 'start', 'phase-end', or 'stop'"
            ));
        }
    };

    // Step 2: Validate topic
    if topic.is_empty() {
        return Err("topic must not be empty".to_string());
    }

    // Sanitize: strip control chars and non-ASCII, truncate to 128
    let clean_topic: String = topic
        .chars()
        .filter(|c| c.is_ascii() && !c.is_ascii_control())
        .take(MAX_CYCLE_TOPIC_LEN)
        .collect();

    if clean_topic.is_empty() {
        return Err("topic contains only invalid characters".to_string());
    }

    // Structural check: must look like a feature ID
    if !is_valid_feature_id(&clean_topic) {
        return Err("topic is not a valid feature cycle identifier".to_string());
    }

    // Step 3: Validate phase (crt-025)
    let validated_phase = validate_phase_field("phase", phase)?;

    // Step 4: Validate next_phase (crt-025)
    let validated_next_phase = validate_phase_field("next_phase", next_phase)?;

    // Step 5: Validate outcome (crt-025, FR-02.6)
    let validated_outcome = match outcome {
        None => None,
        Some(s) => {
            if s.chars().count() > MAX_OUTCOME_LEN {
                return Err("outcome exceeds 512 characters".to_string());
            }
            Some(s.to_string())
        }
    };

    Ok(ValidatedCycleParams {
        cycle_type,
        topic: clean_topic,
        phase: validated_phase,
        outcome: validated_outcome,
        next_phase: validated_next_phase,
    })
}

/// Validate and normalize a phase field (trim, lowercase, reject empty / spaces / > 64 chars).
///
/// Used for both `phase` and `next_phase` parameters (crt-025, C-01).
fn validate_phase_field(field_name: &str, value: Option<&str>) -> Result<Option<String>, String> {
    match value {
        None => Ok(None),
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Err(format!("{field_name} must not be empty when provided"));
            }
            let normalized = trimmed.to_lowercase();
            if normalized.chars().count() > MAX_PHASE_LEN {
                return Err(format!("{field_name} exceeds 64 characters"));
            }
            if normalized.contains(' ') {
                return Err(format!("{field_name} must not contain spaces"));
            }
            Ok(Some(normalized))
        }
    }
}

// -- alc-002: enrollment validation --

/// Validate context_enroll parameters (target_agent_id field only).
///
/// Trust level and capabilities are validated by their dedicated parsing
/// functions (`parse_trust_level`, `parse_capabilities`), called separately
/// in the tool handler.
pub fn validate_enroll_params(params: &EnrollParams) -> Result<(), ServerError> {
    if params.target_agent_id.is_empty() {
        return Err(ServerError::InvalidInput {
            field: "target_agent_id".to_string(),
            reason: "required".to_string(),
        });
    }
    validate_string_field(
        "target_agent_id",
        &params.target_agent_id,
        MAX_AGENT_ID_LEN,
        false,
    )?;
    Ok(())
}

/// Validate retrospective parameters.
pub fn validate_retrospective_params(params: &RetrospectiveParams) -> Result<(), ServerError> {
    if params.feature_cycle.trim().is_empty() {
        return Err(ServerError::InvalidInput {
            field: "feature_cycle".to_string(),
            reason: "must not be empty".to_string(),
        });
    }
    Ok(())
}

/// Parse a trust level string into a TrustLevel enum (case-insensitive, strict).
///
/// Per ADR-001: only four exact values accepted, no fallback default.
pub fn parse_trust_level(s: &str) -> Result<TrustLevel, ServerError> {
    match s.to_lowercase().as_str() {
        "system" => Ok(TrustLevel::System),
        "privileged" => Ok(TrustLevel::Privileged),
        "internal" => Ok(TrustLevel::Internal),
        "restricted" => Ok(TrustLevel::Restricted),
        _ => Err(ServerError::InvalidInput {
            field: "trust_level".to_string(),
            reason: "must be one of: system, privileged, internal, restricted".to_string(),
        }),
    }
}

/// Parse capability strings into Capability enums (case-insensitive, strict, no duplicates).
///
/// Per ADR-001: only four exact values accepted. Duplicates (case-insensitive) are rejected.
pub fn parse_capabilities(caps: &[String]) -> Result<Vec<Capability>, ServerError> {
    if caps.is_empty() {
        return Err(ServerError::InvalidInput {
            field: "capabilities".to_string(),
            reason: "at least one capability required".to_string(),
        });
    }

    let mut result = Vec::with_capacity(caps.len());
    let mut seen = HashSet::new();

    for cap_str in caps {
        let lower = cap_str.to_lowercase();

        if !seen.insert(lower.clone()) {
            return Err(ServerError::InvalidInput {
                field: "capabilities".to_string(),
                reason: format!("duplicate capability: {cap_str}"),
            });
        }

        let capability = match lower.as_str() {
            "read" => Capability::Read,
            "write" => Capability::Write,
            "search" => Capability::Search,
            "admin" => Capability::Admin,
            _ => {
                return Err(ServerError::InvalidInput {
                    field: "capabilities".to_string(),
                    reason: format!(
                        "unknown capability '{cap_str}'. Valid: read, write, search, admin"
                    ),
                });
            }
        };

        result.push(capability);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_title_at_max_length() {
        let s = "a".repeat(MAX_TITLE_LEN);
        assert!(check_length("title", &s, MAX_TITLE_LEN).is_ok());
    }

    #[test]
    fn test_title_over_max_length() {
        let s = "a".repeat(MAX_TITLE_LEN + 1);
        let err = check_length("title", &s, MAX_TITLE_LEN).unwrap_err();
        assert!(matches!(err, ServerError::InvalidInput { field, .. } if field == "title"));
    }

    #[test]
    fn test_content_at_max_length() {
        let s = "a".repeat(MAX_CONTENT_LEN);
        assert!(check_length("content", &s, MAX_CONTENT_LEN).is_ok());
    }

    #[test]
    fn test_content_over_max_length() {
        let s = "a".repeat(MAX_CONTENT_LEN + 1);
        assert!(check_length("content", &s, MAX_CONTENT_LEN).is_err());
    }

    #[test]
    fn test_query_at_max_length() {
        let s = "a".repeat(MAX_QUERY_LEN);
        assert!(check_length("query", &s, MAX_QUERY_LEN).is_ok());
    }

    #[test]
    fn test_query_over_max_length() {
        let s = "a".repeat(MAX_QUERY_LEN + 1);
        assert!(check_length("query", &s, MAX_QUERY_LEN).is_err());
    }

    #[test]
    fn test_topic_at_max_length() {
        let s = "a".repeat(MAX_TOPIC_LEN);
        assert!(check_length("topic", &s, MAX_TOPIC_LEN).is_ok());
    }

    #[test]
    fn test_topic_over_max_length() {
        let s = "a".repeat(MAX_TOPIC_LEN + 1);
        assert!(check_length("topic", &s, MAX_TOPIC_LEN).is_err());
    }

    #[test]
    fn test_source_at_max_length() {
        let s = "a".repeat(MAX_SOURCE_LEN);
        assert!(check_length("source", &s, MAX_SOURCE_LEN).is_ok());
    }

    #[test]
    fn test_source_over_max_length() {
        let s = "a".repeat(MAX_SOURCE_LEN + 1);
        assert!(check_length("source", &s, MAX_SOURCE_LEN).is_err());
    }

    #[test]
    fn test_content_allows_newline() {
        assert!(check_control_chars("content", "hello\nworld", true).is_ok());
    }

    #[test]
    fn test_content_allows_tab() {
        assert!(check_control_chars("content", "hello\tworld", true).is_ok());
    }

    #[test]
    fn test_topic_rejects_newline() {
        let err = check_control_chars("topic", "hello\nworld", false).unwrap_err();
        assert!(matches!(err, ServerError::InvalidInput { field, .. } if field == "topic"));
    }

    #[test]
    fn test_topic_rejects_null() {
        assert!(check_control_chars("topic", "hello\0world", false).is_err());
    }

    #[test]
    fn test_topic_rejects_control_char() {
        assert!(check_control_chars("topic", "hello\x01world", false).is_err());
    }

    #[test]
    fn test_tags_at_max_count() {
        let tags: Vec<String> = (0..MAX_TAGS_COUNT).map(|i| format!("tag{i}")).collect();
        assert!(validate_optional_tags(&Some(tags)).is_ok());
    }

    #[test]
    fn test_tags_over_max_count() {
        let tags: Vec<String> = (0..=MAX_TAGS_COUNT).map(|i| format!("tag{i}")).collect();
        assert!(validate_optional_tags(&Some(tags)).is_err());
    }

    #[test]
    fn test_individual_tag_at_max_length() {
        let tag = "a".repeat(MAX_TAG_LEN);
        assert!(validate_optional_tags(&Some(vec![tag])).is_ok());
    }

    #[test]
    fn test_individual_tag_over_max_length() {
        let tag = "a".repeat(MAX_TAG_LEN + 1);
        assert!(validate_optional_tags(&Some(vec![tag])).is_err());
    }

    #[test]
    fn test_validated_id_positive() {
        assert_eq!(validated_id(1).unwrap(), 1);
    }

    #[test]
    fn test_validated_id_negative() {
        let err = validated_id(-1).unwrap_err();
        assert!(matches!(err, ServerError::InvalidInput { field, .. } if field == "id"));
    }

    #[test]
    fn test_validated_id_zero() {
        assert_eq!(validated_id(0).unwrap(), 0);
    }

    #[test]
    fn test_validated_id_max() {
        assert_eq!(validated_id(i64::MAX).unwrap(), i64::MAX as u64);
    }

    #[test]
    fn test_validated_k_none_defaults_to_5() {
        assert_eq!(validated_k(None).unwrap(), 5);
    }

    #[test]
    fn test_validated_k_positive() {
        assert_eq!(validated_k(Some(10)).unwrap(), 10);
    }

    #[test]
    fn test_validated_k_zero_rejected() {
        assert!(validated_k(Some(0)).is_err());
    }

    #[test]
    fn test_validated_k_negative_rejected() {
        assert!(validated_k(Some(-1)).is_err());
    }

    #[test]
    fn test_validated_k_exceeds_max() {
        assert!(validated_k(Some(101)).is_err());
    }

    #[test]
    fn test_validated_limit_none_defaults_to_10() {
        assert_eq!(validated_limit(None).unwrap(), 10);
    }

    #[test]
    fn test_validated_limit_zero_rejected() {
        assert!(validated_limit(Some(0)).is_err());
    }

    #[test]
    fn test_parse_status_active() {
        assert_eq!(parse_status("active").unwrap(), Status::Active);
    }

    #[test]
    fn test_parse_status_deprecated() {
        assert_eq!(parse_status("deprecated").unwrap(), Status::Deprecated);
    }

    #[test]
    fn test_parse_status_proposed() {
        assert_eq!(parse_status("proposed").unwrap(), Status::Proposed);
    }

    #[test]
    fn test_parse_status_quarantined() {
        assert_eq!(parse_status("quarantined").unwrap(), Status::Quarantined);
        assert_eq!(parse_status("Quarantined").unwrap(), Status::Quarantined);
        assert_eq!(parse_status("QUARANTINED").unwrap(), Status::Quarantined);
    }

    #[test]
    fn test_parse_status_case_insensitive() {
        assert_eq!(parse_status("Active").unwrap(), Status::Active);
        assert_eq!(parse_status("DEPRECATED").unwrap(), Status::Deprecated);
    }

    #[test]
    fn test_parse_status_invalid() {
        assert!(parse_status("invalid").is_err());
    }

    #[test]
    fn test_validate_search_params_minimal() {
        let params = SearchParams {
            query: "test".to_string(),
            topic: None,
            category: None,
            tags: None,
            k: None,
            agent_id: None,
            format: None,
            feature: None,
            helpful: None,
            session_id: None,
        };
        assert!(validate_search_params(&params).is_ok());
    }

    #[test]
    fn test_validate_store_params_minimal() {
        let params = StoreParams {
            content: "test content".to_string(),
            topic: "auth".to_string(),
            category: "convention".to_string(),
            tags: None,
            title: None,
            source: None,
            agent_id: None,
            format: None,
            feature_cycle: None,
            session_id: None,
        };
        assert!(validate_store_params(&params).is_ok());
    }

    #[test]
    fn test_validate_store_params_all_fields() {
        let params = StoreParams {
            content: "test content".to_string(),
            topic: "auth".to_string(),
            category: "convention".to_string(),
            tags: Some(vec!["rust".to_string()]),
            title: Some("Test Title".to_string()),
            source: Some("test-source".to_string()),
            agent_id: Some("agent".to_string()),
            format: Some("json".to_string()),
            feature_cycle: Some("col-001".to_string()),
            session_id: None,
        };
        assert!(validate_store_params(&params).is_ok());
    }

    #[test]
    fn test_validate_store_params_feature_cycle_too_long() {
        let params = StoreParams {
            content: "test".to_string(),
            topic: "t".to_string(),
            category: "outcome".to_string(),
            tags: Some(vec!["type:feature".to_string()]),
            title: None,
            source: None,
            agent_id: None,
            format: None,
            feature_cycle: Some("a".repeat(129)),
            session_id: None,
        };
        assert!(validate_store_params(&params).is_err());
    }

    #[test]
    fn test_validate_store_params_feature_cycle_at_max() {
        let params = StoreParams {
            content: "test".to_string(),
            topic: "t".to_string(),
            category: "outcome".to_string(),
            tags: Some(vec!["type:feature".to_string()]),
            title: None,
            source: None,
            agent_id: None,
            format: None,
            feature_cycle: Some("a".repeat(128)),
            session_id: None,
        };
        assert!(validate_store_params(&params).is_ok());
    }

    #[test]
    fn test_validate_get_params_negative_id() {
        let params = GetParams {
            id: -1,
            agent_id: None,
            format: None,
            feature: None,
            helpful: None,
            session_id: None,
        };
        assert!(validate_get_params(&params).is_err());
    }

    // -- vnc-003: validate_correct_params --

    #[test]
    fn test_validate_correct_params_minimal() {
        let params = CorrectParams {
            original_id: 1,
            content: "corrected content".to_string(),
            reason: None,
            topic: None,
            category: None,
            tags: None,
            title: None,
            agent_id: None,
            format: None,
        };
        assert!(validate_correct_params(&params).is_ok());
    }

    #[test]
    fn test_validate_correct_params_all_fields() {
        let params = CorrectParams {
            original_id: 42,
            content: "corrected".to_string(),
            reason: Some("outdated".to_string()),
            topic: Some("auth".to_string()),
            category: Some("convention".to_string()),
            tags: Some(vec!["rust".to_string()]),
            title: Some("New Title".to_string()),
            agent_id: Some("agent".to_string()),
            format: Some("json".to_string()),
        };
        assert!(validate_correct_params(&params).is_ok());
    }

    #[test]
    fn test_validate_correct_params_negative_id() {
        let params = CorrectParams {
            original_id: -1,
            content: "corrected".to_string(),
            reason: None,
            topic: None,
            category: None,
            tags: None,
            title: None,
            agent_id: None,
            format: None,
        };
        assert!(validate_correct_params(&params).is_err());
    }

    #[test]
    fn test_validate_correct_params_content_too_long() {
        let params = CorrectParams {
            original_id: 1,
            content: "a".repeat(MAX_CONTENT_LEN + 1),
            reason: None,
            topic: None,
            category: None,
            tags: None,
            title: None,
            agent_id: None,
            format: None,
        };
        assert!(validate_correct_params(&params).is_err());
    }

    #[test]
    fn test_validate_correct_params_reason_too_long() {
        let params = CorrectParams {
            original_id: 1,
            content: "ok".to_string(),
            reason: Some("a".repeat(MAX_REASON_LEN + 1)),
            topic: None,
            category: None,
            tags: None,
            title: None,
            agent_id: None,
            format: None,
        };
        assert!(validate_correct_params(&params).is_err());
    }

    #[test]
    fn test_validate_correct_params_content_allows_newline() {
        let params = CorrectParams {
            original_id: 1,
            content: "line1\nline2\ttab".to_string(),
            reason: None,
            topic: None,
            category: None,
            tags: None,
            title: None,
            agent_id: None,
            format: None,
        };
        assert!(validate_correct_params(&params).is_ok());
    }

    // -- vnc-003: validate_deprecate_params --

    #[test]
    fn test_validate_deprecate_params_minimal() {
        let params = DeprecateParams {
            id: 1,
            reason: None,
            agent_id: None,
            format: None,
        };
        assert!(validate_deprecate_params(&params).is_ok());
    }

    #[test]
    fn test_validate_deprecate_params_negative_id() {
        let params = DeprecateParams {
            id: -1,
            reason: None,
            agent_id: None,
            format: None,
        };
        assert!(validate_deprecate_params(&params).is_err());
    }

    #[test]
    fn test_validate_deprecate_params_reason_valid() {
        let params = DeprecateParams {
            id: 1,
            reason: Some("outdated info".to_string()),
            agent_id: None,
            format: None,
        };
        assert!(validate_deprecate_params(&params).is_ok());
    }

    #[test]
    fn test_validate_deprecate_params_reason_too_long() {
        let params = DeprecateParams {
            id: 1,
            reason: Some("a".repeat(MAX_REASON_LEN + 1)),
            agent_id: None,
            format: None,
        };
        assert!(validate_deprecate_params(&params).is_err());
    }

    // -- vnc-003: validate_status_params --

    #[test]
    fn test_validate_status_params_empty() {
        let params = StatusParams {
            topic: None,
            category: None,
            agent_id: None,
            format: None,
            check_embeddings: None,
        };
        assert!(validate_status_params(&params).is_ok());
    }

    #[test]
    fn test_validate_status_params_topic_too_long() {
        let params = StatusParams {
            topic: Some("a".repeat(MAX_TOPIC_LEN + 1)),
            category: None,
            agent_id: None,
            format: None,
            check_embeddings: None,
        };
        assert!(validate_status_params(&params).is_err());
    }

    #[test]
    fn test_validate_status_params_category_control_chars() {
        let params = StatusParams {
            topic: None,
            category: Some("bad\x00cat".to_string()),
            agent_id: None,
            format: None,
            check_embeddings: None,
        };
        assert!(validate_status_params(&params).is_err());
    }

    // -- vnc-003: validate_briefing_params --

    #[test]
    fn test_validate_briefing_params_minimal() {
        let params = BriefingParams {
            role: "architect".to_string(),
            task: "design auth module".to_string(),
            feature: None,
            max_tokens: None,
            agent_id: None,
            format: None,
            helpful: None,
            session_id: None,
        };
        assert!(validate_briefing_params(&params).is_ok());
    }

    #[test]
    fn test_validate_briefing_params_role_too_long() {
        let params = BriefingParams {
            role: "a".repeat(MAX_ROLE_LEN + 1),
            task: "ok".to_string(),
            feature: None,
            max_tokens: None,
            agent_id: None,
            format: None,
            helpful: None,
            session_id: None,
        };
        assert!(validate_briefing_params(&params).is_err());
    }

    #[test]
    fn test_validate_briefing_params_task_too_long() {
        let params = BriefingParams {
            role: "ok".to_string(),
            task: "a".repeat(MAX_TASK_LEN + 1),
            feature: None,
            max_tokens: None,
            agent_id: None,
            format: None,
            helpful: None,
            session_id: None,
        };
        assert!(validate_briefing_params(&params).is_err());
    }

    #[test]
    fn test_validate_briefing_params_feature_valid() {
        let params = BriefingParams {
            role: "dev".to_string(),
            task: "impl".to_string(),
            feature: Some("vnc-003".to_string()),
            max_tokens: None,
            agent_id: None,
            format: None,
            helpful: None,
            session_id: None,
        };
        assert!(validate_briefing_params(&params).is_ok());
    }

    #[test]
    fn test_validate_briefing_params_feature_too_long() {
        let params = BriefingParams {
            role: "dev".to_string(),
            task: "impl".to_string(),
            feature: Some("a".repeat(MAX_FEATURE_LEN + 1)),
            max_tokens: None,
            agent_id: None,
            format: None,
            helpful: None,
            session_id: None,
        };
        assert!(validate_briefing_params(&params).is_err());
    }

    // -- vnc-003: validated_max_tokens --

    #[test]
    fn test_validated_max_tokens_none_default() {
        assert_eq!(validated_max_tokens(None).unwrap(), 3000);
    }

    #[test]
    fn test_validated_max_tokens_valid() {
        assert_eq!(validated_max_tokens(Some(1000)).unwrap(), 1000);
    }

    #[test]
    fn test_validated_max_tokens_min_boundary() {
        assert_eq!(validated_max_tokens(Some(500)).unwrap(), 500);
        assert!(validated_max_tokens(Some(499)).is_err());
    }

    #[test]
    fn test_validated_max_tokens_max_boundary() {
        assert_eq!(validated_max_tokens(Some(10000)).unwrap(), 10000);
        assert!(validated_max_tokens(Some(10001)).is_err());
    }

    #[test]
    fn test_validated_max_tokens_negative_rejected() {
        assert!(validated_max_tokens(Some(-1)).is_err());
        assert!(validated_max_tokens(Some(-100)).is_err());
        assert!(validated_max_tokens(Some(i64::MIN)).is_err());
    }

    #[test]
    fn test_validated_max_tokens_zero_rejected() {
        assert!(validated_max_tokens(Some(0)).is_err());
    }

    // -- alc-002: parse_trust_level --

    #[test]
    fn test_parse_trust_level_system() {
        assert_eq!(parse_trust_level("system").unwrap(), TrustLevel::System);
    }

    #[test]
    fn test_parse_trust_level_privileged() {
        assert_eq!(
            parse_trust_level("privileged").unwrap(),
            TrustLevel::Privileged
        );
    }

    #[test]
    fn test_parse_trust_level_internal() {
        assert_eq!(parse_trust_level("internal").unwrap(), TrustLevel::Internal);
    }

    #[test]
    fn test_parse_trust_level_restricted() {
        assert_eq!(
            parse_trust_level("restricted").unwrap(),
            TrustLevel::Restricted
        );
    }

    #[test]
    fn test_parse_trust_level_case_insensitive() {
        assert_eq!(parse_trust_level("SYSTEM").unwrap(), TrustLevel::System);
        assert_eq!(
            parse_trust_level("Privileged").unwrap(),
            TrustLevel::Privileged
        );
    }

    #[test]
    fn test_parse_trust_level_invalid_admin() {
        assert!(parse_trust_level("admin").is_err());
    }

    #[test]
    fn test_parse_trust_level_empty() {
        assert!(parse_trust_level("").is_err());
    }

    #[test]
    fn test_parse_trust_level_trailing_space() {
        assert!(parse_trust_level("system ").is_err());
    }

    #[test]
    fn test_parse_trust_level_unknown() {
        assert!(parse_trust_level("superadmin").is_err());
    }

    // -- alc-002: parse_capabilities --

    #[test]
    fn test_parse_capabilities_single() {
        let caps = parse_capabilities(&["read".to_string()]).unwrap();
        assert_eq!(caps, vec![Capability::Read]);
    }

    #[test]
    fn test_parse_capabilities_all_four() {
        let caps = parse_capabilities(&[
            "read".to_string(),
            "write".to_string(),
            "search".to_string(),
            "admin".to_string(),
        ])
        .unwrap();
        assert_eq!(
            caps,
            vec![
                Capability::Read,
                Capability::Write,
                Capability::Search,
                Capability::Admin
            ]
        );
    }

    #[test]
    fn test_parse_capabilities_case_insensitive() {
        let caps = parse_capabilities(&["READ".to_string(), "Write".to_string()]).unwrap();
        assert_eq!(caps, vec![Capability::Read, Capability::Write]);
    }

    #[test]
    fn test_parse_capabilities_empty_vec() {
        assert!(parse_capabilities(&[]).is_err());
    }

    #[test]
    fn test_parse_capabilities_duplicate() {
        assert!(parse_capabilities(&["read".to_string(), "read".to_string()]).is_err());
    }

    #[test]
    fn test_parse_capabilities_case_insensitive_duplicate() {
        assert!(parse_capabilities(&["read".to_string(), "READ".to_string()]).is_err());
    }

    #[test]
    fn test_parse_capabilities_unknown() {
        assert!(parse_capabilities(&["unknown".to_string()]).is_err());
    }

    #[test]
    fn test_parse_capabilities_empty_string() {
        assert!(parse_capabilities(&["".to_string()]).is_err());
    }

    // -- alc-002: validate_enroll_params --

    #[test]
    fn test_validate_enroll_params_valid() {
        let params = EnrollParams {
            target_agent_id: "test-agent".to_string(),
            trust_level: "internal".to_string(),
            capabilities: vec!["read".to_string()],
            agent_id: None,
            format: None,
        };
        assert!(validate_enroll_params(&params).is_ok());
    }

    #[test]
    fn test_validate_enroll_params_empty_target() {
        let params = EnrollParams {
            target_agent_id: "".to_string(),
            trust_level: "internal".to_string(),
            capabilities: vec!["read".to_string()],
            agent_id: None,
            format: None,
        };
        assert!(validate_enroll_params(&params).is_err());
    }

    #[test]
    fn test_validate_enroll_params_control_chars() {
        let params = EnrollParams {
            target_agent_id: "agent\x00bad".to_string(),
            trust_level: "internal".to_string(),
            capabilities: vec!["read".to_string()],
            agent_id: None,
            format: None,
        };
        assert!(validate_enroll_params(&params).is_err());
    }

    #[test]
    fn test_validate_enroll_params_max_length() {
        let params_ok = EnrollParams {
            target_agent_id: "a".repeat(100),
            trust_level: "internal".to_string(),
            capabilities: vec!["read".to_string()],
            agent_id: None,
            format: None,
        };
        assert!(validate_enroll_params(&params_ok).is_ok());

        let params_over = EnrollParams {
            target_agent_id: "a".repeat(101),
            trust_level: "internal".to_string(),
            capabilities: vec!["read".to_string()],
            agent_id: None,
            format: None,
        };
        assert!(validate_enroll_params(&params_over).is_err());
    }

    // -- col-002: validate_retrospective_params --

    #[test]
    fn test_validate_retrospective_params_valid() {
        let params = RetrospectiveParams {
            feature_cycle: "col-002".to_string(),
            agent_id: None,
            evidence_limit: None,
            format: None,
        };
        assert!(validate_retrospective_params(&params).is_ok());
    }

    #[test]
    fn test_validate_retrospective_params_empty() {
        let params = RetrospectiveParams {
            feature_cycle: "".to_string(),
            agent_id: None,
            evidence_limit: None,
            format: None,
        };
        assert!(validate_retrospective_params(&params).is_err());
    }

    #[test]
    fn test_validate_retrospective_params_whitespace_only() {
        let params = RetrospectiveParams {
            feature_cycle: "   ".to_string(),
            agent_id: None,
            evidence_limit: None,
            format: None,
        };
        assert!(validate_retrospective_params(&params).is_err());
    }

    // -- col-022 / crt-025: validate_cycle_params -- type parameter (AC-07) --

    #[test]
    fn test_validate_cycle_params_type_start() {
        let result = validate_cycle_params("start", "col-022", None, None, None);
        let v = result.unwrap();
        assert_eq!(v.cycle_type, CycleType::Start);
        assert_eq!(v.topic, "col-022");
    }

    #[test]
    fn test_validate_cycle_params_type_stop() {
        let result = validate_cycle_params("stop", "col-022", None, None, None);
        let v = result.unwrap();
        assert_eq!(v.cycle_type, CycleType::Stop);
    }

    #[test]
    fn test_validate_cycle_params_type_invalid_pause() {
        let result = validate_cycle_params("pause", "col-022", None, None, None);
        let err = result.unwrap_err();
        assert!(err.contains("start"));
        assert!(err.contains("phase-end"));
        assert!(err.contains("stop"));
    }

    #[test]
    fn test_validate_cycle_params_type_invalid_restart() {
        let result = validate_cycle_params("restart", "col-022", None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cycle_params_type_empty() {
        let result = validate_cycle_params("", "col-022", None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cycle_params_type_case_sensitive_start_upper() {
        let result = validate_cycle_params("Start", "col-022", None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cycle_params_type_case_sensitive_stop_upper() {
        let result = validate_cycle_params("STOP", "col-022", None, None, None);
        assert!(result.is_err());
    }

    // -- crt-025: validate_cycle_params -- PhaseEnd type --

    #[test]
    fn test_validate_cycle_params_type_phase_end_accepted() {
        let result = validate_cycle_params("phase-end", "crt-025", None, None, None);
        let v = result.unwrap();
        assert_eq!(v.cycle_type, CycleType::PhaseEnd);
    }

    // -- col-022: validate_cycle_params -- topic parameter (AC-06) --

    #[test]
    fn test_validate_cycle_params_topic_valid() {
        let result = validate_cycle_params("start", "col-022", None, None, None);
        let v = result.unwrap();
        assert_eq!(v.topic, "col-022");
    }

    #[test]
    fn test_validate_cycle_params_topic_empty() {
        let result = validate_cycle_params("start", "", None, None, None);
        let err = result.unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn test_validate_cycle_params_topic_max_length_128() {
        // 128 chars with a hyphen to pass is_valid_feature_id
        let topic = format!("{}-{}", "a".repeat(64), "b".repeat(63));
        assert_eq!(topic.len(), 128);
        let result = validate_cycle_params("start", &topic, None, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_cycle_params_topic_over_max_129() {
        // 129 chars -- sanitize truncates to 128, but the truncated result
        // must still pass is_valid_feature_id. Build a valid 128-char ID
        // and append one extra char.
        let base = format!("{}-{}", "a".repeat(64), "b".repeat(63));
        assert_eq!(base.len(), 128);
        let topic = format!("{}x", base);
        assert_eq!(topic.len(), 129);
        // After truncation to 128, the result is the base which is valid
        let result = validate_cycle_params("start", &topic, None, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_cycle_params_topic_control_chars_stripped() {
        let result = validate_cycle_params("start", "col-022\t\r\n", None, None, None);
        let v = result.unwrap();
        assert_eq!(v.topic, "col-022");
    }

    #[test]
    fn test_validate_cycle_params_topic_only_control_chars() {
        let result = validate_cycle_params("start", "\x00\x01\x02", None, None, None);
        let err = result.unwrap_err();
        assert!(err.contains("invalid characters"));
    }

    // -- col-022: validate_cycle_params -- topic structural check (R-11) --

    #[test]
    fn test_validate_cycle_params_topic_valid_feature_id_format() {
        let result = validate_cycle_params("start", "col-022", None, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_cycle_params_topic_no_hyphen_rejected() {
        let result = validate_cycle_params("start", "foobar", None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cycle_params_topic_leading_hyphen_rejected() {
        let result = validate_cycle_params("start", "-col022", None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cycle_params_topic_trailing_hyphen_rejected() {
        let result = validate_cycle_params("start", "col022-", None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_cycle_params_topic_feature_ids() {
        // Various valid feature IDs
        assert!(validate_cycle_params("start", "col-022", None, None, None).is_ok());
        assert!(validate_cycle_params("start", "nxs-001", None, None, None).is_ok());
        assert!(validate_cycle_params("start", "ASS-014", None, None, None).is_ok());
        assert!(validate_cycle_params("start", "c-1", None, None, None).is_ok());
        assert!(validate_cycle_params("start", "ab-999", None, None, None).is_ok());

        // Invalid: no hyphen
        assert!(validate_cycle_params("start", "col022", None, None, None).is_err());
    }

    // -- col-022: CycleType enum --

    #[test]
    fn test_cycle_type_start_variant() {
        let start = CycleType::Start;
        let stop = CycleType::Stop;
        assert_ne!(start, stop);
    }

    #[test]
    fn test_validated_cycle_params_fields() {
        let params = ValidatedCycleParams {
            cycle_type: CycleType::Start,
            topic: "x-1".to_string(),
            phase: None,
            outcome: None,
            next_phase: None,
        };
        assert_eq!(params.cycle_type, CycleType::Start);
        assert_eq!(params.topic, "x-1");
        assert!(params.phase.is_none());
    }

    // -- col-022: edge cases --

    #[test]
    fn test_validate_cycle_params_topic_with_null_byte() {
        // Null byte is a control char, stripped by sanitize
        let result = validate_cycle_params("start", "col\x00-022", None, None, None);
        let v = result.unwrap();
        assert_eq!(v.topic, "col-022");
    }

    // -- col-022: event type constants --

    #[test]
    fn test_cycle_event_constants() {
        assert_eq!(CYCLE_START_EVENT, "cycle_start");
        assert_eq!(CYCLE_STOP_EVENT, "cycle_stop");
    }

    // -- crt-025: validate_cycle_params -- phase normalization (AC-03, FR-02) --

    #[test]
    fn test_validate_phase_lowercase_normalization() {
        let result = validate_cycle_params("phase-end", "crt-025", Some("Scope"), None, None);
        let v = result.unwrap();
        assert_eq!(v.phase, Some("scope".to_string()));
    }

    #[test]
    fn test_validate_phase_uppercase_normalization() {
        let result =
            validate_cycle_params("phase-end", "crt-025", Some("IMPLEMENTATION"), None, None);
        let v = result.unwrap();
        assert_eq!(v.phase, Some("implementation".to_string()));
    }

    #[test]
    fn test_validate_phase_mixed_case_normalization() {
        let result = validate_cycle_params("phase-end", "crt-025", Some("Design"), None, None);
        let v = result.unwrap();
        assert_eq!(v.phase, Some("design".to_string()));
    }

    #[test]
    fn test_validate_next_phase_normalization() {
        let result = validate_cycle_params("phase-end", "crt-025", None, None, Some("Design"));
        let v = result.unwrap();
        assert_eq!(v.next_phase, Some("design".to_string()));
    }

    #[test]
    fn test_validate_phase_none_always_valid() {
        // phase = None is valid for any event type (FR-02.5)
        assert!(validate_cycle_params("start", "crt-025", None, None, None).is_ok());
        assert!(validate_cycle_params("phase-end", "crt-025", None, None, None).is_ok());
        assert!(validate_cycle_params("stop", "crt-025", None, None, None).is_ok());
    }

    // -- crt-025: validate_cycle_params -- phase format rejection (R-06, FR-02) --

    #[test]
    fn test_validate_phase_space_rejected() {
        let result =
            validate_cycle_params("phase-end", "crt-025", Some("scope review"), None, None);
        let err = result.unwrap_err();
        assert!(err.contains("phase"));
    }

    #[test]
    fn test_validate_phase_leading_space_trimmed_internal_space_rejected() {
        // "a b" has an internal space after trim — rejected
        let result = validate_cycle_params("phase-end", "crt-025", Some("a b"), None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_phase_leading_trailing_space_trimmed_passes() {
        // " scope " trims to "scope" — no internal space, passes
        let result = validate_cycle_params("phase-end", "crt-025", Some(" scope "), None, None);
        let v = result.unwrap();
        assert_eq!(v.phase, Some("scope".to_string()));
    }

    #[test]
    fn test_validate_phase_empty_rejected() {
        let result = validate_cycle_params("phase-end", "crt-025", Some(""), None, None);
        let err = result.unwrap_err();
        assert!(err.contains("phase"));
    }

    #[test]
    fn test_validate_phase_64_char_boundary_accepted() {
        let phase_64 = "a".repeat(64);
        let result = validate_cycle_params("phase-end", "crt-025", Some(&phase_64), None, None);
        let v = result.unwrap();
        assert_eq!(v.phase, Some("a".repeat(64)));
    }

    #[test]
    fn test_validate_phase_65_char_rejected() {
        let phase_65 = "a".repeat(65);
        let result = validate_cycle_params("phase-end", "crt-025", Some(&phase_65), None, None);
        let err = result.unwrap_err();
        assert!(err.contains("phase") || err.contains("64"));
    }

    #[test]
    fn test_validate_phase_underscore_accepted() {
        // Underscore is not a space — passes format check (R-06 clarification)
        let result = validate_cycle_params("phase-end", "crt-025", Some("gate_review"), None, None);
        let v = result.unwrap();
        assert_eq!(v.phase, Some("gate_review".to_string()));
    }

    // -- crt-025: validate_cycle_params -- outcome validation (FR-02.6) --

    #[test]
    fn test_validate_outcome_max_512_chars_accepted() {
        let outcome_512 = "x".repeat(512);
        let result = validate_cycle_params("phase-end", "crt-025", None, Some(&outcome_512), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_outcome_513_chars_rejected() {
        let outcome_513 = "x".repeat(513);
        let result = validate_cycle_params("phase-end", "crt-025", None, Some(&outcome_513), None);
        let err = result.unwrap_err();
        assert!(err.contains("outcome"));
    }

    #[test]
    fn test_validate_outcome_none_always_valid() {
        let result = validate_cycle_params("phase-end", "crt-025", None, None, None);
        let v = result.unwrap();
        assert!(v.outcome.is_none());
    }

    // -- crt-025: CYCLE_PHASE_END_EVENT constant --

    #[test]
    fn test_cycle_phase_end_event_constant_value() {
        assert_eq!(CYCLE_PHASE_END_EVENT, "cycle_phase_end");
    }

    // -- #340: multibyte character boundary tests (chars().count() fix) --

    /// check_length: exactly MAX_TITLE_LEN multibyte chars — must pass.
    ///
    /// Each "🟩" is 4 bytes. Before the fix, 512 * 4 = 2048 bytes > 200 byte limit
    /// would cause a false rejection. After fix, char count is used.
    #[test]
    fn test_check_length_multibyte_at_max_passes() {
        // Use MAX_TITLE_LEN (200) as a representative general-field boundary.
        let s: String = "🟩".repeat(MAX_TITLE_LEN);
        assert_eq!(s.chars().count(), MAX_TITLE_LEN);
        assert!(check_length("title", &s, MAX_TITLE_LEN).is_ok());
    }

    /// check_length: MAX_TITLE_LEN + 1 multibyte chars — must be rejected.
    #[test]
    fn test_check_length_multibyte_over_max_rejected() {
        let s: String = "🟩".repeat(MAX_TITLE_LEN + 1);
        assert_eq!(s.chars().count(), MAX_TITLE_LEN + 1);
        assert!(check_length("title", &s, MAX_TITLE_LEN).is_err());
    }

    /// validate_cycle_params outcome: exactly MAX_OUTCOME_LEN (512) multibyte chars — must pass.
    ///
    /// Before the fix, 512 * 4 = 2048 bytes > 512 would cause a false rejection.
    #[test]
    fn test_validate_outcome_multibyte_at_max_passes() {
        let outcome: String = "🟩".repeat(MAX_OUTCOME_LEN);
        assert_eq!(outcome.chars().count(), MAX_OUTCOME_LEN);
        let result = validate_cycle_params("phase-end", "crt-025", None, Some(&outcome), None);
        assert!(result.is_ok());
    }

    /// validate_cycle_params outcome: MAX_OUTCOME_LEN + 1 multibyte chars — must be rejected.
    #[test]
    fn test_validate_outcome_multibyte_over_max_rejected() {
        let outcome: String = "🟩".repeat(MAX_OUTCOME_LEN + 1);
        assert_eq!(outcome.chars().count(), MAX_OUTCOME_LEN + 1);
        let result = validate_cycle_params("phase-end", "crt-025", None, Some(&outcome), None);
        let err = result.unwrap_err();
        assert!(err.contains("outcome"));
    }

    /// validate_phase_field (via validate_cycle_params): exactly MAX_PHASE_LEN (64) multibyte
    /// chars — must pass.
    ///
    /// Note: validate_phase_field calls .to_lowercase() before the length check, which for
    /// multibyte chars (e.g. emoji) returns the same char, so char count is preserved.
    #[test]
    fn test_validate_phase_multibyte_at_max_passes() {
        // Use chars that survive to_lowercase unchanged (emoji).
        let phase: String = "🟩".repeat(MAX_PHASE_LEN);
        assert_eq!(phase.chars().count(), MAX_PHASE_LEN);
        let result = validate_cycle_params("phase-end", "crt-025", Some(&phase), None, None);
        assert!(result.is_ok());
    }

    /// validate_phase_field: MAX_PHASE_LEN + 1 multibyte chars — must be rejected.
    #[test]
    fn test_validate_phase_multibyte_over_max_rejected() {
        let phase: String = "🟩".repeat(MAX_PHASE_LEN + 1);
        assert_eq!(phase.chars().count(), MAX_PHASE_LEN + 1);
        let result = validate_cycle_params("phase-end", "crt-025", Some(&phase), None, None);
        let err = result.unwrap_err();
        assert!(err.contains("phase") || err.contains("64"));
    }

    /// validate_phase_field (next_phase): exactly MAX_PHASE_LEN multibyte chars — must pass.
    #[test]
    fn test_validate_next_phase_multibyte_at_max_passes() {
        let phase: String = "🟩".repeat(MAX_PHASE_LEN);
        assert_eq!(phase.chars().count(), MAX_PHASE_LEN);
        let result = validate_cycle_params("phase-end", "crt-025", None, None, Some(&phase));
        assert!(result.is_ok());
    }

    /// validate_phase_field (next_phase): MAX_PHASE_LEN + 1 multibyte chars — must be rejected.
    #[test]
    fn test_validate_next_phase_multibyte_over_max_rejected() {
        let phase: String = "🟩".repeat(MAX_PHASE_LEN + 1);
        assert_eq!(phase.chars().count(), MAX_PHASE_LEN + 1);
        let result = validate_cycle_params("phase-end", "crt-025", None, None, Some(&phase));
        let err = result.unwrap_err();
        assert!(err.contains("next_phase") || err.contains("64"));
    }
}
