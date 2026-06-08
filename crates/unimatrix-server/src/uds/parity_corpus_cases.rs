//! Parity-corpus case table, part A (vnc-026, ADR-001 mandatory inventory):
//! canonical events, Gemini aliases, unknown-event passthrough, and stdin
//! shapes (empty/malformed/wrong-typed/oversized/flatten parity).
//!
//! Siblings: `parity_corpus_cases_tools.rs` (UserPromptSubmit + PostToolUse +
//! PostToolUseFailure), `parity_corpus_cases_b.rs` (context_cycle +
//! SubagentStart), `parity_corpus_cases_stdout.rs` (stdout goldens).

use super::Case;
use serde_json::json;

/// Manifest arm keys owned by this table.
pub(crate) const ARM_KEYS_A: &[&str] = &[
    // normalize_event_name
    "normalize_event_name::alias::AfterTool",
    "normalize_event_name::alias::SessionEnd",
    "normalize_event_name::canonical::PreToolUse",
    "normalize_event_name::canonical::PostToolUse",
    "normalize_event_name::canonical::SessionStart",
    "normalize_event_name::canonical::Stop",
    "normalize_event_name::canonical::TaskCompleted",
    "normalize_event_name::canonical::Ping",
    "normalize_event_name::canonical::UserPromptSubmit",
    "normalize_event_name::canonical::PreCompact",
    "normalize_event_name::canonical::PostToolUseFailure",
    "normalize_event_name::canonical::SubagentStart",
    "normalize_event_name::canonical::SubagentStop",
    "normalize_event_name::unknown",
    // parse_hook_input
    "parse_hook_input::ok",
    "parse_hook_input::defensive_default",
    "parse_hook_input::wrong_typed_field",
    "parse_hook_input::lone_surrogate_defensive",
    "parse_hook_input::extra_flatten_preserved",
    "parse_hook_input::provider_named_field_overwritten",
    // read_stdin 1 MiB cap
    "read_stdin::cap_exact_1mib",
    "read_stdin::cap_over_1mib",
    // build_request arms exercised by this table
    "build_request::SessionStart",
    "build_request::SessionStart::role_feature_absent",
    "build_request::Stop",
    "build_request::TaskCompleted",
    "build_request::Ping",
    "build_request::PreCompact",
    "build_request::PostToolUse::non_claude_provider",
    "build_request::wildcard_generic",
    "build_request::session_id_ppid_fallback",
    "build_request::cwd_fallback",
    // extract_event_topic_signal arms exercised by this table
    "topic_signal::generic",
    "topic_signal::generic_null_extra",
];

/// Build a stdin JSON document padded with an ASCII `pad` field to an exact
/// total byte length (read_stdin 1 MiB cap cases).
fn padded_stdin(head: &str, total: usize) -> String {
    let tail = "\"}";
    assert!(total > head.len() + tail.len(), "padding target too small");
    let pad_len = total - head.len() - tail.len();
    let mut s = String::with_capacity(total);
    s.push_str(head);
    s.push_str(&"a".repeat(pad_len));
    s.push_str(tail);
    assert_eq!(s.len(), total);
    s
}

pub(crate) fn cases() -> Vec<Case> {
    let mut v: Vec<Case> = Vec::new();

    // -- Canonical events + Gemini aliases + unknown passthrough --

    v.push(Case::new(
        "event-session-start",
        "SessionStart",
        &[
            "build_request::SessionStart",
            "normalize_event_name::canonical::SessionStart",
            "parse_hook_input::ok",
        ],
        json!({
            "session_id": "sess-corpus",
            "cwd": "/work/project",
            "agent_role": "developer",
            "feature_cycle": "vnc-026"
        })
        .to_string(),
    ));

    v.push(Case::new(
        "event-session-start-no-role",
        "SessionStart",
        &["build_request::SessionStart::role_feature_absent"],
        json!({ "session_id": "sess-corpus", "cwd": "/work/project" }).to_string(),
    ));

    v.push(Case::new(
        "event-stop",
        "Stop",
        &[
            "build_request::Stop",
            "normalize_event_name::canonical::Stop",
        ],
        json!({ "session_id": "sess-corpus", "cwd": "/work/project" }).to_string(),
    ));

    v.push(Case::new(
        "event-task-completed",
        "TaskCompleted",
        &[
            "build_request::TaskCompleted",
            "normalize_event_name::canonical::TaskCompleted",
        ],
        json!({ "session_id": "sess-corpus" }).to_string(),
    ));

    v.push(Case::new(
        "event-ping",
        "Ping",
        &[
            "build_request::Ping",
            "normalize_event_name::canonical::Ping",
        ],
        json!({}).to_string(),
    ));

    v.push(Case::new(
        "event-precompact",
        "PreCompact",
        &[
            "build_request::PreCompact",
            "normalize_event_name::canonical::PreCompact",
        ],
        json!({ "session_id": "sess-corpus", "cwd": "/work/project" }).to_string(),
    ));

    v.push(Case::new(
        "event-subagent-stop",
        "SubagentStop",
        &[
            "build_request::wildcard_generic",
            "normalize_event_name::canonical::SubagentStop",
            "topic_signal::generic",
        ],
        json!({ "session_id": "sess-corpus", "custom_field": "kept verbatim" }).to_string(),
    ));

    v.push(Case::new(
        "event-unknown-passthrough",
        "SomethingNovel",
        &[
            "normalize_event_name::unknown",
            "build_request::wildcard_generic",
        ],
        json!({ "session_id": "sess-corpus", "k": 1 }).to_string(),
    ));

    v.push(Case::new(
        "event-unknown-null-extra",
        "WeirdEvent",
        &["topic_signal::generic_null_extra"],
        "",
    ));

    // NOTE (vnc-027 ADR-004 §4): the `alias-before-tool` case (Gemini BeforeTool
    // alias → non-cycle PreToolUse observation) is RETIRED. The TS client returns
    // a null no-send sentinel for non-cycle PreToolUse, so there is no frame to
    // parity-check; the corpus excludes retired PreToolUse observation by design.
    // The BeforeTool alias path that DOES survive (cycle interception) is covered
    // by `cycle-mcp-context-promotion`.

    v.push(Case::new(
        "alias-after-tool",
        "AfterTool",
        &[
            "normalize_event_name::alias::AfterTool",
            "build_request::PostToolUse::non_claude_provider",
        ],
        // Bash + non-zero exit would be a rework candidate for claude-code;
        // the gemini-cli provider gate must skip the rework path entirely.
        json!({
            "session_id": "gem-1",
            "tool_name": "Bash",
            "tool_input": { "command": "ls" },
            "exit_code": 1
        })
        .to_string(),
    ));

    v.push(Case::new(
        "alias-session-end",
        "SessionEnd",
        &["normalize_event_name::alias::SessionEnd"],
        json!({ "session_id": "gem-1" }).to_string(),
    ));

    // -- Stdin shapes --

    v.push(Case::new(
        "stdin-empty",
        "Stop",
        &[
            "parse_hook_input::defensive_default",
            "build_request::session_id_ppid_fallback",
        ],
        "",
    ));

    v.push(Case::new(
        "stdin-malformed",
        "SessionStart",
        &[
            "parse_hook_input::defensive_default",
            "build_request::cwd_fallback",
        ],
        "{not valid json!",
    ));

    v.push(Case::new(
        "stdin-wrong-typed-field",
        "Stop",
        &["parse_hook_input::wrong_typed_field"],
        json!({ "session_id": 42 }).to_string(),
    ));

    v.push(Case::new(
        "stdin-missing-cwd",
        "SessionStart",
        &["build_request::cwd_fallback"],
        json!({ "session_id": "sess-nocwd" }).to_string(),
    ));

    v.push(Case::new(
        "stdin-extra-fields-preserved",
        "SubagentStop",
        &["parse_hook_input::extra_flatten_preserved"],
        // Unknown keys must survive the flatten in original order (ass-071).
        r#"{"session_id":"sess-corpus","zeta":"last-alpha-first","alpha":{"deep":[1,2,3]},"num":3.5,"flag":true,"nothing":null}"#,
    ));

    v.push(Case::new(
        "stdin-provider-field-overwritten",
        "SubagentStop",
        &["parse_hook_input::provider_named_field_overwritten"],
        // `provider` is a NAMED HookInput field: parsed, then overwritten by
        // the inference path. It must not appear in payload (extra).
        json!({ "session_id": "sess-corpus", "provider": "made-up-provider" }).to_string(),
    ));

    v.push(Case::new(
        "stdin-exactly-1mib",
        "SessionStart",
        &["read_stdin::cap_exact_1mib"],
        padded_stdin(
            r#"{"session_id":"sess-1mib","cwd":"/work/project","pad":""#,
            1_048_576,
        ),
    ));

    v.push(Case::new(
        "stdin-over-1mib",
        "Stop",
        &["read_stdin::cap_over_1mib"],
        // One byte over the cap: truncation chops the closing brace, the parse
        // fails, and the defensive default kicks in (ppid fallback).
        padded_stdin(
            r#"{"session_id":"sess-over","cwd":"/work/project","pad":""#,
            1_048_577,
        ),
    ));

    v.push(Case::new(
        "stdin-lone-surrogate-escape",
        "SubagentStop",
        &["parse_hook_input::lone_surrogate_defensive"],
        // serde_json rejects a lone surrogate escape; the whole parse fails
        // defensively. (JS JSON.parse accepts it — the golden pins Rust truth.)
        r#"{"session_id":"sess-corpus","note":"\ud800 adjacent text"}"#,
    ));

    v
}
