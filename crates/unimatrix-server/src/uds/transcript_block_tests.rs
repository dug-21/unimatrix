//! Tests for the shared transcript-block extraction core (vnc-025).
//!
//! §1: the R-14 move-fidelity inventory — these tests moved from `hook.rs`
//! with bodies unmodified (imports aside). The new `from_bytes` tests
//! (§2 golden parity, §4 injection bound) live in
//! `transcript_block_tests_bytes.rs`.

use super::*;
use crate::uds::hook::MAX_INJECTION_BYTES;

// =========================================================================
// §1 Move fidelity (R-14) — moved verbatim from hook.rs; bodies unmodified.
// =========================================================================

// Helper: write JSONL lines to a temp file, return (TempDir, path_string).
// pub(super) so the sibling `tests_bytes` module reuses it (cumulative
// test infrastructure — no duplicated scaffolding).
pub(super) fn make_jsonl_file(lines: &[&str]) -> (tempfile::TempDir, String) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.jsonl");
    std::fs::write(&path, lines.join("\n")).unwrap();
    let path_str = path.to_str().unwrap().to_string();
    (tmp, path_str)
}

#[test]
fn max_precompact_bytes_constant_defined() {
    assert_eq!(MAX_PRECOMPACT_BYTES, 3000);
    assert_ne!(MAX_PRECOMPACT_BYTES, MAX_INJECTION_BYTES);
    assert_eq!(TAIL_MULTIPLIER, 4);
    assert_eq!(TOOL_RESULT_SNIPPET_BYTES, 300);
    assert_eq!(TOOL_KEY_PARAM_BYTES, 120);
}

#[test]
fn extract_transcript_block_empty_path_returns_none() {
    // Note: extract_transcript_block("") will try to open "" and fail -> None
    let result = extract_transcript_block("");
    assert!(result.is_none());
}

#[test]
fn extract_transcript_block_missing_file_returns_none() {
    let result = extract_transcript_block("/nonexistent/path/session.jsonl");
    assert!(result.is_none());
}

#[test]
fn prepend_transcript_none_block_writes_briefing() {
    let result = prepend_transcript(None, "briefing content");
    assert_eq!(result, "briefing content");
    assert!(!result.contains("=== Recent conversation"));
}

#[test]
fn extract_transcript_block_all_malformed_lines_returns_none() {
    let (_tmp, path) = make_jsonl_file(&["not json", "also not json", "{broken"]);
    let result = extract_transcript_block(&path);
    assert!(result.is_none());
}

#[test]
fn extract_transcript_block_zero_byte_file_returns_none() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("empty.jsonl");
    std::fs::write(&path, b"").unwrap();
    let result = extract_transcript_block(path.to_str().unwrap());
    assert!(result.is_none());
}

#[test]
fn build_exchange_pairs_three_exchanges_most_recent_first() {
    let user_a = r#"{"type":"user","message":{"content":[{"type":"text","text":"A"}]}}"#;
    let asst_a = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"RA"}]}}"#;
    let user_b = r#"{"type":"user","message":{"content":[{"type":"text","text":"B"}]}}"#;
    let asst_b = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"RB"}]}}"#;
    let user_c = r#"{"type":"user","message":{"content":[{"type":"text","text":"C"}]}}"#;
    let asst_c = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"RC"}]}}"#;
    let lines = vec![user_a, asst_a, user_b, asst_b, user_c, asst_c];
    let turns = build_exchange_pairs(&lines);
    // First turn should be most recent (C or RC)
    assert!(!turns.is_empty());
    let first_text = match &turns[0] {
        ExchangeTurn::AssistantText(t) => t.clone(),
        ExchangeTurn::UserText(t) => t.clone(),
        _ => panic!("unexpected"),
    };
    assert!(
        first_text == "RC" || first_text == "C",
        "most recent first: got {first_text}"
    );
}

#[test]
fn build_exchange_pairs_user_tool_result_skipped() {
    let user = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"x","content":"result"}]}}"#;
    let turns = build_exchange_pairs(&[user]);
    assert!(
        turns.is_empty(),
        "tool_result in user turn must not emit UserText"
    );
}

#[test]
fn build_exchange_pairs_tool_only_assistant_turn_emits_pairs() {
    let asst = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tu1","name":"Read","input":{"file_path":"/foo.rs"}},{"type":"thinking","thinking":"..."}]}}"#;
    let user = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu1","content":"file contents"}]}}"#;
    let turns = build_exchange_pairs(&[asst, user]);
    let has_tool_pair = turns
        .iter()
        .any(|t| matches!(t, ExchangeTurn::ToolPair { .. }));
    let has_asst_text = turns
        .iter()
        .any(|t| matches!(t, ExchangeTurn::AssistantText(_)));
    assert!(has_tool_pair, "tool-only assistant turn must emit ToolPair");
    assert!(
        !has_asst_text,
        "tool-only assistant turn must NOT emit AssistantText"
    );
}

#[test]
fn build_exchange_pairs_thinking_only_turn_suppressed() {
    let asst = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"secret thoughts"}]}}"#;
    let turns = build_exchange_pairs(&[asst]);
    assert!(turns.is_empty(), "pure thinking turn must be suppressed");
}

#[test]
fn build_exchange_pairs_malformed_lines_skipped() {
    let user = r#"{"type":"user","message":{"content":[{"type":"text","text":"hello"}]}}"#;
    let lines = vec!["not json", user, "{broken", "also bad"];
    let turns = build_exchange_pairs(&lines);
    assert!(
        !turns.is_empty(),
        "valid lines must produce turns despite malformed lines"
    );
    assert!(!std::panic::catch_unwind(|| build_exchange_pairs(&lines)).is_err());
}

#[test]
fn extract_key_param_known_tools_correct_field() {
    let cases = vec![
        ("Bash", "command", r#"{"command":"ls -la"}"#, "ls -la"),
        ("Read", "file_path", r#"{"file_path":"/foo.rs"}"#, "/foo.rs"),
        ("Edit", "file_path", r#"{"file_path":"/bar.rs"}"#, "/bar.rs"),
        (
            "Write",
            "file_path",
            r#"{"file_path":"/out.rs"}"#,
            "/out.rs",
        ),
        ("Glob", "pattern", r#"{"pattern":"**/*.rs"}"#, "**/*.rs"),
        ("Grep", "pattern", r#"{"pattern":"fn main"}"#, "fn main"),
        (
            "MultiEdit",
            "file_path",
            r#"{"file_path":"/multi.rs"}"#,
            "/multi.rs",
        ),
        (
            "Task",
            "description",
            r#"{"description":"implement X"}"#,
            "implement X",
        ),
        (
            "WebFetch",
            "url",
            r#"{"url":"https://example.com"}"#,
            "https://example.com",
        ),
        (
            "WebSearch",
            "query",
            r#"{"query":"rust async"}"#,
            "rust async",
        ),
    ];
    for (tool, _field, input_json, expected) in cases {
        let input: serde_json::Value = serde_json::from_str(input_json).unwrap();
        let result = extract_key_param(tool, &input);
        assert_eq!(result, expected, "tool: {tool}");
    }
}

#[test]
fn extract_key_param_unknown_tool_first_string_field_fallback() {
    let input: serde_json::Value = serde_json::from_str(r#"{"query":"foo","count":5}"#).unwrap();
    let result = extract_key_param("UnknownTool", &input);
    // Should return first string field value
    assert_eq!(result, "foo");
}

#[test]
fn extract_key_param_no_string_field_returns_empty() {
    let input: serde_json::Value = serde_json::from_str(r#"{"count":5,"flag":true}"#).unwrap();
    let result = extract_key_param("UnknownTool", &input);
    assert_eq!(result, "");
}

#[test]
fn extract_key_param_long_value_truncated() {
    let long_val = "x".repeat(5000);
    let input = serde_json::json!({"file_path": long_val});
    let result = extract_key_param("Read", &input);
    assert!(result.len() <= TOOL_KEY_PARAM_BYTES);
    assert!(std::str::from_utf8(result.as_bytes()).is_ok());
}

#[test]
fn prepend_transcript_both_present_separator_present() {
    let result = prepend_transcript(Some("block"), "briefing");
    assert_eq!(result, "block\n\nbriefing");
}

#[test]
fn prepend_transcript_both_present_transcript_precedes_briefing() {
    let result = prepend_transcript(
        Some("=== Recent conversation ===\n[User] foo\n=== End recent conversation ==="),
        "briefing",
    );
    assert!(result.starts_with("=== Recent conversation"));
    assert!(result.contains("briefing"));
    assert!(
        result.find("=== End recent conversation ===").unwrap() < result.find("briefing").unwrap()
    );
}

#[test]
fn prepend_transcript_transcript_only_has_headers() {
    let block = "=== Recent conversation ===\n[User] foo\n=== End recent conversation ===";
    let result = prepend_transcript(Some(block), "");
    assert_eq!(result, block);
}

#[test]
fn prepend_transcript_both_none_empty_string() {
    let result = prepend_transcript(None, "");
    assert_eq!(result, "");
}

#[test]
fn prepend_transcript_none_block_writes_briefing_verbatim() {
    let result = prepend_transcript(None, "briefing content");
    assert_eq!(result, "briefing content");
    assert!(!result.contains("=== Recent conversation"));
}

#[test]
fn extract_transcript_block_respects_byte_budget() {
    // Create many exchanges that together exceed MAX_PRECOMPACT_BYTES
    let mut lines = Vec::new();
    for i in 0..20 {
        let user = format!(
            r#"{{"type":"user","message":{{"content":[{{"type":"text","text":"user message number {} with some padding to make it longer"}}]}}}}"#,
            i
        );
        let asst = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"assistant response number {} with some padding too"}}]}}}}"#,
            i
        );
        lines.push(user);
        lines.push(asst);
    }
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    let (_tmp, path) = make_jsonl_file(&line_refs);
    let result = extract_transcript_block(&path);
    if let Some(s) = result {
        assert!(
            s.len() <= MAX_PRECOMPACT_BYTES,
            "byte budget exceeded: {} > {}",
            s.len(),
            MAX_PRECOMPACT_BYTES
        );
        assert!(
            s.starts_with("=== Recent conversation"),
            "must start with header"
        );
        assert!(
            s.ends_with("=== End recent conversation ==="),
            "must end with footer"
        );
    }
    // None is also acceptable if all exchanges are too large for the budget
}

#[test]
fn extract_transcript_block_system_only_returns_none() {
    let lines = vec![
        r#"{"type":"system","content":"system message 1"}"#,
        r#"{"type":"system","content":"system message 2"}"#,
    ];
    let (_tmp, path) = make_jsonl_file(&lines);
    let result = extract_transcript_block(&path);
    assert!(result.is_none());
}
