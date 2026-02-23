//! Input validation for all tool parameters.
//!
//! Pure functions -- no I/O, no state. Each function takes parameter references
//! and returns Result<(), ServerError>.

use unimatrix_store::Status;

use crate::error::ServerError;
use crate::tools::{GetParams, LookupParams, SearchParams, StoreParams};

const MAX_TITLE_LEN: usize = 200;
const MAX_CONTENT_LEN: usize = 50_000;
const MAX_TOPIC_LEN: usize = 100;
const MAX_CATEGORY_LEN: usize = 50;
const MAX_TAG_LEN: usize = 50;
const MAX_TAGS_COUNT: usize = 20;
const MAX_QUERY_LEN: usize = 1_000;
const MAX_SOURCE_LEN: usize = 200;
const MAX_K: usize = 100;
const MAX_LIMIT: usize = 100;
const DEFAULT_K: usize = 5;
const DEFAULT_LIMIT: usize = 10;

fn check_length(field_name: &str, value: &str, max: usize) -> Result<(), ServerError> {
    if value.len() > max {
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
        _ => Err(ServerError::InvalidInput {
            field: "status".to_string(),
            reason: "must be active, deprecated, or proposed".to_string(),
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
    Ok(())
}

/// Validate context_get parameters.
pub fn validate_get_params(params: &GetParams) -> Result<(), ServerError> {
    validated_id(params.id)?;
    Ok(())
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
        };
        assert!(validate_store_params(&params).is_ok());
    }

    #[test]
    fn test_validate_get_params_negative_id() {
        let params = GetParams {
            id: -1,
            agent_id: None,
            format: None,
        };
        assert!(validate_get_params(&params).is_err());
    }
}
