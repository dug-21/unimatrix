//! Shared transcript-block extraction core (vnc-025, ADR-005).
//!
//! One core, two thin front-ends: the local PreCompact hook reads the
//! transcript file tail via [`extract_transcript_block`]; the server builds
//! the same block from buffered transcript bytes via
//! [`extract_transcript_block_from_bytes`]. Parity is structural — both call
//! the same private `block_from_lines` pipeline (JSONL lines → exchange
//! turns → budget loop → header/body/footer).
//!
//! Moved verbatim-where-possible from `hook.rs` (R-14: behavior unchanged;
//! the pre-move test-name inventory passes unmodified). No `tracing` call in
//! this module touches line/turn/block content (NFR-01, AC-12).

use std::io::{BufRead, BufReader, Seek, SeekFrom};

/// Maximum byte budget for the PreCompact transcript restoration block (~750 tokens).
/// Separate from MAX_INJECTION_BYTES (1400) per D-4 and AC-10. Value PINNED (R-14.2).
pub const MAX_PRECOMPACT_BYTES: usize = 3000;

/// Tail-bytes window multiplier. Raw JSONL is ~4x larger than extracted text.
/// TAIL_WINDOW_BYTES = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER = 12,000 bytes (ADR-001).
/// Value PINNED (R-14.2).
pub const TAIL_MULTIPLIER: usize = 4;

/// Per-tool-result snippet truncation budget (D-3, FR-03.4).
const TOOL_RESULT_SNIPPET_BYTES: usize = 300;

/// Key-param truncation budget for tool compact representation (OQ-3).
const TOOL_KEY_PARAM_BYTES: usize = 120;

/// A single typed turn extracted from the JSONL transcript window.
/// Internal to this module — not exported.
enum ExchangeTurn {
    UserText(String),
    AssistantText(String),
    ToolPair {
        name: String,
        key_param: String,
        result_snippet: String,
    },
}

/// Truncate a string to at most `max_bytes` bytes, ensuring the result
/// is a valid UTF-8 string (never splits a multi-byte character).
pub(crate) fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    &s[..end]
}

/// Return the most-identifying input field value for a tool call.
///
/// Hardcoded map for 10 known Claude Code tools (OQ-3 settled).
/// Fallback: first string-valued field in the input object.
/// Result truncated to TOOL_KEY_PARAM_BYTES via truncate_utf8.
fn extract_key_param(tool_name: &str, input: &serde_json::Value) -> String {
    let field_name: &str = match tool_name {
        "Bash" => "command",
        "Read" => "file_path",
        "Edit" => "file_path",
        "Write" => "file_path",
        "Glob" => "pattern",
        "Grep" => "pattern",
        "MultiEdit" => "file_path",
        "Task" => "description",
        "WebFetch" => "url",
        "WebSearch" => "query",
        _ => "",
    };

    if !field_name.is_empty() {
        if let Some(val) = input.get(field_name).and_then(|v| v.as_str()) {
            return truncate_utf8(val, TOOL_KEY_PARAM_BYTES).to_string();
        }
    }

    if let Some(obj) = input.as_object() {
        for (_key, val) in obj {
            if let Some(s) = val.as_str() {
                return truncate_utf8(s, TOOL_KEY_PARAM_BYTES).to_string();
            }
        }
    }

    String::new()
}

/// Helper: extract the content array from a JSONL record.
/// Handles two shapes:
///   { "type": "...", "message": { "content": [...] } }  (Claude Code UX format)
///   { "type": "...", "content": [...] }                 (raw API format)
fn get_content_array(record: &serde_json::Value) -> &[serde_json::Value] {
    if let Some(arr) = record
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        return arr;
    }
    if let Some(arr) = record.get("content").and_then(|c| c.as_array()) {
        return arr;
    }
    &[]
}

/// Helper: extract snippet text from a tool_result content block.
/// Returns first type:"text" block text truncated to TOOL_RESULT_SNIPPET_BYTES.
fn extract_tool_result_snippet(tool_result_block: &serde_json::Value) -> String {
    let content = tool_result_block.get("content");
    match content {
        Some(serde_json::Value::String(s)) => {
            truncate_utf8(s, TOOL_RESULT_SNIPPET_BYTES).to_string()
        }
        Some(serde_json::Value::Array(blocks)) => {
            for block in blocks {
                if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        return truncate_utf8(text, TOOL_RESULT_SNIPPET_BYTES).to_string();
                    }
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

/// Parse JSONL lines from a tail window into typed exchange turns.
///
/// Fail-open: malformed lines and unknown type values are skipped silently.
/// Tool-use/result pairing: adjacent-record scan (ADR-002).
/// Returns turns in reverse-chronological order (Vec reversed before return).
fn build_exchange_pairs(lines: &[&str]) -> Vec<ExchangeTurn> {
    let mut turns: Vec<ExchangeTurn> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        let record: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                i += 1;
                continue;
            }
        };

        let record_type = match record.get("type").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => {
                i += 1;
                continue;
            }
        };

        match record_type {
            "user" => {
                let content_arr = get_content_array(&record);
                let user_texts: Vec<&str> = content_arr
                    .iter()
                    .filter_map(|block| {
                        if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                            block.get("text").and_then(|v| v.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();

                if !user_texts.is_empty() {
                    turns.push(ExchangeTurn::UserText(user_texts.join("\n")));
                }
                i += 1;
            }

            "assistant" => {
                let content_arr = get_content_array(&record);

                let asst_texts: Vec<&str> = content_arr
                    .iter()
                    .filter_map(|block| {
                        if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                            block.get("text").and_then(|v| v.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();

                struct ToolUseInfo {
                    id: String,
                    name: String,
                    key_param: String,
                }
                let tool_uses: Vec<ToolUseInfo> = content_arr
                    .iter()
                    .filter_map(|block| {
                        if block.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
                            return None;
                        }
                        let id = block.get("id").and_then(|v| v.as_str())?.to_string();
                        let name = block.get("name").and_then(|v| v.as_str())?.to_string();
                        let input = block
                            .get("input")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null);
                        let key_param = extract_key_param(&name, &input);
                        Some(ToolUseInfo {
                            id,
                            name,
                            key_param,
                        })
                    })
                    .collect();

                let has_text = !asst_texts.is_empty();
                let has_tool_use = !tool_uses.is_empty();

                // Pure thinking turn (no text, no tool_use): suppress entirely
                if !has_text && !has_tool_use {
                    i += 1;
                    continue;
                }

                // Emit AssistantText only if there is actual text (OQ-SPEC-1)
                if has_text {
                    turns.push(ExchangeTurn::AssistantText(asst_texts.join("\n")));
                }

                // Adjacent-record look-ahead for tool_result pairing (ADR-002)
                let mut result_map: std::collections::HashMap<String, String> =
                    std::collections::HashMap::new();

                if has_tool_use && i + 1 < lines.len() {
                    let next_line = lines[i + 1];
                    if !next_line.trim().is_empty() {
                        if let Ok(next_record) =
                            serde_json::from_str::<serde_json::Value>(next_line)
                        {
                            if next_record.get("type").and_then(|v| v.as_str()) == Some("user") {
                                let next_content = get_content_array(&next_record);
                                for block in next_content {
                                    if block.get("type").and_then(|v| v.as_str())
                                        != Some("tool_result")
                                    {
                                        continue;
                                    }
                                    let tool_use_id =
                                        match block.get("tool_use_id").and_then(|v| v.as_str()) {
                                            Some(id) => id.to_string(),
                                            None => continue,
                                        };
                                    let snippet = extract_tool_result_snippet(block);
                                    result_map.insert(tool_use_id, snippet);
                                }
                            }
                        }
                    }
                }

                for tu in &tool_uses {
                    let result_snippet = result_map.get(&tu.id).cloned().unwrap_or_default();
                    turns.push(ExchangeTurn::ToolPair {
                        name: tu.name.clone(),
                        key_param: tu.key_param.clone(),
                        result_snippet,
                    });
                }

                i += 1;
            }

            _ => {
                i += 1;
            }
        }
    }

    turns.reverse();
    turns
}

/// Format a single ExchangeTurn as a text line.
fn format_turn(turn: &ExchangeTurn) -> String {
    match turn {
        ExchangeTurn::UserText(text) => format!("[User] {}", text),
        ExchangeTurn::AssistantText(text) => format!("[Assistant] {}", text),
        ExchangeTurn::ToolPair {
            name,
            key_param,
            result_snippet,
        } => {
            format!(
                "[tool: {}({}) \u{2192} {}]",
                name, key_param, result_snippet
            )
        }
    }
}

/// Shared extraction core: JSONL lines → exchange turns → budget loop →
/// header/body/footer block. Called by both `extract_transcript_block`
/// (after file-open + tail seek + read-lines) and
/// `extract_transcript_block_from_bytes` (after lossy decode + line split).
///
/// Returns None when no complete turn fits the budget (ADR-003 degradation).
fn block_from_lines(lines: &[&str]) -> Option<String> {
    let turns: Vec<ExchangeTurn> = build_exchange_pairs(lines);

    let mut output_parts: Vec<String> = Vec::new();
    let mut bytes_used: usize = 0;
    let mut exchange_count: usize = 0;

    for turn in &turns {
        let turn_text = format_turn(turn);
        let turn_bytes = turn_text.len();
        if bytes_used + turn_bytes > MAX_PRECOMPACT_BYTES {
            break;
        }
        bytes_used += turn_bytes;
        if matches!(turn, ExchangeTurn::UserText(_)) {
            exchange_count += 1;
        }
        output_parts.push(turn_text);
    }

    if output_parts.is_empty() {
        return None;
    }

    let header = format!(
        "=== Recent conversation (last {} exchanges) ===",
        exchange_count
    );
    let footer = "=== End recent conversation ===".to_string();
    let body = output_parts.join("\n");

    Some(format!("{}\n{}\n{}", header, body, footer))
}

/// Read the tail of the transcript file at `path`, parse as JSONL, and format
/// a restoration block within MAX_PRECOMPACT_BYTES.
///
/// Returns None on any failure (ADR-003 degradation contract).
/// Never panics. Never propagates errors. All I/O is std::io — no tokio.
pub fn extract_transcript_block(path: &str) -> Option<String> {
    let inner = || -> Option<String> {
        let mut file = std::fs::File::open(path).ok()?;
        let file_len: u64 = file.metadata().ok()?.len();

        let window: u64 = (MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER) as u64;
        let seek_back: u64 = window.min(file_len);

        if seek_back > 0 {
            file.seek(SeekFrom::End(-(seek_back as i64))).ok()?;
        }

        let reader = BufReader::new(file);
        let raw_lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();

        let line_refs: Vec<&str> = raw_lines.iter().map(|s| s.as_str()).collect();
        block_from_lines(&line_refs)
    };

    inner()
}

/// Build the PreCompact restoration block from raw JSONL transcript-file
/// bytes already windowed by the caller (vnc-025, ADR-005, FR-18).
///
/// Mirrors `extract_transcript_block` minus the file-tail seek: the caller
/// (the server's PreCompact path) provides only the tail window via
/// `TranscriptBuffer::contiguous_tail`. The window may begin mid-line — the
/// partial first line fails the JSON parse inside `build_exchange_pairs` and
/// is filtered, exactly like the path variant's seek landing mid-line.
///
/// Invalid UTF-8 (e.g., a window boundary splitting a multi-byte char) is
/// lossy-decoded, never an error. Never panics. Empty/whitespace input → None.
pub fn extract_transcript_block_from_bytes(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let raw_lines: Vec<&str> = text.lines().collect();
    block_from_lines(&raw_lines)
}

/// Combine optional transcript block with briefing content.
///
/// Cases:
/// 1. Both present: "{transcript}\n\n{briefing}"
/// 2. Transcript only: "{transcript}"
/// 3. Briefing only: "{briefing}"
/// 4. Both empty: ""
pub fn prepend_transcript(transcript: Option<&str>, briefing: &str) -> String {
    let briefing_empty = briefing.is_empty();
    match (transcript, briefing_empty) {
        (Some(t), false) => format!("{}\n\n{}", t, briefing),
        (Some(t), true) => t.to_string(),
        (None, false) => briefing.to_string(),
        (None, true) => String::new(),
    }
}

#[cfg(test)]
#[path = "transcript_block_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "transcript_block_tests_bytes.rs"]
mod tests_bytes;
