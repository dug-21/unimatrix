//! Parity-corpus case table, tools half (vnc-026, ADR-001 mandatory
//! inventory): UserPromptSubmit word-count boundaries, PostToolUse rework
//! extraction (Bash/Edit/Write/MultiEdit), and PostToolUseFailure variants.

use super::Case;
use serde_json::json;

/// Manifest arm keys owned by this table.
pub(crate) const ARM_KEYS_TOOLS: &[&str] = &[
    // UserPromptSubmit
    "build_request::UserPromptSubmit::empty_prompt",
    "build_request::UserPromptSubmit::below_min_words",
    "build_request::UserPromptSubmit::context_search",
    "adversarial::prompt_escapes",
    "topic_signal::user_prompt_submit",
    // PostToolUse
    "build_request::PostToolUse::non_rework_tool",
    "build_request::PostToolUse::missing_tool_name",
    "build_request::PostToolUse::single_rework",
    "build_request::PostToolUse::multiedit_fanout",
    "build_request::PostToolUse::multiedit_empty_pairs",
    "topic_signal::post_tool_use",
    // is_bash_failure
    "is_bash_failure::exit_zero",
    "is_bash_failure::exit_nonzero",
    "is_bash_failure::exit_missing",
    "is_bash_failure::exit_non_integer",
    "is_bash_failure::interrupted_true",
    // extract_file_path
    "extract_file_path::edit_path",
    "extract_file_path::write_file_path",
    "extract_file_path::absent",
    // extract_rework_events_for_multiedit
    "multiedit::edits_present",
    "multiedit::edits_empty",
    "multiedit::edits_missing",
    "multiedit::edits_non_array",
    "multiedit::edit_path_missing",
    // PostToolUseFailure
    "build_request::PostToolUseFailure",
    "topic_signal::post_tool_use_failure",
];

pub(crate) fn cases() -> Vec<Case> {
    let mut v: Vec<Case> = Vec::new();

    // -- UserPromptSubmit boundaries --

    v.push(Case::new(
        "ups-empty-prompt",
        "UserPromptSubmit",
        &[
            "build_request::UserPromptSubmit::empty_prompt",
            "topic_signal::user_prompt_submit",
        ],
        json!({ "session_id": "sess-corpus", "prompt": "" }).to_string(),
    ));

    v.push(Case::new(
        "ups-whitespace-prompt",
        "UserPromptSubmit",
        &["build_request::UserPromptSubmit::empty_prompt"],
        json!({ "session_id": "sess-corpus", "prompt": "   \n\t  " }).to_string(),
    ));

    v.push(Case::new(
        "ups-missing-prompt-field",
        "UserPromptSubmit",
        &["build_request::UserPromptSubmit::empty_prompt"],
        json!({ "session_id": "sess-corpus" }).to_string(),
    ));

    v.push(Case::new(
        "ups-four-words",
        "UserPromptSubmit",
        &["build_request::UserPromptSubmit::below_min_words"],
        json!({ "session_id": "sess-corpus", "prompt": "fix the broken build" }).to_string(),
    ));

    v.push(Case::new(
        "ups-five-words",
        "UserPromptSubmit",
        &[
            "build_request::UserPromptSubmit::context_search",
            "normalize_event_name::canonical::UserPromptSubmit",
        ],
        json!({ "session_id": "sess-corpus", "prompt": "fix the broken build now" }).to_string(),
    ));

    v.push(Case::new(
        "ups-long-multiword",
        "UserPromptSubmit",
        &["build_request::UserPromptSubmit::context_search"],
        json!({
            "session_id": "sess-corpus",
            "prompt": "implement the parity corpus generator for the remote hook client and commit the goldens"
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ups-padded-words-not-counted",
        "UserPromptSubmit",
        &["build_request::UserPromptSubmit::below_min_words"],
        // split_whitespace ignores leading/trailing runs: 2 words, not 5.
        json!({ "session_id": "sess-corpus", "prompt": "   approve   it   " }).to_string(),
    ));

    v.push(Case::new(
        "ups-adversarial-content",
        "UserPromptSubmit",
        &["adversarial::prompt_escapes"],
        // Quotes, backslashes, emoji, control char, U+2028/U+2029 — ≥5 words
        // so the query routes to ContextSearch verbatim.
        json!({
            "session_id": "sess-corpus",
            "prompt": "he said \"do it\" with C:\\path\\to\\file 😀 line\u{2028}sep\u{2029}para \u{0007}bell done"
        })
        .to_string(),
    ));

    // -- PostToolUse (claude-code rework extraction) --

    v.push(Case::new(
        "ptu-bash-exit-zero",
        "PostToolUse",
        &[
            "build_request::PostToolUse::single_rework",
            "is_bash_failure::exit_zero",
            "extract_file_path::absent",
            "topic_signal::post_tool_use",
            "normalize_event_name::canonical::PostToolUse",
        ],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "Bash",
            "tool_input": { "command": "cargo build --workspace" },
            "tool_response": { "stdout": "Finished" },
            "exit_code": 0
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptu-bash-exit-nonzero",
        "PostToolUse",
        &["is_bash_failure::exit_nonzero"],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "Bash",
            "tool_input": { "command": "cargo test" },
            "tool_response": { "stderr": "2 tests failed" },
            "exit_code": 101
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptu-bash-exit-missing",
        "PostToolUse",
        &["is_bash_failure::exit_missing"],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "Bash",
            "tool_input": { "command": "true" }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptu-bash-exit-non-integer",
        "PostToolUse",
        &["is_bash_failure::exit_non_integer"],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "Bash",
            "tool_input": { "command": "false" },
            "exit_code": "1"
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptu-bash-interrupted",
        "PostToolUse",
        &["is_bash_failure::interrupted_true"],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "Bash",
            "tool_input": { "command": "sleep 100" },
            "exit_code": 0,
            "interrupted": true
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptu-edit",
        "PostToolUse",
        &["extract_file_path::edit_path"],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "Edit",
            "tool_input": { "path": "/src/lib.rs", "old": "a", "new": "b" },
            "tool_response": { "ok": true }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptu-write",
        "PostToolUse",
        &["extract_file_path::write_file_path"],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "Write",
            "tool_input": { "file_path": "/src/new_module.rs", "content": "fn main() {}" }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptu-multiedit-fanout",
        "PostToolUse",
        &[
            "build_request::PostToolUse::multiedit_fanout",
            "multiedit::edits_present",
            "multiedit::edit_path_missing",
        ],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "MultiEdit",
            "tool_input": {
                "edits": [
                    { "path": "/src/a.rs", "old": "x", "new": "y" },
                    { "path": "/src/b.rs", "old": "x", "new": "y" },
                    { "old": "no path on this edit", "new": "z" }
                ]
            },
            "tool_response": { "ok": true }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptu-multiedit-empty-edits",
        "PostToolUse",
        &[
            "build_request::PostToolUse::multiedit_empty_pairs",
            "multiedit::edits_empty",
        ],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "MultiEdit",
            "tool_input": { "edits": [] }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptu-multiedit-missing-edits",
        "PostToolUse",
        &["multiedit::edits_missing"],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "MultiEdit",
            "tool_input": { "not_edits": true }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptu-multiedit-non-array-edits",
        "PostToolUse",
        &["multiedit::edits_non_array"],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "MultiEdit",
            "tool_input": { "edits": "not-an-array" }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptu-non-rework-tool",
        "PostToolUse",
        &["build_request::PostToolUse::non_rework_tool"],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "Read",
            "tool_input": { "file_path": "/src/lib.rs" },
            "tool_response": { "content": "..." }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptu-missing-tool-name",
        "PostToolUse",
        &["build_request::PostToolUse::missing_tool_name"],
        json!({
            "session_id": "sess-corpus",
            "tool_input": { "command": "ls" }
        })
        .to_string(),
    ));

    // -- PostToolUseFailure variants --

    v.push(Case::new(
        "ptuf-basic",
        "PostToolUseFailure",
        &[
            "build_request::PostToolUseFailure",
            "topic_signal::post_tool_use_failure",
            "normalize_event_name::canonical::PostToolUseFailure",
        ],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "Bash",
            "error": "command not found: frobnicate",
            "tool_input": { "command": "frobnicate --all" },
            "is_interrupt": false
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptuf-empty-extra",
        "PostToolUseFailure",
        &["build_request::PostToolUseFailure"],
        json!({ "session_id": "sess-corpus" }).to_string(),
    ));

    v.push(Case::new(
        "ptuf-null-extra",
        "PostToolUseFailure",
        &["build_request::PostToolUseFailure"],
        // Malformed stdin → defensive default → extra is null, payload null.
        "definitely not json",
    ));

    v.push(Case::new(
        "ptuf-missing-tool-name",
        "PostToolUseFailure",
        &["build_request::PostToolUseFailure"],
        json!({
            "session_id": "sess-corpus",
            "error": "boom",
            "tool_input": { "x": 1 }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "ptuf-null-error",
        "PostToolUseFailure",
        &["build_request::PostToolUseFailure"],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "Edit",
            "error": null
        })
        .to_string(),
    ));

    v
}
