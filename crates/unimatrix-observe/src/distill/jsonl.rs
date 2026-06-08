//! Claude Code JSONL parse — untrusted-input-hardened (crt-052, C3).
//!
//! Buffer content is untrusted client-disk JSONL (Constraint 7, SR-09). Every
//! malformed / adversarial line is counted and dropped: this parser NEVER
//! returns `Err` and NEVER panics (R-10, AC-V-FUZZ — merge gate).
//!
//! Record shape (Claude Code transcript line):
//! ```json
//! {"type":"user","message":{"role":"user","content":"text or [blocks]"},"timestamp":"..."}
//! {"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"..."}]},"timestamp":"..."}
//! ```
//! `content` is either a bare string or an array of typed blocks. Only `text`
//! blocks are kept; `tool_use` / `tool_result` / `thinking` blocks are dropped.

use serde_json::Value;

/// Maximum bytes for a single JSONL line. Lines longer than this are skipped
/// (oversized-line resource-exhaustion guard, AC-V-FUZZ). Generous relative to
/// real transcript lines (largest observed full session is ~2 MiB; individual
/// lines are far smaller).
pub const MAX_LINE_BYTES: usize = 1024 * 1024; // 1 MiB

/// Maximum JSON nesting depth tolerated when extracting text segments.
/// Deeper structures are treated as malformed and skipped (billion-laughs /
/// deeply-nested-JSON guard, AC-V-FUZZ — bounded, no stack overflow).
pub const MAX_JSON_DEPTH: usize = 64;

/// Maximum total bytes of extracted text accumulated from a single record's
/// content. Bounds a gigantic-field record without per-line rejection.
pub const MAX_TEXT_BYTES: usize = MAX_LINE_BYTES;

/// Role of a kept transcript block. Only `User` / `Assistant` survive; all
/// other record types are dropped before a `ParsedBlock` is produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

/// A parsed `user` / `assistant` text block from the transcript.
///
/// `byte_offset` is the in-snapshot offset of this line's first byte
/// (array-relative). The selection entry adds `base_offset` to make it the
/// logical stream offset (R-12).
#[derive(Debug, Clone)]
pub struct ParsedBlock {
    pub role: Role,
    pub text: String,
    pub ts: Option<String>,
    pub byte_offset: u64,
}

/// Parse snapshot bytes into kept blocks plus a skip-count of dropped lines.
///
/// Operates on `&[u8]`; does not require the buffer to be valid UTF-8 up front.
/// Each line is parse-or-skip:
/// - empty line → skipped silently (not counted),
/// - non-UTF-8 / oversized / un-parseable JSON → **skip-with-count**,
/// - valid JSON of an unknown / non-text record type → dropped (not counted —
///   it is a known-format filter, not a parse failure),
/// - `user` / `assistant` record with no text after extraction → dropped.
///
/// A truncated FINAL line (ring-tail / hole boundary) is the common real case
/// and is tolerated via the per-line parse-or-skip (it fails `from_str` and is
/// counted). NEVER returns `Err`; NEVER panics.
pub fn parse_blocks(bytes: &[u8]) -> (Vec<ParsedBlock>, u64) {
    let mut blocks = Vec::new();
    let mut skip: u64 = 0;
    let mut offset: u64 = 0;

    for line in bytes.split(|&b| b == b'\n') {
        let line_offset = offset;
        // Advance past this line and the newline delimiter. `split` does not
        // re-emit the delimiter, so account for it explicitly.
        offset = offset.saturating_add(line.len() as u64).saturating_add(1);

        if line.is_empty() {
            continue;
        }
        if line.len() > MAX_LINE_BYTES {
            skip += 1; // oversized-line guard
            continue;
        }
        let text_str = match std::str::from_utf8(line) {
            Ok(s) => s,
            Err(_) => {
                skip += 1; // non-UTF-8 line (covers embedded NUL in invalid seq)
                continue;
            }
        };
        let rec: Value = match serde_json::from_str(text_str) {
            Ok(v) => v,
            Err(_) => {
                skip += 1; // truncated / garbage JSON (incl. truncated final line)
                continue;
            }
        };

        let role = match record_role(&rec) {
            Some(r) => r,
            None => continue, // tool_use/tool_result/thinking/unknown → drop, no count
        };

        let text = match extract_text(&rec) {
            Some(t) => t,
            None => continue, // malformed/over-deep content → drop
        };
        if text.is_empty() {
            continue;
        }

        blocks.push(ParsedBlock {
            role,
            text,
            ts: extract_ts(&rec),
            byte_offset: line_offset,
        });
    }

    (blocks, skip)
}

/// Resolve the record role from `type` (preferred) or the nested
/// `message.role`. Returns `None` for any non-user/assistant record.
fn record_role(rec: &Value) -> Option<Role> {
    let ty = rec.get("type").and_then(Value::as_str).or_else(|| {
        rec.get("message")
            .and_then(|m| m.get("role"))
            .and_then(Value::as_str)
    });
    match ty {
        Some("user") => Some(Role::User),
        Some("assistant") => Some(Role::Assistant),
        _ => None,
    }
}

/// Extract the record timestamp if present and a string.
fn extract_ts(rec: &Value) -> Option<String> {
    rec.get("timestamp")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Concatenate the text segments of the record's `message.content`.
///
/// `content` is either a bare string or an array of typed blocks; only `text`
/// blocks contribute. Returns `None` if the structure exceeds `MAX_JSON_DEPTH`
/// (deeply-nested guard) — the line is then dropped. Accumulation stops once
/// `MAX_TEXT_BYTES` is reached (gigantic-field guard); the bounded prefix is
/// still returned so partial real content is not lost.
fn extract_text(rec: &Value) -> Option<String> {
    let content = rec.get("message").and_then(|m| m.get("content"))?;
    let mut out = String::new();
    collect_text(content, 0, &mut out)?;
    Some(out)
}

/// Recursively collect `text` segments with a bounded depth. Returns `None` on
/// depth breach (treated as malformed → skip). Iterative-style early return on
/// byte cap prevents unbounded growth.
fn collect_text(v: &Value, depth: usize, out: &mut String) -> Option<()> {
    if depth > MAX_JSON_DEPTH {
        return None; // billion-laughs / deeply-nested guard
    }
    if out.len() >= MAX_TEXT_BYTES {
        return Some(()); // gigantic-field guard: stop accumulating, keep prefix
    }
    match v {
        // Bare-string content (common user message form).
        Value::String(s) => {
            push_bounded(out, s);
            Some(())
        }
        // Array of typed content blocks.
        Value::Array(items) => {
            for item in items {
                if out.len() >= MAX_TEXT_BYTES {
                    break;
                }
                // A block contributes only its `text` field when type == "text".
                // Other block types (tool_use/tool_result/thinking) are skipped.
                match item {
                    Value::Object(map) => {
                        // Contribute only `text`-typed blocks; tool_use /
                        // tool_result / thinking blocks are skipped.
                        let is_text = map.get("type").and_then(Value::as_str) == Some("text");
                        if let Some(t) = map.get("text").and_then(Value::as_str).filter(|_| is_text)
                        {
                            push_bounded(out, t);
                        }
                    }
                    // Nested arrays / bare strings inside the content array:
                    // recurse with bounded depth so we never blow the stack.
                    Value::String(s) => push_bounded(out, s),
                    Value::Array(_) => {
                        collect_text(item, depth + 1, out)?;
                    }
                    _ => {}
                }
            }
            Some(())
        }
        _ => Some(()),
    }
}

/// Append `s` to `out`, separating multiple segments with a newline, without
/// exceeding `MAX_TEXT_BYTES` (truncates the appended segment if needed).
fn push_bounded(out: &mut String, s: &str) {
    if !out.is_empty() {
        out.push('\n');
    }
    let remaining = MAX_TEXT_BYTES.saturating_sub(out.len());
    if s.len() <= remaining {
        out.push_str(s);
    } else {
        // Truncate on a char boundary to keep `out` valid UTF-8.
        let mut end = remaining;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        out.push_str(&s[..end]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_line(text: &str, ts: &str) -> String {
        format!(
            r#"{{"type":"user","message":{{"role":"user","content":"{text}"}},"timestamp":"{ts}"}}"#
        )
    }

    fn assistant_block_line(text: &str, ts: &str) -> String {
        format!(
            r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"{text}"}}]}},"timestamp":"{ts}"}}"#
        )
    }

    #[test]
    fn test_jsonl_keeps_user_assistant_text_blocks() {
        let mut buf = String::new();
        buf.push_str(&user_line("hello", "2026-01-01T00:00:00Z"));
        buf.push('\n');
        buf.push_str(&assistant_block_line("world", "2026-01-01T00:00:01Z"));
        buf.push('\n');
        // tool_use / tool_result / thinking blocks must be dropped.
        buf.push_str(
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{}}]},"timestamp":"t"}"#,
        );
        buf.push('\n');
        buf.push_str(
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok"}]},"timestamp":"t"}"#,
        );
        buf.push('\n');
        buf.push_str(
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hmm"}]},"timestamp":"t"}"#,
        );

        let (blocks, skip) = parse_blocks(buf.as_bytes());
        assert_eq!(skip, 0);
        assert_eq!(blocks.len(), 2, "only the two text blocks survive");
        assert_eq!(blocks[0].role, Role::User);
        assert_eq!(blocks[0].text, "hello");
        assert_eq!(blocks[1].role, Role::Assistant);
        assert_eq!(blocks[1].text, "world");
    }

    #[test]
    fn test_jsonl_unknown_record_type_skip_with_count() {
        // Valid JSON of an unknown type → dropped, NOT counted (known-format filter).
        let line = r#"{"type":"summary","message":{"role":"summary","content":"x"}}"#;
        let (blocks, skip) = parse_blocks(line.as_bytes());
        assert!(blocks.is_empty());
        assert_eq!(
            skip, 0,
            "unknown-but-valid-JSON type is a drop, not a parse error"
        );
    }

    #[test]
    fn test_jsonl_unknown_record_type_no_panic() {
        let line = br#"{"type":"system","subtype":"compact"}"#;
        let (blocks, _skip) = parse_blocks(line);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_jsonl_tolerates_truncated_final_line() {
        let mut buf = String::new();
        buf.push_str(&user_line("kept", "t1"));
        buf.push('\n');
        buf.push_str(r#"{"type":"assistant","message":{"role":"assist"#); // truncated, no newline
        let (blocks, skip) = parse_blocks(buf.as_bytes());
        assert_eq!(blocks.len(), 1, "prior complete line is parsed");
        assert_eq!(blocks[0].text, "kept");
        assert_eq!(skip, 1, "truncated final line is skip-with-count");
    }

    #[test]
    fn test_jsonl_operates_on_bytes() {
        // Non-UTF-8 bytes in a line → tolerated via skip-with-count, no panic.
        let mut bytes = user_line("ok", "t").into_bytes();
        bytes.push(b'\n');
        bytes.extend_from_slice(&[0xff, 0xfe, 0x00, 0xfd]); // invalid UTF-8 + NUL
        let (blocks, skip) = parse_blocks(&bytes);
        assert_eq!(blocks.len(), 1);
        assert_eq!(skip, 1);
    }

    #[test]
    fn test_jsonl_truncated_json_no_panic() {
        let line = br#"{"type":"user","message":{"role":"user","content":"#;
        let (blocks, skip) = parse_blocks(line);
        assert!(blocks.is_empty());
        assert_eq!(skip, 1);
    }

    #[test]
    fn test_jsonl_non_utf8_bytes_no_panic() {
        let bytes = [0x80u8, 0x81, 0x82, b'\n', 0xc0, 0xc1];
        let (blocks, skip) = parse_blocks(&bytes);
        assert!(blocks.is_empty());
        assert_eq!(skip, 2);
    }

    #[test]
    fn test_jsonl_oversized_single_line_bounded() {
        // A single line larger than MAX_LINE_BYTES → bounded skip, no OOM.
        let mut bytes = vec![b'x'; MAX_LINE_BYTES + 16];
        bytes.push(b'\n');
        bytes.extend_from_slice(user_line("after", "t").as_bytes());
        let (blocks, skip) = parse_blocks(&bytes);
        assert_eq!(
            blocks.len(),
            1,
            "the line after the oversized one still parses"
        );
        assert_eq!(blocks[0].text, "after");
        assert_eq!(skip, 1);
    }

    #[test]
    fn test_jsonl_embedded_nul_no_panic() {
        // Embedded NUL inside otherwise-valid-looking bytes. serde_json rejects
        // a raw control char in a string; either way it must skip-with-count.
        let mut bytes = br#"{"type":"user","message":{"role":"user","content":"a"#.to_vec();
        bytes.push(0x00);
        bytes.extend_from_slice(br#"b"}}"#);
        let (blocks, skip) = parse_blocks(&bytes);
        assert!(blocks.is_empty());
        assert_eq!(skip, 1);
    }

    #[test]
    fn test_jsonl_deeply_nested_json_bounded() {
        // Build a deeply-nested content array well beyond MAX_JSON_DEPTH.
        let depth = MAX_JSON_DEPTH + 50;
        let mut inner = String::from(r#""leaf""#);
        for _ in 0..depth {
            inner = format!("[{inner}]");
        }
        let line = format!(r#"{{"type":"user","message":{{"role":"user","content":{inner}}}}}"#);
        // Must not stack-overflow; the over-deep record is dropped.
        let (blocks, _skip) = parse_blocks(line.as_bytes());
        assert!(
            blocks.is_empty(),
            "over-deep content is dropped, not panicked on"
        );
    }

    #[test]
    fn test_jsonl_bare_string_content_kept() {
        let line = user_line("a plain user message", "t");
        let (blocks, skip) = parse_blocks(line.as_bytes());
        assert_eq!(skip, 0);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "a plain user message");
    }
}
