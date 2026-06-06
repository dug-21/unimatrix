//! Transcript JSONL builders for the parity-corpus SubagentStart tail cases
//! (vnc-026, ADR-001). Shared by `parity_corpus_cases_b.rs`.

use crate::uds::transcript_block::{MAX_PRECOMPACT_BYTES, TAIL_MULTIPLIER};
use serde_json::json;

pub(crate) fn t_user(text: &str) -> String {
    json!({ "type": "user", "message": { "content": [ { "type": "text", "text": text } ] } })
        .to_string()
}

pub(crate) fn t_assistant_text(text: &str) -> String {
    json!({ "type": "assistant", "message": { "content": [ { "type": "text", "text": text } ] } })
        .to_string()
}

pub(crate) fn t_assistant_tool(
    text: &str,
    id: &str,
    name: &str,
    input: serde_json::Value,
) -> String {
    json!({
        "type": "assistant",
        "message": { "content": [
            { "type": "text", "text": text },
            { "type": "tool_use", "id": id, "name": name, "input": input }
        ] }
    })
    .to_string()
}

pub(crate) fn t_tool_result(id: &str, text: &str) -> String {
    json!({
        "type": "user",
        "message": { "content": [
            { "type": "tool_result", "tool_use_id": id,
              "content": [ { "type": "text", "text": text } ] }
        ] }
    })
    .to_string()
}

pub(crate) fn jsonl(lines: &[String]) -> String {
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

/// Transcript longer than the tail window whose window cut lands mid-line
/// (the partial first line fails the JSON parse and is skipped).
pub(crate) fn mid_line_window_transcript() -> String {
    let window = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER;
    let filler = t_user(&"a".repeat(window + 200));
    let transcript = jsonl(&[
        filler,
        t_user("first full line after the mid-line window cut"),
        t_assistant_text("assistant reply inside the window"),
    ]);
    assert!(transcript.len() > window);
    transcript
}

/// Transcript longer than the tail window whose window cut lands INSIDE a
/// multi-byte char (emoji filler line). Tail padding varies the cut offset
/// mod 4 until it is not a char boundary.
pub(crate) fn multibyte_edge_transcript() -> String {
    let window = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER;
    for pad in 0..4 {
        let filler = t_user(&"😀".repeat(window / 4 + 100));
        let tail_text = format!("after the multibyte window edge{}", "!".repeat(pad));
        let transcript = jsonl(&[
            filler,
            t_user(&tail_text),
            t_assistant_text("assistant reply after the emoji filler"),
        ]);
        if transcript.len() > window {
            let cut = transcript.len() - window;
            if !transcript.is_char_boundary(cut) {
                return transcript;
            }
        }
    }
    panic!("could not construct a multibyte window-edge transcript");
}

/// Eight 400-byte user turns: the 3000-byte budget loop breaks partway
/// through (most-recent-first), proving the break branch.
pub(crate) fn budget_overflow_transcript() -> String {
    let lines: Vec<String> = (0..8)
        .map(|i| t_user(&format!("turn {i:02} {}", "x".repeat(392))))
        .collect();
    jsonl(&lines)
}
