//! Feature attribution logic: session-to-feature mapping via content-based sequential scanning.

use crate::types::{ObservationRecord, ParsedSession};

/// Known feature phase prefixes for pattern matching.
const KNOWN_PREFIXES: &[&str] = &[
    "ass", "nxs", "col", "vnc", "alc", "crt", "mtx", "dsn", "nan",
];

/// Check if a string is a valid feature ID (e.g., "col-002", "nxs-001").
fn is_valid_feature_id(s: &str) -> bool {
    let parts: Vec<&str> = s.splitn(2, '-').collect();
    if parts.len() != 2 {
        return false;
    }
    !parts[0].is_empty()
        && parts[0].chars().all(|c| c.is_ascii_alphabetic())
        && !parts[1].is_empty()
        && parts[1].chars().all(|c| c.is_ascii_digit())
}

/// Extract feature ID from a file path like "product/features/col-002/...".
fn extract_from_path(s: &str) -> Option<String> {
    let marker = "product/features/";
    let mut start = 0;
    while let Some(idx) = s[start..].find(marker) {
        let after = start + idx + marker.len();
        if let Some(segment) = s[after..].split('/').next() {
            if is_valid_feature_id(segment) {
                return Some(segment.to_string());
            }
        }
        start = after;
    }
    None
}

/// Extract feature ID pattern from text (word-boundary aware).
fn extract_feature_id_pattern(s: &str) -> Option<String> {
    for word in s.split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '(' || c == ')') {
        let candidate = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        if is_valid_feature_id(candidate) {
            if let Some(prefix) = candidate.split('-').next() {
                if KNOWN_PREFIXES.contains(&prefix) {
                    return Some(candidate.to_string());
                }
            }
        }
    }
    None
}

/// Extract feature ID from git checkout pattern like "feature/col-002".
fn extract_from_git_checkout(s: &str) -> Option<String> {
    if let Some(idx) = s.find("feature/") {
        let rest = &s[idx + 8..];
        let candidate: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-')
            .collect();
        if is_valid_feature_id(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Extract a feature signal from a single record.
fn extract_feature_signal(record: &ObservationRecord) -> Option<String> {
    if let Some(input) = &record.input {
        let input_str = match input {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Object(_) => serde_json::to_string(input).unwrap_or_default(),
            _ => String::new(),
        };

        // Priority order per FR-04.3:
        // (a) File paths matching product/features/{id}/
        if let Some(id) = extract_from_path(&input_str) {
            return Some(id);
        }

        // (b) Task subjects containing feature IDs
        if let Some(id) = extract_feature_id_pattern(&input_str) {
            return Some(id);
        }

        // (c) Git checkout commands with feature/{id}
        if let Some(id) = extract_from_git_checkout(&input_str) {
            return Some(id);
        }
    }
    None
}

/// Attribute parsed sessions to a target feature.
///
/// Walks records in timestamp order within each session, partitions by feature switch points,
/// and returns only records attributed to the target feature.
pub fn attribute_sessions(
    sessions: &[ParsedSession],
    target_feature: &str,
) -> Vec<ObservationRecord> {
    let mut attributed = Vec::new();

    for session in sessions {
        let mut current_feature: Option<String> = None;
        let mut partitions: Vec<(Option<String>, Vec<&ObservationRecord>)> = Vec::new();
        let mut current_records: Vec<&ObservationRecord> = Vec::new();

        for record in &session.records {
            if let Some(signal) = extract_feature_signal(record) {
                if current_feature.as_deref() != Some(&signal) {
                    // Feature switch point
                    if !current_records.is_empty() {
                        partitions.push((current_feature.clone(), std::mem::take(&mut current_records)));
                    }
                    current_feature = Some(signal);
                }
            }
            current_records.push(record);
        }
        if !current_records.is_empty() {
            partitions.push((current_feature.clone(), current_records));
        }

        // FR-04.4: Records before any feature ID -> attributed to first feature found
        let first_feature = partitions
            .iter()
            .find_map(|(f, _)| f.clone());

        for (feature, records) in &partitions {
            let effective_feature = feature.as_ref().or(first_feature.as_ref());
            if effective_feature.is_some_and(|f| f == target_feature) {
                for record in records {
                    attributed.push((*record).clone());
                }
            }
        }
    }

    attributed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookType;

    fn make_record(ts: u64, tool: Option<&str>, input_str: Option<&str>) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
            session_id: "sess-1".to_string(),
            tool: tool.map(|s| s.to_string()),
            input: input_str.map(|s| serde_json::Value::String(s.to_string())),
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_session(id: &str, records: Vec<ObservationRecord>) -> ParsedSession {
        ParsedSession {
            session_id: id.to_string(),
            records,
        }
    }

    #[test]
    fn test_extract_feature_from_path() {
        assert_eq!(
            extract_from_path("product/features/col-002/SCOPE.md"),
            Some("col-002".to_string())
        );
    }

    #[test]
    fn test_extract_feature_from_task_subject() {
        assert_eq!(
            extract_feature_id_pattern("Working on col-002 design"),
            Some("col-002".to_string())
        );
    }

    #[test]
    fn test_extract_feature_from_git_checkout() {
        assert_eq!(
            extract_from_git_checkout("git checkout -b feature/col-002"),
            Some("col-002".to_string())
        );
    }

    #[test]
    fn test_extract_no_feature_signal() {
        assert_eq!(extract_from_path("some/other/path.rs"), None);
        assert_eq!(extract_feature_id_pattern("regular text"), None);
        assert_eq!(extract_from_git_checkout("git status"), None);
    }

    #[test]
    fn test_is_valid_feature_id_positive() {
        assert!(is_valid_feature_id("col-002"));
        assert!(is_valid_feature_id("nxs-001"));
        assert!(is_valid_feature_id("alc-002"));
    }

    #[test]
    fn test_is_valid_feature_id_negative() {
        assert!(!is_valid_feature_id("col"));
        assert!(!is_valid_feature_id("002"));
        assert!(!is_valid_feature_id("col-"));
        assert!(!is_valid_feature_id("-002"));
        assert!(!is_valid_feature_id(""));
        assert!(!is_valid_feature_id("col-abc"));
    }

    #[test]
    fn test_attribute_single_feature_session() {
        let records = vec![
            make_record(1000, Some("Read"), Some("product/features/col-002/SCOPE.md")),
            make_record(2000, Some("Write"), Some("product/features/col-002/test.rs")),
        ];
        let sessions = vec![make_session("s1", records)];

        let result = attribute_sessions(&sessions, "col-002");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_attribute_two_feature_session() {
        let records = vec![
            make_record(1000, Some("Read"), Some("product/features/col-001/SCOPE.md")),
            make_record(2000, Some("Write"), Some("product/features/col-001/test.rs")),
            make_record(3000, Some("Read"), Some("product/features/col-002/SCOPE.md")),
            make_record(4000, Some("Write"), Some("product/features/col-002/test.rs")),
        ];
        let sessions = vec![make_session("s1", records)];

        let result = attribute_sessions(&sessions, "col-002");
        assert_eq!(result.len(), 2);
        assert!(result[0].ts >= 3000);
    }

    #[test]
    fn test_attribute_no_feature_session() {
        let records = vec![
            make_record(1000, Some("Read"), Some("/tmp/random.rs")),
            make_record(2000, Some("Bash"), Some("ls -la")),
        ];
        let sessions = vec![make_session("s1", records)];

        let result = attribute_sessions(&sessions, "col-002");
        assert!(result.is_empty());
    }

    #[test]
    fn test_attribute_pre_feature_records() {
        let records = vec![
            make_record(1000, Some("Read"), Some("/tmp/setup.rs")),
            make_record(2000, Some("Read"), Some("product/features/col-002/SCOPE.md")),
            make_record(3000, Some("Write"), Some("product/features/col-002/test.rs")),
        ];
        let sessions = vec![make_session("s1", records)];

        let result = attribute_sessions(&sessions, "col-002");
        // Pre-feature record (ts=1000) attributed to first feature found (col-002)
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_attribute_multiple_sessions() {
        let s1 = make_session("s1", vec![
            make_record(1000, Some("Read"), Some("product/features/col-002/SCOPE.md")),
        ]);
        let s2 = make_session("s2", vec![
            make_record(2000, Some("Read"), Some("product/features/nxs-001/SCOPE.md")),
        ]);
        let s3 = make_session("s3", vec![
            make_record(3000, Some("Read"), Some("product/features/col-002/test.rs")),
        ]);

        let result = attribute_sessions(&[s1, s2, s3], "col-002");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_attribute_three_feature_session() {
        let records = vec![
            make_record(1000, Some("Read"), Some("product/features/col-001/x")),
            make_record(2000, Some("Read"), Some("product/features/col-002/x")),
            make_record(3000, Some("Read"), Some("product/features/col-001/y")),
        ];
        let sessions = vec![make_session("s1", records)];

        let col002 = attribute_sessions(&sessions, "col-002");
        assert_eq!(col002.len(), 1);
        assert_eq!(col002[0].ts, 2000);

        let col001 = attribute_sessions(&sessions, "col-001");
        assert_eq!(col001.len(), 2);
    }

    #[test]
    fn test_attribute_empty_sessions() {
        let result = attribute_sessions(&[], "col-002");
        assert!(result.is_empty());
    }
}
