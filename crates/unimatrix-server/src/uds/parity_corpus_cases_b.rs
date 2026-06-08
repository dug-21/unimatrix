//! Parity-corpus case table, part B (vnc-026, ADR-001 mandatory inventory):
//! PreToolUse `context_cycle` interception (bare/prefixed/near-miss/invalid/
//! promotion/goal-overflow) and SubagentStart prompt_snippet +
//! transcript-tail variants. Stdout goldens live in
//! `parity_corpus_cases_stdout.rs`; transcript builders in
//! `parity_corpus_transcripts.rs`.

use super::Case;
use super::transcripts::{
    budget_overflow_transcript, jsonl, mid_line_window_transcript, multibyte_edge_transcript,
    t_assistant_text, t_assistant_tool, t_tool_result, t_user,
};
use crate::uds::transcript_block::MAX_PRECOMPACT_BYTES;
use serde_json::json;

/// Manifest arm keys owned by this table.
pub(crate) const ARM_KEYS_B: &[&str] = &[
    // PreToolUse routing. vnc-027 ADR-004 §4: non-cycle PreToolUse observation is
    // RETIRED (TS returns a null no-send sentinel), so `no_promotion`,
    // `not_context_cycle`, `near_miss_not_intercepted`, `missing_tool_input`, and
    // `validation_failed` arms (all null-sentinel paths) are excluded from the
    // post-reduction corpus. Only the surviving cycle-interception arms remain.
    "build_request::PreToolUse::mcp_context_promotion",
    // build_cycle_event_or_fallthrough (surviving cycle frames)
    "cycle::bare_name",
    "cycle::prefixed_name",
    "cycle::start",
    "cycle::phase_end",
    "cycle::stop",
    "cycle::goal_truncation",
    "cycle::goal_ignored_non_start",
    // SubagentStart build_request arm
    "build_request::SubagentStart::context_search",
    "build_request::SubagentStart::empty_snippet",
    "topic_signal::subagent_start",
    // run() step 5b fallback
    "subagent_fallback::query_some",
    "subagent_fallback::missing_file",
    "subagent_fallback::empty_transcript_path",
    "subagent_fallback::no_transcript_path",
    "subagent_fallback::not_applicable",
    // transcript_block.rs branches
    "transcript::user_text",
    "transcript::assistant_text",
    "transcript::tool_pairing",
    "transcript::malformed_lines_skipped",
    "transcript::thinking_only_suppressed",
    "transcript::mid_line_window",
    "transcript::multibyte_window_edge",
    "transcript::budget_loop_break",
    "transcript::no_turns_none",
    "transcript::tool_result_string_content",
    "transcript::key_param_fallback",
    "transcript::raw_content_shape",
];

/// Shared SubagentStart prompt_snippet stdin (also used by stdout cases).
pub(crate) fn sas_snippet_stdin() -> String {
    json!({
        "session_id": "sess-corpus",
        "agent_type": "developer",
        "prompt_snippet": "implement the parity corpus generator dev test"
    })
    .to_string()
}

pub(crate) fn cases() -> Vec<Case> {
    let mut v: Vec<Case> = Vec::new();

    // -- PreToolUse / context_cycle interception --
    //
    // vnc-027 ADR-004 §4: non-cycle PreToolUse observation is retired (TS returns
    // a null no-send sentinel), so `ptu-pre-non-cycle` is removed. The
    // `normalize_event_name::canonical::PreToolUse` arm stays live (PreToolUse
    // still fires for cycle interception) and is now anchored on `cycle-start-bare`.

    v.push(Case::new(
        "cycle-start-bare",
        "PreToolUse",
        &[
            "cycle::bare_name",
            "cycle::start",
            "normalize_event_name::canonical::PreToolUse",
        ],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "context_cycle",
            "tool_input": {
                "type": "start",
                "topic": "vnc-026",
                "goal": "Ship the remote HTTP hook client"
            }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "cycle-start-prefixed",
        "PreToolUse",
        &["cycle::prefixed_name"],
        json!({
            "session_id": "sess-corpus",
            "tool_name": "mcp__unimatrix__context_cycle",
            "tool_input": { "type": "start", "topic": "vnc-026" }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "cycle-phase-end",
        "PreToolUse",
        &["cycle::phase_end"],
        // phase / next_phase are trimmed + lowercased by validation.
        json!({
            "session_id": "sess-corpus",
            "tool_name": "context_cycle",
            "tool_input": {
                "type": "phase-end",
                "topic": "vnc-026",
                "phase": " Design ",
                "outcome": "approved with notes",
                "next_phase": "Implement"
            }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "cycle-stop",
        "PreToolUse",
        &["cycle::stop", "cycle::goal_ignored_non_start"],
        // goal present but type != start → goal must NOT enter the payload.
        json!({
            "session_id": "sess-corpus",
            "tool_name": "context_cycle",
            "tool_input": { "type": "stop", "topic": "vnc-026", "goal": "ignored for stop" }
        })
        .to_string(),
    ));

    // vnc-027 ADR-004 §4: the near-miss (F-02 not-intercepted), invalid-type,
    // invalid-topic, and missing-tool-input cases all resolve to the null no-send
    // sentinel in the TS client, so they are retired from the post-reduction
    // corpus. The F-02 exact-equality security gate is preserved as
    // defense-in-depth and pinned by build-request-tools unit tests, not by a
    // parity golden (there is no frame to compare).

    v.push(Case::new(
        "cycle-goal-overflow-multibyte",
        "PreToolUse",
        &["cycle::goal_truncation"],
        // 342 × '€' (3 bytes) = 1026 bytes > MAX_GOAL_BYTES (1024); byte 1024
        // is mid-char, so truncation backs off to the 1023-byte boundary.
        json!({
            "session_id": "sess-corpus",
            "tool_name": "context_cycle",
            "tool_input": { "type": "start", "topic": "vnc-026", "goal": "€".repeat(342) }
        })
        .to_string(),
    ));

    v.push(Case::new(
        "cycle-mcp-context-promotion",
        "BeforeTool",
        &["build_request::PreToolUse::mcp_context_promotion"],
        // Gemini shape: bare tool name only in mcp_context; promotion copies
        // it into extra so the interception fires (provider: gemini-cli).
        json!({
            "session_id": "gem-1",
            "mcp_context": {
                "server_name": "unimatrix",
                "tool_name": "context_cycle",
                "url": "ctx://unimatrix"
            },
            "tool_input": {
                "type": "start",
                "topic": "vnc-026",
                "goal": "Promoted from mcp_context"
            }
        })
        .to_string(),
    ));

    // -- SubagentStart: prompt_snippet + transcript tail --

    v.push(Case::new(
        "sas-prompt-snippet",
        "SubagentStart",
        &[
            "build_request::SubagentStart::context_search",
            "subagent_fallback::not_applicable",
            "normalize_event_name::canonical::SubagentStart",
        ],
        sas_snippet_stdin(),
    ));

    v.push(Case::new(
        "sas-no-snippet-no-transcript",
        "SubagentStart",
        &[
            "build_request::SubagentStart::empty_snippet",
            "subagent_fallback::no_transcript_path",
            "topic_signal::subagent_start",
        ],
        json!({ "session_id": "sess-corpus", "agent_type": "developer" }).to_string(),
    ));

    v.push(
        Case::new(
            "sas-whitespace-snippet-tail",
            "SubagentStart",
            &[
                "build_request::SubagentStart::empty_snippet",
                "subagent_fallback::query_some",
            ],
            json!({
                "session_id": "sess-corpus",
                "agent_type": "rust-developer",
                "prompt_snippet": "  \n\t ",
                "transcript_path": "transcript.jsonl"
            })
            .to_string(),
        )
        .with_transcript(jsonl(&[
            t_user("please build the parity corpus generator"),
            t_assistant_text("starting on the generator now"),
        ])),
    );

    v.push(
        Case::new(
            "sas-tail-basic",
            "SubagentStart",
            &[
                "subagent_fallback::query_some",
                "transcript::user_text",
                "transcript::assistant_text",
                "transcript::tool_pairing",
            ],
            json!({
                "session_id": "sess-corpus",
                "agent_type": "developer",
                "transcript_path": "transcript.jsonl"
            })
            .to_string(),
        )
        .with_transcript(jsonl(&[
            t_user("implement the corpus generator in the server crate"),
            t_assistant_tool(
                "spawning the implementation agent",
                "tu-1",
                "Task",
                json!({ "description": "implement parity corpus generator" }),
            ),
            t_tool_result("tu-1", "agent spawned successfully"),
        ])),
    );

    v.push(Case::new(
        "sas-tail-missing-file",
        "SubagentStart",
        &["subagent_fallback::missing_file"],
        json!({
            "session_id": "sess-corpus",
            "agent_type": "developer",
            "transcript_path": "missing-transcript.jsonl"
        })
        .to_string(),
    ));

    v.push(Case::new(
        "sas-tail-empty-path",
        "SubagentStart",
        &["subagent_fallback::empty_transcript_path"],
        json!({
            "session_id": "sess-corpus",
            "agent_type": "developer",
            "transcript_path": ""
        })
        .to_string(),
    ));

    v.push(
        Case::new(
            "sas-tail-malformed-lines",
            "SubagentStart",
            &["transcript::malformed_lines_skipped"],
            json!({ "session_id": "sess-corpus", "transcript_path": "transcript.jsonl" })
                .to_string(),
        )
        .with_transcript(jsonl(&[
            "this is not json at all".to_string(),
            "{\"type\":".to_string(),
            json!({ "no_type_field": true }).to_string(),
            json!({ "type": "unknown_record_kind", "x": 1 }).to_string(),
            t_user("only this valid user line survives the malformed window"),
        ])),
    );

    v.push(
        Case::new(
            "sas-tail-thinking-only",
            "SubagentStart",
            &["transcript::thinking_only_suppressed"],
            json!({ "session_id": "sess-corpus", "transcript_path": "transcript.jsonl" })
                .to_string(),
        )
        .with_transcript(jsonl(&[
            t_user("a question that prompts pure thinking"),
            json!({
                "type": "assistant",
                "message": { "content": [
                    { "type": "thinking", "thinking": "internal reasoning, never emitted" }
                ] }
            })
            .to_string(),
            t_assistant_text("the visible answer"),
        ])),
    );

    v.push(
        Case::new(
            "sas-tail-window-mid-line",
            "SubagentStart",
            &["transcript::mid_line_window"],
            json!({ "session_id": "sess-corpus", "transcript_path": "transcript.jsonl" })
                .to_string(),
        )
        .with_transcript(mid_line_window_transcript()),
    );

    v.push(
        Case::new(
            "sas-tail-multibyte-window-edge",
            "SubagentStart",
            &["transcript::multibyte_window_edge"],
            json!({ "session_id": "sess-corpus", "transcript_path": "transcript.jsonl" })
                .to_string(),
        )
        .with_transcript(multibyte_edge_transcript()),
    );

    v.push(
        Case::new(
            "sas-tail-budget-overflow",
            "SubagentStart",
            &["transcript::budget_loop_break"],
            json!({ "session_id": "sess-corpus", "transcript_path": "transcript.jsonl" })
                .to_string(),
        )
        .with_transcript(budget_overflow_transcript()),
    );

    v.push(
        Case::new(
            "sas-tail-single-turn-too-big",
            "SubagentStart",
            &["transcript::no_turns_none"],
            // The lone turn exceeds the whole budget → block is None → the
            // fallback keeps the RecordEvent.
            json!({ "session_id": "sess-corpus", "transcript_path": "transcript.jsonl" })
                .to_string(),
        )
        .with_transcript(jsonl(&[t_user(&"y".repeat(MAX_PRECOMPACT_BYTES + 500))])),
    );

    v.push(
        Case::new(
            "sas-tail-tool-result-string-content",
            "SubagentStart",
            &["transcript::tool_result_string_content"],
            json!({ "session_id": "sess-corpus", "transcript_path": "transcript.jsonl" })
                .to_string(),
        )
        .with_transcript(jsonl(&[
            t_assistant_tool(
                "running a quick check",
                "tu-2",
                "Bash",
                json!({ "command": "cargo fmt --check" }),
            ),
            json!({
                "type": "user",
                "message": { "content": [
                    { "type": "tool_result", "tool_use_id": "tu-2",
                      "content": "plain string tool result" }
                ] }
            })
            .to_string(),
        ])),
    );

    v.push(
        Case::new(
            "sas-tail-key-param-fallback",
            "SubagentStart",
            &["transcript::key_param_fallback"],
            json!({ "session_id": "sess-corpus", "transcript_path": "transcript.jsonl" })
                .to_string(),
        )
        .with_transcript(jsonl(&[t_assistant_tool(
            "calling an unmapped tool",
            "tu-3",
            "CustomFrobnicator",
            json!({ "count": 3, "label": "first string field wins" }),
        )])),
    );

    v.push(
        Case::new(
            "sas-tail-raw-content-shape",
            "SubagentStart",
            &["transcript::raw_content_shape"],
            // Raw API format: content array directly on the record, no
            // message wrapper.
            json!({ "session_id": "sess-corpus", "transcript_path": "transcript.jsonl" })
                .to_string(),
        )
        .with_transcript(jsonl(&[
            json!({ "type": "user",
                    "content": [ { "type": "text", "text": "raw shape user text" } ] })
            .to_string(),
            json!({ "type": "assistant",
                    "content": [ { "type": "text", "text": "raw shape assistant text" } ] })
            .to_string(),
        ])),
    );

    v
}
