//! Feature attribution logic: session-to-feature mapping via content-based sequential scanning.

use crate::types::{ObservationRecord, ParsedSession};

const MAX_FEATURE_ID_LEN: usize = 128;

/// Check if a string is a plausible feature ID.
///
/// Permissive safety gating: non-empty, reasonable length, contains a hyphen,
/// only safe characters (ASCII alphanumeric, hyphen, underscore, dot).
/// No leading/trailing hyphens.
///
/// Unimatrix is domain-agnostic (ASS-009) -- feature ID format is a
/// project-level concern, not an engine-level concern.
fn is_valid_feature_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_FEATURE_ID_LEN
        && s.contains('-')
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
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
///
/// Accepts any feature ID matching the `alpha-digits` pattern (e.g., "col-002", "eng-001").
/// No prefix allowlist — the structural pattern validated by `is_valid_feature_id` is sufficient.
fn extract_feature_id_pattern(s: &str) -> Option<String> {
    for word in
        s.split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '(' || c == ')')
    {
        let candidate = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        if is_valid_feature_id(candidate) {
            return Some(candidate.to_string());
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

/// Extract a topic signal from raw text using a priority chain.
///
/// Priority order (first match wins):
/// 1. File path: `product/features/{id}/...`
/// 2. Feature ID pattern: word-boundary `{alpha}-{digits}` tokens
/// 3. Git checkout: `feature/{id}` in git commands
///
/// Individual extractors stay private (ADR-017-001). This facade is the
/// only public entry point for hook-side topic attribution.
pub fn extract_topic_signal(text: &str) -> Option<String> {
    // Priority 1: file path (highest confidence)
    if let Some(id) = extract_from_path(text) {
        return Some(id);
    }
    // Priority 2: feature ID pattern
    if let Some(id) = extract_feature_id_pattern(text) {
        return Some(id);
    }
    // Priority 3: git checkout pattern (lowest confidence)
    extract_from_git_checkout(text)
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
                        partitions.push((
                            current_feature.clone(),
                            std::mem::take(&mut current_records),
                        ));
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
        let first_feature = partitions.iter().find_map(|(f, _)| f.clone());

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
    fn make_record(ts: u64, tool: Option<&str>, input_str: Option<&str>) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
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
        assert!(!is_valid_feature_id("nohyphen"));
    }

    #[test]
    fn test_attribute_single_feature_session() {
        let records = vec![
            make_record(
                1000,
                Some("Read"),
                Some("product/features/col-002/SCOPE.md"),
            ),
            make_record(
                2000,
                Some("Write"),
                Some("product/features/col-002/test.rs"),
            ),
        ];
        let sessions = vec![make_session("s1", records)];

        let result = attribute_sessions(&sessions, "col-002");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_attribute_two_feature_session() {
        let records = vec![
            make_record(
                1000,
                Some("Read"),
                Some("product/features/col-001/SCOPE.md"),
            ),
            make_record(
                2000,
                Some("Write"),
                Some("product/features/col-001/test.rs"),
            ),
            make_record(
                3000,
                Some("Read"),
                Some("product/features/col-002/SCOPE.md"),
            ),
            make_record(
                4000,
                Some("Write"),
                Some("product/features/col-002/test.rs"),
            ),
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
            make_record(
                2000,
                Some("Read"),
                Some("product/features/col-002/SCOPE.md"),
            ),
            make_record(
                3000,
                Some("Write"),
                Some("product/features/col-002/test.rs"),
            ),
        ];
        let sessions = vec![make_session("s1", records)];

        let result = attribute_sessions(&sessions, "col-002");
        // Pre-feature record (ts=1000) attributed to first feature found (col-002)
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_attribute_multiple_sessions() {
        let s1 = make_session(
            "s1",
            vec![make_record(
                1000,
                Some("Read"),
                Some("product/features/col-002/SCOPE.md"),
            )],
        );
        let s2 = make_session(
            "s2",
            vec![make_record(
                2000,
                Some("Read"),
                Some("product/features/nxs-001/SCOPE.md"),
            )],
        );
        let s3 = make_session(
            "s3",
            vec![make_record(
                3000,
                Some("Read"),
                Some("product/features/col-002/test.rs"),
            )],
        );

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

    #[test]
    fn test_extract_feature_id_pattern_accepts_arbitrary_prefixes() {
        // Feature IDs with non-project prefixes should be accepted (#59)
        assert_eq!(
            extract_feature_id_pattern("Working on eng-001 design"),
            Some("eng-001".to_string())
        );
        assert_eq!(
            extract_feature_id_pattern("Review spike-042 results"),
            Some("spike-042".to_string())
        );
        assert_eq!(
            extract_feature_id_pattern("Deploy api-100"),
            Some("api-100".to_string())
        );
    }

    #[test]
    fn test_attribute_sessions_with_arbitrary_prefix_feature() {
        // End-to-end: attribution works for non-project feature prefixes (#59)
        let records = vec![
            make_record(1000, Some("Read"), Some("Working on eng-001 task")),
            make_record(2000, Some("Write"), Some("Still on eng-001")),
        ];
        let sessions = vec![make_session("s1", records)];

        let result = attribute_sessions(&sessions, "eng-001");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_is_valid_feature_id_suffixed() {
        // AC-1, AC-2: Suffixed feature IDs now accepted (#79)
        assert!(is_valid_feature_id("col-010b"));
        assert!(is_valid_feature_id("col-002b"));
    }

    #[test]
    fn test_is_valid_feature_id_domain_agnostic() {
        // AC-4 through AC-7: Domain-agnostic feature ID formats (#79)
        assert!(is_valid_feature_id("PROJ-123"));
        assert!(is_valid_feature_id("sprint-7-auth"));
        assert!(is_valid_feature_id("v2.1-migration"));
        assert!(is_valid_feature_id("my_project-feat_1"));
    }

    #[test]
    fn test_is_valid_feature_id_no_hyphen() {
        // AC-9: Strings without hyphens rejected (#79)
        assert!(!is_valid_feature_id("nohyphen"));
        assert!(!is_valid_feature_id("justletters"));
        assert!(!is_valid_feature_id("12345"));
    }

    #[test]
    fn test_is_valid_feature_id_special_chars() {
        // AC-10: Injection characters rejected (#79)
        assert!(!is_valid_feature_id("a]b-c"));
        assert!(!is_valid_feature_id("feat<script>-1"));
        assert!(!is_valid_feature_id("col-001;drop"));
    }

    #[test]
    fn test_is_valid_feature_id_whitespace() {
        // AC-11: Whitespace rejected (#79)
        assert!(!is_valid_feature_id("a b-c"));
        assert!(!is_valid_feature_id("col -001"));
    }

    #[test]
    fn test_is_valid_feature_id_length_boundary() {
        // AC-12: 128/129 length boundary (#79)
        let at_limit = format!("{}-{}", "a".repeat(64), "b".repeat(63));
        assert_eq!(at_limit.len(), 128);
        assert!(is_valid_feature_id(&at_limit));

        let over_limit = format!("{}-{}", "a".repeat(64), "b".repeat(64));
        assert_eq!(over_limit.len(), 129);
        assert!(!is_valid_feature_id(&over_limit));
    }

    #[test]
    fn test_is_valid_feature_id_leading_trailing_hyphen() {
        // Leading/trailing hyphen rejected (#79)
        assert!(!is_valid_feature_id("-abc"));
        assert!(!is_valid_feature_id("abc-"));
        assert!(!is_valid_feature_id("-"));
    }

    #[test]
    fn test_attribute_sessions_suffixed_feature() {
        // AC-14: E2E attribution with suffixed feature ID (#79)
        let records = vec![
            make_record(
                1000,
                Some("Read"),
                Some("product/features/col-010b/SCOPE.md"),
            ),
            make_record(
                2000,
                Some("Write"),
                Some("product/features/col-010b/test.rs"),
            ),
        ];
        let sessions = vec![make_session("s1", records)];

        let result = attribute_sessions(&sessions, "col-010b");
        assert_eq!(result.len(), 2);
    }

    // -- col-017: extract_topic_signal facade tests (T-01, T-02, T-03) --

    #[test]
    fn test_extract_topic_signal_from_path() {
        // AC-01: file path input
        assert_eq!(
            extract_topic_signal("editing product/features/col-002/SCOPE.md"),
            Some("col-002".to_string())
        );
    }

    #[test]
    fn test_extract_topic_signal_from_pattern() {
        // AC-02: feature ID pattern
        assert_eq!(
            extract_topic_signal("Working on col-002 design"),
            Some("col-002".to_string())
        );
    }

    #[test]
    fn test_extract_topic_signal_from_git() {
        // AC-03: git branch
        assert_eq!(
            extract_topic_signal("git checkout -b feature/col-002"),
            Some("col-002".to_string())
        );
    }

    #[test]
    fn test_extract_topic_signal_priority_path_over_pattern() {
        // AC-04: path > pattern priority (AR-1)
        // Input has both a path and a standalone pattern; path should win
        let input = "reading product/features/col-002/SCOPE.md while working on nxs-001";
        assert_eq!(extract_topic_signal(input), Some("col-002".to_string()));
    }

    #[test]
    fn test_extract_topic_signal_priority_pattern_over_git() {
        // AC-04: pattern > git priority (AR-1)
        let input = "working on col-002 then git checkout feature/nxs-001";
        assert_eq!(extract_topic_signal(input), Some("col-002".to_string()));
    }

    #[test]
    fn test_extract_topic_signal_none() {
        // AC-05: no signal
        assert_eq!(extract_topic_signal("regular text with no features"), None);
        assert_eq!(extract_topic_signal(""), None);
    }

    #[test]
    fn test_extract_topic_signal_false_positive_awareness() {
        // T-03: false-positive awareness (R4)
        // Per ASS-009, is_valid_feature_id is intentionally permissive (domain-agnostic).
        // Patterns like "utf-8", "x86-64", "sha-256" do pass validation.
        // Majority vote at session level mitigates: occasional false positives
        // are outvoted by repeated real signals across a session.
        assert_eq!(
            extract_topic_signal("encoding utf-8"),
            Some("utf-8".to_string())
        );
        assert_eq!(
            extract_topic_signal("architecture x86-64"),
            Some("x86-64".to_string())
        );
        assert_eq!(
            extract_topic_signal("hash sha-256"),
            Some("sha-256".to_string())
        );

        // Strings without hyphens are correctly rejected
        assert_eq!(extract_topic_signal("just a number 42"), None);
        assert_eq!(extract_topic_signal("plain text"), None);
    }

    #[test]
    fn test_extract_topic_signal_git_only() {
        // When only git pattern is present
        assert_eq!(
            extract_topic_signal("git checkout feature/nxs-001"),
            Some("nxs-001".to_string())
        );
    }
}
